//! TeX files carried in the binary (D2b tier 1): the font programs most documents use, so a
//! document finds them with no TeX installation and no search (pillar 2). The engine's file
//! lookup reads one from here when the disk does not have it; the bytes are the pinned
//! file's, exactly, so output is the same either way.
//!
//! Layout: file names through the perfect hash `emf`; `em.r` holds three words a file (offset
//! and length of its compressed bytes in `em.z`, its kind); `em.z` the zlib streams; `em.id`
//! the content fingerprint; `em.gn` two words a file (offset and length in `em.gb`) and
//! `em.gb` each `.pfb`'s built-in encoding as stored text, one line a code, an empty line
//! `.notdef` (`X.pfb#names`): a font whose program the binary carries in another form needs
//! only these names, read with no decompression.
//!
//! A Type 1 program (`.pfb`) is stored decrypted. Its eexec section (key 55665) and each
//! charstring in it (key 4330) are encryptions of text that compresses well; encrypted, it
//! does not compress at all. Both encryptions are deterministic, so encrypting the stored
//! text again gives the file's bytes exactly. The segment headers stay in place, so the
//! decrypted file has the original's layout.

use super::data_blob::{Blob, U32s};
use super::data_phf::Phf;

/// Compressed as is.
const RAW: u32 = 0;
/// Stored as is, not compressed: a small text file a document reads whole on every run
/// (`rushtex.map`, an `.enc`), so reading it is a copy.
const PLAIN: u32 = 3;
/// A `.pfb` stored decrypted.
const PFB: u32 = 1;
/// A `.pfb` stored decrypted and not compressed: a font is subset from it on every use, so
/// reading it is a copy, not an inflate.
const PFB_PLAIN: u32 = 2;

/// Type 1's eexec and charstring keys and constants (Adobe Type 1 Font Format §7.1).
const EEXEC_KEY: u16 = 55665;
const CHARSTRING_KEY: u16 = 4330;
const C1: u16 = 52845;
const C2: u16 = 22719;

fn decrypt(data: &mut [u8], mut r: u16) {
    for b in data {
        let c = *b;
        *b = c ^ (r >> 8) as u8;
        r = (c as u16).wrapping_add(r).wrapping_mul(C1).wrapping_add(C2);
    }
}

fn encrypt(data: &mut [u8], mut r: u16) {
    for b in data {
        let c = *b ^ (r >> 8) as u8;
        *b = c;
        r = (c as u16).wrapping_add(r).wrapping_mul(C1).wrapping_add(C2);
    }
}

