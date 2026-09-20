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

use crate::font_metrics::{Face, FontFamily};
use crate::page_layout::{Marker, Page, Paint, PAGE_H, PAGE_W};
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
    destination_buffer.reserve(pages.len() * 2048 + 4096);
    let total_pages = pages.len().max(1);

    // Collect all faces used across all pages in this document
    let mut doc_used_faces = 0u32;
    for page in pages {
        doc_used_faces |= page.compute_used_faces();
    }
    // If document has no text at all (e.g. blank document), default to Roman
    if doc_used_faces == 0 {
        doc_used_faces = 1 << (Face::Roman as u8);
    }
    let font_family = pages.first().map(|p| p.font_family).unwrap_or_default();
    let num_fonts = doc_used_faces.count_ones() as usize;

    let mut writer = Writer::new(destination_buffer, total_pages, num_fonts * 3);
    writer.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    // Object plan:
    // 1 catalog
    // 2 pages node
    // 3.. 3+total_pages-1 page dicts
    // 3+total_pages .. 3+2*total_pages-1 contents
    // then only the font objects that are used in the document
    let catalog_id = 1u32;
    let pages_id = 2u32;
    let first_page_id = 3u32;
    let first_content_id = first_page_id + total_pages as u32;

    // Allocate sequential IDs only for used fonts
    let mut font_object_ids = [0u32; 18];
    let mut font_descriptor_ids = [0u32; 18];
    let mut font_file_ids = [0u32; 18];
    let mut next_font_id = first_content_id + total_pages as u32;
    for (face_idx, obj_id) in font_object_ids.iter_mut().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            let is_cm_embedded = font_family == FontFamily::ComputerModern
                && matches!(
                    face,
                    Face::Roman
                        | Face::Bold
                        | Face::Italic
                        | Face::MathItalic
                        | Face::Symbol
                        | Face::Extension
                        | Face::Roman7
                        | Face::Roman5
                        | Face::MathItalic7
                        | Face::MathItalic5
                        | Face::Symbol7
                        | Face::Symbol5
                        | Face::AmsA
                        | Face::AmsB
                );
            *obj_id = next_font_id;
            next_font_id += 1;
            if is_cm_embedded {
                font_descriptor_ids[face_idx] = next_font_id;
                font_file_ids[face_idx] = next_font_id + 1;
                next_font_id += 2;
            }
        }
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
            w.write_ref(first_page_id + page_idx);
        }
        w.write_all(b" ] >>\n");
    });

    let mut uncompressed_stream = Vec::with_capacity(16384);
    let mut compressed_stream = Vec::with_capacity(16384);
    let mut compressor = crate::deflate_compress::DeflateCompressor::new();

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
            for (face_idx, &font_id) in font_object_ids.iter().enumerate() {
                if (doc_used_faces & (1 << face_idx)) != 0 {
                    let face = Face::from_u8(face_idx as u8);
                    w.write_all(b" ");
                    w.write_all(face.resource().as_bytes());
                    w.write_all(b" ");
                    w.write_ref(font_id);
                }
            }
            w.write_all(b" >> >> /Contents ");
            w.write_ref(content_object_id);
            w.write_all(b" >>\n");
        });
    }

    for (page_idx, page) in pages.iter().enumerate().take(total_pages) {
        uncompressed_stream.clear();
        build_stream_into(page, &mut uncompressed_stream);
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

    for (face_idx, &id) in font_object_ids.iter().enumerate() {
        if (doc_used_faces & (1 << face_idx)) != 0 {
            let face = Face::from_u8(face_idx as u8);
            if font_family == FontFamily::ComputerModern {
                match face {
                    Face::Roman => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMR10_PROGRAM,
                            &crate::backend::backend_fonts::CMR10_WIDTHS,
                            false,
                        );
                    }
                    Face::Bold => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMBX10_PROGRAM,
                            &crate::backend::backend_fonts::CMBX10_WIDTHS,
                            false,
                        );
                    }
                    Face::Italic => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMTI10_PROGRAM,
                            &crate::backend::backend_fonts::CMTI10_WIDTHS,
                            false,
                        );
                    }
                    Face::MathItalic => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMMI10_PROGRAM,
                            &crate::backend::backend_fonts::CMMI10_WIDTHS,
                            false,
                        );
                    }
                    Face::Symbol => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMSY10_PROGRAM,
                            &crate::backend::backend_fonts::CMSY10_WIDTHS,
                            false,
                        );
                    }
                    Face::Extension => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMEX10_PROGRAM,
                            &crate::backend::backend_fonts::CMEX10_WIDTHS,
                            false,
                        );
                    }
                    Face::Roman7 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMR7_PROGRAM,
                            &crate::backend::backend_fonts::CMR7_WIDTHS,
                            false,
                        );
                    }
                    Face::Roman5 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMR5_PROGRAM,
                            &crate::backend::backend_fonts::CMR5_WIDTHS,
                            false,
                        );
                    }
                    Face::MathItalic7 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMMI7_PROGRAM,
                            &crate::backend::backend_fonts::CMMI7_WIDTHS,
                            false,
                        );
                    }
                    Face::MathItalic5 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMMI5_PROGRAM,
                            &crate::backend::backend_fonts::CMMI5_WIDTHS,
                            false,
                        );
                    }
                    Face::Symbol7 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMSY7_PROGRAM,
                            &crate::backend::backend_fonts::CMSY7_WIDTHS,
                            false,
                        );
                    }
                    Face::Symbol5 => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::CMSY5_PROGRAM,
                            &crate::backend::backend_fonts::CMSY5_WIDTHS,
                            false,
                        );
                    }
                    Face::AmsA => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::MSAM10_PROGRAM,
                            &crate::backend::backend_fonts::MSAM10_WIDTHS,
                            false,
                        );
                    }
                    Face::AmsB => {
                        write_embedded_cm_font(
                            &mut writer,
                            id,
                            font_descriptor_ids[face_idx],
                            font_file_ids[face_idx],
                            &crate::backend::cm_fonts::MSBM10_PROGRAM,
                            &crate::backend::backend_fonts::MSBM10_WIDTHS,
                            false,
                        );
                    }
                    Face::Dingbats => write_font(&mut writer, id, "ZapfDingbats", false),
                    _ => write_font(&mut writer, id, face.pdf_name(font_family), true),
                }
            } else {
                match face {
                    Face::Symbol | Face::Extension => write_font(&mut writer, id, "Symbol", false),
                    Face::Dingbats => write_font(&mut writer, id, "ZapfDingbats", false),
                    _ => write_font(&mut writer, id, face.pdf_name(font_family), true),
                }
            }
        }
    }

    writer.finish(catalog_id)
}

