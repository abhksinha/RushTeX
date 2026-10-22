//! TeX files carried in the binary (D2b tier 1): the font programs most documents use, so a
//! document finds them with no TeX installation and no search (pillar 2). The engine's file
//! lookup reads one from here when the disk does not have it; the bytes are the pinned
//! file's, exactly, so output is the same either way.
//!
//! Layout: file names through the perfect hash `emf`; `em.r` holds three words a file (offset
//! and length of its compressed bytes in `em.z`, its kind); `em.z` the zlib streams; `em.id`
//! the content fingerprint.
//!
//! A Type 1 program (`.pfb`) is stored decrypted. Its eexec section (key 55665) and each
//! charstring in it (key 4330) are encryptions of text that compresses well; encrypted, it
//! does not compress at all. Both encryptions are deterministic, so encrypting the stored
//! text again gives the file's bytes exactly. The segment headers stay in place, so the
//! decrypted file has the original's layout.

use super::data_blob::{Blob, U32s};
use super::data_phf::Phf;

/// Stored as is.
const RAW: u32 = 0;
/// A `.pfb` stored decrypted.
const PFB: u32 = 1;

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
    id: u64,
}

impl<'a> EmbedTable<'a> {
    /// The table in `b`, every record's range checked once.
    pub fn from_blob(b: &Blob<'a>) -> Option<EmbedTable<'a>> {
        let names = Phf::from_blob(b, "emf")?;
        let (recs, data, id) = (b.u32s("em.r")?, b.section("em.z")?, b.u32s("em.id")?);
        let sound = recs.len() == 3 * names.len() && id.len() == 2 && (0..names.len()).all(|f| (recs.get(3 * f) as usize).checked_add(recs.get(3 * f + 1) as usize).is_some_and(|e| e <= data.len()) && matches!(recs.get(3 * f + 2), RAW | PFB));
        sound.then(|| EmbedTable { names, recs, data, id: (id.get(1) as u64) << 32 | id.get(0) as u64 })
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

    /// File `name`'s bytes, exactly as the pinned file holds them.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let f = self.names.get(name.as_bytes())? as usize;
        let (at, n, kind) = (self.recs.get(3 * f) as usize, self.recs.get(3 * f + 1) as usize, self.recs.get(3 * f + 2));
        let mut out = Vec::new();
        crate::backend::backend_inflate::zlib_decompress(&self.data[at..at + n], &mut out)?;
        if kind == PFB {
            transform_pfb(&mut out, false)?;
        }
        Some(out)
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
    let (mut recs, mut data) = (Vec::new(), Vec::new());
    let mut id = crate::backend::backend_hash::ContentHasher::new();
    let mut z = crate::backend::backend_deflate::DeflateCompressor::new();
    for (name, bytes) in files {
        id.bytes(name.as_bytes());
        id.bytes(bytes);
        let mut body = bytes.clone();
        let kind = if name.ends_with(".pfb") && transform_pfb(&mut body, true).is_some() { PFB } else { RAW };
        let at = data.len();
        z.compress_zlib(&body, &mut data);
        recs.extend_from_slice(&[at as u32, (data.len() - at) as u32, kind]);
    }
    let size = data.len();
    w.u32s("em.r", &recs);
    w.bytes("em.z", data);
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
