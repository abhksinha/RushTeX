//! Font packs (native-architecture.md §4b, `2026-10-27-font-packs-blueprint.md`): the pinned
//! font files the binary does not carry, grouped by family into files of the `data_embed`
//! format, kept in the local cache and read there as cheaply as the embedded ones.
//!
//! The catalog (`assets/tables/packs.rtdb`, in the binary) maps each packed file's name to its
//! pack, and each pack to its name, content id and size, so a lookup needs no search. A pack
//! is `<cache>/packs/<name>-<id>.rtpk`, loaded on first use and checked against the catalog's
//! content id; a pack of another build is not used. Pack files are immutable: the id names
//! their bytes.

use super::data_blob::{Blob, U16s, U32s};
use super::data_embed::EmbedTable;
use super::data_phf::Phf;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The catalog this build reads packs through (empty without the `fonts-core` feature's tables).
static CATALOG_BLOB: &[u8] = include_bytes!("../../assets/tables/packs.rtdb");

pub struct Catalog<'a> {
    /// Packed file names (basenames, as the engine asks for them).
    files: Phf<'a>,
    /// Per file: its pack.
    pack_of: U16s<'a>,
    /// Pack names, one per line, in pack order.
    names: Vec<&'a str>,
    /// Per pack: content id (low, high) and size in bytes.
    recs: U32s<'a>,
}

impl<'a> Catalog<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<Catalog<'a>> {
        let files = Phf::from_blob(b, "pkf")?;
        let (pack_of, recs) = (b.u16s("pk.f")?, b.u32s("pk.r")?);
        let names: Vec<&str> = std::str::from_utf8(b.section("pk.n")?).ok()?.split('\n').filter(|n| !n.is_empty()).collect();
        let sound = pack_of.len() == files.len() && recs.len() == 3 * names.len() && (0..pack_of.len()).all(|i| (pack_of.get(i) as usize) < names.len());
        sound.then_some(Catalog { files, pack_of, names, recs })
    }

    pub fn packs(&self) -> usize {
        self.names.len()
    }

    pub fn name(&self, k: usize) -> &'a str {
        self.names[k]
    }

    pub fn id(&self, k: usize) -> u64 {
        (self.recs.get(3 * k + 1) as u64) << 32 | self.recs.get(3 * k) as u64
    }

    pub fn size(&self, k: usize) -> u32 {
        self.recs.get(3 * k + 2)
    }

    /// The pack's file name in the cache: `<name>-<id>.rtpk`.
    pub fn file_name(&self, k: usize) -> String {
        format!("{}-{:016x}.rtpk", self.name(k), self.id(k))
    }

    /// The pack holding file `name` (a basename; a `#layout`-style suffix is ignored).
    pub fn pack_of(&self, name: &str) -> Option<usize> {
        let base = name.split_once('#').map_or(name, |(b, _)| b);
        self.files.get(base.as_bytes()).map(|i| self.pack_of.get(i as usize) as usize)
    }
}

pub fn catalog() -> Option<&'static Catalog<'static>> {
    static C: OnceLock<Option<Catalog<'static>>> = OnceLock::new();
    C.get_or_init(|| Blob::new(CATALOG_BLOB).and_then(|b| Catalog::from_blob(&b))).as_ref()
}

/// Where packs are looked for, in order: `$RUSHTEX_CACHE/packs` (else `$XDG_CACHE_HOME/rushtex/packs`,
/// else `~/.cache/rushtex/packs`), then `$RUSHTEX_PACKS` (an installer's system directory).
pub fn pack_dirs() -> &'static [PathBuf] {
    static D: OnceLock<Vec<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let cache = env("RUSHTEX_CACHE").or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("rushtex"))).or_else(|| env("HOME").map(|h| h.join(".cache/rushtex")));
        cache.map(|c| c.join("packs")).into_iter().chain(env("RUSHTEX_PACKS")).collect()
    })
}

type Slots = Vec<OnceLock<Option<EmbedTable<'static>>>>;

/// One slot per pack of the catalog, filled on its first use.
fn slots() -> Option<&'static Slots> {
    static LOADED: OnceLock<Slots> = OnceLock::new();
    let c = catalog()?;
    Some(LOADED.get_or_init(|| (0..c.packs()).map(|_| OnceLock::new()).collect()))
}

/// Pack `k`, read from the first directory that has it and kept for the process; `None`
/// when no directory has it or its content id is not the catalog's.
pub fn pack(k: usize) -> Option<&'static EmbedTable<'static>> {
    let c = catalog()?;
    slots()?
        .get(k)?
        .get_or_init(|| {
            let file = c.file_name(k);
            let bytes = pack_dirs().iter().find_map(|d| std::fs::read(d.join(&file)).ok())?;
            // A pack lives as long as the process, as the embedded table does.
            let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            Blob::new(bytes).and_then(|b| EmbedTable::from_blob(&b)).filter(|t| t.content_id() == c.id(k))
        })
        .as_ref()
}

