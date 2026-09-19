//! Hand-rolled PDF 1.4 writer. Zero compression libraries: content streams
//! are raw (for a text-only page that is smaller than a flate dance would be
//! once you count the encoder). Standard-14 fonts — nothing embedded.

use crate::layout::{Marker, Paint, Page, PAGE_H, PAGE_W};
use crate::metrics::Face;
use crate::Error;

pub fn emit(pages: &[Page<'_>]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(pages.len() * 1024 + 2048);
    emit_into(pages, &mut out)?;
    Ok(out)
}

pub fn emit_into(pages: &[Page<'_>], out: &mut Vec<u8>) -> Result<(), Error> {
    out.clear();
    let n = pages.len().max(1);
    let mut w = Writer::new(out, n);
    w.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    // Object plan:
    // 1 catalog
    // 2 pages node
    // 3.. 3+n-1 page dicts
    // 3+n .. 3+2n-1 contents
    // then 4 font objects
    let catalog_id = 1u32;
    let pages_id = 2u32;
    let first_page = 3u32;
    let first_content = first_page + n as u32;
    let font_roman = first_content + n as u32;
    let font_bold = font_roman + 1;
    let font_ital = font_roman + 2;
    let font_bi = font_roman + 3;

    // 1 Catalog
    w.obj(catalog_id, |w| {
        w.write_all(b"<< /Type /Catalog /Pages ");
        w.write_ref(pages_id);
        w.write_all(b" >>\n");
    });

    // 2 Pages
    w.obj(pages_id, |w| {
        w.write_all(b"<< /Type /Pages /Count ");
        w.write_int(n as i32);
        w.write_all(b" /Kids [");
        for i in 0..n as u32 {
            w.write_all(b" ");
            w.write_ref(first_page + i);
        }
        w.write_all(b" ] >>\n");
    });

    let mut stream_buf = Vec::with_capacity(4096);

    for i in 0..n {
        let pid = first_page + i as u32;
        let cid = first_content + i as u32;
        let page_w = pages.get(i).map(|p| p.width).unwrap_or(PAGE_W);
        let page_h = pages.get(i).map(|p| p.height).unwrap_or(PAGE_H);
        w.obj(pid, |w| {
            w.write_all(b"<< /Type /Page /Parent ");
            w.write_ref(pages_id);
            w.write_all(b" /MediaBox [0 0 ");
            w.write_num(page_w);
            w.write_all(b" ");
            w.write_num(page_h);
            w.write_all(b"] /Resources << /Font <<");
            w.write_all(b" /Fr ");
            w.write_ref(font_roman);
            w.write_all(b" /Fb ");
            w.write_ref(font_bold);
            w.write_all(b" /Fi ");
            w.write_ref(font_ital);
            w.write_all(b" /Fbi ");
            w.write_ref(font_bi);
            w.write_all(b" >> >> /Contents ");
            w.write_ref(cid);
            w.write_all(b" >>\n");
        });
    }

    for i in 0..n {
        stream_buf.clear();
        build_stream_into(&pages[i], &mut stream_buf);
        let cid = first_content + i as u32;
        w.obj(cid, |w| {
            w.write_all(b"<< /Length ");
            w.write_int(stream_buf.len() as i32);
            w.write_all(b" >>\nstream\n");
            w.write_all(&stream_buf);
            if !stream_buf.ends_with(b"\n") {
                w.write_all(b"\n");
            }
            w.write_all(b"endstream\n");
        });
    }

    write_font(&mut w, font_roman, "Helvetica");
    write_font(&mut w, font_bold, "Helvetica-Bold");
    write_font(&mut w, font_ital, "Helvetica-Oblique");
    write_font(&mut w, font_bi, "Helvetica-BoldOblique");

    w.finish(catalog_id)
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
    let mut last_x = 0.0f32;
    let mut last_y = 0.0f32;
    let mut pos_inited = false;

    let mut in_text = true;
    for op in &page.ops {
        if let Paint::Rule { x, y, w, h, fill } = op {
            if in_text {
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

        let (x, y, face, size) = match op {
            Paint::GlyphRun {
                x, y, face, size, ..
            }
            | Paint::Marker {
                x, y, face, size, ..
            } => (*x, *y, *face, *size),
            Paint::CaptionTag { x, y, .. } => (*x, *y, Face::Roman, crate::layout::FONT_SIZE),
            Paint::Rule { .. } => unreachable!(),
        };

        if !in_text {
            s.extend_from_slice(b"BT\n");
            in_text = true;
            last_face = None;
            pos_inited = false;
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
            Paint::Rule { .. } => {}
        }
        s.extend_from_slice(b") Tj\n");

        last_x = x;
        last_y = y;
    }
    if in_text {
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