/// The charstrings of eexec text `text`: each `<n> RD ` or `<n> -| ` is followed by `n`
/// bytes of charstring, which `f` transforms in place. The scan skips each charstring, so
/// it finds the same places whether they are encrypted or not.
fn charstrings(text: &mut [u8], f: fn(&mut [u8], u16)) {
    let mut i = 0;
    while i + 4 <= text.len() {
        let w = &text[i..i + 4];
        if w == b" RD " || w == b" -| " {
            let mut d = i;
            while d > 0 && text[d - 1].is_ascii_digit() {
                d -= 1;
            }
            if d < i {
                let n: usize = std::str::from_utf8(&text[d..i]).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
                let start = i + 4;
                let end = (start + n).min(text.len());
                f(&mut text[start..end], CHARSTRING_KEY);
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// A Type 1 program's binary text (eexec), decrypted in place: eexec, then each charstring.
pub fn decrypt_binary(text: &mut [u8]) {
    decrypt(text, EEXEC_KEY);
    charstrings(text, decrypt);
}

/// [`decrypt_binary`]'s inverse.
pub fn encrypt_binary(text: &mut [u8]) {
    charstrings(text, encrypt);
    encrypt(text, EEXEC_KEY);
}

/// One charstring encrypted in place (key 4330).
pub fn encrypt_charstring(cs: &mut [u8]) {
    encrypt(cs, CHARSTRING_KEY);
}

/// The charstrings found in binary text `text` (decrypted) encrypted in place, by the same
/// scan [`decrypt_binary`] uses to find them.
pub fn encrypt_charstrings(text: &mut [u8]) {
    charstrings(text, encrypt);
}

/// Binary text whose charstrings are already encrypted, given its eexec encryption.
pub fn encrypt_eexec(text: &mut [u8]) {
    encrypt(text, EEXEC_KEY);
}

/// The binary (type 2) segments of a `.pfb` as ranges; `None` if it is not one.
fn pfb_binary_segments(pfb: &[u8]) -> Option<Vec<std::ops::Range<usize>>> {
    let (mut i, mut v) = (0, Vec::new());
    while i < pfb.len() {
        match pfb.get(i..i + 2)? {
            [0x80, 3] => return Some(v),
            [0x80, t @ (1 | 2)] => {
                let n = u32::from_le_bytes(pfb.get(i + 2..i + 6)?.try_into().ok()?) as usize;
                let r = i + 6..i + 6 + n;
                pfb.get(r.clone())?;
                if *t == 2 {
                    v.push(r.clone());
                }
                i = r.end;
            }
            _ => return None,
        }
    }
    Some(v)
}

/// Runs the eexec transform over a `.pfb`'s binary segments as one stream, then the
/// charstring transform (`decode`), or the other order (`!decode`).
fn transform_pfb(pfb: &mut [u8], decode: bool) -> Option<()> {
    let segs = pfb_binary_segments(pfb)?;
    let mut text: Vec<u8> = segs.iter().flat_map(|r| pfb[r.clone()].iter().copied()).collect();
    if decode {
        decrypt(&mut text, EEXEC_KEY);
        charstrings(&mut text, decrypt);
    } else {
        charstrings(&mut text, encrypt);
        encrypt(&mut text, EEXEC_KEY);
    }
    let mut at = 0;
    for r in segs {
        let n = r.len();
        pfb[r].copy_from_slice(&text[at..at + n]);
        at += n;
    }
    Some(())
}

pub struct EmbedTable<'a> {
    names: Phf<'a>,
    recs: U32s<'a>,
    data: &'a [u8],
    glyph_recs: U32s<'a>,
    glyphs: &'a [u8],
    id: u64,
}

impl<'a> EmbedTable<'a> {
    /// The table in `b`, every record's range checked once.
    pub fn from_blob(b: &Blob<'a>) -> Option<EmbedTable<'a>> {
        let names = Phf::from_blob(b, "emf")?;
        let (recs, data, id) = (b.u32s("em.r")?, b.section("em.z")?, b.u32s("em.id")?);
        let (glyph_recs, glyphs) = (b.u32s("em.gn")?, b.section("em.gb")?);
        let sound = recs.len() == 3 * names.len() && id.len() == 2 && (0..names.len()).all(|f| (recs.get(3 * f) as usize).checked_add(recs.get(3 * f + 1) as usize).is_some_and(|e| e <= data.len()) && matches!(recs.get(3 * f + 2), RAW | PFB | PFB_PLAIN | PLAIN));
        let glyphs_sound = glyph_recs.len() == 2 * names.len() && (0..names.len()).all(|f| (glyph_recs.get(2 * f) as usize).checked_add(glyph_recs.get(2 * f + 1) as usize).is_some_and(|e| e <= glyphs.len()));
        (sound && glyphs_sound).then(|| EmbedTable { names, recs, data, glyph_recs, glyphs, id: (id.get(1) as u64) << 32 | id.get(0) as u64 })
    }

    pub fn content_id(&self) -> u64 {
        self.id
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.len() == 0
    }

    pub fn file_name(&self, i: u32) -> &'a str {
        std::str::from_utf8(self.names.name(i)).unwrap_or("")
    }

    /// Whether the table carries file `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.names.get(name.as_bytes()).is_some()
    }

    /// File `name`'s bytes, exactly as the pinned file holds them. `X.pfb#head` is a `.pfb`'s
    /// first segment (its clear text) alone, which needs no encryption: what a font whose
    /// program the binary carries in another form still reads from its file. `X.pfb#clear`
    /// is the whole `.pfb` with its binary text decrypted ([`decrypt_binary`]), as stored.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        if let Some(file) = name.strip_suffix("#names") {
            return self.glyph_names(file).map(<[u8]>::to_vec);
        }
        if let Some(file) = name.strip_suffix("#clear") {
            let f = self.names.get(file.as_bytes())? as usize;
            let (at, n, kind) = (self.recs.get(3 * f) as usize, self.recs.get(3 * f + 1) as usize, self.recs.get(3 * f + 2));
            if kind == PFB_PLAIN {
                return Some(self.data[at..at + n].to_vec());
            }
            if kind != PFB {
                return None;
            }
            let mut out = Vec::new();
            crate::backend::backend_inflate::zlib_decompress(&self.data[at..at + n], &mut out)?;
            return Some(out);
        }
        if let Some(file) = name.strip_suffix("#head") {
            let f = self.names.get(file.as_bytes())? as usize;
            let (at, n) = (self.recs.get(3 * f) as usize, self.recs.get(3 * f + 1) as usize);
            let z = &self.data[at..at + n];
            if self.recs.get(3 * f + 2) == PFB_PLAIN {
                return pfb_head(z).map(|h| z[..6 + h.len()].to_vec());
            }
            // The clear text is a few kilobytes: a prefix is decompressed, all of it only when
            // the segment is longer.
            let mut out = Vec::new();
            crate::backend::backend_inflate::zlib_decompress_to(z, &mut out, 8192)?;
            let head = match out.get(..6)? {
                [0x80, 1, a, b, c, d] => 6 + u32::from_le_bytes([*a, *b, *c, *d]) as usize,
                _ => return None,
            };
            if out.len() < head {
                out.clear();
                crate::backend::backend_inflate::zlib_decompress(z, &mut out)?;
            }
            out.truncate(head.min(out.len()));
            return Some(out);
        }
        let f = self.names.get(name.as_bytes())? as usize;
        let (at, n, kind) = (self.recs.get(3 * f) as usize, self.recs.get(3 * f + 1) as usize, self.recs.get(3 * f + 2));
        let mut out = Vec::new();
        if kind == PFB_PLAIN || kind == PLAIN {
            out.extend_from_slice(&self.data[at..at + n]);
        } else {
            crate::backend::backend_inflate::zlib_decompress(&self.data[at..at + n], &mut out)?;
        }
        if kind == PFB || kind == PFB_PLAIN {
            transform_pfb(&mut out, false)?;
        }
        Some(out)
    }
}

