//! The one table format: a header naming little-endian sections, read in place.
//!
//! Layout: `RTDB`, format version (`u32`), section count (`u32`), then per section a 8-byte
//! name, its offset and its length in bytes (`u32` each). Sections start on 8-byte
//! boundaries. `Blob::new` checks the header and every section's bounds once; after that a
//! section is a borrowed slice and a column read is one bounds-checked load.

const MAGIC: &[u8; 4] = b"RTDB";
/// Bumped when the layout of any section changes; a blob of another version is refused.
pub const VERSION: u32 = 1;
const ENTRY: usize = 16;

#[derive(Clone, Copy)]
pub struct Blob<'a> {
    bytes: &'a [u8],
    count: usize,
}

#[inline]
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

impl<'a> Blob<'a> {
    /// The blob in `bytes`, when its header and section bounds are sound.
    pub fn new(bytes: &'a [u8]) -> Option<Blob<'a>> {
        if bytes.get(..4)? != MAGIC || u32_at(bytes, 4)? != VERSION {
            return None;
        }
        let count = u32_at(bytes, 8)? as usize;
        let table_end = 12usize.checked_add(count.checked_mul(ENTRY)?)?;
        if table_end > bytes.len() {
            return None;
        }
        for i in 0..count {
            let e = 12 + i * ENTRY;
            let (off, len) = (u32_at(bytes, e + 8)? as usize, u32_at(bytes, e + 12)? as usize);
            if off < table_end || off.checked_add(len)? > bytes.len() {
                return None;
            }
        }
        Some(Blob { bytes, count })
    }

    /// The section named `name` (at most 8 bytes), or `None`.
    pub fn section(&self, name: &str) -> Option<&'a [u8]> {
        let mut key = [0u8; 8];
        key.get_mut(..name.len())?.copy_from_slice(name.as_bytes());
        (0..self.count).find_map(|i| {
            let e = 12 + i * ENTRY;
            (self.bytes[e..e + 8] == key).then(|| {
                let off = u32_at(self.bytes, e + 8).unwrap_or(0) as usize;
                let len = u32_at(self.bytes, e + 12).unwrap_or(0) as usize;
                &self.bytes[off..off + len]
            })
        })
    }

    pub fn u16s(&self, name: &str) -> Option<U16s<'a>> {
        let s = self.section(name)?;
        let (c, rest) = s.as_chunks::<2>();
        rest.is_empty().then_some(U16s(c))
    }

    pub fn u32s(&self, name: &str) -> Option<U32s<'a>> {
        let s = self.section(name)?;
        let (c, rest) = s.as_chunks::<4>();
        rest.is_empty().then_some(U32s(c))
    }
}

/// A column of `u16` read in place.
#[derive(Clone, Copy)]
pub struct U16s<'a>(&'a [[u8; 2]]);

impl U16s<'_> {
    #[inline]
    pub fn get(&self, i: usize) -> u16 {
        u16::from_le_bytes(self.0[i])
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A column of `u32` read in place.
#[derive(Clone, Copy)]
pub struct U32s<'a>(&'a [[u8; 4]]);

impl U32s<'_> {
    #[inline]
    pub fn get(&self, i: usize) -> u32 {
        u32::from_le_bytes(self.0[i])
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().map(|&b| u32::from_le_bytes(b))
    }
}

/// Builds a blob: sections in the order added.
#[cfg(any(test, feature = "gen"))]
#[derive(Default)]
pub struct BlobWriter {
    sections: Vec<([u8; 8], Vec<u8>)>,
}

#[cfg(any(test, feature = "gen"))]
impl BlobWriter {
    pub fn bytes(&mut self, name: &str, data: Vec<u8>) {
        assert!(name.len() <= 8, "section name {name:?} is longer than 8 bytes");
        assert!(self.sections.iter().all(|(n, _)| n[..name.len()] != *name.as_bytes() || n[name.len()..].iter().any(|&b| b != 0)), "section {name:?} twice");
        let mut key = [0u8; 8];
        key[..name.len()].copy_from_slice(name.as_bytes());
        self.sections.push((key, data));
    }

    pub fn u16s(&mut self, name: &str, v: &[u16]) {
        self.bytes(name, v.iter().flat_map(|x| x.to_le_bytes()).collect());
    }

    pub fn u32s(&mut self, name: &str, v: &[u32]) {
        self.bytes(name, v.iter().flat_map(|x| x.to_le_bytes()).collect());
    }

    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.sections.len() as u32).to_le_bytes());
        let mut at = (12 + self.sections.len() * ENTRY).next_multiple_of(8);
        for (name, data) in &self.sections {
            out.extend_from_slice(name);
            out.extend_from_slice(&(at as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            at = (at + data.len()).next_multiple_of(8);
        }
        for (_, data) in &self.sections {
            out.resize(out.len().next_multiple_of(8), 0);
            out.extend_from_slice(data);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bounds() {
        let mut w = BlobWriter::default();
        w.u32s("a", &[1, 2, 0xdead_beef]);
        w.u16s("bb", &[7, 65535]);
        w.bytes("names", b"xyz".to_vec());
        let bytes = w.finish();
        let b = Blob::new(&bytes).unwrap();
        let a = b.u32s("a").unwrap();
        assert_eq!((a.len(), a.get(2)), (3, 0xdead_beef));
        assert_eq!(b.u16s("bb").unwrap().get(1), 65535);
        assert_eq!(b.section("names"), Some(&b"xyz"[..]));
        assert!(b.section("nope").is_none() && b.u32s("names").is_none());
        // Truncated or foreign bytes are refused, never read out of bounds.
        assert!(Blob::new(&bytes[..bytes.len() - 1]).is_none());
        assert!(Blob::new(b"RTDB").is_none() && Blob::new(b"").is_none());
    }
}
