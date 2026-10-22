//! Name → ID for a static name set: a perfect hash with an 8-bit fingerprint per slot.
//!
//! A query hashes the name once (`hash64`), reads its bucket's pilot (2 bytes), then one slot
//! word (`u32`: ID in the low 24 bits, fingerprint in the high 8). A name outside the set
//! fails the fingerprint 255 times in 256 without touching the names; a fingerprint match is
//! confirmed against the name's bytes (`offs[id]..offs[id + 1]` of the name blob), so a
//! hit is always exact. IDs are the caller's order of the names (ordered by use, the hot
//! ones share cache lines in every column by ID); the slot holding a name is independent
//! of its ID.
//!
//! Sections `{name}.h` (header: names, buckets, slots, seed), `{name}.p` (pilots, `u16`),
//! `{name}.s` (slots, `u32`), `{name}.o` (name offsets, `u32`, one more than names),
//! `{name}.n` (name bytes).

use super::data_blob::{Blob, U16s, U32s};
use crate::backend::backend_hash::{hash64, mix64};

/// An unused slot. No name has ID `0xFF_FFFF`, so no full slot equals it.
const EMPTY: u32 = u32::MAX;
const ID_BITS: u32 = 24;
pub const MAX_NAMES: usize = (1 << ID_BITS) - 1;

#[inline(always)]
fn fingerprint(h: u64) -> u32 {
    (h >> 56) as u32
}

/// Maps 32 hash bits onto `0..n` with one multiply (no division).
#[inline(always)]
fn range(x: u32, n: u32) -> u32 {
    ((x as u64 * n as u64) >> 32) as u32
}

#[inline(always)]
fn bucket(h: u64, buckets: u32) -> u32 {
    range(h as u32, buckets)
}

#[inline(always)]
fn slot(h: u64, pilot: u16, slots: u32) -> u32 {
    range((mix64(h, pilot as u64) >> 16) as u32, slots)
}

#[derive(Clone, Copy)]
pub struct Phf<'a> {
    seed: u64,
    buckets: u32,
    pilots: U16s<'a>,
    slots: U32s<'a>,
    offs: U32s<'a>,
    names: &'a [u8],
}

impl<'a> Phf<'a> {
    /// The table stored under `name`, when its sections agree with each other.
    pub fn from_blob(b: &Blob<'a>, name: &str) -> Option<Phf<'a>> {
        let h = b.u32s(&format!("{name}.h"))?;
        if h.len() != 5 {
            return None;
        }
        let (n, buckets, nslots) = (h.get(0) as usize, h.get(1), h.get(2));
        let seed = (h.get(4) as u64) << 32 | h.get(3) as u64;
        let pilots = b.u16s(&format!("{name}.p"))?;
        let slots = b.u32s(&format!("{name}.s"))?;
        let offs = b.u32s(&format!("{name}.o"))?;
        let names = b.section(&format!("{name}.n"))?;
        let sound = buckets > 0
            && nslots as usize >= n
            && pilots.len() == buckets as usize
            && slots.len() == nslots as usize
            && offs.len() == n + 1
            && offs.get(0) == 0
            && offs.get(n) as usize == names.len()
            && (1..=n).all(|i| offs.get(i - 1) <= offs.get(i))
            && slots.iter().all(|s| s == EMPTY || ((s & 0xFF_FFFF) as usize) < n);
        sound.then_some(Phf { seed, buckets, pilots, slots, offs, names })
    }

    /// The ID of `key`, or `None` when it is not in the set.
    #[inline]
    pub fn get(&self, key: &[u8]) -> Option<u32> {
        let h = hash64(self.seed, key);
        let p = self.pilots.get(bucket(h, self.buckets) as usize);
        let s = self.slots.get(slot(h, p, self.slots.len() as u32) as usize);
        if s == EMPTY || s >> ID_BITS != fingerprint(h) {
            return None;
        }
        let id = s & 0xFF_FFFF;
        (self.name(id) == key).then_some(id)
    }

    /// The name with ID `id`.
    #[inline]
    pub fn name(&self, id: u32) -> &'a [u8] {
        &self.names[self.offs.get(id as usize) as usize..self.offs.get(id as usize + 1) as usize]
    }

