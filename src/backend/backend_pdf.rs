//! # Standards-Compliant PDF 1.4 Serializer (`pdf`)
//!
//! Provides a high-throughput, hand-rolled PDF 1.4 binary generator with pure-Rust
//! RFC 1950 `/Filter /FlateDecode` stream compression.
//!
//! ## Architectural Principles
//!
//! - **Single-Pass Emission**: Emits PDF objects directly into preallocated buffers without intermediate
//!   tree copying or secondary passes.
//! - **Built-in Standard-14 Fonts**: Uses PDF Type 1 standard fonts (Helvetica, Helvetica-Bold,
//!   Helvetica-Oblique, Helvetica-BoldOblique) with `WinAnsiEncoding`. No external font files or embedding required.
//! - **L1-Resident DEFLATE Compression**: Page content streams are dynamically compressed using
//!   [`crate::deflate::DeflateCompressor`] resident in L1 data cache, halving file sizes with negligible CPU cost.
//! - **Deterministic Object Layout**:
//!   - Object 1: Catalog dictionary (`/Type /Catalog`)
//!   - Object 2: Pages tree root (`/Type /Pages`)
//!   - Objects `3..(3+n-1)`: Page dictionaries (`/Type /Page`)
//!   - Objects `(3+n)..(3+2n-1)`: Content stream objects (`/Filter /FlateDecode`)
//!   - Objects `(3+2n)..(3+2n+3)`: Standard-14 Type 1 font dictionaries
//!   - Fixed-width 20-byte cross-reference table (`xref`), trailer dictionary, and `startxref`.
//!
//! ## Public APIs
//!
//! - [`emit`]: Serializes pages to an owned `Vec<u8>` containing the complete PDF document.
//! - [`emit_into`]: Serializes pages into a preallocated `Vec<u8>` buffer using compression.
//! - [`emit_into_opts`]: Serializes pages with an optional compression toggle (`enable_compression`).
//! - [`emit_into_consume`]: Serializes pages while progressively deallocating each page's display list.

use crate::font_metrics::{Face, FontFamily};
use crate::backend::backend_shape::{self as shape, Glyph, TextFont};
use crate::backend::optical::OpticalMaster;
use crate::page_layout::{Marker, Page, Paint, GLYPH_RUN_COUNT_MASK, GLYPH_RUN_SHAPED, PAGE_H, PAGE_W};
use crate::Error;

enum PagesSlice<'p, 'a> {
    Borrowed(&'p [Page<'a>]),
    Mut(&'p mut [Page<'a>]),
}

impl<'p, 'a> PagesSlice<'p, 'a> {
    #[inline(always)]
    fn len(&self) -> usize {
        match self {
            Self::Borrowed(p) => p.len(),
            Self::Mut(p) => p.len(),
        }
    }

    #[inline(always)]
    fn as_slice(&self) -> &[Page<'a>] {
        match self {
            Self::Borrowed(p) => p,
            Self::Mut(p) => p,
        }
    }
}

