//! TeX font metrics as a static table (pillar 2): the pinned TFM files, generated once by
//! `rushtex-gen fonts` into `assets/tables/fonts.rtdb` and read in place.
//!
//! A TFM's parts (tex.web §539–§546) are word arrays, every word's value kept as a
//! little-endian quad. The engine resolves a font's nine arrays once when the font is loaded
//! (`TfmView`) and its per-character paths index them; a TFM read from a file at run time lands
//! in the same quad layout, so the engine has one representation of metrics.
//!
//! Layout, by access pattern (a font is read whole, never one array across fonts):
//! - **One record per font** (`fonts.r`, 64 bytes, one cache line): `bc`, tier, design size,
//!   and an offset and length for each of the nine arrays in the word region.
//! - **Word region** (`fonts.w`): font after font, so a font's arrays are contiguous and its
//!   first use touches one or two pages. An array equal to one already written (a family's
//!   lig/kern program at another size) is not written again: the record points at the first.
//! - **Order and tiers.** IDs follow popularity (how many corpus documents load the font, then
//!   name), so the fonts most documents use share the first pages. A font's tier marks where
//!   a later split into separately shipped or compressed blobs falls (tier 1 embedded).

use super::data_blob::{Blob, U32s};
use super::data_phf::Phf;

/// The nine arrays, in their order in a `TfmView` and a font record (short names for
/// messages): char_info, width, height, depth, italic, lig/kern, kern, extensible, param.
pub const POOLS: [&str; 9] = ["ci", "wd", "ht", "dp", "ic", "lk", "kn", "ex", "pa"];

/// One font's metrics: §539's arrays, borrowed. `info[c - bc]` is the `char_info` word of
/// `c`; `param[0]` is `\fontdimen1`. Fix_words are stored as `u32` and read back with `as i32`.
#[derive(PartialEq, Clone, Copy, Default)]
pub struct TfmView<'a> {
    pub bc: u8,
    /// The design size, a fix_word shifted right by 4 (§568).
    pub design: i32,
    pub info: U32s<'a>,
    pub widths: U32s<'a>,
    pub heights: U32s<'a>,
    pub depths: U32s<'a>,
    pub italics: U32s<'a>,
    pub lig_kern: U32s<'a>,
    pub kerns: U32s<'a>,
    pub exten: U32s<'a>,
    pub params: U32s<'a>,
}

impl<'a> TfmView<'a> {
    /// A view of nine quad arrays in `POOLS` order.
    pub fn from_pools(bc: u8, design: i32, p: [U32s<'a>; 9]) -> TfmView<'a> {
        let [info, widths, heights, depths, italics, lig_kern, kerns, exten, params] = p;
        TfmView { bc, design, info, widths, heights, depths, italics, lig_kern, kerns, exten, params }
    }

    pub fn pools(&self) -> [U32s<'a>; 9] {
        [self.info, self.widths, self.heights, self.depths, self.italics, self.lig_kern, self.kerns, self.exten, self.params]
    }
}

/// A TFM file's parts as values (§560 `read_font_info`'s checks): `None` when the bytes are
/// not a well-formed TFM.
pub struct TfmParts {
    pub bc: u8,
    pub design: i32,
    /// In `POOLS` order.
    pub pools: [Vec<u32>; 9],
}

pub fn parse(b: &[u8]) -> Option<TfmParts> {
    let half = |i: usize| -> Option<usize> { Some(u16::from_be_bytes([*b.get(2 * i)?, *b.get(2 * i + 1)?]) as usize) };
    let word = |i: usize| -> Option<u32> { Some(u32::from_be_bytes(b.get(4 * i..4 * i + 4)?.try_into().ok()?)) };
    let (lf, lh, bc, ec) = (half(0)?, half(1)?, half(2)?, half(3)?);
    let (nw, nh, nd, ni, nl, nk, ne, np) = (half(4)?, half(5)?, half(6)?, half(7)?, half(8)?, half(9)?, half(10)?, half(11)?);
    let chars = if ec + 1 >= bc { ec + 1 - bc } else { return None };
    if b.len() < 4 * lf || lf != 6 + lh + chars + nw + nh + nd + ni + nl + nk + ne + np || lh < 2 || ec > 255 {
        return None;
    }
    let design = word(7)? as i32 >> 4;
    let mut at = 6 + lh;
    let mut pools: [Vec<u32>; 9] = Default::default();
    for (pool, n) in pools.iter_mut().zip([chars, nw, nh, nd, ni, nl, nk, ne, np]) {
        *pool = (at..at + n).map(word).collect::<Option<Vec<u32>>>()?;
        at += n;
    }
    Some(TfmParts { bc: bc as u8, design, pools })
}

/// Bytes per font record: `bc` (u8), tier (u8), 2 spare, design (i32), nine `u32` offsets
/// and nine `u16` lengths into the word region, 2 spare.
const REC: usize = 64;

/// The table of every pinned font.
pub struct FontTable<'a> {
    names: Phf<'a>,
    recs: &'a [[u8; REC]],
    words: &'a [[u8; 4]],
    id: u64,
}

#[inline]
fn rec_u32(r: &[u8; REC], at: usize) -> u32 {
    u32::from_le_bytes([r[at], r[at + 1], r[at + 2], r[at + 3]])
}

#[inline]
fn rec_u16(r: &[u8; REC], at: usize) -> usize {
    u16::from_le_bytes([r[at], r[at + 1]]) as usize
}

impl<'a> FontTable<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<FontTable<'a>> {
        let names = Phf::from_blob(b, "fonts")?;
        let (recs, rest) = b.section("fonts.r")?.as_chunks::<REC>();
        let words = b.u32s("fonts.w")?.quads();
        let id = b.u32s("fonts.id")?;
        // Every array of every record in bounds, checked once, so `view` never panics.
        let sound = rest.is_empty()
            && recs.len() == names.len()
            && id.len() == 2
            && recs.iter().all(|r| (0..9).all(|k| (rec_u32(r, 8 + 4 * k) as usize).checked_add(rec_u16(r, 44 + 2 * k)).is_some_and(|e| e <= words.len())));
        sound.then(|| FontTable { names, recs, words, id: (id.get(1) as u64) << 32 | id.get(0) as u64 })
    }