impl<'a> EmbedTable<'a> {
    /// `.pfb` `file`'s built-in encoding (`X.pfb#names`): 256 lines, the glyph name of each
    /// code, an empty line for `.notdef`. Borrowed from the table, nothing decompressed.
    pub fn glyph_names(&self, file: &str) -> Option<&'a [u8]> {
        let f = self.names.get(file.as_bytes())? as usize;
        let (at, n) = (self.glyph_recs.get(2 * f) as usize, self.glyph_recs.get(2 * f + 1) as usize);
        (n > 0).then(|| &self.glyphs[at..at + n])
    }
}

/// A Type 1 program's built-in encoding (`dup 65 /A put` in its cleartext), by code.
/// The cleartext is Latin-1 (a copyright notice may hold `©`), so it is read bytewise.
pub fn builtin_encoding_names(clear: &[u8]) -> Vec<String> {
    let mut names = vec![String::from(".notdef"); 256];
    let text: String = clear.iter().map(|&c| char::from(c)).collect();
    let mut t = text.split_whitespace();
    while let Some(w) = t.next() {
        if w != "dup" {
            continue;
        }
        let (Some(code), Some(name)) = (t.next(), t.next()) else { break };
        if let (Ok(c), Some(n)) = (code.parse::<usize>(), name.strip_prefix('/')) {
            if c < 256 {
                names[c] = n.to_string();
            }
        }
    }
    names
}

/// [`EmbedTable::glyph_names`]'s text for a `.pfb`'s clear text (its first segment).
#[cfg(any(test, feature = "gen"))]
fn names_text(clear: &[u8]) -> Vec<u8> {
    let names = builtin_encoding_names(clear);
    let lines: Vec<&str> = names.iter().map(|n| if n == ".notdef" { "" } else { n.as_str() }).collect();
    lines.join("\n").into_bytes()
}

/// A `.pfb`'s first segment (its clear text), if it is one.
fn pfb_head(pfb: &[u8]) -> Option<&[u8]> {
    match pfb.get(..6)? {
        [0x80, 1, a, b, c, d] => pfb.get(6..6 + u32::from_le_bytes([*a, *b, *c, *d]) as usize),
        _ => None,
    }
}

/// The `fonts-core` feature carries the table; without it the binary has no embedded files.
#[cfg(feature = "fonts-core")]
static EMBED_BLOB: &[u8] = include_bytes!("../../assets/tables/embed.rtdb");
#[cfg(not(feature = "fonts-core"))]
static EMBED_BLOB: &[u8] = &[];

/// The embedded files, checked on first use; `None` if the table is not one of this build
/// (or is the empty placeholder).
pub fn embedded() -> Option<&'static EmbedTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<EmbedTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(EMBED_BLOB).and_then(|b| EmbedTable::from_blob(&b))).as_ref()
}

// ---- Generation -------------------------------------------------------------------------