pub fn emit(pages: &[Page<'_>]) -> Result<Vec<u8>, Error> {
    let mut destination_buffer = Vec::with_capacity((pages.len() * 10240).max(16384));
    emit_into(pages, &mut destination_buffer)?;
    Ok(destination_buffer)
}

pub fn emit_into(pages: &[Page<'_>], destination_buffer: &mut Vec<u8>) -> Result<(), Error> {
    emit_into_opts(pages, destination_buffer, true)
}

pub fn emit_into_opts(
    pages: &[Page<'_>],
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<(), Error> {
    emit_into_internal(PagesSlice::Borrowed(pages), destination_buffer, enable_compression)
}

pub fn emit_into_consume(
    pages: &mut [Page<'_>],
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<(), Error> {
    emit_into_internal(PagesSlice::Mut(pages), destination_buffer, enable_compression)
}

/// 8192-bit stack/cache-resident bitset for O(1) mathematical CID tracking and sorting.
#[repr(C, align(64))]
#[derive(Clone)]
pub(crate) struct CidBitSet {
    pub(crate) words: [u64; 128],
    pub(crate) overflow: Vec<u16>,
}

impl CidBitSet {
    #[inline(always)]
    pub(crate) const fn new() -> Self {
        Self {
            words: [0u64; 128],
            overflow: Vec::new(),
        }
    }

    #[inline(always)]
    pub(crate) fn insert(&mut self, gid: u16) {
        let idx = (gid as usize) >> 6;
        if idx < 128 {
            self.words[idx] |= 1u64 << ((gid as usize) & 63);
        } else if !self.overflow.contains(&gid) {
            self.overflow.push(gid);
        }
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        if !self.overflow.is_empty() {
            return false;
        }
        for &w in &self.words {
            if w != 0 {
                return false;
            }
        }
        true
    }

    pub(crate) fn to_sorted_vec(&self) -> Vec<u16> {
        let mut result = Vec::with_capacity(64);
        for (w_idx, &word) in self.words.iter().enumerate() {
            if word != 0 {
                let base = (w_idx << 6) as u16;
                let mut temp = word;
                while temp != 0 {
                    let trailing = temp.trailing_zeros();
                    result.push(base + trailing as u16);
                    temp &= temp - 1;
                }
            }
        }
        if !self.overflow.is_empty() {
            result.extend_from_slice(&self.overflow);
            result.sort_unstable();
            result.dedup();
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FinishedPageMeta {
    pub(crate) width: f32,
    pub(crate) height: f32,
}

pub(crate) struct PageStreamAccumulator<'w> {
    buf: &'w mut Vec<u8>,
    offsets: Vec<usize>,
    page_metas: Vec<FinishedPageMeta>,
    doc_used_faces: u32,
    doc_used_masters: u32,
    cid_set: CidBitSet,
    uncompressed_stream: Vec<u8>,
    compressed_stream: Vec<u8>,
    compressor: crate::deflate_compress::DeflateCompressor,
    enable_compression: bool,
}

impl<'w> PageStreamAccumulator<'w> {
    pub(crate) fn new(destination_buffer: &'w mut Vec<u8>, enable_compression: bool) -> Self {
        destination_buffer.clear();
        destination_buffer.reserve(32768);
        destination_buffer.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
        let mut offsets = Vec::with_capacity(64);
        offsets.resize(1, 0);
        Self {
            buf: destination_buffer,
            offsets,
            page_metas: Vec::with_capacity(32),
            doc_used_faces: 0,
            doc_used_masters: 0,
            cid_set: CidBitSet::new(),
            uncompressed_stream: Vec::with_capacity(16384),
            compressed_stream: Vec::with_capacity(16384),
            compressor: crate::deflate_compress::DeflateCompressor::new(),
            enable_compression,
        }
    }

    pub(crate) fn push_page(&mut self, page: &mut Page<'_>) {
        self.doc_used_faces |= page.used_faces;
        for op in &page.ops {
            let (faces, masters) = op_font_usage(op, page.font_family);
            self.doc_used_faces |= faces;
            self.doc_used_masters |= masters;
            if let Paint::VectorGlyph { glyph_id, .. } | Paint::CidGlyphRun { glyph_id, .. } = op {
                self.cid_set.insert(*glyph_id);
            }
        }

        self.uncompressed_stream.clear();
        build_stream_into(page, &mut self.uncompressed_stream, true);

        let content_id = (3 + self.page_metas.len()) as u32;
        let idx = content_id as usize;
        if idx >= self.offsets.len() {
            self.offsets.resize(idx + 1, 0);
        }
        self.offsets[idx] = self.buf.len();

        write_u64(self.buf, content_id as u64);
        self.buf.extend_from_slice(b" 0 obj\n");

        if self.enable_compression {
            self.compressed_stream.clear();
            self.compressor.compress_zlib(&self.uncompressed_stream, &mut self.compressed_stream);
            self.buf.extend_from_slice(b"<< /Filter /FlateDecode /Length ");
            write_i64(self.buf, self.compressed_stream.len() as i64);
            self.buf.extend_from_slice(b" >>\nstream\n");
            self.buf.extend_from_slice(&self.compressed_stream);
            self.buf.extend_from_slice(b"\nendstream\nendobj\n");
        } else {
            self.buf.extend_from_slice(b"<< /Length ");
            write_i64(self.buf, self.uncompressed_stream.len() as i64);
            self.buf.extend_from_slice(b" >>\nstream\n");
            self.buf.extend_from_slice(&self.uncompressed_stream);
            if !self.uncompressed_stream.ends_with(b"\n") {
                self.buf.push(b'\n');
            }
            self.buf.extend_from_slice(b"endstream\nendobj\n");
        }

        self.page_metas.push(FinishedPageMeta {
            width: page.width,
            height: page.height,
        });

        page.ops = Vec::new();
        page.text_pool = Vec::new();
    }

    pub(crate) fn finish(
        self,
        font_family: FontFamily,
    ) -> Result<usize, Error> {
        let math_cids = self.cid_set.to_sorted_vec();
        let page_count = self.page_metas.len();
        finish_streaming_pdf(
            self.buf,
            self.offsets,
            &self.page_metas,
            font_family,
            self.doc_used_faces,
            self.doc_used_masters,
            &math_cids,
            self.enable_compression,
        )?;
        Ok(page_count)
    }
}

#[derive(Clone, Copy)]
struct MathFontIds {
    font_id: u32,
    cidfont_id: u32,
    descriptor_id: u32,
    file_id: Option<u32>,
    tounicode_id: u32,
}

#[derive(Clone, Copy)]
struct TounicodeIds {
    ot1: u32,
    /// The OT1 typewriter map (cmtt).
    ot1tt: u32,
    oml: u32,
    oms: u32,
    omx: u32,
    msam: u32,
    msbm: u32,
}

#[allow(clippy::too_many_arguments)]
fn finish_streaming_pdf(
    destination_buffer: &mut Vec<u8>,
    offsets: Vec<usize>,
    metas: &[FinishedPageMeta],
    font_family: FontFamily,
    doc_used_faces: u32,
    doc_used_masters: u32,
    math_cids: &[u16],
    enable_compression: bool,
) -> Result<(), Error> {
    let total_pages = metas.len().max(1);
    let mut doc_used_faces = doc_used_faces;
    // A document without text still gets one font resource.
    if doc_used_faces == 0 && doc_used_masters == 0 {
        doc_used_faces = 1 << (Face::Roman as u8);
    }
    let has_math_font = !math_cids.is_empty();

    let catalog_id = 1u32;
    let pages_id = 2u32;
    let first_page_dict_id = 3 + total_pages as u32;

    // Allocate sequential IDs only for used fonts
    let mut font_object_ids = [0u32; Face::COUNT];
    let mut font_descriptor_ids = [0u32; Face::COUNT];
    let mut font_file_ids = [0u32; Face::COUNT];
    let mut next_font_id = 3 + 2 * total_pages as u32;

    for (face_idx, obj_id) in font_object_ids.iter_mut().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            let is_cm_embedded = (font_family == FontFamily::ComputerModern
                && matches!(
                    face,
                    Face::Roman
                        | Face::Bold
                        | Face::Italic
                        | Face::MathItalic
                        | Face::Symbol
                        | Face::Roman7
                        | Face::Roman5
                        | Face::MathItalic7
                        | Face::MathItalic5
                        | Face::Symbol7
                        | Face::Symbol5
                        | Face::AmsA
                        | Face::AmsB
                ))
                || matches!(face, Face::Extension | Face::AmsA | Face::AmsB)
                || crate::backend::cm_fonts::extra_cm_program(face, font_family).is_some();
            *obj_id = next_font_id;
            next_font_id += 1;
            if is_cm_embedded {
                font_descriptor_ids[face_idx] = next_font_id;
                font_file_ids[face_idx] = next_font_id + 1;
                next_font_id += 2;
            }
        }
    }

    let master_ids = MasterFontIds::allocate(doc_used_masters, font_family, &mut next_font_id);

    let mut tounicode_ids = TounicodeIds { ot1: 0, ot1tt: 0, oml: 0, oms: 0, omx: 0, msam: 0, msbm: 0 };
    if font_family == FontFamily::ComputerModern {
        let has_ot1 = (doc_used_faces & Face::OT1_MASK & !Face::OT1TT_MASK) != 0 || master_ids.used & !OpticalMaster::TT_MASK != 0;
        if has_ot1 {
            tounicode_ids.ot1 = next_font_id;
            next_font_id += 1;
        }
        let has_tt = (doc_used_faces & Face::OT1TT_MASK) != 0 || master_ids.used & OpticalMaster::TT_MASK != 0;
        if has_tt {
            tounicode_ids.ot1tt = next_font_id;
            next_font_id += 1;
        }

        let has_oml = (doc_used_faces & Face::OML_MASK) != 0;
        if has_oml {
            tounicode_ids.oml = next_font_id;
            next_font_id += 1;
        }

        let has_oms = (doc_used_faces & Face::OMS_MASK) != 0;
        if has_oms {
            tounicode_ids.oms = next_font_id;
            next_font_id += 1;
        }
    }

    let has_msam = (doc_used_faces & (1 << Face::AmsA as u8)) != 0;
    if has_msam {
        tounicode_ids.msam = next_font_id;
        next_font_id += 1;
    }

    let has_msbm = (doc_used_faces & (1 << Face::AmsB as u8)) != 0;
    if has_msbm {
        tounicode_ids.msbm = next_font_id;
        next_font_id += 1;
    }

    let has_omx = (doc_used_faces & Face::OMX_MASK) != 0;
    if has_omx {
        tounicode_ids.omx = next_font_id;
        next_font_id += 1;
    }

    let math_ids = if has_math_font {
        let f_id = next_font_id;
        next_font_id += 1;
        let c_id = next_font_id;
        next_font_id += 1;
        let d_id = next_font_id;
        next_font_id += 1;
        let file_id = next_font_id;
        next_font_id += 1;
        let tu_id = next_font_id;
        Some(MathFontIds {
            font_id: f_id,
            cidfont_id: c_id,
            descriptor_id: d_id,
            file_id: Some(file_id),
            tounicode_id: tu_id,
        })
    } else {
        None
    };

    let mut writer = Writer::from_existing(destination_buffer, offsets);

    if metas.is_empty() {
        writer.obj(3, |w| {
            w.write_all(b"<< /Length 0 >>\nstream\n\nendstream\n");
        });
    }

    // 1 Catalog
    writer.obj(catalog_id, |w| {
        w.write_all(b"<< /Type /Catalog /Pages ");
        w.write_ref(pages_id);
        w.write_all(b" >>\n");
    });

    // 2 Pages
    writer.obj(pages_id, |w| {
        w.write_all(b"<< /Type /Pages /Count ");
        w.write_int(total_pages as i32);
        w.write_all(b" /Kids [");
        for page_idx in 0..total_pages as u32 {
            w.write_all(b" ");
            w.write_ref(first_page_dict_id + page_idx);
        }
        w.write_all(b" ] >>\n");
    });

    let mut font_res_bytes = Vec::with_capacity(128);
    for (face_idx, &font_id) in font_object_ids.iter().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            font_res_bytes.extend_from_slice(b" ");
            font_res_bytes.extend_from_slice(face.resource().as_bytes());
            font_res_bytes.extend_from_slice(b" ");
            write_u64(&mut font_res_bytes, font_id as u64);
            font_res_bytes.extend_from_slice(b" 0 R");
        }
    }
    master_ids.write_resources(&mut font_res_bytes);
    if let Some(ref m) = math_ids {
        font_res_bytes.extend_from_slice(b" /F_Math ");
        write_u64(&mut font_res_bytes, m.font_id as u64);
        font_res_bytes.extend_from_slice(b" 0 R");
    }

    // Page dicts
    for page_idx in 0..total_pages {
        let page_dict_id = first_page_dict_id + page_idx as u32;
        let content_id = 3 + page_idx as u32;
        let (page_w, page_h) = metas.get(page_idx).map(|m| (m.width, m.height)).unwrap_or((PAGE_W, PAGE_H));
        writer.obj(page_dict_id, |w| {
            w.write_all(b"<< /Type /Page /Parent ");
            w.write_ref(pages_id);
            w.write_all(b" /MediaBox [0 0 ");
            w.write_num(page_w * crate::config::BP_PER_PT);
            w.write_all(b" ");
            w.write_num(page_h * crate::config::BP_PER_PT);
            w.write_all(b"] /Resources << /Font <<");
            w.write_all(&font_res_bytes);
            w.write_all(b" >> >> /Contents ");
            w.write_ref(content_id);
            w.write_all(b" >>\n");
        });
    }

    let mut uncompressed_stream = Vec::with_capacity(16384);
    emit_font_objects(
        &mut writer,
        doc_used_faces,
        font_family,
        &font_object_ids,
        &font_descriptor_ids,
        &font_file_ids,
        tounicode_ids,
        math_ids,
        math_cids,
        &mut uncompressed_stream,
        enable_compression,
    );
    master_ids.emit(&mut writer, tounicode_ids.ot1, tounicode_ids.ot1tt);

    writer.finish(catalog_id)
}

fn emit_into_internal(
    mut pages: PagesSlice<'_, '_>,
    destination_buffer: &mut Vec<u8>,
    enable_compression: bool,
) -> Result<(), Error> {
    destination_buffer.clear();
    destination_buffer.reserve((pages.len() * if enable_compression { 3500 } else { 7000 }).max(16384));
    let total_pages = pages.len().max(1);

    let mut doc_used_faces = 0u32;
    let mut doc_used_masters = 0u32;
    let mut cid_set = CidBitSet::new();
    for page in pages.as_slice().iter().take(total_pages) {
        doc_used_faces |= page.used_faces;
        for op in &page.ops {
            let (faces, masters) = op_font_usage(op, page.font_family);
            doc_used_faces |= faces;
            doc_used_masters |= masters;
            if let Paint::VectorGlyph { glyph_id, .. } | Paint::CidGlyphRun { glyph_id, .. } = op {
                cid_set.insert(*glyph_id);
            }
        }
    }
    let math_cids = cid_set.to_sorted_vec();
    let has_math_font = !math_cids.is_empty();

    if doc_used_faces == 0 && doc_used_masters == 0 {
        doc_used_faces = 1 << (Face::Roman as u8);
    }
    let font_family = pages.as_slice().first().map(|p| p.font_family).unwrap_or_default();
    let num_fonts = (doc_used_faces.count_ones() + doc_used_masters.count_ones()) as usize;

    let mut writer = Writer::new(destination_buffer, total_pages, num_fonts * 3 + 12);
    writer.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let catalog_id = 1u32;
    let pages_id = 2u32;
    let first_page_id = 3u32;
    let first_content_id = first_page_id + total_pages as u32;

    let mut font_object_ids = [0u32; Face::COUNT];
    let mut font_descriptor_ids = [0u32; Face::COUNT];
    let mut font_file_ids = [0u32; Face::COUNT];
    let mut next_font_id = first_content_id + total_pages as u32;
    for (face_idx, obj_id) in font_object_ids.iter_mut().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            let is_cm_embedded = (font_family == FontFamily::ComputerModern
                && matches!(
                    face,
                    Face::Roman
                        | Face::Bold
                        | Face::Italic
                        | Face::MathItalic
                        | Face::Symbol
                        | Face::Roman7
                        | Face::Roman5
                        | Face::MathItalic7
                        | Face::MathItalic5
                        | Face::Symbol7
                        | Face::Symbol5
                        | Face::AmsA
                        | Face::AmsB
                ))
                || matches!(face, Face::Extension | Face::AmsA | Face::AmsB)
                || crate::backend::cm_fonts::extra_cm_program(face, font_family).is_some();
            *obj_id = next_font_id;
            next_font_id += 1;
            if is_cm_embedded {
                font_descriptor_ids[face_idx] = next_font_id;
                font_file_ids[face_idx] = next_font_id + 1;
                next_font_id += 2;
            }
        }
    }

    let master_ids = MasterFontIds::allocate(doc_used_masters, font_family, &mut next_font_id);

    let mut tounicode_ids = TounicodeIds { ot1: 0, ot1tt: 0, oml: 0, oms: 0, omx: 0, msam: 0, msbm: 0 };
    if font_family == FontFamily::ComputerModern {
        let has_ot1 = (doc_used_faces & Face::OT1_MASK & !Face::OT1TT_MASK) != 0 || master_ids.used & !OpticalMaster::TT_MASK != 0;
        if has_ot1 {
            tounicode_ids.ot1 = next_font_id;
            next_font_id += 1;
        }
        let has_tt = (doc_used_faces & Face::OT1TT_MASK) != 0 || master_ids.used & OpticalMaster::TT_MASK != 0;
        if has_tt {
            tounicode_ids.ot1tt = next_font_id;
            next_font_id += 1;
        }

        let has_oml = (doc_used_faces & Face::OML_MASK) != 0;
        if has_oml {
            tounicode_ids.oml = next_font_id;
            next_font_id += 1;
        }

        let has_oms = (doc_used_faces & Face::OMS_MASK) != 0;
        if has_oms {
            tounicode_ids.oms = next_font_id;
            next_font_id += 1;
        }
    }

    let has_msam = (doc_used_faces & (1 << Face::AmsA as u8)) != 0;
    if has_msam {
        tounicode_ids.msam = next_font_id;
        next_font_id += 1;
    }

    let has_msbm = (doc_used_faces & (1 << Face::AmsB as u8)) != 0;
    if has_msbm {
        tounicode_ids.msbm = next_font_id;
        next_font_id += 1;
    }

    let has_omx = (doc_used_faces & Face::OMX_MASK) != 0;
    if has_omx {
        tounicode_ids.omx = next_font_id;
        next_font_id += 1;
    }

    let math_ids = if has_math_font {
        let f_id = next_font_id;
        next_font_id += 1;
        let c_id = next_font_id;
        next_font_id += 1;
        let d_id = next_font_id;
        next_font_id += 1;
        let file_id = next_font_id;
        next_font_id += 1;
        let tu_id = next_font_id;
        Some(MathFontIds {
            font_id: f_id,
            cidfont_id: c_id,
            descriptor_id: d_id,
            file_id: Some(file_id),
            tounicode_id: tu_id,
        })
    } else {
        None
    };

    // 1 Catalog
    writer.obj(catalog_id, |w| {
        w.write_all(b"<< /Type /Catalog /Pages ");
        w.write_ref(pages_id);
        w.write_all(b" >>\n");
    });

    // 2 Pages
    writer.obj(pages_id, |w| {
        w.write_all(b"<< /Type /Pages /Count ");
        w.write_int(total_pages as i32);
        w.write_all(b" /Kids [");
        for page_idx in 0..total_pages as u32 {
            w.write_all(b" ");
            w.write_ref(first_page_id + page_idx);
        }
        w.write_all(b" ] >>\n");
    });

    let mut uncompressed_stream = Vec::with_capacity(16384);
    let mut compressed_stream = Vec::with_capacity(16384);
    let mut compressor = crate::deflate_compress::DeflateCompressor::new();

    let mut font_res_bytes = Vec::with_capacity(128);
    for (face_idx, &font_id) in font_object_ids.iter().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            font_res_bytes.extend_from_slice(b" ");
            font_res_bytes.extend_from_slice(face.resource().as_bytes());
            font_res_bytes.extend_from_slice(b" ");
            write_u64(&mut font_res_bytes, font_id as u64);
            font_res_bytes.extend_from_slice(b" 0 R");
        }
    }
    master_ids.write_resources(&mut font_res_bytes);
    if let Some(ref m) = math_ids {
        font_res_bytes.extend_from_slice(b" /F_Math ");
        write_u64(&mut font_res_bytes, m.font_id as u64);
        font_res_bytes.extend_from_slice(b" 0 R");
    }

    for page_idx in 0..total_pages {
        let page_object_id = first_page_id + page_idx as u32;
        let content_object_id = first_content_id + page_idx as u32;
        let page_width = pages.as_slice().get(page_idx).map(|p| p.width).unwrap_or(PAGE_W);
        let page_height = pages.as_slice().get(page_idx).map(|p| p.height).unwrap_or(PAGE_H);
        writer.obj(page_object_id, |w| {
            w.write_all(b"<< /Type /Page /Parent ");
            w.write_ref(pages_id);
            w.write_all(b" /MediaBox [0 0 ");
            w.write_num(page_width * crate::config::BP_PER_PT);
            w.write_all(b" ");
            w.write_num(page_height * crate::config::BP_PER_PT);
            w.write_all(b"] /Resources << /Font <<");
            w.write_all(&font_res_bytes);
            w.write_all(b" >> >> /Contents ");
            w.write_ref(content_object_id);
            w.write_all(b" >>\n");
        });
    }

    let mut write_content_obj = |page_idx: usize, content_data: &[u8], is_comp: bool| {
        let content_object_id = first_content_id + page_idx as u32;
        if is_comp {
            writer.obj(content_object_id, |w| {
                w.write_all(b"<< /Filter /FlateDecode /Length ");
                w.write_int(content_data.len() as i32);
                w.write_all(b" >>\nstream\n");
                w.write_all(content_data);
                w.write_all(b"\nendstream\n");
            });
        } else {
            writer.obj(content_object_id, |w| {
                w.write_all(b"<< /Length ");
                w.write_int(content_data.len() as i32);
                w.write_all(b" >>\nstream\n");
                w.write_all(content_data);
                if !content_data.ends_with(b"\n") {
                    w.write_all(b"\n");
                }
                w.write_all(b"endstream\n");
            });
        }
    };

    let num_threads = if std::env::var_os("RUSHTEX_SINGLE_THREAD").is_some() {
        1
    } else {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    };

    if enable_compression && total_pages >= 4 && num_threads > 1 {
        let page_slice = pages.as_slice();
        let mut compressed_pages: Vec<Vec<u8>> = (0..total_pages)
            .map(|_| Vec::with_capacity(4096))
            .collect();

        let chunk_size = total_pages.div_ceil(num_threads);
        std::thread::scope(|s| {
            for (thread_idx, chunk) in compressed_pages.chunks_mut(chunk_size).enumerate() {
                let start_page = thread_idx * chunk_size;
                let page_subset = &page_slice[start_page..start_page + chunk.len()];
                s.spawn(move || {
                    let mut local_compressor = crate::deflate_compress::DeflateCompressor::new();
                    let mut local_uncompressed = Vec::with_capacity(16384);
                    for (i, page) in page_subset.iter().enumerate() {
                        local_uncompressed.clear();
                        build_stream_into(page, &mut local_uncompressed, has_math_font);
                        chunk[i].clear();
                        local_compressor.compress_zlib(&local_uncompressed, &mut chunk[i]);
                    }
                });
            }
        });

        for (page_idx, comp_data) in compressed_pages.iter().enumerate() {
            write_content_obj(page_idx, comp_data, true);
        }

        if let PagesSlice::Mut(slice) = &mut pages {
            for page in slice.iter_mut().take(total_pages) {
                page.ops = Vec::new();
                page.text_pool = Vec::new();
            }
        }
    } else {
        match &mut pages {
            PagesSlice::Borrowed(slice) => {
                for (page_idx, page) in slice.iter().enumerate().take(total_pages) {
                    uncompressed_stream.clear();
                    build_stream_into(page, &mut uncompressed_stream, has_math_font);
                    if enable_compression {
                        compressed_stream.clear();
                        compressor.compress_zlib(&uncompressed_stream, &mut compressed_stream);
                        write_content_obj(page_idx, &compressed_stream, true);
                    } else {
                        write_content_obj(page_idx, &uncompressed_stream, false);
                    }
                }
            }
            PagesSlice::Mut(slice) => {
                for (page_idx, page) in slice.iter_mut().enumerate().take(total_pages) {
                    uncompressed_stream.clear();
                    build_stream_into(page, &mut uncompressed_stream, has_math_font);
                    if enable_compression {
                        compressed_stream.clear();
                        compressor.compress_zlib(&uncompressed_stream, &mut compressed_stream);
                        write_content_obj(page_idx, &compressed_stream, true);
                    } else {
                        write_content_obj(page_idx, &uncompressed_stream, false);
                    }
                    page.ops = Vec::new();
                    page.text_pool = Vec::new();
                }
            }
        }
    }

    emit_font_objects(
        &mut writer,
        doc_used_faces,
        font_family,
        &font_object_ids,
        &font_descriptor_ids,
        &font_file_ids,
        tounicode_ids,
        math_ids,
        &math_cids,
        &mut uncompressed_stream,
        enable_compression,
    );
    master_ids.emit(&mut writer, tounicode_ids.ot1, tounicode_ids.ot1tt);

    writer.finish(catalog_id)
}