    /// Font `id`'s nine arrays, resolved once (one record read).
    pub fn view(&self, id: u32) -> TfmView<'a> {
        let r = &self.recs[id as usize];
        let words = self.words;
        TfmView::from_pools(r[0], rec_u32(r, 4) as i32, std::array::from_fn(|k| {
            let off = rec_u32(r, 8 + 4 * k) as usize;
            U32s::new(&words[off..off + rec_u16(r, 44 + 2 * k)])
        }))
    }

    /// The tier font `id` belongs to (1: the most used fonts).
    pub fn tier(&self, id: u32) -> u8 {
        self.recs[id as usize][1]
    }

    /// The ID of the font whose TFM is `name` (`cmr10`).
    #[inline]
    pub fn lookup(&self, name: &str) -> Option<u32> {
        self.names.get(name.as_bytes())
    }

    pub fn name(&self, id: u32) -> &'a [u8] {
        self.names.name(id)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The generator's hash of the table's contents: a format made with one table refuses
    /// another (`engine_snapshot::fingerprint`).
    pub fn content_id(&self) -> u64 {
        self.id
    }
}

static FONTS_BLOB: &[u8] = include_bytes!("../../assets/tables/fonts.rtdb");

/// The pinned fonts, checked once per process; `None` only if the embedded blob is unsound.
pub fn fonts() -> Option<&'static FontTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<FontTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(FONTS_BLOB).as_ref().and_then(FontTable::from_blob)).as_ref()
}

/// One font for the writer: its TFM name, parts and tier.
#[cfg(any(test, feature = "gen"))]
pub struct GenFont {
    pub name: String,
    pub parts: TfmParts,
    pub tier: u8,
}

/// Writes `fonts` (ID = index: the caller orders them by popularity) as the sections
/// `FontTable::from_blob` reads. Returns the bytes the shared arrays saved.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, fonts: &[GenFont]) -> Result<usize, String> {
    let names: Vec<&[u8]> = fonts.iter().map(|f| f.name.as_bytes()).collect();
    super::data_phf::write(w, "fonts", &names).map_err(|e| format!("{e:?}"))?;
    let mut words: Vec<u32> = Vec::new();
    let mut seen: std::collections::HashMap<&[u32], u32> = std::collections::HashMap::new();
    let (mut recs, mut saved) = (Vec::with_capacity(fonts.len() * REC), 0usize);
    let mut id = crate::backend::backend_hash::ContentHasher::new();
    for f in fonts {
        let p = &f.parts;
        let mut rec = [0u8; REC];
        rec[0] = p.bc;
        rec[1] = f.tier;
        rec[4..8].copy_from_slice(&p.design.to_le_bytes());
        for (k, a) in p.pools.iter().enumerate() {
            let len = u16::try_from(a.len()).map_err(|_| format!("{}: array {} has {} words", f.name, POOLS[k], a.len()))?;
            let off = match seen.get(a.as_slice()) {
                Some(&off) if !a.is_empty() => {
                    saved += 4 * a.len();
                    off
                }
                _ => {
                    let off = words.len() as u32;
                    words.extend_from_slice(a);
                    if !a.is_empty() {
                        seen.insert(a, off);
                    }
                    off
                }
            };
            rec[8 + 4 * k..12 + 4 * k].copy_from_slice(&off.to_le_bytes());
            rec[44 + 2 * k..46 + 2 * k].copy_from_slice(&len.to_le_bytes());
            id.u32(len as u32);
            a.iter().for_each(|&v| id.u32(v));
        }
        id.bytes(f.name.as_bytes());
        id.u32(p.bc as u32 | (f.tier as u32) << 8);
        id.u32(p.design as u32);
        recs.extend_from_slice(&rec);
    }
    w.bytes("fonts.r", recs);
    w.u32s("fonts.w", &words);
    let h = id.finish() as u64;
    w.u32s("fonts.id", &[h as u32, (h >> 32) as u32]);
    Ok(saved)
}

