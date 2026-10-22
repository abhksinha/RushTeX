//! The package registry as a static table (pillar 2, D2): every pinned `.sty` and `.cls`,
//! loaded once by LaTeX itself at generation (`rushtex-gen pkgs`). For each it records the
//! files it loads directly, the options it declares and the control sequences it defines.
//!
//! What it serves:
//! - **The pre-scan (D3).** A command name of the document gives, through one perfect hash,
//!   the packages that define it. Each package gives the files it loads, whose closure the
//!   pre-scan takes (`Csr::closure`).
//! - **The fallback decision (F1).** A package outside the registry, or outside the native
//!   set, sends the document to the engine.
//!
//! Layout:
//! - **Files** (`pkf`): every file any registered file loads, by name (`amsmath.sty`,
//!   `t1cmr.fd`). IDs give the registered packages and classes first, in name order.
//! - **Edges** (`pk.ld`, CSR): file ID to the file IDs it loads directly, in load order.
//! - **Options** (`pk.op`, CSR): package ID to option string IDs, in declaration order.
//! - **Commands** (`pkc`): every control sequence a registered file defines, by name.
//!   `pk.by` (CSR) gives the package IDs defining it, in ID order.
//! - **Strings** (`pk.str`, offsets `pk.so`): option names, each stored once.
//!
//! Edges and options are read for one package at a time; the command index is read once per
//! distinct name in a document. Each answer is one contiguous row.

use super::data_blob::{Blob, U32s};
use super::data_csr::Csr;
use super::data_phf::Phf;

/// The file the registry run writes.
pub const LOAD_FILE: &str = "rtpkgs";

/// The package registry, read in place.
pub struct PkgTable<'a> {
    files: Phf<'a>,
    registered: u32,
    loads: Csr<'a>,
    options: Csr<'a>,
    commands: Phf<'a>,
    defined_by: Csr<'a>,
    strings: &'a [u8],
    offsets: U32s<'a>,
    id: u64,
}

impl<'a> PkgTable<'a> {
    /// The table in `b`, every index checked once.
    pub fn from_blob(b: &Blob<'a>) -> Option<PkgTable<'a>> {
        let (files, commands) = (Phf::from_blob(b, "pkf")?, Phf::from_blob(b, "pkc")?);
        let (loads, options, defined_by) = (Csr::from_blob(b, "pk.ld")?, Csr::from_blob(b, "pk.op")?, Csr::from_blob(b, "pk.by")?);
        let (strings, offsets, head) = (b.section("pk.str")?, b.u32s("pk.so")?, b.u32s("pk.id")?);
        let registered = head.try_get(2)?;
        let n_str = offsets.len().checked_sub(1)? as u32;
        let rows_in = |c: &Csr, rows: usize, max: u32| c.len() == rows && (0..rows as u32).all(|r| c.row(r).all(|v| v < max));
        let sound = head.len() == 3
            && registered as usize <= files.len()
            && rows_in(&loads, files.len(), files.len() as u32)
            && rows_in(&options, registered as usize, n_str)
            && rows_in(&defined_by, commands.len(), registered)
            && offsets.iter().zip(offsets.iter().skip(1)).all(|(a, b)| a <= b)
            && offsets.get(n_str as usize) as usize == strings.len()
            && std::str::from_utf8(strings).is_ok();
        sound.then(|| PkgTable { files, registered, loads, options, commands, defined_by, strings, offsets, id: (head.get(1) as u64) << 32 | head.get(0) as u64 })
    }

    pub fn content_id(&self) -> u64 {
        self.id
    }

    /// The ID of file `name` (`amsmath.sty`); below `registered()` it is a registered package
    /// or class.
    #[inline]
    pub fn file(&self, name: &str) -> Option<u32> {
        self.files.get(name.as_bytes())
    }

    /// The number of registered packages and classes.
    pub fn registered(&self) -> u32 {
        self.registered
    }

    pub fn file_name(&self, id: u32) -> &'a str {
        std::str::from_utf8(self.files.name(id)).unwrap_or("")
    }

    /// The files `id` loads directly, in load order.
    pub fn loads(&self, id: u32) -> U32s<'a> {
        self.loads.row_slice(id)
    }

    /// The loads relation, for closures over a document's packages.
    pub fn load_graph(&self) -> &Csr<'a> {
        &self.loads
    }

    /// The options package `id` declares (`\DeclareOption`), in order.
    pub fn options(&self, id: u32) -> impl Iterator<Item = &'a str> + '_ {
        let row = if id < self.registered { self.options.row_slice(id) } else { U32s::default() };
        row.iter().map(|s| self.string(s))
    }

    /// The registered packages that define the control sequence `name` (no backslash).
    pub fn defined_by(&self, name: &str) -> U32s<'a> {
        self.commands.get(name.as_bytes()).map_or_else(U32s::default, |c| self.defined_by.row_slice(c))
    }

    #[inline]
    fn string(&self, s: u32) -> &'a str {
        let (a, b) = (self.offsets.get(s as usize) as usize, self.offsets.get(s as usize + 1) as usize);
        std::str::from_utf8(&self.strings[a..b]).unwrap_or("")
    }
}

