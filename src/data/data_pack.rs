//! Font packs (native-architecture.md §4b, `2026-10-27-font-packs-blueprint.md`): the pinned
//! font files the binary does not carry, grouped by family into indexed pack files of one-file
//! `data_embed` tables, kept in the local cache and read there as cheaply as the embedded ones.
//!
//! The catalog (`assets/tables/packs.rtdb`, in the binary) maps each packed file's name to its
//! pack, and each pack to its name, content id, size and SHA-256, so a lookup needs no search.
//! A pack is `<name>.rtpack`: in the binary with the `fonts-full` feature, else in
//! `<cache>/packs/`. Format 2 (`PACK_FORMAT`, `2026-11-03-pack-index.md`): a fixed header, an
//! index (file name to entry offset, length and checksum), then one entry per file, a one-file
//! `data_embed` table with the program plain and ready to subset. A run reads the header and
//! index of a pack on its first use (checked against the catalog's content id: a pack of
//! another build or format is not used, and named for reinstalling), then only the entries of
//! the files it needs, one seek and read each, each checked against its index checksum; the
//! whole file's SHA-256 is checked once, at install. `rushtex fonts install <pack>` fetches it from the release
//! `fonts-v<FONTS_RELEASE>` with the system's `curl` and installs it only when its SHA-256 is
//! the pinned one; the compiler itself never downloads anything.

use super::data_blob::{Blob, U16s, U32s};
use super::data_embed::EmbedTable;
use super::data_phf::Phf;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The catalog this build reads packs through.
static CATALOG_BLOB: &[u8] = include_bytes!("../../assets/tables/packs.rtdb");

/// The release the packs of this catalog are published in: `fonts-v<N>`. Bumped whenever
/// `rushtex-gen packs` gives different packs (the pin, `packs.tsv` or `embed.set` changed).
pub const FONTS_RELEASE: u32 = 2;

/// Where `rushtex fonts install` fetches packs from (`RUSHTEX_FONTS_URL` overrides it, a
/// `file://` folder for a local test).
fn release_url() -> String {
    std::env::var("RUSHTEX_FONTS_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| format!("https://github.com/abhksinha/RushTeX/releases/download/fonts-v{FONTS_RELEASE}"))
}

/// Every pack, carried in the binary: the `fonts-full` build (offline, CI). The names are the
/// catalog's (`full_build_carries_every_pack`).
#[cfg(feature = "fonts-full")]
static FULL: [(&str, &[u8]); 7] = [
    ("cm-super-tt", include_bytes!("../../assets/packs/cm-super-tt.rtpack")),
    ("cm-super", include_bytes!("../../assets/packs/cm-super.rtpack")),
    ("latin-modern", include_bytes!("../../assets/packs/latin-modern.rtpack")),
    ("tex-gyre", include_bytes!("../../assets/packs/tex-gyre.rtpack")),
    ("libertine", include_bytes!("../../assets/packs/libertine.rtpack")),
    ("urw-base", include_bytes!("../../assets/packs/urw-base.rtpack")),
    ("fonts", include_bytes!("../../assets/packs/fonts.rtpack")),
];

/// Pack `name`'s bytes when the binary carries it.
fn carried_pack(name: &str) -> Option<&'static [u8]> {
    #[cfg(feature = "fonts-full")]
    return FULL.iter().find(|p| p.0 == name).map(|p| p.1);
    #[cfg(not(feature = "fonts-full"))]
    {
        let _ = name;
        None
    }
}

pub struct Catalog<'a> {
    /// Packed file names (basenames, as the engine asks for them).
    files: Phf<'a>,
    /// Per file: its pack.
    pack_of: U16s<'a>,
    /// Pack names, one per line, in pack order.
    names: Vec<&'a str>,
    /// Per pack: content id (low, high) and size in bytes.
    recs: U32s<'a>,
    /// Per pack: the SHA-256 of its file, 32 bytes.
    sha: &'a [u8],
}

impl<'a> Catalog<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<Catalog<'a>> {
        let files = Phf::from_blob(b, "pkf")?;
        let (pack_of, recs, sha) = (b.u16s("pk.f")?, b.u32s("pk.r")?, b.section("pk.s")?);
        let names: Vec<&str> = std::str::from_utf8(b.section("pk.n")?).ok()?.split('\n').filter(|n| !n.is_empty()).collect();
        let sound = pack_of.len() == files.len() && recs.len() == 3 * names.len() && sha.len() == 32 * names.len() && (0..pack_of.len()).all(|i| (pack_of.get(i) as usize) < names.len());
        sound.then_some(Catalog { files, pack_of, names, recs, sha })
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