/// Every word of every font in `t` against its source parts: the generator's self-check.
#[cfg(any(test, feature = "gen"))]
pub fn check(t: &FontTable, fonts: &[GenFont]) -> Result<(), String> {
    if t.len() != fonts.len() {
        return Err(format!("{} fonts in the table, {} sources", t.len(), fonts.len()));
    }
    for (id, f) in fonts.iter().enumerate() {
        let (id, name, p) = (id as u32, &f.name, &f.parts);
        if t.lookup(name) != Some(id) || t.name(id) != name.as_bytes() {
            return Err(format!("{name}: name lookup"));
        }
        let v = t.view(id);
        if v.bc != p.bc || v.design != p.design || t.tier(id) != f.tier {
            return Err(format!("{name}: bc, design size or tier"));
        }
        for (k, (got, want)) in v.pools().iter().zip(&p.pools).enumerate() {
            if !got.iter().eq(want.iter().copied()) {
                return Err(format!("{name}: pool {}", POOLS[k]));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data_blob::BlobWriter;

    /// A TFM with two characters, one lig/kern step and two parameters (§539's layout).
    fn tiny_tfm() -> Vec<u8> {
        let (lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np) = (2u16, 65u16, 66u16, 2u16, 2u16, 1u16, 1u16, 1u16, 1u16, 0u16, 2u16);
        let lf = 6 + lh + (ec - bc + 1) + nw + nh + nd + ni + nl + nk + ne + np;
        let mut b = Vec::new();
        for h in [lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np] {
            b.extend_from_slice(&h.to_be_bytes());
        }
        let words: [u32; 2 + 2 + 2 + 2 + 1 + 1 + 1 + 1 + 2] = [0, 10 << 20 | 0x55, 0x0110_0000, 0x0110_0100, 0, 1 << 20, 0, 2 << 20, 0, 0, 0x0042_8000, 1 << 19, 1 << 20, 2 << 20];
        for w in words {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b
    }

    #[test]
    fn parse_write_check_view() {
        let p = parse(&tiny_tfm()).unwrap();
        assert_eq!((p.bc, p.design, p.pools[0].len(), p.pools[8].len()), (65, (10 << 20 | 0x55) >> 4, 2, 2));
        assert!(parse(&tiny_tfm()[..40]).is_none());
        let fonts = vec![GenFont { name: "tiny".into(), parts: p, tier: 1 }, GenFont { name: "tiny2".into(), parts: parse(&tiny_tfm()).unwrap(), tier: 2 }];
        let mut w = BlobWriter::default();
        // The second font's arrays all equal the first's: none is written again.
        // Within the first, the italic array equals the depth array ([0]): 4 + 48 bytes.
        assert_eq!(write(&mut w, &fonts).unwrap(), 52);
        let bytes = w.finish();
        let b = Blob::new(&bytes).unwrap();
        let t = FontTable::from_blob(&b).unwrap();
        check(&t, &fonts).unwrap();
        let v = t.view(t.lookup("tiny2").unwrap());
        assert_eq!((v.info.get(1), v.lig_kern.get(0), v.params.get(1)), (0x0110_0100, 0x0042_8000, 2 << 20));
        assert!(t.lookup("cmr10").is_none());
        assert_ne!(t.content_id(), 0);
        assert_eq!((t.tier(0), t.tier(1)), (1, 2));
    }

    /// The embedded table is sound and holds the pinned Computer Modern fonts.
    #[test]
    fn embedded_table() {
        let t = fonts().expect("assets/tables/fonts.rtdb is a sound table");
        if t.is_empty() {
            return; // the bootstrap blob, before `rushtex-gen fonts` has run
        }
        let v = t.view(t.lookup("cmr10").expect("cmr10 is pinned"));
        assert_eq!((v.bc, v.design, v.info.len()), (0, 10 << 16, 128));
    }
}
