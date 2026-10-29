//! What a run needs from the host system: TeX's files, found as kpathsea finds them, and
//! the run's date. The engine itself has no file system (it builds for WASM too); the
//! command line and the `format_smoke` harness hand it `Kpse::read`.
//!
//! kpathsea's rules, from `pathsearch.c`, `db.c` and `tex-file.c`, in our representation:
//! one `kpsewhich -show-path` per file format gives the search path (spawned together,
//! once, and cached on disk), and the `ls-R` databases are read into one flat array of
//! (name hash, directory) sorted by hash. A lookup is a binary search and a `match` of the
//! directory against each path element, with no process per file (a `kpsewhich` costs
//! about 25 ms, most of a small document's run).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The file formats the engine reads, by kpathsea's `-show-path` name. The engine names a
/// file and the suffix picks the format, as `kpsewhich` does; every other suffix is
/// `\input`'s and `\openin`'s `kpse_tex_format`.
const FORMATS: [&str; 9] = ["tex", "tfm", "vf", "map", "enc files", "type1 fonts", "opentype fonts", "truetype fonts", "pk"];

/// tex-file.c: `SUFFIXES (".tex")` and `ALT_TEX_SUFFIXES`. A name ending in one of these is
/// looked up as written; any other is tried with `.tex` appended first
/// (`try_std_extension_first`).
const TEX_SUFFIXES: [&str; 9] = [".tex", ".sty", ".cls", ".fd", ".aux", ".bbl", ".def", ".clo", ".ldf"];

fn format_of(name: &str) -> usize {
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    match ext {
        "tfm" => 1,
        "vf" => 2,
        "map" => 3,
        "enc" => 4,
        "pfb" | "pfa" => 5,
        "otf" | "OTF" => 6,
        "ttf" | "ttc" | "TTF" | "TTC" | "dfont" => 7,
        e if e.ends_with("pk") && e.len() > 2 && e[..e.len() - 2].bytes().all(|b| b.is_ascii_digit()) => 8,
        _ => 0,
    }
}

/// A reader over the saved index's bytes.
struct Bytes<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn len(&mut self) -> Option<usize> {
        usize::try_from(u64::from_le_bytes(self.take(8)?.try_into().ok()?)).ok()
    }

    /// A length, then that many bytes.
    fn part(&mut self) -> Option<&'a [u8]> {
        let n = self.len()?;
        self.take(n)
    }
}

/// The names `data_embed` gives the stored forms of a file (`EmbedTable::read`).
const STORED_FORMS: [&str; 4] = ["#clear", "#layout", "#names", "#head"];

/// The saved index's first bytes (`Index::save`); a new layout takes a new number.
const INDEX_MAGIC: &[u8] = b"RTKPSEI1";