    /// The SHA-256 of pack `k`'s file.
    pub fn sha256(&self, k: usize) -> &'a [u8] {
        &self.sha[32 * k..32 * k + 32]
    }

    /// The pack's file name, in the cache and in the release: `<name>.rtpack`.
    pub fn file_name(&self, k: usize) -> String {
        format!("{}.rtpack", self.name(k))
    }

    /// The pack named `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| *n == name)
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

/// Where packs are looked for, after the binary (`fonts-full`), in order:
/// `$RUSHTEX_CACHE/packs` (else `$XDG_CACHE_HOME/rushtex/packs`, else
/// `~/.cache/rushtex/packs`), then `$RUSHTEX_PACKS` (an installer's system directory).
pub fn pack_dirs() -> &'static [PathBuf] {
    static D: OnceLock<Vec<PathBuf>> = OnceLock::new();
    D.get_or_init(|| {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let cache = env("RUSHTEX_CACHE").or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("rushtex"))).or_else(|| env("HOME").map(|h| h.join(".cache/rushtex")));
        cache.map(|c| c.join("packs")).into_iter().chain(env("RUSHTEX_PACKS")).collect()
    })
}

/// A pack file's first bytes: `RTPK`, the pack format, the index's length and its `hash64`.
const PACK_MAGIC: &[u8; 4] = b"RTPK";
/// The pack file format. Bumped whenever the layout changes; a pack of another format (an
/// installed one from an older build: format 1 was a whole `data_embed` table) is refused and
/// named for reinstalling. Format 2: a fixed header, an index, then one entry per file.
pub const PACK_FORMAT: u32 = 2;
/// The fixed header: magic, format, index length (`u32` each), the index's `hash64` (`u64`),
/// padded to 8 bytes so the index starts aligned.
const HEAD: usize = 24;
/// The seed of the per-entry and index checksums (`hash64`).
const SUM_SEED: u64 = 0x7274_7061_636b;

/// Where a pack's entries are read from: the binary's bytes (`fonts-full`, or a pack being
/// checked) or its file, one seek and one read per entry.
enum Source<'a> {
    Bytes(&'a [u8]),
    File(std::sync::Mutex<std::fs::File>),
}

/// A pack's index, read once: file name to entry, and each entry's place, length and checksum.
/// An entry is a one-file `data_embed` table (the file in the same ready-to-subset form the
/// binary carries its own fonts in, its `#layout` and `#names` beside it), read on first use
/// and kept for the process.
pub struct Pack<'a> {
    names: Phf<'a>,
    /// Per entry: offset from the payload's start, length, checksum (low, high).
    recs: U32s<'a>,
    id: u64,
    /// Where the entries start in the pack's file.
    payload: usize,
    src: Source<'a>,
    entries: Vec<OnceLock<Option<EmbedTable<'a>>>>,
}

/// A pack file's header: the index's length and checksum, or `None` when it is not a pack of
/// this format.
fn pack_head(head: &[u8]) -> Option<(usize, u64)> {
    let word = |at: usize| head.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    if head.get(..4)? != PACK_MAGIC || word(4)? != PACK_FORMAT {
        return None;
    }
    Some((word(8)? as usize, (word(16)? as u64) << 32 | word(12)? as u64))
}