#[allow(clippy::too_many_arguments)]
fn emit_font_objects(
    writer: &mut Writer,
    doc_used_faces: u32,
    font_family: FontFamily,
    font_object_ids: &[u32; Face::COUNT],
    font_descriptor_ids: &[u32; Face::COUNT],
    font_file_ids: &[u32; Face::COUNT],
    tounicode_ids: TounicodeIds,
    math_ids: Option<MathFontIds>,
    math_cids: &[u16],
    uncompressed_stream: &mut Vec<u8>,
    enable_compression: bool,
) {
    let ot1_tounicode_id = tounicode_ids.ot1;
    let tt_tounicode_id = tounicode_ids.ot1tt;
    let oml_tounicode_id = tounicode_ids.oml;
    let oms_tounicode_id = tounicode_ids.oms;
    let omx_tounicode_id = tounicode_ids.omx;
    let msam_tounicode_id = tounicode_ids.msam;
    let msbm_tounicode_id = tounicode_ids.msbm;

    for (face_idx, &id) in font_object_ids.iter().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            if let Some((program, widths)) = crate::backend::cm_fonts::extra_cm_program(face, font_family) {
                let bit = face.bit();
                let tounicode = if bit & Face::OML_MASK != 0 {
                    oml_tounicode_id
                } else if bit & Face::OMS_MASK != 0 {
                    oms_tounicode_id
                } else if bit & Face::OMX_MASK != 0 {
                    omx_tounicode_id
                } else if bit & Face::OT1TT_MASK != 0 {
                    tt_tounicode_id
                } else {
                    ot1_tounicode_id
                };
                let ids = CmFontObjectIds {
                    font_id: id,
                    descriptor_id: font_descriptor_ids[face_idx],
                    file_id: font_file_ids[face_idx],
                    tounicode_id: (tounicode > 0).then_some(tounicode),
                };
                write_embedded_cm_font(writer, ids, program, widths, false);
                continue;
            }
            if font_family == FontFamily::ComputerModern {
                let ids = CmFontObjectIds {
                    font_id: id,
                    descriptor_id: font_descriptor_ids[face_idx],
                    file_id: font_file_ids[face_idx],
                    tounicode_id: match face {
                        Face::Roman | Face::Bold | Face::Italic | Face::Roman7 | Face::Roman5 => {
                            if ot1_tounicode_id > 0 {
                                Some(ot1_tounicode_id)
                            } else {
                                None
                            }
                        }
                        Face::MathItalic | Face::MathItalic7 | Face::MathItalic5 => {
                            if oml_tounicode_id > 0 {
                                Some(oml_tounicode_id)
                            } else {
                                None
                            }
                        }
                        Face::Symbol | Face::Symbol7 | Face::Symbol5 => {
                            if oms_tounicode_id > 0 {
                                Some(oms_tounicode_id)
                            } else {
                                None
                            }
                        }
                        Face::Extension => {
                            if omx_tounicode_id > 0 {
                                Some(omx_tounicode_id)
                            } else {
                                None
                            }
                        }
                        Face::AmsA => {
                            if msam_tounicode_id > 0 {
                                Some(msam_tounicode_id)
                            } else {
                                None
                            }
                        }
                        Face::AmsB => {
                            if msbm_tounicode_id > 0 {
                                Some(msbm_tounicode_id)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                };
                match face {
                    Face::Roman => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMR10_PROGRAM,
                        &crate::backend::backend_fonts::CMR10_WIDTHS,
                        false,
                    ),
                    Face::Bold => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMBX10_PROGRAM,
                        &crate::backend::backend_fonts::CMBX10_WIDTHS,
                        false,
                    ),
                    Face::Italic => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMTI10_PROGRAM,
                        &crate::backend::backend_fonts::CMTI10_WIDTHS,
                        false,
                    ),
                    Face::MathItalic => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMMI10_PROGRAM,
                        &crate::backend::backend_fonts::CMMI10_WIDTHS,
                        false,
                    ),
                    Face::Symbol => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMSY10_PROGRAM,
                        &crate::backend::backend_fonts::CMSY10_WIDTHS,
                        false,
                    ),
                    Face::Extension => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMEX10_PROGRAM,
                        &crate::backend::backend_fonts::CMEX10_WIDTHS,
                        false,
                    ),
                    Face::Roman7 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMR7_PROGRAM,
                        &crate::backend::backend_fonts::CMR7_WIDTHS,
                        false,
                    ),
                    Face::Roman5 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMR5_PROGRAM,
                        &crate::backend::backend_fonts::CMR5_WIDTHS,
                        false,
                    ),
                    Face::MathItalic7 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMMI7_PROGRAM,
                        &crate::backend::backend_fonts::CMMI7_WIDTHS,
                        false,
                    ),
                    Face::MathItalic5 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMMI5_PROGRAM,
                        &crate::backend::backend_fonts::CMMI5_WIDTHS,
                        false,
                    ),
                    Face::Symbol7 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMSY7_PROGRAM,
                        &crate::backend::backend_fonts::CMSY7_WIDTHS,
                        false,
                    ),
                    Face::Symbol5 => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::CMSY5_PROGRAM,
                        &crate::backend::backend_fonts::CMSY5_WIDTHS,
                        false,
                    ),
                    Face::AmsA => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::MSAM10_PROGRAM,
                        &crate::backend::backend_fonts::MSAM10_WIDTHS,
                        false,
                    ),
                    Face::AmsB => write_embedded_cm_font(
                        writer,
                        ids,
                        &crate::backend::cm_fonts::MSBM10_PROGRAM,
                        &crate::backend::backend_fonts::MSBM10_WIDTHS,
                        false,
                    ),
                    Face::Dingbats => write_font(writer, id, "ZapfDingbats", false),
                    _ => write_font(writer, id, face.pdf_name(font_family), true),
                }
            } else {
                match face {
                    Face::Extension => {
                        let ids = CmFontObjectIds {
                            font_id: id,
                            descriptor_id: font_descriptor_ids[face_idx],
                            file_id: font_file_ids[face_idx],
                            tounicode_id: if omx_tounicode_id > 0 {
                                Some(omx_tounicode_id)
                            } else {
                                None
                            },
                        };
                        write_embedded_cm_font(
                            writer,
                            ids,
                            &crate::backend::cm_fonts::CMEX10_PROGRAM,
                            &crate::backend::backend_fonts::CMEX10_WIDTHS,
                            false,
                        );
                    }
                    Face::AmsA | Face::AmsB => {
                        let (program, widths, tounicode) = if face == Face::AmsA {
                            (
                                &crate::backend::cm_fonts::MSAM10_PROGRAM,
                                &crate::backend::backend_fonts::MSAM10_WIDTHS,
                                msam_tounicode_id,
                            )
                        } else {
                            (
                                &crate::backend::cm_fonts::MSBM10_PROGRAM,
                                &crate::backend::backend_fonts::MSBM10_WIDTHS,
                                msbm_tounicode_id,
                            )
                        };
                        let ids = CmFontObjectIds {
                            font_id: id,
                            descriptor_id: font_descriptor_ids[face_idx],
                            file_id: font_file_ids[face_idx],
                            tounicode_id: (tounicode > 0).then_some(tounicode),
                        };
                        write_embedded_cm_font(writer, ids, program, widths, false);
                    }
                    Face::Symbol => write_font(writer, id, "Symbol", false),
                    Face::Dingbats => write_font(writer, id, "ZapfDingbats", false),
                    _ if shape::needs_ligature_encoding(font_family, face) => {
                        write_text_font_with_ligatures(writer, id, face.pdf_name(font_family))
                    }
                    _ => write_font(writer, id, face.pdf_name(font_family), true),
                }
            }
        }
    }

    if ot1_tounicode_id > 0 {
        emit_tounicode_cmap(writer, ot1_tounicode_id, &crate::backend::cm_fonts::CMR_TO_UNICODE, uncompressed_stream);
    }
    if tt_tounicode_id > 0 {
        emit_tounicode_cmap(writer, tt_tounicode_id, &crate::backend::cm_fonts::CMTT_TO_UNICODE, uncompressed_stream);
    }
    if oml_tounicode_id > 0 {
        emit_tounicode_cmap(writer, oml_tounicode_id, &crate::backend::cm_fonts::CMMI_TO_UNICODE, uncompressed_stream);
    }
    if oms_tounicode_id > 0 {
        emit_tounicode_cmap(writer, oms_tounicode_id, &crate::backend::cm_fonts::CMSY_TO_UNICODE, uncompressed_stream);
    }
    if omx_tounicode_id > 0 {
        emit_tounicode_cmap(writer, omx_tounicode_id, &crate::backend::cm_fonts::CMEX_TO_UNICODE, uncompressed_stream);
    }
    if msam_tounicode_id > 0 {
        emit_tounicode_cmap(writer, msam_tounicode_id, &crate::backend::cm_fonts::MSAM_TO_UNICODE, uncompressed_stream);
    }
    if msbm_tounicode_id > 0 {
        emit_tounicode_cmap(writer, msbm_tounicode_id, &crate::backend::cm_fonts::MSBM_TO_UNICODE, uncompressed_stream);
    }

    if let Some(m) = math_ids {
        let math_font_id = m.font_id;
        let math_cidfont_id = m.cidfont_id;
        let math_descriptor_id = m.descriptor_id;
        let math_file_id = m.file_id;
        let math_tounicode_id = m.tounicode_id;
        let math_ps_name = font_family.default_math_ps_name();

        // 1. Type 0 composite font
        writer.obj(math_font_id, |w| {
            w.write_all(b"<< /Type /Font /Subtype /Type0 /BaseFont /");
            w.write_all(math_ps_name.as_bytes());
            w.write_all(b" /Encoding /Identity-H /DescendantFonts [ ");
            w.write_ref(math_cidfont_id);
            w.write_all(b" ] /ToUnicode ");
            w.write_ref(math_tounicode_id);
            w.write_all(b" >>\n");
        });

        // 2. Descendant CIDFontType0
        writer.obj(math_cidfont_id, |w| {
            w.write_all(b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /");
            w.write_all(math_ps_name.as_bytes());
            w.write_all(b" /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor ");
            w.write_ref(math_descriptor_id);
            w.write_all(b" /DW 1000 /W [");
            for &cid in math_cids {
                let adv = crate::math::fonts::static_script_advance(cid, font_family).unwrap_or(1000);
                w.write_all(b" ");
                w.write_int(cid as i32);
                w.write_all(b" [");
                w.write_int(adv as i32);
                w.write_all(b"]");
            }
            w.write_all(b" ] >>\n");
        });

        // 3. FontDescriptor
        let bbox = match font_family {
            FontFamily::Times => [-1042, -3060, 4082, 3560],
            _ => [-983, -3150, 2997, 3350],
        };
        writer.obj(math_descriptor_id, |w| {
            w.write_all(b"<< /Type /FontDescriptor /FontName /");
            w.write_all(math_ps_name.as_bytes());
            w.write_all(b" /Flags 4 /FontBBox [");
            w.write_int(bbox[0]);
            w.write_all(b" ");
            w.write_int(bbox[1]);
            w.write_all(b" ");
            w.write_int(bbox[2]);
            w.write_all(b" ");
            w.write_int(bbox[3]);
            w.write_all(b"] /Ascent 800 /Descent -200 /CapHeight 700 /ItalicAngle 0 /StemV 80");
            if let Some(fid) = math_file_id {
                w.write_all(b" /FontFile3 ");
                w.write_ref(fid);
            }
            w.write_all(b" >>\n");
        });

        // 4. FontFile3 (CFF stream)
        if let Some(fid) = math_file_id {
            if enable_compression {
                let comp_bytes = crate::math::fonts::static_cff_compressed(font_family);
                writer.obj(fid, |w| {
                    w.write_all(b"<< /Subtype /CIDFontType0C /Filter /FlateDecode /Length ");
                    w.write_int(comp_bytes.len() as i32);
                    w.write_all(b" >>\nstream\n");
                    w.write_all(comp_bytes);
                    w.write_all(b"\nendstream\n");
                });
            } else {
                let raw_bytes = crate::math::fonts::static_cff_raw(font_family);
                writer.obj(fid, |w| {
                    w.write_all(b"<< /Subtype /CIDFontType0C /Length ");
                    w.write_int(raw_bytes.len() as i32);
                    w.write_all(b" >>\nstream\n");
                    w.write_all(raw_bytes);
                    if !raw_bytes.ends_with(b"\n") {
                        w.write_all(b"\n");
                    }
                    w.write_all(b"endstream\n");
                });
            }
        }

        // 5. ToUnicode CMap
        emit_cid_tounicode_cmap(writer, math_tounicode_id, math_cids, uncompressed_stream);
    }
}

