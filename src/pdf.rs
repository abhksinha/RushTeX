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

use crate::layout::{Marker, Paint, Page, PAGE_H, PAGE_W};
use crate::metrics::Face;
use crate::Error;

pub fn emit(pages: &[Page<'_>]) -> Result<Vec<u8>, Error> {
    let mut destination_buffer = Vec::with_capacity(pages.len() * 1024 + 2048);
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
    destination_buffer.clear();
    let total_pages = pages.len().max(1);
    let mut writer = Writer::new(destination_buffer, total_pages);
    writer.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    // Object plan:
    // 1 catalog
    // 2 pages node
    // 3.. 3+total_pages-1 page dicts
    // 3+total_pages .. 3+2*total_pages-1 contents
    // then 4 font objects
    let catalog_id = 1u32;
    let pages_id = 2u32;
    let first_page_id = 3u32;
    let first_content_id = first_page_id + total_pages as u32;
    let font_roman_id = first_content_id + total_pages as u32;
    let font_bold_id = font_roman_id + 1;
    let font_italic_id = font_roman_id + 2;
    let font_bold_italic_id = font_roman_id + 3;

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

    let mut uncompressed_stream = Vec::with_capacity(4096);
    let mut compressed_stream = Vec::with_capacity(4096);
    let mut compressor = crate::deflate::DeflateCompressor::new();

    for page_idx in 0..total_pages {
        let page_object_id = first_page_id + page_idx as u32;
        let content_object_id = first_content_id + page_idx as u32;
        let page_width = pages.get(page_idx).map(|p| p.width).unwrap_or(PAGE_W);
        let page_height = pages.get(page_idx).map(|p| p.height).unwrap_or(PAGE_H);
        writer.obj(page_object_id, |w| {
            w.write_all(b"<< /Type /Page /Parent ");
            w.write_ref(pages_id);
            w.write_all(b" /MediaBox [0 0 ");
            w.write_num(page_width);
            w.write_all(b" ");
            w.write_num(page_height);
            w.write_all(b"] /Resources << /Font <<");
            w.write_all(b" /Fr ");
            w.write_ref(font_roman_id);
            w.write_all(b" /Fb ");
            w.write_ref(font_bold_id);
            w.write_all(b" /Fi ");
            w.write_ref(font_italic_id);
            w.write_all(b" /Fbi ");
            w.write_ref(font_bold_italic_id);
            w.write_all(b" >> >> /Contents ");
            w.write_ref(content_object_id);
            w.write_all(b" >>\n");
        });
    }

    for page_idx in 0..total_pages {
        uncompressed_stream.clear();
        build_stream_into(&pages[page_idx], &mut uncompressed_stream);
        let content_object_id = first_content_id + page_idx as u32;
        if enable_compression {
            compressed_stream.clear();
            compressor.compress_zlib(&uncompressed_stream, &mut compressed_stream);
            writer.obj(content_object_id, |w| {
                w.write_all(b"<< /Filter /FlateDecode /Length ");
                w.write_int(compressed_stream.len() as i32);
                w.write_all(b" >>\nstream\n");
                w.write_all(&compressed_stream);
                w.write_all(b"\nendstream\n");
            });
        } else {
            writer.obj(content_object_id, |w| {
                w.write_all(b"<< /Length ");
                w.write_int(uncompressed_stream.len() as i32);
                w.write_all(b" >>\nstream\n");
                w.write_all(&uncompressed_stream);
                if !uncompressed_stream.ends_with(b"\n") {
                    w.write_all(b"\n");
                }
                w.write_all(b"endstream\n");
            });
        }
    }

    write_font(&mut writer, font_roman_id, "Helvetica");
    write_font(&mut writer, font_bold_id, "Helvetica-Bold");
    write_font(&mut writer, font_italic_id, "Helvetica-Oblique");
    write_font(&mut writer, font_bold_italic_id, "Helvetica-BoldOblique");

    writer.finish(catalog_id)
}

fn write_font(w: &mut Writer, id: u32, name: &str) {
    w.obj(id, |w| {
        w.write_all(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        w.write_all(name.as_bytes());
        w.write_all(b" /Encoding /WinAnsiEncoding >>\n");
    });
}

fn build_stream_into(page: &Page<'_>, s: &mut Vec<u8>) {
    s.extend_from_slice(b"BT\n");
    let mut last_face: Option<Face> = None;
    let mut last_size: Option<f32> = None;
    let mut last_word_spacing: Option<f32> = None;
    let mut last_x = 0.0f32;
    let mut last_y = 0.0f32;
    let mut pos_inited = false;

    let mut in_text = true;
    for op in &page.ops {
        if let Paint::Rule { x, y, w, h, fill } = op {
            if in_text {
                if last_word_spacing.map_or(false, |w| w != 0.0) {
                    s.extend_from_slice(b"0 Tw\n");
                    last_word_spacing = Some(0.0);
                }
                s.extend_from_slice(b"ET\n");
                in_text = false;
            }
            s.extend_from_slice(b"0.15 w ");
            write_f32(s, *x);
            s.push(b' ');
            write_f32(s, *y);
            s.push(b' ');
            write_f32(s, *w);
            s.push(b' ');
            write_f32(s, *h);
            if *fill {
                s.extend_from_slice(b" re f\n");
            } else {
                s.extend_from_slice(b" re S\n");
            }
            pos_inited = false;
            continue;
        }

        let (x, y, face, size, word_spacing) = match op {
            Paint::GlyphRun {
                x, y, face, size, word_spacing, ..
            } => (*x, *y, *face, *size, *word_spacing),
            Paint::Marker {
                x, y, face, size, ..
            }
            | Paint::PageNumber {
                x, y, face, size, ..
            } => (*x, *y, *face, *size, 0.0),
            Paint::CaptionTag { x, y, .. } => (*x, *y, Face::Roman, crate::layout::FONT_SIZE, 0.0),
            Paint::Rule { .. } => unreachable!(),
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
            write_f32(s, word_spacing);
            s.extend_from_slice(b" Tw\n");
            last_word_spacing = Some(word_spacing);
        }

        if last_face != Some(face) || last_size != Some(size) {
            s.extend_from_slice(face.resource().as_bytes());
            s.push(b' ');
            write_f32(s, size);
            s.extend_from_slice(b" Tf\n");
            last_face = Some(face);
            last_size = Some(size);
        }

        if !pos_inited {
            s.extend_from_slice(b"1 0 0 1 ");
            write_f32(s, x);
            s.push(b' ');
            write_f32(s, y);
            s.extend_from_slice(b" Tm\n");
            pos_inited = true;
        } else {
            write_f32(s, x - last_x);
            s.push(b' ');
            write_f32(s, y - last_y);
            s.extend_from_slice(b" Td\n");
        }

        s.push(b'(');
        match op {
            Paint::GlyphRun {
                part_start,
                part_count,
                ..
            } => {
                let start = *part_start as usize;
                let count = *part_count as usize;
                for &p in &page.text_pool[start..start + count] {
                    pdf_escape(s, p);
                }
            }
            Paint::Marker { marker, .. } => write_marker(s, *marker),
            Paint::CaptionTag { n, .. } => {
                s.extend_from_slice(b"Figure ");
                write_u64(s, *n as u64);
                s.extend_from_slice(b": ");
            }
            Paint::PageNumber { page_number, .. } => {
                write_u64(s, *page_number as u64);
            }
            Paint::Rule { .. } => {}
        }
        s.extend_from_slice(b") Tj\n");

        last_x = x;
        last_y = y;
    }
    if in_text {
        if last_word_spacing.map_or(false, |w| w != 0.0) {
            s.extend_from_slice(b"0 Tw\n");
        }
        s.extend_from_slice(b"ET\n");
    }
}

fn write_marker(out: &mut Vec<u8>, marker: Marker) {
    match marker {
        Marker::Bullet => out.push(0x95), // WinAnsi bullet
        Marker::Number(n) => {
            let mut buf = [0u8; 10];
            let mut i = 10;
            let mut v = n;
            if v == 0 {
                i -= 1;
                buf[i] = b'0';
            } else {
                while v > 0 {
                    i -= 1;
                    buf[i] = b'0' + (v % 10) as u8;
                    v /= 10;
                }
            }
            out.extend_from_slice(&buf[i..]);
            out.push(b'.');
        }
    }
}

fn pdf_escape(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 128 {
            match b {
                b'\\' | b'(' | b')' => {
                    out.push(b'\\');
                    out.push(b);
                }
                b'\n' => out.extend_from_slice(b"\\n"),
                b'\r' => out.extend_from_slice(b"\\r"),
                b'\t' => out.extend_from_slice(b"\\t"),
                _ => out.push(b),
            }
            i += 1;
        } else {
            let rest = &text[i..];
            let c = rest.chars().next().unwrap();
            i += c.len_utf8();
            let wb = to_winansi(c);
            match wb {
                b'\\' | b'(' | b')' => {
                    out.push(b'\\');
                    out.push(wb);
                }
                _ => out.push(wb),
            }
        }
    }
}

fn to_winansi(c: char) -> u8 {
    match c {
        ch if ch.is_ascii() => ch as u8,
        '–' => 0x96,
        '—' => 0x97,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '…' => 0x85,
        '•' => 0x95,
        ' ' => 0xA0,
        'ä' => 0xE4,
        'ö' => 0xF6,
        'ü' => 0xFC,
        'Ä' => 0xC4,
        'Ö' => 0xD6,
        'Ü' => 0xDC,
        'ß' => 0xDF,
        'é' => 0xE9,
        'è' => 0xE8,
        'à' => 0xE0,
        'ç' => 0xE7,
        _ => b'?',
    }
}

fn write_f32(out: &mut Vec<u8>, v: f32) {
    if !v.is_finite() {
        out.push(b'0');
        return;
    }
    let scaled = (v * 100.0).round() as i64;
    let neg = scaled < 0;
    let n = scaled.unsigned_abs();
    let whole = n / 100;
    let frac = n % 100;
    if neg {
        out.push(b'-');
    }
    let mut tmp = [0u8; 20];
    let mut i = 20;
    let mut w = whole;
    if w == 0 {
        i -= 1;
        tmp[i] = b'0';
    } else {
        while w > 0 {
            i -= 1;
            tmp[i] = b'0' + (w % 10) as u8;
            w /= 10;
        }
    }
    out.extend_from_slice(&tmp[i..]);
    if frac == 0 {
        return;
    }
    out.push(b'.');
    out.push(b'0' + (frac / 10) as u8);
    if frac % 10 != 0 {
        out.push(b'0' + (frac % 10) as u8);
    }
}

struct Writer<'w> {
    buf: &'w mut Vec<u8>,
    offsets: Vec<usize>,
}