static PKGS_BLOB: &[u8] = include_bytes!("../../assets/tables/pkgs.rtdb");

/// The embedded package registry, checked on first use.
pub fn pkgs() -> Option<&'static PkgTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<PkgTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(PKGS_BLOB).and_then(|b| PkgTable::from_blob(&b))).as_ref()
}

// ---- Generation -------------------------------------------------------------------------

/// One registered file as the generator records it.
#[cfg(any(test, feature = "gen"))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenPkg {
    pub name: String,
    /// Files it loads directly, in order.
    pub loads: Vec<String>,
    pub options: Vec<String>,
    /// Control sequences its own loading defines or changes, sorted.
    pub defines: Vec<String>,
}

/// The source that loads `file` (a `.sty` on `article`, or a `.cls` as the class) with the
/// load hooks recording, in `rtpkgs`:
/// - `B <file>` and `A <file>` around every file read (`file/before`, `file/after`), so the
///   nesting gives the direct loads;
/// - `O <package> <option>` for each `\DeclareOption` (`\@declareoption`).
#[cfg(any(test, feature = "gen"))]
pub fn load_source(file: &str) -> String {
    let (stem, cls) = match file.strip_suffix(".cls") {
        Some(s) => (s, true),
        None => (file.strip_suffix(".sty").unwrap_or(file), false),
    };
    let mut s = String::from(concat!(
        "\\makeatletter\\newwrite\\rt@out\\immediate\\openout\\rt@out=rtpkgs\n",
        "\\AddToHook{file/before}{\\immediate\\write\\rt@out{B \\CurrentFile}}\n",
        "\\AddToHook{file/after}{\\immediate\\write\\rt@out{A \\CurrentFile}}\n",
        "\\let\\rt@decl\\@declareoption\n",
        "\\long\\def\\@declareoption#1#2{\\immediate\\write\\rt@out{O \\@currname.\\@currext\\space\\detokenize{#1}}\\rt@decl{#1}{#2}}\n",
        "\\makeatother\n",
    ));
    if cls {
        s.push_str(&format!("\\documentclass{{{stem}}}\n"));
    } else {
        s.push_str(&format!("\\documentclass{{article}}\\usepackage{{{stem}}}\n"));
    }
    s.push_str("\\makeatletter\\immediate\\closeout\\rt@out\\csname @@end\\endcsname\n");
    s
}

/// The baseline every package's definitions are measured against: the same run without it.
#[cfg(any(test, feature = "gen"))]
pub fn base_source(cls: bool) -> String {
    let mut s = load_source("article.cls");
    if !cls {
        return s;
    }
    // A class is measured against the format alone.
    s = s.replace("\\documentclass{article}\n", "");
    s
}

/// The direct loads of `top` and the options of every file, from a load run's output.
#[cfg(any(test, feature = "gen"))]
pub fn parse_load(out: &str, top: &str) -> (Vec<String>, Vec<(String, String)>) {
    let mut stack: Vec<&str> = Vec::new();
    let (mut loads, mut options) = (Vec::new(), Vec::new());
    for line in out.lines() {
        if let Some(f) = line.strip_prefix("B ") {
            if stack.last() == Some(&top) && !loads.iter().any(|l: &String| l == f) {
                loads.push(f.to_string());
            }
            stack.push(f);
        } else if line.starts_with("A ") {
            stack.pop();
        } else if let Some(r) = line.strip_prefix("O ") {
            if let Some((p, o)) = r.split_once(' ') {
                options.push((p.to_string(), o.to_string()));
            }
        }
    }
    (loads, options)
}