    /// The number of names.
    pub fn len(&self) -> usize {
        self.offs.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes per structure, for size reports: (pilots, slots, offsets, names).
    pub fn sizes(&self) -> (usize, usize, usize, usize) {
        (self.pilots.len() * 2, self.slots.len() * 4, self.offs.len() * 4, self.names.len())
    }
}

/// Why a table could not be built.
#[cfg(any(test, feature = "gen"))]
#[derive(Debug, PartialEq, Eq)]
pub enum BuildError {
    Duplicate(Vec<u8>),
    TooMany,
    /// No seed placed every bucket (does not happen at these load factors; reported, not
    /// looped on).
    NoSeed,
}

/// Writes `names` (ID = index) as the sections `Phf::from_blob` reads under `name`.
///
/// Buckets of about 4 names over 2% spare slots; buckets placed largest first, each trying
/// pilots until all its names land in free, distinct slots. Seeds are tried in a fixed
/// order, so the same names always give the same bytes.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, name: &str, names: &[&[u8]]) -> Result<(), BuildError> {
    let n = names.len();
    if n > MAX_NAMES {
        return Err(BuildError::TooMany);
    }
    {
        let mut sorted: Vec<&[u8]> = names.to_vec();
        sorted.sort_unstable();
        if let Some(d) = sorted.windows(2).find(|p| p[0] == p[1]) {
            return Err(BuildError::Duplicate(d[0].to_vec()));
        }
    }
    let buckets = (n.div_ceil(4)).max(1) as u32;
    let nslots = ((n as u64 * 100).div_ceil(98) as u32).max(1);
    let mut placed = None;
    'seeds: for attempt in 0..32u64 {
        let seed = mix64(0x5eed_0000 + attempt, 0x7ab1e5);
        let hashes: Vec<u64> = names.iter().map(|k| hash64(seed, k)).collect();
        // Names by bucket, CSR-style: counts, then offsets, then the names.
        let mut start = vec![0u32; buckets as usize + 1];
        for &h in &hashes {
            start[bucket(h, buckets) as usize + 1] += 1;
        }
        for i in 0..buckets as usize {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut members = vec![0u32; n];
        for (id, &h) in hashes.iter().enumerate() {
            let b = bucket(h, buckets) as usize;
            members[fill[b] as usize] = id as u32;
            fill[b] += 1;
        }
        let mut order: Vec<u32> = (0..buckets).collect();
        order.sort_by_key(|&b| std::cmp::Reverse(start[b as usize + 1] - start[b as usize]));
        let mut slots = vec![EMPTY; nslots as usize];
        let mut pilots = vec![0u16; buckets as usize];
        let mut tried = Vec::with_capacity(8);
        for &b in &order {
            let ids = &members[start[b as usize] as usize..start[b as usize + 1] as usize];
            if ids.is_empty() {
                break;
            }
            let found = (0..=u16::MAX).find(|&p| {
                tried.clear();
                for &id in ids {
                    let s = slot(hashes[id as usize], p, nslots);
                    if slots[s as usize] != EMPTY || tried.contains(&s) {
                        return false;
                    }
                    tried.push(s);
                }
                true
            });
            let Some(p) = found else { continue 'seeds };
            pilots[b as usize] = p;
            for &id in ids {
                let h = hashes[id as usize];
                slots[slot(h, p, nslots) as usize] = fingerprint(h) << ID_BITS | id;
            }
        }
        placed = Some((seed, pilots, slots));
        break;
    }
    let (seed, pilots, slots) = placed.ok_or(BuildError::NoSeed)?;
    let mut offs = Vec::with_capacity(n + 1);
    offs.push(0u32);
    let mut blob = Vec::with_capacity(names.iter().map(|k| k.len()).sum());
    for k in names {
        blob.extend_from_slice(k);
        offs.push(blob.len() as u32);
    }
    w.u32s(&format!("{name}.h"), &[n as u32, buckets, nslots, seed as u32, (seed >> 32) as u32]);
    w.u16s(&format!("{name}.p"), &pilots);
    w.u32s(&format!("{name}.s"), &slots);
    w.u32s(&format!("{name}.o"), &offs);
    w.bytes(&format!("{name}.n"), blob);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data_blob::BlobWriter;

    fn check(names: &[&[u8]], misses: &[&[u8]]) -> (usize, usize, usize, usize) {
        let mut w = BlobWriter::default();
        write(&mut w, "cs", names).unwrap();
        let bytes = w.finish();
        let b = Blob::new(&bytes).unwrap();
        let t = Phf::from_blob(&b, "cs").unwrap();
        assert_eq!(t.len(), names.len());
        for (id, k) in names.iter().enumerate() {
            assert_eq!(t.get(k), Some(id as u32), "{:?}", String::from_utf8_lossy(k));
            assert_eq!(t.name(id as u32), *k);
        }
        for k in misses {
            assert_eq!(t.get(k), None, "{:?}", String::from_utf8_lossy(k));
        }
        t.sizes()
    }

    #[test]
    fn small_sets_and_edges() {
        check(&[], &[b"x", b""]);
        check(&[b""], &[b"x"]);
        check(&[b"relax", b"par", b"@ifnextchar", b"\xff\x00"], &[b"rela", b"relaxx", b"Par", b""]);
        let mut w = BlobWriter::default();
        assert_eq!(write(&mut w, "d", &[b"a", b"b", b"a"]), Err(BuildError::Duplicate(b"a".to_vec())));
    }

    /// A name set the size of the LaTeX format's (about 30k names), shaped like its names.
    #[test]
    fn format_sized_set() {
        let owned: Vec<Vec<u8>> = (0..30_000u32).map(|i| format!("@{}x{}", ["if", "tempa", "sect", "l@", "c@"][i as usize % 5], i * 7919 % 100_003).into_bytes()).collect();
        let names: Vec<&[u8]> = owned.iter().map(|v| v.as_slice()).collect();
        let miss_owned: Vec<Vec<u8>> = (0..5_000u32).map(|i| format!("mydoc{i}").into_bytes()).collect();
        let misses: Vec<&[u8]> = miss_owned.iter().map(|v| v.as_slice()).collect();
        let (p, s, o, _) = check(&names, &misses);
        // 4.0 bits of pilot and about 32.7 bits of slot per name.
        assert!(p <= 30_000 / 2 + 2 && s <= 30_000 * 4 * 100 / 98 + 8, "{p} {s}");
        assert_eq!(o, 30_001 * 4);
    }

    /// `RT_PHF_NAMES=names.txt cargo test --release phf_real_names -- --ignored --nocapture`:
    /// the format's real names (from `RT_NAMES` of `format_smoke`), with the size report.
    #[test]
    #[ignore]
    fn phf_real_names() {
        let Ok(path) = std::env::var("RT_PHF_NAMES") else { return };
        let text = std::fs::read(path).unwrap();
        let mut names: Vec<&[u8]> = text.split(|&b| b == b'\n').filter(|l| !l.is_empty()).collect();
        names.sort_unstable();
        names.dedup();
        let t = std::time::Instant::now();
        let (p, s, o, n) = check(&names, &[b"mydocmacro", b"zzz@not@a@name", b"\\relax"]);
        let total = p + s + o + n;
        // Lookup cost: every name (hits) and as many non-names (misses), against std's map.
        let mut w = BlobWriter::default();
        write(&mut w, "cs", &names).unwrap();
        let bytes = w.finish();
        let b = Blob::new(&bytes).unwrap();
        let phf = Phf::from_blob(&b, "cs").unwrap();
        let miss_owned: Vec<Vec<u8>> = names.iter().map(|k| [k, &b"@doc"[..]].concat()).collect();
        let map: std::collections::HashMap<&[u8], u32> = names.iter().enumerate().map(|(i, k)| (*k, i as u32)).collect();
        let time = |f: &dyn Fn(&[u8]) -> Option<u32>, keys: &mut dyn Iterator<Item = &[u8]>| {
            let keys: Vec<&[u8]> = keys.collect();
            let t = std::time::Instant::now();
            let mut acc = 0u64;
            for _ in 0..20 {
                for k in &keys {
                    acc = acc.wrapping_add(std::hint::black_box(f(k)).map_or(1, |v| v as u64));
                }
            }
            std::hint::black_box(acc);
            t.elapsed().as_nanos() as f64 / (20 * keys.len()) as f64
        };
        let (ph, pm) = (time(&|k| phf.get(k), &mut names.iter().copied()), time(&|k| phf.get(k), &mut miss_owned.iter().map(|v| v.as_slice())));
        let (mh, mm) = (time(&|k| map.get(k).copied(), &mut names.iter().copied()), time(&|k| map.get(k).copied(), &mut miss_owned.iter().map(|v| v.as_slice())));
        eprintln!("PHF lookup ns: hit {ph:.1}, miss {pm:.1};  std HashMap: hit {mh:.1}, miss {mm:.1}");
        eprintln!("PHF {} names: pilots {p} B, slots {s} B, offsets {o} B, names {n} B; total {total} B ({:.1} bits/name beyond the names); built and checked in {:?}", names.len(), (p + s + o) as f64 * 8.0 / names.len() as f64, t.elapsed());
    }
}