impl<'w> Writer<'w> {
    fn new(buf: &'w mut Vec<u8>, page_count: usize) -> Self {
        let n = page_count.max(1);
        Self {
            buf,
            offsets: Vec::with_capacity(2 * n + 6),
        }
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
        self.offsets.push(self.buf.len());
        write_u64(self.buf, id as u64);
        self.buf.extend_from_slice(b" 0 obj\n");
        body(self);
        self.buf.extend_from_slice(b"endobj\n");
    }

    fn finish(self, root: u32) -> Result<(), Error> {
        let max_id = self.offsets.len() as u32;
        let xref_at = self.buf.len();
        self.buf.extend_from_slice(b"xref\n0 ");
        write_u64(self.buf, max_id as u64 + 1);
        self.buf.push(b'\n');
        self.buf.extend_from_slice(b"0000000000 65535 f \n");
        for &off in &self.offsets {
            write_xref_entry(self.buf, off);
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

fn write_i64(out: &mut Vec<u8>, n: i64) {
    if n < 0 {
        out.push(b'-');
        write_u64(out, n.unsigned_abs());
    } else {
        write_u64(out, n as u64);
    }
}

fn write_u64(out: &mut Vec<u8>, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = 20;
    if n == 0 {
        out.push(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    out.extend_from_slice(&buf[i..]);
}

fn write_xref_entry(out: &mut Vec<u8>, off: usize) {
    let mut buf = [b'0'; 20];
    let mut n = off;
    let mut i = 10;
    if n == 0 {
        // already zeros
    } else {
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    out.extend_from_slice(&buf[..10]);
    out.extend_from_slice(b" 00000 n \n");
}