/// Writes `files` (name, bytes), sorted by name; returns the compressed size.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, files: &[(String, Vec<u8>)]) -> Result<usize, String> {
    let mut files: Vec<&(String, Vec<u8>)> = files.iter().collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<&[u8]> = files.iter().map(|f| f.0.as_bytes()).collect();
    super::data_phf::write(w, "emf", &names).map_err(|e| format!("{e:?}"))?;
    let (mut recs, mut data, mut glyph_recs, mut glyphs) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut id = crate::backend::backend_hash::ContentHasher::new();
    let mut z = crate::backend::backend_deflate::DeflateCompressor::new();
    for (name, bytes) in files {
        id.bytes(name.as_bytes());
        id.bytes(bytes);
        let mut body = bytes.clone();
        // Every carried `.pfb` is subset from its decrypted text on each use (`write_font`
        // embeds a font the map file names from its own file), so it is stored plain:
        // reading it is a copy, not an inflate.
        let kind = if name.ends_with(".pfb") && transform_pfb(&mut body, true).is_some() {
            PFB_PLAIN
        } else if name.ends_with(".enc") || name.ends_with(".map") {
            PLAIN
        } else {
            RAW
        };
        let at = data.len();
        if kind == PFB_PLAIN || kind == PLAIN {
            data.extend_from_slice(&body);
        } else {
            z.compress_zlib(&body, &mut data);
        }
        recs.extend_from_slice(&[at as u32, (data.len() - at) as u32, kind]);
        let names = if name.ends_with(".pfb") { pfb_head(bytes).map(names_text).unwrap_or_default() } else { Vec::new() };
        glyph_recs.extend_from_slice(&[glyphs.len() as u32, names.len() as u32]);
        glyphs.extend_from_slice(&names);
    }
    let size = data.len();
    w.u32s("em.r", &recs);
    w.bytes("em.z", data);
    w.u32s("em.gn", &glyph_recs);
    w.bytes("em.gb", glyphs);
    let h = id.finish() as u64;
    w.u32s("em.id", &[h as u32, (h >> 32) as u32]);
    Ok(size)
}

/// Every file read back from the table equals its source.
#[cfg(any(test, feature = "gen"))]
pub fn check(t: &EmbedTable, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    if t.len() != files.len() {
        return Err(format!("{} files in the table, {} given", t.len(), files.len()));
    }
    for (name, bytes) in files {
        if t.read(name).as_deref() != Some(bytes.as_slice()) {
            return Err(format!("{name}: read back differs"));
        }
        let names = if name.ends_with(".pfb") { pfb_head(bytes).map(names_text) } else { None };
        if t.glyph_names(name).map(<[u8]>::to_vec) != names.filter(|n| !n.is_empty()) {
            return Err(format!("{name}: glyph names read back differ"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::data_blob::BlobWriter;
    use super::*;

    /// A small `.pfb`: cleartext, an eexec section holding two charstrings, a trailer.
    fn pfb() -> Vec<u8> {
        let mut plain = b"\x01\x02\x03\x04dup /Private 8 dict dup begin /CharStrings 2 dict dup begin\n/a 5 RD ".to_vec();
        let mut cs = *b"\x00\x00\x00\x00\x0e";
        encrypt(&mut cs, CHARSTRING_KEY);
        plain.extend_from_slice(&cs);
        plain.extend_from_slice(b" ND\n/b 3 -| \x10\x20\x30 |-\nend end\nmark currentfile closefile\n");
        encrypt(&mut plain, EEXEC_KEY);
        let seg = |t: u8, d: &[u8]| [&[0x80, t][..], &(d.len() as u32).to_le_bytes(), d].concat();
        [seg(1, b"%!PS-AdobeFont-1.0: Test\ncurrentfile eexec\n"), seg(2, &plain[..30]), seg(2, &plain[30..]), seg(1, b"0000\ncleartomark\n"), vec![0x80, 3]].concat()
    }

    #[test]
    fn files_read_back_exactly() {
        let files = vec![("test.pfb".to_string(), pfb()), ("8r.enc".to_string(), b"/TeXBase1Encoding [ /.notdef ] def\n".to_vec()), ("odd.pfb".to_string(), b"not a pfb".to_vec())];
        let mut decoded = files[0].1.clone();
        transform_pfb(&mut decoded, true).unwrap();
        assert!(decoded.windows(9).any(|w| w == b"/Private "), "eexec text is decrypted");
        let mut w = BlobWriter::default();
        write(&mut w, &files).unwrap();
        let bytes = w.finish();
        let t = EmbedTable::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        check(&t, &files).unwrap();
        assert_eq!(t.read("missing.pfb"), None);
    }

    #[cfg(feature = "fonts-core")]
    #[test]
    fn tier_one_is_carried() {
        let t = embedded().expect("the embedded table");
        let cmr10 = t.read("cmr10.pfb").expect("cmr10.pfb");
        assert!(cmr10.starts_with(&[0x80, 1]) && cmr10.windows(6).any(|w| w == b"/CMR10"));
    }
}