/// The font an op's text is set in, as `(face bit, master bit)`: a text op set in a
/// design-size master uses only that master, so the face's 10pt font is not embedded for it.
#[inline]
fn op_font_usage(op: &Paint, family: FontFamily) -> (u32, u32) {
    if let Some(m) = text_op_master(op, family) {
        return (0, 1 << m as u8);
    }
    let face = match op {
        Paint::GlyphRun { face, .. }
        | Paint::Marker { face, .. }
        | Paint::PageNumber { face, .. }
        | Paint::SectionNumber { face, .. }
        | Paint::FigureTag { face, .. }
        | Paint::TableTag { face, .. }
        | Paint::EquationNumber { face, .. } => *face,
        Paint::CaptionTag { .. } | Paint::TableCaptionTag { .. } => Face::Roman,
        _ => return (0, 0),
    };
    (face.bit(), 0)
}

/// Design-size master of a text paint op. Math glyph runs (not shaped) keep the font their
/// TFM metrics came from.
#[inline]
fn text_op_master(op: &Paint, family: FontFamily) -> Option<OpticalMaster> {
    let (face, size) = match op {
        Paint::GlyphRun { face, size, part_count, .. } if part_count & GLYPH_RUN_SHAPED != 0 => (*face, *size),
        Paint::Marker { face, size, .. }
        | Paint::PageNumber { face, size, .. }
        | Paint::SectionNumber { face, size, .. }
        | Paint::FigureTag { face, size, .. }
        | Paint::TableTag { face, size, .. }
        | Paint::EquationNumber { face, size, .. } => (*face, *size),
        Paint::CaptionTag { size, .. } | Paint::TableCaptionTag { size, .. } => (Face::Roman, *size),
        _ => return None,
    };
    OpticalMaster::select(family, face, size)
}

/// Object ids of the embedded design-size masters a document uses.
struct MasterFontIds {
    used: u32,
    first_id: u32,
}

impl MasterFontIds {
    /// Reserves three objects (font, descriptor, program) per used master.
    fn allocate(used: u32, family: FontFamily, next_id: &mut u32) -> Self {
        let used = if family == FontFamily::ComputerModern { used } else { 0 };
        let first_id = *next_id;
        *next_id += 3 * used.count_ones();
        Self { used, first_id }
    }

    fn ids(&self) -> impl Iterator<Item = (OpticalMaster, u32)> + '_ {
        (0..OpticalMaster::COUNT as u8)
            .filter(|i| self.used & (1 << i) != 0)
            .enumerate()
            .map(|(k, i)| (OpticalMaster::from_u8(i), self.first_id + 3 * k as u32))
    }

    fn write_resources(&self, out: &mut Vec<u8>) {
        for (m, id) in self.ids() {
            out.push(b' ');
            out.extend_from_slice(m.resource().as_bytes());
            out.push(b' ');
            write_u64(out, id as u64);
            out.extend_from_slice(b" 0 R");
        }
    }

    fn emit(&self, writer: &mut Writer, ot1_tounicode_id: u32, tt_tounicode_id: u32) {
        for (m, id) in self.ids() {
            let tu = if m.is_typewriter() { tt_tounicode_id } else { ot1_tounicode_id };
            let ids = CmFontObjectIds {
                font_id: id,
                descriptor_id: id + 1,
                file_id: id + 2,
                tounicode_id: (tu > 0).then_some(tu),
            };
            write_embedded_cm_font(writer, ids, m.program(), m.widths(), false);
        }
    }
}

#[derive(Clone, Copy)]
struct CmFontObjectIds {
    font_id: u32,
    descriptor_id: u32,
    file_id: u32,
    tounicode_id: Option<u32>,
}

fn build_widths_block(widths: &[u16; 128]) -> Vec<u8> {
    let mut b = Vec::with_capacity(512);
    b.extend_from_slice(b" /FirstChar 0 /LastChar 127 /Widths [");
    for &width in widths.iter() {
        b.push(b' ');
        write_i64(&mut b, width as i64);
    }
    b.extend_from_slice(b" ]");
    b
}

fn get_cached_cm_widths_block(widths: &[u16; 128]) -> std::borrow::Cow<'static, [u8]> {
    use std::borrow::Cow;
    use std::sync::OnceLock;
    use crate::backend::backend_fonts::*;
    static CMR10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMBX10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMTI10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMEX10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMR7_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMR5_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI7_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI5_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY7_W: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY5_W: OnceLock<Vec<u8>> = OnceLock::new();
    static MSAM10_W: OnceLock<Vec<u8>> = OnceLock::new();
    static MSBM10_W: OnceLock<Vec<u8>> = OnceLock::new();

    let block: &'static Vec<u8> = if std::ptr::eq(widths, &CMR10_WIDTHS.0) {
        CMR10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMBX10_WIDTHS.0) {
        CMBX10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMTI10_WIDTHS.0) {
        CMTI10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMMI10_WIDTHS.0) {
        CMMI10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMSY10_WIDTHS.0) {
        CMSY10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMEX10_WIDTHS.0) {
        CMEX10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMR7_WIDTHS.0) {
        CMR7_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMR5_WIDTHS.0) {
        CMR5_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMMI7_WIDTHS.0) {
        CMMI7_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMMI5_WIDTHS.0) {
        CMMI5_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMSY7_WIDTHS.0) {
        CMSY7_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &CMSY5_WIDTHS.0) {
        CMSY5_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &MSAM10_WIDTHS.0) {
        MSAM10_W.get_or_init(|| build_widths_block(widths))
    } else if std::ptr::eq(widths, &MSBM10_WIDTHS.0) {
        MSBM10_W.get_or_init(|| build_widths_block(widths))
    } else {
        // Cold path (once per face per document): the optical-size / math-alphabet faces.
        return Cow::Owned(build_widths_block(widths));
    };
    Cow::Borrowed(block.as_slice())
}