/// File `name` from its pack (`EmbedTable::read`'s names, `X.pfb#layout` and kin included).
pub fn read(name: &str) -> Option<Vec<u8>> {
    let k = catalog()?.pack_of(name)?;
    pack(k)?.read(name)
}

/// The map lines (`<pack>.map`) of the packs loaded so far, which the PDF writer adds to
/// `rushtex.map`: a font from a pack was loaded (its `.tfm`) before its page is written.
pub fn loaded_maps() -> Vec<u8> {
    let (Some(c), Some(s)) = (catalog(), slots()) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, slot) in s.iter().enumerate() {
        if let Some(Some(t)) = slot.get() {
            if let Some(m) = t.read(&format!("{}.map", c.name(k))) {
                out.extend_from_slice(&m);
            }
        }
    }
    out
}

/// Whether file `name` is in a pack (installed or not).
pub fn packed(name: &str) -> bool {
    catalog().is_some_and(|c| c.pack_of(name).is_some())
}

/// Copies every pack of this build's catalog found in `from` (a directory) into the first
/// pack directory, after checking each one's content id: `rushtex fonts install --from DIR`.
pub fn install_from(from: &std::path::Path) -> Result<(usize, usize), String> {
    let c = catalog().ok_or("this build has no pack catalog")?;
    let dest = pack_dirs().first().ok_or("no cache directory (set RUSHTEX_CACHE or HOME)")?;
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let (mut done, mut missing) = (0, 0);
    for k in 0..c.packs() {
        let file = c.file_name(k);
        let Ok(bytes) = std::fs::read(from.join(&file)) else {
            missing += 1;
            continue;
        };
        let ok = Blob::new(&bytes).and_then(|b| EmbedTable::from_blob(&b)).is_some_and(|t| t.content_id() == c.id(k));
        if !ok {
            return Err(format!("{file}: not the pack this build expects"));
        }
        // Written beside its final name, then renamed: a reader never sees half a pack.
        let tmp = dest.join(format!("{file}.part"));
        std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dest.join(&file)).map_err(|e| format!("{file}: {e}"))?;
        done += 1;
    }
    Ok((done, missing))
}

/// One line per pack: name, size, and where it is installed (or `not installed`).
pub fn list() -> Vec<String> {
    let Some(c) = catalog() else { return vec!["this build has no pack catalog".into()] };
    (0..c.packs())
        .map(|k| {
            let file = c.file_name(k);
            let at = pack_dirs().iter().map(|d| d.join(&file)).find(|p| p.exists());
            format!("{:<24} {:>8} KB  {}", c.name(k), c.size(k) / 1024, at.map_or("not installed".into(), |p| p.display().to_string()))
        })
        .collect()
}

// ---- Generation -------------------------------------------------------------------------

/// Writes the catalog: `packs` (name, content id, size) and `files` (basename, pack index),
/// file IDs in the order given.
#[cfg(any(test, feature = "gen"))]
pub fn write_catalog(w: &mut super::data_blob::BlobWriter, packs: &[(String, u64, u32)], files: &[(String, u16)]) -> Result<(), String> {
    let names: Vec<&[u8]> = files.iter().map(|f| f.0.as_bytes()).collect();
    super::data_phf::write(w, "pkf", &names).map_err(|e| format!("{e:?}"))?;
    w.u16s("pk.f", &files.iter().map(|f| f.1).collect::<Vec<_>>());
    w.bytes("pk.n", packs.iter().flat_map(|p| format!("{}\n", p.0).into_bytes()).collect());
    w.u32s("pk.r", &packs.iter().flat_map(|p| [p.1 as u32, (p.1 >> 32) as u32, p.2]).collect::<Vec<_>>());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_reads_back() {
        let mut w = super::super::data_blob::BlobWriter::default();
        let packs = [("a".to_string(), 0x1234_5678_9abc_def0, 10), ("b".to_string(), 7, 20)];
        let files = [("x.pfb".to_string(), 1), ("y.tfm".to_string(), 0), ("z.enc".to_string(), 1)];
        write_catalog(&mut w, &packs, &files).unwrap();
        let bytes = w.finish();
        let c = Catalog::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        assert_eq!((c.packs(), c.name(1), c.id(0), c.size(1)), (2, "b", 0x1234_5678_9abc_def0, 20));
        assert_eq!((c.pack_of("x.pfb#layout"), c.pack_of("y.tfm"), c.pack_of("w.pfb")), (Some(1), Some(0), None));
        assert_eq!(c.file_name(0), "a-123456789abcdef0.rtpk");
    }

    /// The catalog in the binary reads, and maps every file to a pack it lists.
    #[test]
    fn built_in_catalog_is_sound() {
        let c = catalog().expect("assets/tables/packs.rtdb");
        assert!(c.packs() > 0);
    }
}