impl<'a> Pack<'a> {
    /// The pack whose index is `index` (checked against `sum`), its entries in `src` from
    /// `payload` on.
    fn new(index: &'a [u8], sum: u64, payload: usize, src: Source<'a>) -> Option<Pack<'a>> {
        if crate::backend::backend_hash::hash64(SUM_SEED, index) != sum {
            return None;
        }
        let b = Blob::new(index)?;
        let names = Phf::from_blob(&b, "pxf")?;
        let (recs, id) = (b.u32s("px.r")?, b.u32s("px.id")?);
        (recs.len() == 4 * names.len() && id.len() == 2).then(|| Pack { entries: (0..names.len()).map(|_| OnceLock::new()).collect(), names, recs, id: (id.get(1) as u64) << 32 | id.get(0) as u64, payload, src })
    }

    /// The pack in `bytes` (a whole pack file), nothing but its index read.
    pub fn from_bytes(bytes: &'a [u8]) -> Option<Pack<'a>> {
        let (n, sum) = pack_head(bytes)?;
        let index = bytes.get(HEAD..HEAD.checked_add(n)?)?;
        Pack::new(index, sum, (HEAD + n).next_multiple_of(8), Source::Bytes(bytes))
    }

    /// The pack in file `file`: its header and index read, `Err` when the file is there but is
    /// not a pack of this format (an older build's: reinstall it).
    fn open(path: &std::path::Path) -> Result<Option<Pack<'static>>, ()> {
        use std::io::Read;
        let Ok(mut f) = std::fs::File::open(path) else { return Ok(None) };
        let mut head = [0u8; HEAD];
        f.read_exact(&mut head).map_err(|_| ())?;
        let (n, sum) = pack_head(&head).ok_or(())?;
        let mut index = vec![0u8; n];
        f.read_exact(&mut index).map_err(|_| ())?;
        // The index lives as long as the process, as the embedded tables do.
        let index: &'static [u8] = Box::leak(index.into_boxed_slice());
        Pack::new(index, sum, (HEAD + n).next_multiple_of(8), Source::File(std::sync::Mutex::new(f))).map(Some).ok_or(())
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

    /// Entry `i`'s table: its bytes read (a slice of the binary's, or one seek and read of the
    /// file) and checked against the index's length and checksum.
    fn entry(&self, i: usize) -> Option<&EmbedTable<'a>> {
        self.entries
            .get(i)?
            .get_or_init(|| {
                let (at, n) = (self.payload.checked_add(self.recs.get(4 * i) as usize)?, self.recs.get(4 * i + 1) as usize);
                let sum = (self.recs.get(4 * i + 3) as u64) << 32 | self.recs.get(4 * i + 2) as u64;
                let bytes: &'a [u8] = match &self.src {
                    Source::Bytes(b) => b.get(at..at.checked_add(n)?)?,
                    Source::File(f) => {
                        use std::io::{Read, Seek};
                        let mut f = f.lock().ok()?;
                        f.seek(std::io::SeekFrom::Start(at as u64)).ok()?;
                        let mut v = vec![0u8; n];
                        f.read_exact(&mut v).ok()?;
                        Box::leak(v.into_boxed_slice())
                    }
                };
                if crate::backend::backend_hash::hash64(SUM_SEED, bytes) != sum {
                    return None;
                }
                Blob::new(bytes).and_then(|b| EmbedTable::from_blob(&b))
            })
            .as_ref()
    }

    /// File `name` (`EmbedTable::read`'s names, `X.pfb#layout` and kin included), reading only
    /// its own entry.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let base = name.split_once('#').map_or(name, |(b, _)| b);
        self.entry(self.names.get(base.as_bytes())? as usize)?.read(name)
    }

    /// Every entry read and checked (installing a pack, generating one).
    pub fn sound(&self) -> bool {
        (0..self.len()).all(|i| self.entry(i).is_some())
    }
}

/// A pack's state in this process: read (its index), not installed, or installed in a format
/// or build this one does not read.
enum Slot {
    Ready(Pack<'static>),
    Absent,
    Stale,
}

/// One slot per pack of the catalog, filled on its first use.
fn slots() -> Option<&'static [OnceLock<Slot>]> {
    static LOADED: OnceLock<Vec<OnceLock<Slot>>> = OnceLock::new();
    let c = catalog()?;
    Some(LOADED.get_or_init(|| (0..c.packs()).map(|_| OnceLock::new()).collect()))
}

/// Pack `k`'s slot: from the binary (`fonts-full`) or the first directory that has its file,
/// only its index read; `Stale` when that file is not a pack of this format or its content id
/// is not the catalog's.
fn slot(k: usize) -> Option<&'static Slot> {
    let c = catalog()?;
    Some(slots()?.get(k)?.get_or_init(|| {
        let found = match carried_pack(c.name(k)) {
            Some(b) => Pack::from_bytes(b).map(Some).ok_or(()),
            None => {
                let file = c.file_name(k);
                pack_dirs().iter().map(|d| Pack::open(&d.join(&file))).find(|p| !matches!(p, Ok(None))).unwrap_or(Ok(None))
            }
        };
        match found {
            Ok(Some(p)) if p.content_id() == c.id(k) => Slot::Ready(p),
            Ok(None) => Slot::Absent,
            _ => Slot::Stale,
        }
    }))
}