fn build_descriptor_head(program: &crate::backend::cm_fonts::CmFontProgram) -> Vec<u8> {
    let mut b = Vec::with_capacity(256);
    b.extend_from_slice(b"<< /Type /FontDescriptor /FontName /");
    b.extend_from_slice(program.name.as_bytes());
    b.extend_from_slice(b" /Flags ");
    write_i64(&mut b, program.flags as i64);
    b.extend_from_slice(b" /FontBBox [");
    write_i64(&mut b, program.bbox[0] as i64);
    b.push(b' ');
    write_i64(&mut b, program.bbox[1] as i64);
    b.push(b' ');
    write_i64(&mut b, program.bbox[2] as i64);
    b.push(b' ');
    write_i64(&mut b, program.bbox[3] as i64);
    b.extend_from_slice(b"] /Ascent ");
    write_i64(&mut b, program.ascent as i64);
    b.extend_from_slice(b" /Descent ");
    write_i64(&mut b, program.descent as i64);
    b.extend_from_slice(b" /CapHeight ");
    write_i64(&mut b, program.cap_height as i64);
    b.extend_from_slice(b" /ItalicAngle ");
    write_i64(&mut b, program.italic_angle as i64);
    b.extend_from_slice(b" /StemV ");
    write_i64(&mut b, program.stem_v as i64);
    b.extend_from_slice(b" /FontFile3 ");
    b
}

fn get_cached_cm_descriptor_head(
    program: &crate::backend::cm_fonts::CmFontProgram,
) -> std::borrow::Cow<'static, [u8]> {
    use std::borrow::Cow;
    use std::sync::OnceLock;
    static CMR10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMBX10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMTI10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMEX10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMR7_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMR5_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI7_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMMI5_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY7_D: OnceLock<Vec<u8>> = OnceLock::new();
    static CMSY5_D: OnceLock<Vec<u8>> = OnceLock::new();
    static MSAM10_D: OnceLock<Vec<u8>> = OnceLock::new();
    static MSBM10_D: OnceLock<Vec<u8>> = OnceLock::new();

    let head: &'static Vec<u8> = match program.name {
        "CMR10" => CMR10_D.get_or_init(|| build_descriptor_head(program)),
        "CMBX10" => CMBX10_D.get_or_init(|| build_descriptor_head(program)),
        "CMTI10" => CMTI10_D.get_or_init(|| build_descriptor_head(program)),
        "CMMI10" => CMMI10_D.get_or_init(|| build_descriptor_head(program)),
        "CMSY10" => CMSY10_D.get_or_init(|| build_descriptor_head(program)),
        "CMEX10" => CMEX10_D.get_or_init(|| build_descriptor_head(program)),
        "CMR7" => CMR7_D.get_or_init(|| build_descriptor_head(program)),
        "CMR5" => CMR5_D.get_or_init(|| build_descriptor_head(program)),
        "CMMI7" => CMMI7_D.get_or_init(|| build_descriptor_head(program)),
        "CMMI5" => CMMI5_D.get_or_init(|| build_descriptor_head(program)),
        "CMSY7" => CMSY7_D.get_or_init(|| build_descriptor_head(program)),
        "CMSY5" => CMSY5_D.get_or_init(|| build_descriptor_head(program)),
        "MSAM10" => MSAM10_D.get_or_init(|| build_descriptor_head(program)),
        "MSBM10" => MSBM10_D.get_or_init(|| build_descriptor_head(program)),
        // Cold path (once per face per document): the optical-size / math-alphabet faces.
        _ => return Cow::Owned(build_descriptor_head(program)),
    };
    Cow::Borrowed(head.as_slice())
}