/// FNV-1a: the name hash of the flat index.
fn hash(s: &[u8]) -> u64 {
    s.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Files by name: `dirs` are absolute directories with a trailing `/`, all in `text`;
/// `files` is (name hash, directory, name start, name length), sorted by hash with the
/// order read kept among equal hashes (kpathsea's hash buckets append).
#[derive(Default)]
struct Index {
    text: Vec<u8>,
    dirs: Vec<(u32, u32)>,
    files: Vec<(u64, u32, u32, u32)>,
}

impl Index {
    fn dir(&mut self, d: &[u8]) {
        let at = self.text.len() as u32;
        self.text.extend_from_slice(d);
        if !d.ends_with(b"/") {
            self.text.push(b'/');
        }
        self.dirs.push((at, self.text.len() as u32 - at));
    }

    fn file(&mut self, name: &[u8]) {
        let Some(d) = self.dirs.len().checked_sub(1) else { return };
        let at = self.text.len() as u32;
        self.text.extend_from_slice(name);
        self.files.push((hash(name), d as u32, at, name.len() as u32));
    }

    fn seal(&mut self) {
        self.files.sort_by_key(|f| f.0);
        self.text.shrink_to_fit();
    }

    /// What the saved index depends on: each root's database path, size and modification
    /// time (nanoseconds), one line per root.
    fn stamp(roots: &[&str]) -> Vec<u8> {
        let mut s = Vec::new();
        for root in roots {
            for name in ["ls-R", "ls-r"] {
                let path = format!("{root}/{name}");
                if let Ok(m) = std::fs::metadata(&path) {
                    let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                    s.extend_from_slice(format!("{path}\t{}\t{t}\n", m.len()).as_bytes());
                    break;
                }
            }
        }
        s
    }

    /// Saved as: magic, the stamp, the roots with a database, then `text`, `dirs` and
    /// `files` as little-endian arrays, each behind its length. Written beside its final
    /// name and renamed, so a reader never sees half of it.
    fn save(&self, path: &Path, stamp: &[u8], roots: &[Vec<u8>]) {
        let mut out = Vec::with_capacity(self.text.len() + self.dirs.len() * 8 + self.files.len() * 20 + stamp.len() + 64);
        let len = |out: &mut Vec<u8>, n: usize| out.extend_from_slice(&(n as u64).to_le_bytes());
        out.extend_from_slice(INDEX_MAGIC);
        len(&mut out, stamp.len());
        out.extend_from_slice(stamp);
        let roots = roots.join(&b'\n');
        len(&mut out, roots.len());
        out.extend_from_slice(&roots);
        len(&mut out, self.text.len());
        out.extend_from_slice(&self.text);
        len(&mut out, self.dirs.len());
        for &(a, b) in &self.dirs {
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        len(&mut out, self.files.len());
        for &(h, d, a, n) in &self.files {
            out.extend_from_slice(&h.to_le_bytes());
            for v in [d, a, n] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let tmp = path.with_extension("part");
        if std::fs::write(&tmp, &out).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// The index saved at `path` when its stamp is `stamp`, with the roots that had a
    /// database; `None` when it is missing, of another stamp, or not sound.
    fn load(path: &Path, stamp: &[u8]) -> Option<(Index, Vec<Vec<u8>>)> {
        let b = std::fs::read(path).ok()?;
        if b.get(..INDEX_MAGIC.len())? != INDEX_MAGIC {
            return None;
        }
        let mut r = Bytes { b: &b, at: INDEX_MAGIC.len() };
        if r.part()? != stamp {
            return None;
        }
        let roots: Vec<Vec<u8>> = r.part()?.split(|&c| c == b'\n').filter(|r| !r.is_empty()).map(<[u8]>::to_vec).collect();
        let text = r.part()?.to_vec();
        let u32_at = |s: &[u8], i: usize| u32::from_le_bytes([s[i], s[i + 1], s[i + 2], s[i + 3]]);
        let n = r.len()?;
        let raw = r.take(n.checked_mul(8)?)?;
        let dirs: Vec<(u32, u32)> = raw.as_chunks::<8>().0.iter().map(|c| (u32_at(c, 0), u32_at(c, 4))).collect();
        let n = r.len()?;
        let raw = r.take(n.checked_mul(20)?)?;
        let files: Vec<(u64, u32, u32, u32)> = raw.as_chunks::<20>().0.iter().map(|c| (u64::from_le_bytes(c[..8].try_into().unwrap()), u32_at(c, 8), u32_at(c, 12), u32_at(c, 16))).collect();
        let sound = dirs.iter().all(|&(a, l)| (a as usize).checked_add(l as usize).is_some_and(|e| e <= text.len())) && files.iter().all(|&(_, d, a, l)| (d as usize) < dirs.len() && (a as usize).checked_add(l as usize).is_some_and(|e| e <= text.len()));
        sound.then_some((Index { text, dirs, files }, roots))
    }

    /// db.c `kpathsea_db_search`: each directory holding `base` whose path matches `elt`,
    /// in the order read.
    fn each(&self, base: &[u8], elt: &[u8], hit: &mut dyn FnMut(&[u8]) -> bool) -> bool {
        let h = hash(base);
        let from = self.files.partition_point(|f| f.0 < h);
        let mut full = Vec::new();
        for &(fh, d, at, len) in &self.files[from..] {
            if fh != h {
                break;
            }
            if &self.text[at as usize..(at + len) as usize] != base {
                continue;
            }
            let (da, dl) = self.dirs[d as usize];
            full.clear();
            full.extend_from_slice(&self.text[da as usize..(da + dl) as usize]);
            full.extend_from_slice(base);
            if matches(&full, elt) && hit(&full) {
                return true;
            }
        }
        false
    }

    /// `ls-R`: `dir:` lines (relative to the database's own directory when they start with
    /// `./`), then the names in it; blank lines between.
    fn read_ls_r(&mut self, root: &[u8], text: &[u8]) {
        let mut have_dir = false;
        for line in text.split(|&b| b == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.is_empty() || line[0] == b'%' {
                continue;
            }
            if let Some(d) = line.strip_suffix(b":") {
                let mut abs = Vec::with_capacity(root.len() + d.len() + 1);
                if let Some(rel) = d.strip_prefix(b"./").or(if d == b"." { Some(&b""[..]) } else { None }) {
                    abs.extend_from_slice(root);
                    abs.push(b'/');
                    abs.extend_from_slice(rel);
                } else {
                    abs.extend_from_slice(d);
                }
                self.dir(&abs);
                have_dir = true;
            } else if have_dir && line != b"." && line != b".." {
                self.file(line);
            }
        }
    }

    /// kpathsea's disk search of a `//` element (elt-dirs.c `do_subdir`): the directory and
    /// every directory under it, in `readdir` order, each before its subdirectories.
    fn walk(&mut self, dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        self.dir(dir.as_os_str().as_encoded_bytes());
        let mut subs = Vec::new();
        for ent in rd.flatten() {
            let name = ent.file_name();
            let Ok(ft) = ent.file_type() else { continue };
            if ft.is_dir() {
                subs.push(ent.path());
            } else {
                self.file(name.as_encoded_bytes());
            }
        }
        for s in subs {
            self.walk(&s);
        }
    }
}

/// db.c `match`: whether file path `f` lies in path element `elt`, where `//` stands for any
/// number of directories and the file must be directly in the last directory named.
fn matches(f: &[u8], elt: &[u8]) -> bool {
    match_at(f, 0, elt, 0)
}

/// `match` from `f[start..]` and `elt[j..]`; as in C, the characters before both starts are
/// still seen (`path_elt[-1]`), and `start` is the recursion's `original_filename`.
fn match_at(f: &[u8], start: usize, elt: &[u8], j: usize) -> bool {
    let (mut i, mut j) = (start, j);
    while i < f.len() && j < elt.len() {
        if f[i] == elt[j] {
            i += 1;
            j += 1;
            continue;
        }
        if elt[j] == b'/' && start < i && j > 0 && elt[j - 1] == b'/' {
            while j < elt.len() && elt[j] == b'/' {
                j += 1;
            }
            if j == elt.len() {
                return true;
            }
            // An intermediate `//`: the rest of `elt` at each later directory of `f`.
            while i < f.len() {
                if f[i - 1] == b'/' && f[i] == elt[j] && match_at(f, i, elt, j) {
                    return true;
                }
                i += 1;
            }
        }
        return false;
    }
    if j < elt.len() {
        return false;
    }
    if i < f.len() && f[i] == b'/' {
        i += 1;
    }
    (i == start || f[i - 1] == b'/') && !f[i..].contains(&b'/')
}

/// One search path element: its text with `!!` removed and whether it may search the disk.
struct Elt {
    path: Vec<u8>,
    disk: bool,
}

/// kpathsea's search, ready to look files up.
pub struct Kpse {
    paths: [Vec<Elt>; 9],
    db_roots: Vec<Vec<u8>>,
    db: Index,
    /// Disk indexes of the `//` elements no database covers, made when first searched.
    disk: RefCell<HashMap<Vec<u8>, Index>>,
    /// The document's own directory, searched first: pdfLaTeX runs in it.
    doc_dir: Option<PathBuf>,
    /// Where each name was found, so a name is searched once; the file itself is read
    /// again when asked for (from the OS cache), never kept here.
    found: RefCell<HashMap<String, Option<PathBuf>>>,
}

/// The TeX Live the zero-shift references, the generated tables and CI are pinned to
/// (`scripts/texlive/manifest.tsv`, `progress.md` A1): exactness is proven against it only.
pub const PINNED_TEXLIVE: &str = "2026";
/// The pinned LaTeX kernel: `\fmtversion` and `\patch@level` of its `latex.ltx`.
pub const PINNED_LATEX: (&str, u32) = ("2025-11-01", 0);

/// `\fmtversion` and `\patch@level` as `latex.ltx` (`ltvers.dtx`) defines them.
pub fn latex_version(latex_ltx: &[u8]) -> Option<(String, u32)> {
    // Once per format build: the file is not necessarily UTF-8, and this path is cold.
    let text = String::from_utf8_lossy(latex_ltx);
    let rest = &text[text.find("\\edef\\fmtversion")?..];
    let date = rest[rest.find('{')? + 1..].split('}').next()?.trim().to_string();
    const PL: &str = "\\def\\patch@level{";
    let pl = rest.find(PL).map(|i| &rest[i + PL.len()..]).and_then(|r| r.split('}').next()).and_then(|n| n.trim().parse().ok())?;
    Some((date, pl))
}

/// The file at `p` and its path, when it can be read.
fn read_at(p: &Path) -> Option<(PathBuf, Vec<u8>)> {
    std::fs::read(p).ok().map(|b| (p.to_path_buf(), b))
}

impl Kpse {
    /// The search paths of `progname` (`pdflatex`), from `cache` when it is still valid,
    /// else from `kpsewhich` (all formats at once) and written to `cache`. `None` without
    /// a TeX installation.
    pub fn new(progname: &str, cache: Option<&Path>, doc_dir: Option<PathBuf>) -> Option<Kpse> {
        let key = env_key(progname);
        // `RUSHTEX_KPSE_PATHS`: a search-path file used whatever the environment, so a pinned
        // snapshot of the TeX tree runs where no `kpsewhich` is installed (CI).
        let forced = std::env::var_os("RUSHTEX_KPSE_PATHS").and_then(|f| std::fs::read_to_string(f).ok());
        let lines = forced
            .map(|s| s.lines().skip(1).map(String::from).collect::<Vec<_>>())
            .or_else(|| cache.and_then(|c| std::fs::read_to_string(c).ok()).filter(|s| s.lines().next() == Some(key.as_str())).map(|s| s.lines().skip(1).map(String::from).collect::<Vec<_>>()));
        let lines = match lines {
            Some(l) if l.len() == FORMATS.len() + 1 => l,
            _ => {
                let l = ask_kpsewhich(progname)?;
                if let Some(c) = cache {
                    if let Some(d) = c.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    let _ = std::fs::write(c, format!("{key}\n{}\n", l.join("\n")));
                }
                l
            }
        };
        let elts = |s: &str| -> Vec<Elt> {
            s.split(':').filter(|e| !e.is_empty()).map(|e| match e.strip_prefix("!!") {
                Some(p) => Elt { path: p.as_bytes().to_vec(), disk: false },
                None => Elt { path: e.as_bytes().to_vec(), disk: true },
            }).collect()
        };
        let paths: [Vec<Elt>; 9] = std::array::from_fn(|i| elts(&lines[i]));
        // The `ls-R` databases, indexed once and kept beside the search paths
        // (`kpse-index`), valid while every database keeps its size and modification time.
        let roots: Vec<&str> = lines[FORMATS.len()].split(':').map(|r| r.trim_start_matches("!!").trim_end_matches('/')).filter(|r| !r.is_empty()).collect();
        let stamp = Index::stamp(&roots);
        let saved = cache.map(|c| c.with_file_name("kpse-index"));
        let (db, db_roots) = match saved.as_ref().and_then(|p| Index::load(p, &stamp)) {
            Some(loaded) => loaded,
            None => {
                let mut db = Index::default();
                let mut db_roots = Vec::new();
                for root in &roots {
                    let text = std::fs::read(format!("{root}/ls-R")).or_else(|_| std::fs::read(format!("{root}/ls-r")));
                    if let Ok(text) = text {
                        db.read_ls_r(root.as_bytes(), &text);
                        db_roots.push(root.as_bytes().to_vec());
                    }
                }
                db.seal();
                if let Some(p) = &saved {
                    db.save(p, &stamp, &db_roots);
                }
                (db, db_roots)
            }
        };
        Some(Kpse { paths, db_roots, db, disk: Default::default(), doc_dir, found: Default::default() })
    }

    /// The file `name`, as the engine names it (`\input`, `\font`, the font map), read
    /// whole; `None` when kpathsea would not find it. Each name is looked up once.
    /// Every file found so far, by path, sorted (`RT_FILES`, the pin's file list).
    pub fn found_paths(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = self.found.borrow().values().flatten().cloned().collect();
        v.sort();
        v.dedup();
        v
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        // The files carried in the binary (`data_embed`): where the disk has none, and first
        // under `RT_EMBED_FIRST` (the gate that proves them equal to the disk's).
        let embedded = || crate::data::data_embed::embedded().and_then(|t| t.read(name));
        if std::env::var_os("RT_EMBED_FIRST").is_some() {
            if let Some(b) = embedded() {
                return Some(b);
            }
        }
        // Then an installed font pack (`data_pack`, native-architecture.md §4b).
        self.read_disk(name).or_else(embedded).or_else(|| crate::data::data_pack::read(name))
    }

    fn read_disk(&self, name: &str) -> Option<Vec<u8>> {
        // The stored forms of the binary's files (`X.pfb#clear`, `data_embed`) are never
        // files on disk; a search for one would walk every directory without an `ls-R`.
        if STORED_FORMS.iter().any(|f| name.ends_with(f)) {
            return None;
        }
        if let Some(p) = self.found.borrow().get(name) {
            return p.as_ref().and_then(|p| std::fs::read(p).ok());
        }
        let v = self.find(name);
        self.found.borrow_mut().insert(name.to_string(), v.as_ref().map(|v| v.0.clone()));
        v.map(|v| v.1)
    }

    fn find(&self, name: &str) -> Option<(PathBuf, Vec<u8>)> {
        if let Some(b) = self.doc_dir.as_ref().and_then(|d| read_at(&d.join(name))) {
            return Some(b);
        }
        let format = format_of(name);
        let plain = format != 0 || TEX_SUFFIXES.iter().any(|s| name.ends_with(s));
        let suffixed = format!("{name}.tex");
        let targets: &[&str] = if plain { &[name] } else { &[&suffixed, name] };
        // An absolute or explicitly relative name is read as written (kpathsea
        // `kpathsea_absolute_p`).
        if name.starts_with('/') || name.starts_with("./") || name.starts_with("../") {
            return targets.iter().find_map(|t| read_at(Path::new(t)));
        }
        // pathsearch.c `search_list`: each element in turn, every target name in it.
        for elt in &self.paths[format] {
            for t in targets {
                if let Some(b) = self.search(elt, t) {
                    return Some(b);
                }
            }
        }
        None
    }

    /// pathsearch.c `path_search` of one element: the database when one covers the
    /// element, else the disk when the element allows it.
    fn search(&self, elt: &Elt, name: &str) -> Option<(PathBuf, Vec<u8>)> {
        // db.c: a name with directories (`sub/file.tex`) is its last part, looked up in
        // the element extended by the directories.
        let (dirs, base) = name.rsplit_once('/').map_or(("", name), |(d, b)| (d, b));
        let mut path = elt.path.clone();
        if !dirs.is_empty() {
            path.push(b'/');
            path.extend_from_slice(dirs.as_bytes());
        }
        let mut got = None;
        let mut read = |f: &[u8]| -> bool {
            got = std::str::from_utf8(f).ok().and_then(|f| read_at(Path::new(f)));
            got.is_some()
        };
        if self.db_roots.iter().any(|r| path.starts_with(r) && path.get(r.len()).is_none_or(|&c| c == b'/')) {
            self.db.each(base.as_bytes(), &path, &mut read);
            return got;
        }
        if !elt.disk {
            return None;
        }
        // elt-dirs.c: an element without `//` is that one directory; with it, the walk of
        // its root (the part before the first `//`).
        let Some(cut) = elt.path.windows(2).position(|w| w == b"//") else {
            let mut f = elt.path.clone();
            if !f.ends_with(b"/") {
                f.push(b'/');
            }
            f.extend_from_slice(name.as_bytes());
            return std::str::from_utf8(&f).ok().and_then(|f| read_at(Path::new(f)));
        };
        let root = elt.path[..cut].to_vec();
        let mut disk = self.disk.borrow_mut();
        let ix = disk.entry(root).or_insert_with_key(|root| {
            let mut ix = Index::default();
            if let Ok(r) = std::str::from_utf8(root) {
                ix.walk(Path::new(r));
            }
            ix.seal();
            ix
        });
        ix.each(base.as_bytes(), &path, &mut read);
        got
    }
}

/// What the cached paths depend on: the program name, `PATH` (which TeX installation runs)
/// and every variable kpathsea reads for a search path.
fn env_key(progname: &str) -> String {
    let mut vars: Vec<(String, String)> = std::env::vars().filter(|(k, _)| k == "PATH" || k.starts_with("TEXMF") || k.ends_with("INPUTS") || k.ends_with("FONTS") || k.ends_with("FONTMAPS") || k == "TEXPKS" || k == "TEXMFDBS").collect();
    vars.sort();
    let mut s = String::from(progname);
    for (k, v) in vars {
        s.push('\0');
        s.push_str(&k);
        s.push('=');
        s.push_str(&v);
    }
    format!("rushtex-kpse 1 {:016x}", hash(s.as_bytes()))
}

/// One `kpsewhich -show-path` per format and the `ls-R` roots, all spawned before any is
/// waited for.
fn ask_kpsewhich(progname: &str) -> Option<Vec<String>> {
    let prog = format!("-progname={progname}");
    let mut kids = Vec::new();
    for f in FORMATS {
        kids.push(std::process::Command::new("kpsewhich").args([prog.as_str(), &format!("-show-path={f}")]).stdout(std::process::Stdio::piped()).spawn().ok()?);
    }
    kids.push(std::process::Command::new("kpsewhich").args([prog.as_str(), "-expand-braces=$TEXMFDBS"]).stdout(std::process::Stdio::piped()).spawn().ok()?);
    kids.into_iter().map(|k| k.wait_with_output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_string())).collect()
}

/// The run's date: `SOURCE_DATE_EPOCH` when `FORCE_SOURCE_DATE` is 1, as web2c, else now.
pub fn run_date() -> i64 {
    let forced = std::env::var("FORCE_SOURCE_DATE").is_ok_and(|v| v == "1");
    std::env::var("SOURCE_DATE_EPOCH").ok().filter(|_| forced).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64))
}

#[cfg(test)]
mod tests {
    #[test]
    fn latex_version_of_ltvers() {
        let src = b"\\def\\fmtname{LaTeX2e}\n\\edef\\fmtversion\n   {2025-11-01}\n\\def\\patch@level{0}\n";
        assert_eq!(super::latex_version(src), Some(("2025-11-01".to_string(), 0)));
    }

    use super::*;

    #[test]
    fn kpathsea_match() {
        let m = |f: &str, e: &str| matches(f.as_bytes(), e.as_bytes());
        assert!(m("/t/tex/latex/base/article.cls", "/t/tex/latex//"));
        assert!(m("/t/tex/latex/article.cls", "/t/tex/latex//"));
        assert!(!m("/t/tex/generic/x.tex", "/t/tex/latex//"));
        assert!(m("/t/fonts/tfm/public/cm/cmr10.tfm", "/t/fonts//tfm//"));
        assert!(m("/t/vf/ptmr.vf", "/t/vf"));
        assert!(!m("/t/vf/sub/ptmr.vf", "/t/vf"));
        assert!(m("/t/a/b/c/x.sty", "/t//c"));
        assert!(!m("/t/a/b/c/d/x.sty", "/t//c"));
    }

    #[test]
    fn ls_r_and_formats() {
        let mut ix = Index::default();
        ix.read_ls_r(b"/r", b"% ls-R\n./tex/a:\nx.sty\nb\n\n./tex/a/b:\nx.sty\n");
        ix.seal();
        let mut seen = Vec::new();
        ix.each(b"x.sty", b"/r/tex//", &mut |f| {
            seen.push(String::from_utf8(f.to_vec()).unwrap());
            false
        });
        assert_eq!(seen, ["/r/tex/a/x.sty", "/r/tex/a/b/x.sty"]);
        assert_eq!((format_of("cmr10.tfm"), format_of("cmr10.600pk"), format_of("foo.pgf"), format_of("x.pk")), (1, 8, 0, 0));
    }
}
