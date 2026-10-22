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
        let lines = cache.and_then(|c| std::fs::read_to_string(c).ok()).filter(|s| s.lines().next() == Some(key.as_str())).map(|s| s.lines().skip(1).map(String::from).collect::<Vec<_>>());
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
        let mut db = Index::default();
        let mut db_roots = Vec::new();
        for root in lines[FORMATS.len()].split(':').map(|r| r.trim_start_matches("!!").trim_end_matches('/')).filter(|r| !r.is_empty()) {
            let text = std::fs::read(format!("{root}/ls-R")).or_else(|_| std::fs::read(format!("{root}/ls-r")));
            if let Ok(text) = text {
                db.read_ls_r(root.as_bytes(), &text);
                db_roots.push(root.as_bytes().to_vec());
            }
        }
        db.seal();
        Some(Kpse { paths, db_roots, db, disk: Default::default(), doc_dir, found: Default::default() })
    }

    /// The file `name`, as the engine names it (`\input`, `\font`, the font map), read
    /// whole; `None` when kpathsea would not find it. Each name is looked up once.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
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