fn write_embedded_cm_font(
    writer: &mut Writer,
    font_id: u32,
    descriptor_id: u32,
    file_id: u32,
    program: &crate::backend::cm_fonts::CmFontProgram,
    widths: &[u16; 128],
    with_encoding: bool,
) {
    writer.obj(font_id, |w| {
        w.write_all(b"<< /Type /Font /Subtype /Type1 /BaseFont /");
        w.write_all(program.name.as_bytes());
        w.write_all(b" /FontDescriptor ");
        w.write_ref(descriptor_id);
        w.write_all(b" /FirstChar 0 /LastChar 127 /Widths [");
        for &width in widths.iter() {
            w.write_all(b" ");
            w.write_int(width as i32);
        }
        w.write_all(b" ]");
        if with_encoding {
            w.write_all(b" /Encoding /WinAnsiEncoding");
        }
        w.write_all(b" >>\n");
    });

    writer.obj(descriptor_id, |w| {
        w.write_all(b"<< /Type /FontDescriptor /FontName /");
        w.write_all(program.name.as_bytes());
        w.write_all(b" /Flags ");
        w.write_int(program.flags as i32);
        w.write_all(b" /FontBBox [");
        w.write_int(program.bbox[0]);
        w.write_all(b" ");
        w.write_int(program.bbox[1]);
        w.write_all(b" ");
        w.write_int(program.bbox[2]);
        w.write_all(b" ");
        w.write_int(program.bbox[3]);
        w.write_all(b"] /Ascent ");
        w.write_int(program.ascent);
        w.write_all(b" /Descent ");
        w.write_int(program.descent);
        w.write_all(b" /CapHeight ");
        w.write_int(program.cap_height);
        w.write_all(b" /ItalicAngle ");
        w.write_int(program.italic_angle);
        w.write_all(b" /StemV ");
        w.write_int(program.stem_v);
        w.write_all(b" /FontFile ");
        w.write_ref(file_id);
        w.write_all(b" >>\n");
    });

    writer.obj(file_id, |w| {
        w.write_all(b"<< /Filter /FlateDecode /Length ");
        w.write_int(program.compressed_stream.len() as i32);
        w.write_all(b" /Length1 ");
        w.write_int(program.length1 as i32);
        w.write_all(b" /Length2 ");
        w.write_int(program.length2 as i32);
        w.write_all(b" /Length3 ");
        w.write_int(program.length3 as i32);
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
                if last_word_spacing.is_some_and(|w| w != 0.0) {
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
            Paint::CaptionTag { x, y, .. } | Paint::TableCaptionTag { x, y, .. } => {
                (*x, *y, Face::Roman, crate::page_layout::FONT_SIZE, 0.0)
            }
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
                face,
                ..
            } => {
                let start = *part_start as usize;
                let count = *part_count as usize;
                for &p in &page.text_pool[start..start + count] {
                    if p == " " {
                        s.push(b' ');
                    } else {
                        pdf_escape(s, p, *face);
                    }
                }
            }
            Paint::Marker { marker, .. } => write_marker(s, *marker),
            Paint::CaptionTag { n, .. } => {
                s.extend_from_slice(b"Figure ");
                write_u64(s, *n as u64);
                s.extend_from_slice(b": ");
            }
            Paint::TableCaptionTag { n, .. } => {
                s.extend_from_slice(b"Table ");
                write_u64(s, *n as u64);
                s.extend_from_slice(b": ");
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
                s.extend_from_slice(b"Figure ");
                write_u64(s, *n as u64);
            }
            Paint::TableTag { n, .. } => {
                s.extend_from_slice(b"Table ");
                write_u64(s, *n as u64);
            }
            Paint::EquationNumber { n, .. } => {
                s.push(b'(');
                write_u64(s, *n as u64);
                s.push(b')');
            }
            Paint::Rule { .. } => {}
        }
        s.extend_from_slice(b") Tj\n");

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

fn write_marker(out: &mut Vec<u8>, marker: Marker) {
    match marker {
        Marker::Bullet => out.push(0x95), // WinAnsi bullet
        Marker::Dash => out.push(0x96),   // WinAnsi en-dash
        Marker::Asterisk => out.push(b'*'),
        Marker::MiddleDot => out.push(0xB7), // WinAnsi middle dot
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

fn pdf_escape(out: &mut Vec<u8>, text: &str, face: Face) {
    let bytes = text.as_bytes();
    let needs_escape = bytes
        .iter()
        .any(|&b| !(32..127).contains(&b) || matches!(b, b'\\' | b'(' | b')'));
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
            let wb = if face == Face::Symbol {
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
        '±' => 0xB1,
        '×' => 0xD7,
        '÷' => 0xF7,
        'µ' => 0xB5,
        '°' => 0xB0,
        '§' => 0xA7,
        '¶' => 0xB6,
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

fn write_f32(out: &mut Vec<u8>, v: f32) {
    if !v.is_finite() {
        out.push(b'0');
        return;
    }
    let scaled = (v * 100.0).round() as i64;
    let neg = scaled < 0;
    let n = scaled.unsigned_abs();
    let whole = n / 100;
    let frac = (n % 100) as usize;
    if neg {
        out.push(b'-');
    }
    write_u64(out, whole);
    if frac == 0 {
        return;
    }
    out.push(b'.');
    let frac_idx = frac * 2;
    out.push(DEC_DIGITS_LUT[frac_idx]);
    let d2 = DEC_DIGITS_LUT[frac_idx + 1];
    if d2 != b'0' {
        out.push(d2);
    }
}

struct Writer<'w> {
    buf: &'w mut Vec<u8>,
    offsets: Vec<usize>,
}

impl<'w> Writer<'w> {
    fn new(buf: &'w mut Vec<u8>, page_count: usize, font_count: usize) -> Self {
        let n = page_count.max(1);
        Self {
            buf,
            offsets: Vec::with_capacity(2 * n + 2 + font_count),
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

fn write_xref_entry(out: &mut Vec<u8>, off: usize) {
    let mut buf = [b'0'; 10];
    let mut n = off;
    let mut i = 10;
    while n >= 100 && i >= 2 {
        let rem = (n % 100) * 2;
        n /= 100;
        i -= 2;
        buf[i] = DEC_DIGITS_LUT[rem];
        buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
    }
    if n >= 10 && i >= 2 {
        let rem = n * 2;
        i -= 2;
        buf[i] = DEC_DIGITS_LUT[rem];
        buf[i + 1] = DEC_DIGITS_LUT[rem + 1];
    } else if n > 0 && i >= 1 {
        i -= 1;
        buf[i] = b'0' + n as u8;
    }
    out.extend_from_slice(&buf);
    out.extend_from_slice(b" 00000 n \n");
}