fn write_embedded_cm_font(
    writer: &mut Writer,
    ids: CmFontObjectIds,
    program: &crate::backend::cm_fonts::CmFontProgram,
    widths: &[u16; 128],
    with_encoding: bool,
) {
    let widths_block = get_cached_cm_widths_block(widths);
    writer.obj(ids.font_id, |w| {
        w.write_all(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        w.write_all(program.name.as_bytes());
        w.write_all(b" /FontDescriptor ");
        w.write_ref(ids.descriptor_id);
        w.write_all(&widths_block);
        if with_encoding {
            w.write_all(b" /Encoding /WinAnsiEncoding");
        } else if !program.diff_encoding.is_empty() {
            w.write_all(b" /Encoding << /Type /Encoding /Differences [ ");
            w.write_all(program.diff_encoding.as_bytes());
            w.write_all(b" ] >>");
        }
        if let Some(tuid) = ids.tounicode_id {
            w.write_all(b" /ToUnicode ");
            w.write_ref(tuid);
        }
        w.write_all(b" >>\n");
    });

    let desc_head = get_cached_cm_descriptor_head(program);
    writer.obj(ids.descriptor_id, |w| {
        w.write_all(&desc_head);
        w.write_ref(ids.file_id);
        w.write_all(b" >>\n");
    });

    writer.obj(ids.file_id, |w| {
        w.write_all(b"<< /Subtype /Type1C /Filter /FlateDecode /Length ");
        w.write_int(program.compressed_stream.len() as i32);
        w.write_all(b" >>\nstream\n");
        w.write_all(program.compressed_stream);
        if !program.compressed_stream.ends_with(b"\n") {
            w.write_all(b"\n");
        }
        w.write_all(b"endstream\n");
    });
}

fn write_font(w: &mut Writer, id: u32, name: &str, with_encoding: bool) {
    w.obj(id, |w| {
        w.write_all(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        w.write_all(name.as_bytes());
        if with_encoding {
            w.write_all(b" /Encoding /WinAnsiEncoding >>\n");
        } else {
            w.write_all(b" >>\n");
        }
    });
}

/// Base-14 text font whose WinAnsi encoding gains the `fi`/`fl` ligature glyphs in two
/// undefined slots, so shaped text can address them.
fn write_text_font_with_ligatures(w: &mut Writer, id: u32, name: &str) {
    w.obj(id, |w| {
        w.write_all(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        w.write_all(name.as_bytes());
        w.write_all(b" /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding /Differences [ ");
        w.write_int(crate::backend::backend_afm_ligkern::WINANSI_FI as i32);
        w.write_all(b" /fi ");
        w.write_int(crate::backend::backend_afm_ligkern::WINANSI_FL as i32);
        w.write_all(b" /fl ] >> >>\n");
    });
}

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

#[inline(always)]
fn write_hex2(out: &mut [u8], byte: u8) {
    out[0] = b'<';
    out[1] = HEX_DIGITS[(byte >> 4) as usize];
    out[2] = HEX_DIGITS[(byte & 0x0F) as usize];
    out[3] = b'>';
}

#[inline(always)]
fn write_hex4(out: &mut [u8], val: u16) {
    out[0] = b'<';
    out[1] = HEX_DIGITS[((val >> 12) & 0x0F) as usize];
    out[2] = HEX_DIGITS[((val >> 8) & 0x0F) as usize];
    out[3] = HEX_DIGITS[((val >> 4) & 0x0F) as usize];
    out[4] = HEX_DIGITS[(val & 0x0F) as usize];
    out[5] = b'>';
}

fn build_tounicode_cmap_bytes(map: &[u16; 128]) -> Vec<u8> {
    let mut s = Vec::with_capacity(1500);
    s.extend_from_slice(b"/CIDInit /ProcSet findresource begin\n");
    s.extend_from_slice(b"12 dict begin\nbegincmap\n");
    s.extend_from_slice(b"/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.extend_from_slice(b"/CMapName /RushTeX-ToUnicode def\n");
    s.extend_from_slice(b"/CMapType 2 def\n");
    s.extend_from_slice(b"1 begincodespacerange\n<00> <7F>\nendcodespacerange\n");

    let mut entries: [(u8, u16); 128] = [(0, 0); 128];
    let mut count = 0;
    for (src, &dst) in map.iter().enumerate() {
        if dst != 0 {
            entries[count] = (src as u8, dst);
            count += 1;
        }
    }

    let mut i = 0;
    while i < count {
        let chunk = (count - i).min(100);
        write_u64(&mut s, chunk as u64);
        s.extend_from_slice(b" beginbfchar\n");
        for j in 0..chunk {
            let (src, dst) = entries[i + j];
            let mut hex_buf = [0u8; 6];
            write_hex2(&mut hex_buf[0..4], src);
            s.extend_from_slice(&hex_buf[..4]);
            s.extend_from_slice(b" <");
            // Ligatures extract as their letters (U+FB00-FB04 decomposed), as pdfTeX's
            // glyph names do.
            let letters: &[u8] = match dst {
                0xFB00 => b"ff",
                0xFB01 => b"fi",
                0xFB02 => b"fl",
                0xFB03 => b"ffi",
                0xFB04 => b"ffl",
                _ => &[],
            };
            if letters.is_empty() {
                write_hex4(&mut hex_buf, dst);
                s.extend_from_slice(&hex_buf[1..5]);
            } else {
                for &c in letters {
                    write_hex4(&mut hex_buf, c as u16);
                    s.extend_from_slice(&hex_buf[1..5]);
                }
            }
            s.extend_from_slice(b">\n");
        }
        s.extend_from_slice(b"endbfchar\n");
        i += chunk;
    }

    s.extend_from_slice(b"endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

fn get_cached_tounicode_cmap(map: &[u16; 128]) -> &'static [u8] {
    use std::sync::OnceLock;
    static OT1_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static OT1TT_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static OML_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static OMS_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static OMX_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static MSAM_CACHE: OnceLock<Vec<u8>> = OnceLock::new();
    static MSBM_CACHE: OnceLock<Vec<u8>> = OnceLock::new();

    if std::ptr::eq(map, &crate::backend::cm_fonts::CMR_TO_UNICODE) {
        OT1_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::CMTT_TO_UNICODE) {
        OT1TT_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::CMMI_TO_UNICODE) {
        OML_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::CMSY_TO_UNICODE) {
        OMS_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::CMEX_TO_UNICODE) {
        OMX_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::MSAM_TO_UNICODE) {
        MSAM_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else if std::ptr::eq(map, &crate::backend::cm_fonts::MSBM_TO_UNICODE) {
        MSBM_CACHE.get_or_init(|| build_tounicode_cmap_bytes(map))
    } else {
        static FALLBACK: OnceLock<Vec<u8>> = OnceLock::new();
        FALLBACK.get_or_init(|| build_tounicode_cmap_bytes(map))
    }
}

fn emit_tounicode_cmap(writer: &mut Writer, id: u32, map: &[u16; 128], _s: &mut Vec<u8>) {
    let stream_bytes = get_cached_tounicode_cmap(map);
    writer.obj(id, |w| {
        w.write_all(b"<< /Length ");
        w.write_int(stream_bytes.len() as i32);
        w.write_all(b" >>\nstream\n");
        w.write_all(stream_bytes);
        if !stream_bytes.ends_with(b"\n") {
            w.write_all(b"\n");
        }
        w.write_all(b"endstream\n");
    });
}

fn emit_cid_tounicode_cmap(writer: &mut Writer, id: u32, cids: &[u16], s: &mut Vec<u8>) {
    s.clear();
    s.extend_from_slice(b"/CIDInit /ProcSet findresource begin\n");
    s.extend_from_slice(b"12 dict begin\nbegincmap\n");
    s.extend_from_slice(b"/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    s.extend_from_slice(b"/CMapName /RushTeX-CIDToUnicode def\n");
    s.extend_from_slice(b"/CMapType 2 def\n");
    s.extend_from_slice(b"1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");

    let mut i = 0;
    while i < cids.len() {
        let chunk = (cids.len() - i).min(100);
        write_u64(s, chunk as u64);
        s.extend_from_slice(b" beginbfchar\n");
        for j in 0..chunk {
            let gid = cids[i + j];
            let u = fallback_unicode_for_glyph(gid).unwrap_or(gid as u32);
            if u <= 0xFFFF {
                let mut line = [0u8; 14];
                write_hex4(&mut line[0..6], gid);
                line[6] = b' ';
                write_hex4(&mut line[7..13], u as u16);
                line[13] = b'\n';
                s.extend_from_slice(&line);
            } else {
                let cp = u - 0x10000;
                let lead = (0xD800 + (cp >> 10)) as u16;
                let trail = (0xDC00 + (cp & 0x3FF)) as u16;
                let mut line = [0u8; 20];
                write_hex4(&mut line[0..6], gid);
                line[6] = b' ';
                line[7] = b'<';
                line[8] = HEX_DIGITS[((lead >> 12) & 0x0F) as usize];
                line[9] = HEX_DIGITS[((lead >> 8) & 0x0F) as usize];
                line[10] = HEX_DIGITS[((lead >> 4) & 0x0F) as usize];
                line[11] = HEX_DIGITS[(lead & 0x0F) as usize];
                line[12] = HEX_DIGITS[((trail >> 12) & 0x0F) as usize];
                line[13] = HEX_DIGITS[((trail >> 8) & 0x0F) as usize];
                line[14] = HEX_DIGITS[((trail >> 4) & 0x0F) as usize];
                line[15] = HEX_DIGITS[(trail & 0x0F) as usize];
                line[16] = b'>';
                line[17] = b'\n';
                s.extend_from_slice(&line[..18]);
            }
        }
        s.extend_from_slice(b"endbfchar\n");
        i += chunk;
    }

    s.extend_from_slice(b"endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");

    writer.obj(id, |w| {
        w.write_all(b"<< /Length ");
        w.write_int(s.len() as i32);
        w.write_all(b" >>\nstream\n");
        w.write_all(s);
        if !s.ends_with(b"\n") {
            w.write_all(b"\n");
        }
        w.write_all(b"endstream\n");
    });
}

#[inline(always)]
fn emit_hex_gid(s: &mut Vec<u8>, gid: u16) {
    let mut buf = [0u8; 6];
    write_hex4(&mut buf, gid);
    s.extend_from_slice(&buf);
}

fn fallback_unicode_for_glyph(gid: u16) -> Option<u32> {
    if (1120..=1145).contains(&gid) {
        return Some(crate::math::fonts::calligraphy::NEWCM_SCRIPT_GLYPHS[(gid - 1120) as usize].unicode);
    }
    match gid {
        // STIX2 script glyphs:
        1245 => Some(0x0212C), // B
        1248 => Some(0x02130), // E
        1249 => Some(0x02131), // F
        1221 => Some(0x0210B), // H
        1226 => Some(0x02110), // I
        1228 => Some(0x02112), // L
        1250 => Some(0x02133), // M
        1234 => Some(0x0211B), // R
        3412 => Some(0x1D49C), // A
        3413 => Some(0x1D49E), // C
        3414 => Some(0x1D49F), // D
        3415 => Some(0x1D4A2), // G
        3416 => Some(0x1D4A5), // J
        3417 => Some(0x1D4A6), // K
        3418 => Some(0x1D4A9), // N
        3419 => Some(0x1D4AA), // O
        3420 => Some(0x1D4AB), // P
        3421 => Some(0x1D4AC), // Q
        3422 => Some(0x1D4AE), // S
        3423 => Some(0x1D4AF), // T
        3424 => Some(0x1D4B0), // U
        3425 => Some(0x1D4B1), // V
        3426 => Some(0x1D4B2), // W
        3427 => Some(0x1D4B3), // X
        3428 => Some(0x1D4B4), // Y
        3429 => Some(0x1D4B5), // Z
        // Math symbols:
        832..=835 => Some(0x222B),
        1075..=1080 => Some(0x221A),
        1085..=1088 => Some(0x0028),
        1089..=1092 => Some(0x0029),
        1093..=1096 => Some(0x005B),
        1097..=1100 => Some(0x005D),
        1101..=1104 => Some(0x007B),
        1105..=1108 => Some(0x007D),
        1300..=1305 => Some(0x2211),
        1306..=1310 => Some(0x220F),
        // Blackboard bold:
        636 | 1236 => Some(0x211D),
        614 | 1217 => Some(0x2102),
        628 | 1230 => Some(0x2115),
        640 | 1238 => Some(0x2124),
        633 | 1233 => Some(0x211A),
        632 | 1232 => Some(0x2119),
        // Bold Latin:
        2121 | 3295 => Some(0x1D42F),
        2224 | 3407 => Some(0x1D483),
        2074 | 3246 => Some(0x1D400),
        2101 | 3273 => Some(0x1D41B),
        2123 | 3297 => Some(0x1D431),
        2124 | 3298 => Some(0x1D432),
        2125 | 3299 => Some(0x1D433),
        // Bold Greek:
        2754 | 3957 => Some(0x1D6C2),
        2755 | 3958 => Some(0x1D6C3),
        2756 | 3959 => Some(0x1D6C4),
        2762 | 3965 => Some(0x1D6CA),
        2770 | 3973 => Some(0x1D6D2),
        2772 | 3975 => Some(0x1D6D4),
        2776 | 3979 => Some(0x1D6D8),
        // Fraktur:
        2349 | 3536 => Some(0x1D524),
        620 | 1222 => Some(0x210C),
        2322 | 3509 => Some(0x1D504),
        _ => None,
    }
}

/// Writes `word` followed by an interword space inside an open PDF string. With
/// `space_units` set (inside a `TJ` array), the space is a horizontal displacement
/// of that many 1/1000 em; otherwise it is a literal space character.
#[inline]
fn write_tag_word_space(s: &mut Vec<u8>, word: &[u8], space_units: Option<u32>) {
    s.extend_from_slice(word);
    match space_units {
        Some(u) => {
            s.extend_from_slice(b") -");
            write_u64(s, u as u64);
            s.extend_from_slice(b" (");
        }
        None => s.push(b' '),
    }
}

/// Emits a body-text run with each word shaped by its font's ligature/kern program.
///
/// Ligatures become their glyph codes; implicit kerns become `TJ` displacements (in 1/1000
/// text-space units, negated: a negative kern moves the next glyph left). Runs without kerns
/// keep the plain `Tj` form. The resulting advance equals `TextFont::width_units` per word,
/// which is what layout measured.
fn emit_shaped_glyph_run(s: &mut Vec<u8>, parts: &[&str], font: &TextFont, face: Face) {
    let open = s.len();
    s.extend_from_slice(b"[(");
    let mut kerned = false;
    for &p in parts {
        if p == " " {
            s.push(b' ');
            continue;
        }
        font.shape(p, |g, kern| {
            match g {
                Glyph::Code(code) => push_string_byte(s, code),
                Glyph::Raw(c) => {
                    let mut buf = [0u8; 4];
                    pdf_escape(s, c.encode_utf8(&mut buf), face);
                }
            }
            if kern != 0 {
                s.push(b')');
                write_tj_kern(s, kern);
                s.push(b'(');
                kerned = true;
            }
        });
    }
    if kerned {
        s.extend_from_slice(b")] TJ\n");
    } else {
        // No kerns: drop the array bracket (one byte, so the stream needs no shifting).
        s[open] = b' ';
        s.extend_from_slice(b") Tj\n");
    }
}

/// Writes a TJ displacement for a kern of `kern` 1/10000 em: `-kern / 10` thousandths of
/// text space, with at most one decimal (integer formatting, no float round trip).
#[inline]
fn write_tj_kern(out: &mut Vec<u8>, kern: i32) {
    let v = -kern;
    if v < 0 {
        out.push(b'-');
    }
    let a = v.unsigned_abs();
    write_u64(out, (a / 10) as u64);
    let frac = (a % 10) as u8;
    if frac != 0 {
        out.push(b'.');
        out.push(b'0' + frac);
    }
}

/// Writes one glyph code into a PDF literal string, escaping delimiters and non-printables.
#[inline(always)]
fn push_string_byte(out: &mut Vec<u8>, b: u8) {
    match b {
        b'\\' | b'(' | b')' => {
            out.push(b'\\');
            out.push(b);
        }
        32..=126 => out.push(b),
        _ => {
            out.push(b'\\');
            out.push(b'0' + ((b >> 6) & 7));
            out.push(b'0' + ((b >> 3) & 7));
            out.push(b'0' + (b & 7));
        }
    }
}

#[inline]
fn emit_glyph_run_parts(s: &mut Vec<u8>, parts: &[&str], face: Face) {
    for &p in parts {
        if p == " " {
            s.push(b' ');
        } else {
            pdf_escape(s, p, face);
        }
    }
}

fn build_stream_into(page: &Page<'_>, s: &mut Vec<u8>, has_math_font: bool) {
    // Layout coordinates are TeX points; one transform maps them to PDF big points.
    s.extend_from_slice(b"0.996264 0 0 0.996264 0 0 cm\nBT\n");
    let mut last_face: Option<Face> = None;
    let mut last_master: Option<OpticalMaster> = None;
    let mut last_size: Option<f32> = None;
    let mut last_word_spacing: Option<f32> = None;
    let mut last_x = 0.0f32;
    let mut last_y = 0.0f32;
    let mut pos_inited = false;

    let mut in_text = true;
    for op in &page.ops {
        if let Paint::Color { color } = op {
            // Colour operators are legal inside and outside text objects.
            write_color(s, *color);
            continue;
        }
        if let Paint::Rule { x, y, w, h, fill } = op {
            if in_text {
                if last_word_spacing.is_some_and(|w| w != 0.0) {
                    s.extend_from_slice(b"0 Tw\n");
                    last_word_spacing = Some(0.0);
                }
                s.extend_from_slice(b"ET\n");
                in_text = false;
            }
            write_rule(s, *x, *y, *w, *h, *fill);
            pos_inited = false;
            continue;
        }

        if let Paint::VectorGlyph {
            x,
            y,
            size,
            glyph_id,
        } = op
        {
            if in_text {
                if last_word_spacing.is_some_and(|w| w != 0.0) {
                    s.extend_from_slice(b"0 Tw\n");
                    last_word_spacing = Some(0.0);
                }
                s.extend_from_slice(b"ET\n");
                in_text = false;
            }
            if let Some(path) = crate::math::fonts::get_vector_glyph_path(*glyph_id) {
                s.extend_from_slice(b"q\n");
                write_f32_4dec(s, *size * 0.001);
                s.extend_from_slice(b" 0 0 ");
                write_f32_4dec(s, *size * 0.001);
                s.push(b' ');
                write_f32(s, *x);
                s.push(b' ');
                write_f32(s, *y);
                s.extend_from_slice(b" cm\n");
                s.extend_from_slice(path);
                s.extend_from_slice(b"\nf\nQ\n");
            }
            if has_math_font {
                s.extend_from_slice(b"BT\n/F_Math ");
                write_f32(s, *size);
                s.extend_from_slice(b" Tf\n3 Tr\n1 0 0 1 ");
                write_f32(s, *x);
                s.push(b' ');
                write_f32(s, *y);
                s.extend_from_slice(b" Tm\n");
                emit_hex_gid(s, *glyph_id);
                s.extend_from_slice(b" Tj\n0 Tr\nET\n");
            }
            pos_inited = false;
            continue;
        }

        if let Paint::CidGlyphRun {
            x,
            y,
            size,
            glyph_id,
        } = op
        {
            if in_text {
                if last_word_spacing.is_some_and(|w| w != 0.0) {
                    s.extend_from_slice(b"0 Tw\n");
                    last_word_spacing = Some(0.0);
                }
                s.extend_from_slice(b"ET\n");
                in_text = false;
            }
            s.extend_from_slice(b"BT\n/F_Math ");
            write_f32(s, *size);
            s.extend_from_slice(b" Tf\n1 0 0 1 ");
            write_f32(s, *x);
            s.push(b' ');
            write_f32(s, *y);
            s.extend_from_slice(b" Tm\n");
            emit_hex_gid(s, *glyph_id);
            s.extend_from_slice(b" Tj\nET\n");
            pos_inited = false;
            continue;
        }

        let (x, y, face, size, word_spacing) = match op {
            Paint::GlyphRun {
                x,
                y,
                face,
                size,
                word_spacing,
                ..
            } => (*x, *y, *face, *size, *word_spacing),
            Paint::Marker {
                x, y, face, size, ..
            }
            | Paint::PageNumber {
                x, y, face, size, ..
            }
            | Paint::SectionNumber {
                x, y, face, size, ..
            }
            | Paint::FigureTag {
                x, y, face, size, ..
            }
            | Paint::TableTag {
                x, y, face, size, ..
            }
            | Paint::EquationNumber {
                x, y, face, size, ..
            } => (*x, *y, *face, *size, 0.0),
            Paint::CaptionTag { x, y, size, .. } | Paint::TableCaptionTag { x, y, size, .. } => {
                (*x, *y, Face::Roman, *size, 0.0)
            }
            Paint::Rule { .. } | Paint::VectorGlyph { .. } | Paint::CidGlyphRun { .. } | Paint::Color { .. } => unreachable!(),
        };

        if !in_text {
            s.extend_from_slice(b"BT\n");
            in_text = true;
            last_face = None;
            last_size = None;
            last_word_spacing = None;
            pos_inited = false;
        }

        if last_word_spacing != Some(word_spacing) {
            write_tw(s, word_spacing);
            last_word_spacing = Some(word_spacing);
        }

        let master = text_op_master(op, page.font_family);
        if last_face != Some(face) || last_size != Some(size) || last_master != master {
            match master {
                Some(m) => write_tf_resource(s, m.resource(), size),
                None => write_tf(s, face, size),
            }
            last_face = Some(face);
            last_size = Some(size);
            last_master = master;
        }

        if !pos_inited {
            write_tm(s, x, y);
            pos_inited = true;
        } else {
            // Td is relative and written to 0.01pt, so the offset is taken between the
            // rounded positions: rounding errors then cannot accumulate along a page.
            let centi = |v: f32| (v as f64 * 100.0).round();
            write_td(s, ((centi(x) - centi(last_x)) / 100.0) as f32, ((centi(y) - centi(last_y)) / 100.0) as f32);
        }

        if let Paint::GlyphRun {
            part_start,
            part_count,
            face,
            ..
        } = op
        {
            let start = *part_start as usize;
            let count = (*part_count & GLYPH_RUN_COUNT_MASK) as usize;
            let parts = &page.text_pool[start..start + count];
            let font = if *part_count & GLYPH_RUN_SHAPED != 0 {
                shape::text_font_at(page.font_family, *face, size)
            } else {
                None
            };
            match font {
                Some(font) => emit_shaped_glyph_run(s, parts, &font, *face),
                None => {
                    s.push(b'(');
                    emit_glyph_run_parts(s, parts, *face);
                    s.extend_from_slice(b") Tj\n");
                }
            }
            last_x = x;
            last_y = y;
            continue;
        }

        // Label tags ("Figure 1:", "Table 1") contain an interword space. In the
        // OT1-encoded Computer Modern fonts code 32 is the `/suppress` glyph, not a
        // space, so the space is emitted as a TJ displacement of the font's
        // interword width (fontdimen 2) instead of as a character.
        let is_label_tag = matches!(
            op,
            Paint::CaptionTag { .. }
                | Paint::TableCaptionTag { .. }
                | Paint::FigureTag { .. }
                | Paint::TableTag { .. }
        );
        let tag_space: Option<u32> = if is_label_tag && page.font_family == FontFamily::ComputerModern {
            Some(crate::font_metrics::resolve_space_units(page.font_family, face))
        } else {
            None
        };
        if tag_space.is_some() {
            s.extend_from_slice(b"[(");
        } else {
            s.push(b'(');
        }
        match op {
            Paint::GlyphRun { .. } => {}
            Paint::Marker { marker, face, .. } => write_marker(s, *marker, *face),
            Paint::CaptionTag { n, .. } => {
                write_tag_word_space(s, b"Figure", tag_space);
                write_u64(s, *n as u64);
                s.push(b':');
                if tag_space.is_none() {
                    s.push(b' ');
                }
            }
            Paint::TableCaptionTag { n, .. } => {
                write_tag_word_space(s, b"Table", tag_space);
                write_u64(s, *n as u64);
                s.push(b':');
                if tag_space.is_none() {
                    s.push(b' ');
                }
            }
            Paint::PageNumber { page_number, .. } => {
                write_u64(s, *page_number as u64);
            }
            Paint::SectionNumber { number, level, .. } => match level {
                1 => write_u64(s, number[0] as u64),
                2 => {
                    write_u64(s, number[0] as u64);
                    s.push(b'.');
                    write_u64(s, number[1] as u64);
                }
                3 => {
                    write_u64(s, number[0] as u64);
                    s.push(b'.');
                    write_u64(s, number[1] as u64);
                    s.push(b'.');
                    write_u64(s, number[2] as u64);
                }
                _ => {
                    write_u64(s, number[0] as u64);
                    s.push(b'.');
                    write_u64(s, number[1] as u64);
                    s.push(b'.');
                    write_u64(s, number[2] as u64);
                    s.push(b'.');
                    write_u64(s, number[3] as u64);
                }
            },
            Paint::FigureTag { n, .. } => {
                write_tag_word_space(s, b"Figure", tag_space);
                write_u64(s, *n as u64);
            }
            Paint::TableTag { n, .. } => {
                write_tag_word_space(s, b"Table", tag_space);
                write_u64(s, *n as u64);
            }
            Paint::EquationNumber { n, .. } => {
                s.push(b'(');
                write_u64(s, *n as u64);
                s.push(b')');
            }
            Paint::Rule { .. } | Paint::VectorGlyph { .. } | Paint::CidGlyphRun { .. } | Paint::Color { .. } => {}
        }
        if tag_space.is_some() {
            s.extend_from_slice(b")] TJ\n");
        } else {
            s.extend_from_slice(b") Tj\n");
        }

        last_x = x;
        last_y = y;
    }
    if in_text {
        if last_word_spacing.is_some_and(|w| w != 0.0) {
            s.extend_from_slice(b"0 Tw\n");
        }
        s.extend_from_slice(b"ET\n");
    }
}

fn write_marker(out: &mut Vec<u8>, marker: Marker, face: Face) {
    match marker {
        Marker::Bullet => {
            if matches!(face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                out.push(0x0F); // CMSY10 bullet
            } else {
                out.push(0x95); // WinAnsi bullet
            }
        }
        Marker::Dash => {
            if matches!(face, Face::Roman | Face::Roman7 | Face::Roman5) {
                out.push(0x7B); // OT1 en-dash
            } else {
                out.push(0x96); // WinAnsi en-dash
            }
        }
        Marker::Asterisk => out.push(b'*'),
        Marker::MiddleDot => {
            if matches!(face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                out.push(0x01); // CMSY10 periodcentered
            } else {
                out.push(0xB7); // WinAnsi middle dot
            }
        }
        Marker::Number(n) => {
            write_u64(out, n as u64);
            out.push(b'.');
        }
        Marker::ParenNumber(n) => {
            out.push(b'(');
            write_u64(out, n as u64);
            out.push(b')');
        }
        Marker::Alpha(n) => {
            out.push(b'(');
            let ch = b'a' + ((n.max(1) - 1) % 26) as u8;
            out.push(ch);
            out.push(b')');
        }
        Marker::AlphaDot(n) => {
            let ch = b'a' + ((n.max(1) - 1) % 26) as u8;
            out.push(ch);
            out.push(b'.');
        }
        Marker::Roman(n) => {
            let s = crate::page_layout::to_roman(n);
            out.extend_from_slice(s.as_bytes());
            out.push(b'.');
        }
        Marker::UpperRoman(n) => {
            let s = crate::page_layout::to_upper_roman(n);
            out.extend_from_slice(s.as_bytes());
            out.push(b'.');
        }
        Marker::UpperAlpha(n) => {
            let ch = b'A' + ((n.max(1) - 1) % 26) as u8;
            out.push(ch);
            out.push(b'.');
        }
    }
}

const NEEDS_ESCAPE: [bool; 256] = {
    let mut lut = [true; 256];
    let mut b = 32u8;
    while b < 127 {
        if b != b'\\' && b != b'(' && b != b')' {
            lut[b as usize] = false;
        }
        b += 1;
    }
    lut
};

#[inline(always)]
fn pdf_escape(out: &mut Vec<u8>, text: &str, face: Face) {
    let bytes = text.as_bytes();
    let needs_escape = bytes.iter().any(|&b| NEEDS_ESCAPE[b as usize]);
    if !needs_escape {
        out.extend_from_slice(bytes);
        return;
    }
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 128 {
            match b {
                b'\\' | b'(' | b')' => {
                    if i > start {
                        out.extend_from_slice(&bytes[start..i]);
                    }
                    out.push(b'\\');
                    out.push(b);
                    i += 1;
                    start = i;
                }
                b if b < 32 => {
                    if i > start {
                        out.extend_from_slice(&bytes[start..i]);
                    }
                    out.push(b'\\');
                    out.push(b'0' + ((b >> 6) & 7));
                    out.push(b'0' + ((b >> 3) & 7));
                    out.push(b'0' + (b & 7));
                    i += 1;
                    start = i;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            if i > start {
                out.extend_from_slice(&bytes[start..i]);
            }
            let rest = &text[i..];
            let c = rest.chars().next().unwrap();
            i += c.len_utf8();
            let wb = if matches!(face, Face::Symbol | Face::Symbol7 | Face::Symbol5) {
                crate::font_metrics::unicode_to_symbol_byte(c).unwrap_or(b'?')
            } else {
                to_winansi(c)
            };
            match wb {
                b'\\' | b'(' | b')' => {
                    out.push(b'\\');
                    out.push(wb);
                }
                b if !(32..128).contains(&b) => {
                    out.push(b'\\');
                    out.push(b'0' + ((b >> 6) & 7));
                    out.push(b'0' + ((b >> 3) & 7));
                    out.push(b'0' + (b & 7));
                }
                _ => out.push(wb),
            }
            start = i;
        }
    }
    if start < bytes.len() {
        out.extend_from_slice(&bytes[start..]);
    }
}

#[inline]
fn to_winansi(c: char) -> u8 {
    let cp = c as u32;
    if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
        return cp as u8;
    }
    match c {
        '–' => 0x96,
        '—' => 0x97,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '…' => 0x85,
        '•' => 0x95,
        '†' => 0x86,
        '‡' => 0x87,
        _ => b'?',
    }
}

const DEC_DIGITS_LUT: &[u8; 200] = b"\
00010203040506070809\
10111213141516171819\
20212223242526272829\
30313233343536373839\
40414243444546474849\
50515253545556575859\
60616263646566676869\
70717273747576777879\
80818283848586878889\
90919293949596979899";

#[inline(always)]
fn format_f32_buf(out: &mut [u8], v: f32) -> usize {
    if !v.is_finite() {
        out[0] = b'0';
        return 1;
    }
    let scaled = (v * 100.0).round() as i64;
    let neg = scaled < 0;
    let n = scaled.unsigned_abs();
    let mut whole = n / 100;
    let frac = (n % 100) as usize;

    let mut buf = [0u8; 32];
    let mut i = 24;

    if whole == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while whole >= 100 {
            let rem = ((whole % 100) as usize) * 2;
            whole /= 100;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
        if whole < 10 {
            i -= 1;
            buf[i] = b'0' + whole as u8;
        } else {
            let rem = (whole as usize) * 2;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
    }

    if neg {
        i -= 1;
        buf[i] = b'-';
    }

    let mut end = 24;
    if frac != 0 {
        buf[end] = b'.';
        let frac_idx = frac * 2;
        buf[end + 1] = DEC_DIGITS_LUT[frac_idx];
        let d2 = DEC_DIGITS_LUT[frac_idx + 1];
        if d2 != b'0' {
            buf[end + 2] = d2;
            end += 3;
        } else {
            end += 2;
        }
    }

    let len = end - i;
    out[..len].copy_from_slice(&buf[i..end]);
    len
}

#[inline(always)]
fn write_f32(out: &mut Vec<u8>, v: f32) {
    let mut buf = [0u8; 32];
    let len = format_f32_buf(&mut buf, v);
    out.extend_from_slice(&buf[..len]);
}

#[inline(always)]
fn write_td(out: &mut Vec<u8>, dx: f32, dy: f32) {
    let mut buf = [0u8; 64];
    let l1 = format_f32_buf(&mut buf, dx);
    buf[l1] = b' ';
    let l2 = format_f32_buf(&mut buf[l1 + 1..], dy);
    let p = l1 + 1 + l2;
    buf[p..p + 4].copy_from_slice(b" Td\n");
    out.extend_from_slice(&buf[..p + 4]);
}

#[inline(always)]
fn write_tm(out: &mut Vec<u8>, x: f32, y: f32) {
    let mut buf = [0u8; 64];
    buf[..8].copy_from_slice(b"1 0 0 1 ");
    let l1 = format_f32_buf(&mut buf[8..], x);
    let p1 = 8 + l1;
    buf[p1] = b' ';
    let l2 = format_f32_buf(&mut buf[p1 + 1..], y);
    let p = p1 + 1 + l2;
    buf[p..p + 4].copy_from_slice(b" Tm\n");
    out.extend_from_slice(&buf[..p + 4]);
}

#[inline(always)]
fn write_tf(out: &mut Vec<u8>, face: Face, size: f32) {
    write_tf_resource(out, face.resource(), size);
}

#[inline(always)]
fn write_tf_resource(out: &mut Vec<u8>, resource: &str, size: f32) {
    let mut buf = [0u8; 32];
    let res = resource.as_bytes();
    let r_len = res.len();
    buf[..r_len].copy_from_slice(res);
    buf[r_len] = b' ';
    let s_len = format_f32_buf(&mut buf[r_len + 1..], size);
    let p = r_len + 1 + s_len;
    buf[p..p + 4].copy_from_slice(b" Tf\n");
    out.extend_from_slice(&buf[..p + 4]);
}

#[inline(always)]
fn write_tw(out: &mut Vec<u8>, word_spacing: f32) {
    let mut buf = [0u8; 32];
    let s_len = format_f32_buf(&mut buf, word_spacing);
    buf[s_len..s_len + 4].copy_from_slice(b" Tw\n");
    out.extend_from_slice(&buf[..s_len + 4]);
}

#[inline(always)]
/// `r g b rg r g b RG`, each channel with up to three decimals as xcolor writes them.
fn write_color(out: &mut Vec<u8>, color: crate::frontend::ast::PackedColor) {
    let ch = color.channels();
    for op in [b"rg\n".as_slice(), b"RG\n".as_slice()] {
        for c in ch {
            if c == 0 || c == 1000 {
                out.push(if c == 0 { b'0' } else { b'1' });
            } else {
                out.push(b'.');
                let digits = [b'0' + (c / 100) as u8, b'0' + (c / 10 % 10) as u8, b'0' + (c % 10) as u8];
                let len = 3 - digits.iter().rev().take_while(|&&d| d == b'0').count();
                out.extend_from_slice(&digits[..len]);
            }
            out.push(b' ');
        }
        out.extend_from_slice(op);
    }
}

fn write_rule(out: &mut Vec<u8>, x: f32, y: f32, w: f32, h: f32, fill: bool) {
    let mut buf = [0u8; 96];
    buf[..7].copy_from_slice(b"0.15 w ");
    let mut pos = 7;
    pos += format_f32_buf(&mut buf[pos..], x);
    buf[pos] = b' ';
    pos += 1;
    pos += format_f32_buf(&mut buf[pos..], y);
    buf[pos] = b' ';
    pos += 1;
    pos += format_f32_buf(&mut buf[pos..], w);
    buf[pos] = b' ';
    pos += 1;
    pos += format_f32_buf(&mut buf[pos..], h);
    if fill {
        buf[pos..pos + 6].copy_from_slice(b" re f\n");
        pos += 6;
    } else {
        buf[pos..pos + 6].copy_from_slice(b" re S\n");
        pos += 6;
    }
    out.extend_from_slice(&buf[..pos]);
}

fn write_f32_4dec(out: &mut Vec<u8>, v: f32) {
    if !v.is_finite() {
        out.push(b'0');
        return;
    }
    let scaled = (v * 10000.0).round() as i64;
    let neg = scaled < 0;
    let n = scaled.unsigned_abs();
    let mut whole = n / 10000;
    let frac = (n % 10000) as usize;

    let mut buf = [0u8; 32];
    let mut i = 24;

    if whole == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while whole >= 100 {
            let rem = ((whole % 100) as usize) * 2;
            whole /= 100;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
        if whole < 10 {
            i -= 1;
            buf[i] = b'0' + whole as u8;
        } else {
            let rem = (whole as usize) * 2;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
    }

    if neg {
        i -= 1;
        buf[i] = b'-';
    }

    let mut end = 24;
    if frac != 0 {
        buf[end] = b'.';
        let high = frac / 100;
        let low = frac % 100;
        let h_idx = high * 2;
        let l_idx = low * 2;
        let d0 = DEC_DIGITS_LUT[h_idx];
        let d1 = DEC_DIGITS_LUT[h_idx + 1];
        let d2 = DEC_DIGITS_LUT[l_idx];
        let d3 = DEC_DIGITS_LUT[l_idx + 1];
        buf[end + 1] = d0;
        if d1 != b'0' || d2 != b'0' || d3 != b'0' {
            buf[end + 2] = d1;
            if d2 != b'0' || d3 != b'0' {
                buf[end + 3] = d2;
                if d3 != b'0' {
                    buf[end + 4] = d3;
                    end += 5;
                } else {
                    end += 4;
                }
            } else {
                end += 3;
            }
        } else {
            end += 2;
        }
    }

    out.extend_from_slice(&buf[i..end]);
}

struct Writer<'w> {
    buf: &'w mut Vec<u8>,
    offsets: Vec<usize>,
}

impl<'w> Writer<'w> {
    fn new(buf: &'w mut Vec<u8>, page_count: usize, font_count: usize) -> Self {
        let n = page_count.max(1);
        let mut offsets = Vec::with_capacity(2 * n + 16 + font_count * 4);
        offsets.resize(1, 0);
        Self {
            buf,
            offsets,
        }
    }

    fn from_existing(buf: &'w mut Vec<u8>, offsets: Vec<usize>) -> Self {
        Self { buf, offsets }
    }

    #[inline]
    fn write_all(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    #[inline]
    fn write_int(&mut self, n: i32) {
        write_i64(self.buf, n as i64);
    }

    #[inline]
    fn write_num(&mut self, v: f32) {
        write_f32(self.buf, v);
    }

    #[inline]
    fn write_ref(&mut self, id: u32) {
        write_u64(self.buf, id as u64);
        self.buf.extend_from_slice(b" 0 R");
    }

    fn obj(&mut self, id: u32, body: impl FnOnce(&mut Self)) {
        let idx = id as usize;
        if idx >= self.offsets.len() {
            self.offsets.resize(idx + 1, 0);
        }
        self.offsets[idx] = self.buf.len();
        write_u64(self.buf, id as u64);
        self.buf.extend_from_slice(b" 0 obj\n");
        body(self);
        self.buf.extend_from_slice(b"endobj\n");
    }

    fn finish(self, root: u32) -> Result<(), Error> {
        let max_id = (self.offsets.len().saturating_sub(1)) as u32;
        self.buf.reserve((max_id as usize) * 20 + 256);
        let xref_at = self.buf.len();
        self.buf.extend_from_slice(b"xref\n0 ");
        write_u64(self.buf, max_id as u64 + 1);
        self.buf.push(b'\n');
        self.buf.extend_from_slice(b"0000000000 65535 f \n");

        let old_len = self.buf.len();
        let total_xref_bytes = (max_id as usize) * 20;
        self.buf.resize(old_len + total_xref_bytes, 0);
        let mut chunk_slice = &mut self.buf[old_len..];

        for &off in &self.offsets[1..] {
            let (entry_buf, rest) = chunk_slice.split_at_mut(20);
            chunk_slice = rest;
            if off == 0 {
                entry_buf.copy_from_slice(b"0000000000 65535 f \n");
                continue;
            }
            entry_buf.copy_from_slice(b"0000000000 00000 n \n");
            let mut n = off;
            let mut i = 10;
            while n >= 100 && i >= 2 {
                let rem = (n % 100) * 2;
                n /= 100;
                i -= 2;
                entry_buf[i] = DEC_DIGITS_LUT[rem];
                entry_buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
            }
            if n >= 10 && i >= 2 {
                let rem = n * 2;
                i -= 2;
                entry_buf[i] = DEC_DIGITS_LUT[rem];
                entry_buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
            } else if n > 0 && i >= 1 {
                i -= 1;
                entry_buf[i] = b'0' + n as u8;
            }
        }

        self.buf.extend_from_slice(b"trailer\n<< /Size ");
        write_u64(self.buf, max_id as u64 + 1);
        self.buf.extend_from_slice(b" /Root ");
        write_u64(self.buf, root as u64);
        self.buf.extend_from_slice(b" 0 R >>\nstartxref\n");
        write_u64(self.buf, xref_at as u64);
        self.buf.extend_from_slice(b"\n%%EOF\n");
        Ok(())
    }
}

#[inline(always)]
fn write_i64(out: &mut Vec<u8>, n: i64) {
    let mut buf = [0u8; 21];
    let mut i = 21;
    let mut u = n.unsigned_abs();
    if u == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while u >= 100 {
            let rem = ((u % 100) as usize) * 2;
            u /= 100;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
        if u < 10 {
            i -= 1;
            buf[i] = b'0' + u as u8;
        } else {
            let rem = (u as usize) * 2;
            i -= 2;
            buf[i] = DEC_DIGITS_LUT[rem];
            buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
        }
    }
    if n < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    out.extend_from_slice(&buf[i..]);
}

#[inline(always)]
fn write_u64(out: &mut Vec<u8>, mut n: u64) {
    if n == 0 {
        out.push(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n >= 100 {
        let rem = ((n % 100) as usize) * 2;
        n /= 100;
        i -= 2;
        buf[i] = DEC_DIGITS_LUT[rem];
        buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
    }
    if n < 10 {
        i -= 1;
        buf[i] = b'0' + n as u8;
    } else {
        let rem = (n as usize) * 2;
        i -= 2;
        buf[i] = DEC_DIGITS_LUT[rem];
        buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
    }
    out.extend_from_slice(&buf[i..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cid_tounicode_cmap_generation() {
        let mut buf = Vec::new();
        let mut writer = Writer::new(&mut buf, 1, 1);
        let cids = [832u16, 1236u16, 2121u16];
        let mut scratch = Vec::new();
        emit_cid_tounicode_cmap(&mut writer, 42, &cids, &mut scratch);
        let _ = writer.finish(42);
        let s = String::from_utf8_lossy(&buf);
        assert!(s.contains("/CMapName /RushTeX-CIDToUnicode def\n"));
        assert!(s.contains("3 beginbfchar\n"));
        assert!(s.contains("<0340> <222B>\n"));
        assert!(s.contains("<04D4> <211D>\n"));
        assert!(s.contains("<0849> <D835DC2F>\n"));
    }
}