/// Pack `k`, when it is installed (or carried) and of this build.
pub fn pack(k: usize) -> Option<&'static Pack<'static>> {
    match slot(k)? {
        Slot::Ready(p) => Some(p),
        _ => None,
    }
}

/// Whether pack `k` is installed but in a format or build this one does not read: an older
/// RushTeX's pack, to reinstall with `rushtex fonts install <pack>`.
pub fn stale(k: usize) -> bool {
    matches!(slot(k), Some(Slot::Stale))
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
        if let Some(Slot::Ready(p)) = slot.get() {
            if let Some(m) = p.read(&format!("{}.map", c.name(k))) {
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

/// Whether `bytes` are pack `k` of the catalog: its pinned SHA-256 (checked here, once, when it
/// is installed; a run checks only the entries it reads, against the index's checksums), a
/// pack of this format with the catalog's content id, and every entry sound.
fn verified(c: &Catalog, k: usize, bytes: &[u8]) -> bool {
    crate::backend::backend_hash::sha256(bytes) == c.sha256(k) && Pack::from_bytes(bytes).is_some_and(|p| p.content_id() == c.id(k) && p.sound())
}

/// The directory packs are installed into, made if needed.
fn install_dir() -> Result<&'static PathBuf, String> {
    let dest = pack_dirs().first().ok_or("no cache directory (set RUSHTEX_CACHE or HOME)")?;
    std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(dest)
}

/// Pack `k`'s file at `part` moved into place after checking it, else removed: written beside
/// its final name first, so a reader never sees half a pack.
fn install_checked(c: &Catalog, k: usize, part: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let bytes = std::fs::read(part).map_err(|e| format!("{}: {e}", part.display()))?;
    if !verified(c, k, &bytes) {
        let _ = std::fs::remove_file(part);
        return Err(format!("{}: not the pack this build expects (its SHA-256 is not the pinned one)", c.file_name(k)));
    }
    std::fs::rename(part, dest.join(c.file_name(k))).map_err(|e| format!("{}: {e}", c.file_name(k)))
}

/// Copies every pack of this build's catalog found in `from` (a directory) into the first
/// pack directory, after checking each one's SHA-256: `rushtex fonts install --from DIR`.
pub fn install_from(from: &std::path::Path) -> Result<(usize, usize), String> {
    let c = catalog().ok_or("this build has no pack catalog")?;
    let dest = install_dir()?;
    let (mut done, mut missing) = (0, 0);
    for k in 0..c.packs() {
        let file = c.file_name(k);
        let Ok(bytes) = std::fs::read(from.join(&file)) else {
            missing += 1;
            continue;
        };
        let part = dest.join(format!("{file}.{}.part", std::process::id()));
        std::fs::write(&part, &bytes).map_err(|e| format!("{}: {e}", part.display()))?;
        install_checked(c, k, &part, dest)?;
        done += 1;
    }
    Ok((done, missing))
}

/// Fetches pack `name` from the release `fonts-v<FONTS_RELEASE>` with the system's `curl` (no
/// HTTP code in RushTeX) and installs it once its SHA-256 is the pinned one: `rushtex fonts
/// install <pack>`. Returns where it was installed.
pub fn install(name: &str) -> Result<PathBuf, String> {
    let c = catalog().ok_or("this build has no pack catalog")?;
    let k = c.find(name).ok_or_else(|| format!("no font pack named {name} (rushtex fonts list names them)"))?;
    let dest = install_dir()?;
    let file = c.file_name(k);
    let url = format!("{}/{file}", release_url());
    let part = dest.join(format!("{file}.{}.part", std::process::id()));
    let status = std::process::Command::new("curl").args(["--fail", "--location", "--silent", "--show-error", "--retry", "2", "--output"]).arg(&part).arg(&url).status();
    match status {
        Ok(s) if s.success() => {}
        Ok(_) => {
            let _ = std::fs::remove_file(&part);
            return Err(format!("could not download {url}"));
        }
        Err(e) => return Err(format!("could not run curl ({e}); download {url} yourself and run rushtex fonts install --from <its folder>")),
    }
    install_checked(c, k, &part, dest)?;
    Ok(dest.join(file))
}

/// One line per pack: name, size, and where it is installed (or `not installed`).
pub fn list() -> Vec<String> {
    let Some(c) = catalog() else { return vec!["this build has no pack catalog".into()] };
    (0..c.packs())
        .map(|k| {
            let file = c.file_name(k);
            let at = pack_dirs().iter().map(|d| d.join(&file)).find(|p| p.exists());
            let at = if carried_pack(c.name(k)).is_some() {
                "in this binary".into()
            } else if stale(k) {
                format!("{} (another version's; reinstall with: rushtex fonts install {})", at.map_or_else(String::new, |p| p.display().to_string()), c.name(k))
            } else {
                at.map_or("not installed".into(), |p| p.display().to_string())
            };
            format!("{:<24} {:>8} KB  {at}", c.name(k), c.size(k) / 1024)
        })
        .collect()
}

// ---- Generation -------------------------------------------------------------------------

/// Writes the catalog: `packs` (name, content id, size, SHA-256) and `files` (basename, pack
/// index), file IDs in the order given.
#[cfg(any(test, feature = "gen"))]
pub fn write_catalog(w: &mut super::data_blob::BlobWriter, packs: &[(String, u64, u32, [u8; 32])], files: &[(String, u16)]) -> Result<(), String> {
    let names: Vec<&[u8]> = files.iter().map(|f| f.0.as_bytes()).collect();
    super::data_phf::write(w, "pkf", &names).map_err(|e| format!("{e:?}"))?;
    w.u16s("pk.f", &files.iter().map(|f| f.1).collect::<Vec<_>>());
    w.bytes("pk.n", packs.iter().flat_map(|p| format!("{}\n", p.0).into_bytes()).collect());
    w.u32s("pk.r", &packs.iter().flat_map(|p| [p.1 as u32, (p.1 >> 32) as u32, p.2]).collect::<Vec<_>>());
    w.bytes("pk.s", packs.iter().flat_map(|p| p.3).collect());
    Ok(())
}

/// A pack file of `files` (name, bytes): the header, the index, then per file (sorted by name)
/// a one-file `data_embed` table, a `.pfb` stored plain and ready to subset when `plain` says
/// so (else compressed), each read back and checked. Returns the bytes and the content id (of
/// the names and bytes, as `data_embed::write` makes it).
#[cfg(any(test, feature = "gen"))]
pub fn write_pack(files: &[(String, Vec<u8>)], plain: &dyn Fn(&str) -> bool) -> Result<(Vec<u8>, u64), String> {
    use super::data_blob::BlobWriter;
    use crate::backend::backend_hash::{hash64, ContentHasher};
    let mut files: Vec<&(String, Vec<u8>)> = files.iter().collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let (mut payload, mut recs, mut id) = (Vec::new(), Vec::new(), ContentHasher::new());
    for f in &files {
        id.bytes(f.0.as_bytes());
        id.bytes(&f.1);
        let mut w = BlobWriter::default();
        let one = [(f.0.clone(), f.1.clone())];
        super::data_embed::write(&mut w, &one, plain)?;
        let entry = w.finish();
        let table = Blob::new(&entry).and_then(|b| EmbedTable::from_blob(&b)).ok_or("not an embed table")?;
        super::data_embed::check(&table, &one)?;
        payload.resize(payload.len().next_multiple_of(8), 0);
        let sum = hash64(SUM_SEED, &entry);
        recs.extend_from_slice(&[payload.len() as u32, entry.len() as u32, sum as u32, (sum >> 32) as u32]);
        payload.extend_from_slice(&entry);
    }
    let id = id.finish() as u64;
    let mut w = BlobWriter::default();
    let names: Vec<&[u8]> = files.iter().map(|f| f.0.as_bytes()).collect();
    super::data_phf::write(&mut w, "pxf", &names).map_err(|e| format!("{e:?}"))?;
    w.u32s("px.r", &recs);
    w.u32s("px.id", &[id as u32, (id >> 32) as u32]);
    let index = w.finish();
    let sum = hash64(SUM_SEED, &index);
    let mut out = Vec::with_capacity(HEAD + index.len() + payload.len() + 8);
    out.extend_from_slice(PACK_MAGIC);
    for word in [PACK_FORMAT, index.len() as u32, sum as u32, (sum >> 32) as u32, 0] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&index);
    out.resize(out.len().next_multiple_of(8), 0);
    out.extend_from_slice(&payload);
    // Read back through the reader: every file, through its own entry.
    let p = Pack::from_bytes(&out).ok_or("the pack does not read back")?;
    if p.content_id() != id || p.len() != files.len() || files.iter().any(|f| p.read(&f.0).as_deref() != Some(f.1.as_slice())) {
        return Err("a file of the pack reads back differently".into());
    }
    Ok((out, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_reads_back() {
        let mut w = super::super::data_blob::BlobWriter::default();
        let packs = [("a".to_string(), 0x1234_5678_9abc_def0, 10, [1; 32]), ("b".to_string(), 7, 20, [2; 32])];
        let files = [("x.pfb".to_string(), 1), ("y.tfm".to_string(), 0), ("z.enc".to_string(), 1)];
        write_catalog(&mut w, &packs, &files).unwrap();
        let bytes = w.finish();
        let c = Catalog::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        assert_eq!((c.packs(), c.name(1), c.id(0), c.size(1)), (2, "b", 0x1234_5678_9abc_def0, 20));
        assert_eq!((c.pack_of("x.pfb#layout"), c.pack_of("y.tfm"), c.pack_of("w.pfb")), (Some(1), Some(0), None));
        assert_eq!((c.file_name(0), c.sha256(1), c.find("b")), ("a.rtpack".to_string(), &[2; 32][..], Some(1)));
    }

    /// A pack reads back every file through its own entry, in both forms of its programs; a
    /// truncated or altered entry, a format-1 pack (a whole `data_embed` table) and a damaged
    /// index are refused.
    #[test]
    fn pack_reads_back_by_entry() {
        let pfb = super::super::data_embed::tests::pfb();
        let files = vec![("b.tfm".to_string(), vec![7u8; 300]), ("a.enc".to_string(), b"/E [ /a ] def\n".to_vec()), ("x.pfb".to_string(), pfb.clone()), ("p.map".to_string(), b"x X <x.pfb\n".to_vec())];
        for plain in [true, false] {
            let (bytes, id) = write_pack(&files, &|_| plain).unwrap();
            let p = Pack::from_bytes(&bytes).unwrap();
            assert!(p.sound() && p.content_id() == id && p.len() == 4);
            assert_eq!(p.read("b.tfm").as_deref(), Some(&[7u8; 300][..]));
            assert_eq!(p.read("x.pfb").as_deref(), Some(&pfb[..]));
            assert!(p.read("x.pfb#head").is_some_and(|h| pfb.starts_with(&h)));
            assert_eq!(p.read("missing.tfm"), None);
            let mut bad = bytes.clone();
            let last = bad.len() - 1;
            bad[last] ^= 1;
            let q = Pack::from_bytes(&bad).unwrap();
            assert!(!q.sound() && q.read("a.enc").is_some(), "only the altered entry is refused");
            let mut bad = bytes.clone();
            bad[HEAD + 1] ^= 1;
            assert!(Pack::from_bytes(&bad).is_none(), "a damaged index");
        }
        let mut w = super::super::data_blob::BlobWriter::default();
        super::super::data_embed::write(&mut w, &files, &|_| true).unwrap();
        assert!(Pack::from_bytes(&w.finish()).is_none(), "a format-1 pack");
    }

    /// The catalog in the binary reads, and maps every file to a pack it lists.
    #[test]
    fn built_in_catalog_is_sound() {
        let c = catalog().expect("assets/tables/packs.rtdb");
        assert!(c.packs() > 0);
    }

    /// The `fonts-full` build carries every pack of the catalog, each the pinned file.
    #[cfg(feature = "fonts-full")]
    #[test]
    fn full_build_carries_every_pack() {
        let c = catalog().expect("assets/tables/packs.rtdb");
        assert_eq!(FULL.len(), c.packs());
        for k in 0..c.packs() {
            assert!(carried_pack(c.name(k)).is_some_and(|b| verified(c, k, b)), "{}", c.name(k));
        }
    }
}