/// Writes the registry: `pkgs` (sorted by name) get the first file IDs.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, pkgs: &[GenPkg]) -> Result<(usize, usize), String> {
    let mut files: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
    let mut others: Vec<&str> = pkgs.iter().flat_map(|p| p.loads.iter().map(String::as_str)).filter(|f| !pkgs.iter().any(|p| p.name == *f)).collect();
    others.sort_unstable();
    others.dedup();
    files.extend(others);
    let index: std::collections::HashMap<&str, u32> = files.iter().enumerate().map(|(i, f)| (*f, i as u32)).collect();
    let names: Vec<&[u8]> = files.iter().map(|f| f.as_bytes()).collect();
    super::data_phf::write(w, "pkf", &names).map_err(|e| format!("{e:?}"))?;
    let mut loads: Vec<Vec<u32>> = vec![Vec::new(); files.len()];
    for (i, p) in pkgs.iter().enumerate() {
        loads[i] = p.loads.iter().map(|f| index[f.as_str()]).collect();
    }
    super::data_csr::write(w, "pk.ld", &loads);
    let (mut pool, mut offsets, mut ids) = (Vec::new(), vec![0u32], std::collections::HashMap::<&str, u32>::new());
    let mut opts: Vec<Vec<u32>> = Vec::with_capacity(pkgs.len());
    for p in pkgs {
        opts.push(p.options.iter().map(|o| {
            *ids.entry(o.as_str()).or_insert_with(|| {
                pool.extend_from_slice(o.as_bytes());
                offsets.push(pool.len() as u32);
                offsets.len() as u32 - 2
            })
        }).collect());
    }
    super::data_csr::write(w, "pk.op", &opts);
    let mut by: std::collections::BTreeMap<&str, Vec<u32>> = Default::default();
    for (i, p) in pkgs.iter().enumerate() {
        for c in &p.defines {
            by.entry(c.as_str()).or_default().push(i as u32);
        }
    }
    let cnames: Vec<&[u8]> = by.keys().map(|c| c.as_bytes()).collect();
    super::data_phf::write(w, "pkc", &cnames).map_err(|e| format!("{e:?}"))?;
    let rows: Vec<Vec<u32>> = by.values().cloned().collect();
    super::data_csr::write(w, "pk.by", &rows);
    let mut id = crate::backend::backend_hash::ContentHasher::new();
    for p in pkgs {
        for s in std::iter::once(&p.name).chain(&p.loads).chain(&p.options).chain(&p.defines) {
            id.bytes(s.as_bytes());
            id.u32(0);
        }
        id.u32(1);
    }
    let h = id.finish() as u64;
    w.bytes("pk.str", pool);
    w.u32s("pk.so", &offsets);
    w.u32s("pk.id", &[h as u32, (h >> 32) as u32, pkgs.len() as u32]);
    Ok((files.len(), by.len()))
}

/// Every entry of the table against the recorded packages: the generator's self-check.
#[cfg(any(test, feature = "gen"))]
pub fn check(t: &PkgTable, pkgs: &[GenPkg]) -> Result<(), String> {
    if t.registered() as usize != pkgs.len() {
        return Err(format!("{} registered, {} recorded", t.registered(), pkgs.len()));
    }
    for (i, p) in pkgs.iter().enumerate() {
        let id = i as u32;
        if t.file(&p.name) != Some(id) || t.file_name(id) != p.name {
            return Err(format!("{}: name", p.name));
        }
        let loads: Vec<&str> = t.loads(id).iter().map(|f| t.file_name(f)).collect();
        if loads != p.loads.iter().map(String::as_str).collect::<Vec<_>>() {
            return Err(format!("{}: loads {loads:?}, recorded {:?}", p.name, p.loads));
        }
        if !t.options(id).eq(p.options.iter().map(String::as_str)) {
            return Err(format!("{}: options", p.name));
        }
        for c in &p.defines {
            if !t.defined_by(c).iter().any(|x| x == id) {
                return Err(format!("{}: defines \\{c}", p.name));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::data_blob::BlobWriter;
    use super::*;

    #[test]
    fn loads_nest_and_the_table_reads_back() {
        let out = "B amsmath.sty\nO amsmath.sty leqno\nB amstext.sty\nB amsgen.sty\nA amsgen.sty\nA amstext.sty\nB amsbsy.sty\nA amsbsy.sty\nA amsmath.sty\n";
        let (loads, opts) = parse_load(out, "amsmath.sty");
        assert_eq!(loads, ["amstext.sty", "amsbsy.sty"]);
        assert_eq!(opts, [("amsmath.sty".to_string(), "leqno".to_string())]);
        let pkgs = vec![
            GenPkg { name: "amsmath.sty".into(), loads, options: vec!["leqno".into(), "reqno".into()], defines: vec!["align".into(), "text".into()] },
            GenPkg { name: "amstext.sty".into(), loads: vec!["amsgen.sty".into()], options: vec![], defines: vec!["text".into()] },
        ];
        let mut w = BlobWriter::default();
        write(&mut w, &pkgs).unwrap();
        let bytes = w.finish();
        let t = PkgTable::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        check(&t, &pkgs).unwrap();
        assert_eq!(t.defined_by("text").iter().collect::<Vec<_>>(), [0, 1]);
        assert!(t.file("amsbsy.sty").is_some_and(|f| f >= t.registered()));
    }
}
