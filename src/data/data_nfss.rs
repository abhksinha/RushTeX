//! NFSS font shapes as a static table (pillar 2): every pinned `.fd` file, read once by LaTeX
//! itself at generation (`rushtex-gen nfss`), so the macro code some `.fd` files hold (scale
//! options, `\EC@family`) has run; what they declared is stored, and resolved in place.
//!
//! Layout, by access pattern (a selection reads one shape's entries in order, then one name):
//! - **Shapes** by name (`T1/cmr/m/n`) through the perfect hash `nfsh`; IDs follow the sorted
//!   names, so a family's shapes are adjacent. `nf.r` holds each shape's first entry (one
//!   more at the end); `nf.c` its loading code (`\DeclareFontShape`'s sixth argument).
//! - **Entries** (`nf.e`): one 16-byte record per size of the spec, in declaration order,
//!   four to a cache line: `lo`, `hi` (or a single size's text), the scale, the argument, the
//!   size function and the kind. A shape's entries are contiguous.
//! - **Spec hashes** (`nf.h`, two words a shape): the hash of each shape's spec text, the
//!   name of the control sequence `\DeclareFontShape` stores. The native selection checks a
//!   document's shape against it once per spec, so a shape redeclared by a document or a
//!   package is never read from the table.
//! - **Families** by name (`T1+cmr`) through `nffa`; `nf.fr` holds three words each: the
//!   `\hyphenchar` and `\skewchar` their loading code sets (`NONE` when it sets none) and any
//!   other loading code, which a native package or the engine fallback must run.
//! - **Strings** (`nf.str`, offsets `nf.so`): font names, name prefixes, substitution
//!   targets, single sizes' texts and loading code, each stored once; string 0 is empty.
//!
//! Sizes are compared in scaled points as `\ifdim` compares them (ltfsstrc.dtx
//! `\check@range`, `\check@single`); names are built from the size's text as `\f@size` holds it.

use super::data_blob::{Blob, U32s};
use super::data_phf::Phf;
use crate::backend::scaled::{round_decimals, xn_over_d};

/// The file the declaration run writes.
pub const DECLARE_FILE: &str = "rtnfss";
/// The file the self-check run writes.
pub const CHECK_FILE: &str = "rtnfssck";

/// `\maxdimen`, the upper bound of a range without one (`\check@range`).
const MAX_DIMEN: i32 = 0x3FFF_FFFF;
/// A family's `\hyphenchar` or `\skewchar` its loading code does not set.
pub const NONE: i32 = i32::MIN;

/// The size functions of ltfssbas.dtx (`\DeclareSizeFunction`), by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SizeFn {
    Plain,
    S,
    Gen,
    SGen,
    GenB,
    SGenB,
    Sub,
    SSub,
    Alias,
    SubF,
    SSubF,
    Fixed,
    SFixed,
}

const SIZE_FNS: [(&str, SizeFn); 13] = [
    ("", SizeFn::Plain),
    ("s", SizeFn::S),
    ("gen", SizeFn::Gen),
    ("sgen", SizeFn::SGen),
    ("genb", SizeFn::GenB),
    ("sgenb", SizeFn::SGenB),
    ("sub", SizeFn::Sub),
    ("ssub", SizeFn::SSub),
    ("alias", SizeFn::Alias),
    ("subf", SizeFn::SubF),
    ("ssubf", SizeFn::SSubF),
    ("fixed", SizeFn::Fixed),
    ("sfixed", SizeFn::SFixed),
];

impl SizeFn {
    fn from_u8(b: u8) -> Option<SizeFn> {
        SIZE_FNS.get(b as usize).map(|&(_, f)| f)
    }

    #[cfg(any(test, feature = "gen"))]
    fn named(name: &str) -> Option<SizeFn> {
        SIZE_FNS.iter().find(|(n, _)| *n == name).map(|&(_, f)| f)
    }
}

/// `<decimal>pt` in scaled points, as `scan_dimen` reads it (§448, §452): integer part, then
/// the fraction by `round_decimals`. `None` for text that is not a plain decimal.
pub fn decimal_sp(text: &str) -> Option<i32> {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    if int.is_empty() && frac.is_empty() || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) || int.len() > 5 {
        return None;
    }
    let i: i32 = if int.is_empty() { 0 } else { int.parse().ok()? };
    (i < 16384).then(|| (i << 16) + round_decimals(frac.as_bytes()))
}

/// A factor `[s]` times the size, as `\@tempdimb \optional@arg\@tempdimb` computes it
/// (§455: the integer part times the unit, plus `xn_over_d` of the unit by the fraction).
/// `factor` is the integer part above 16 bits and the fraction (2^-16) below.
#[inline]
pub fn scale_by(factor: u32, size: i32) -> i32 {
    (factor >> 16) as i32 * size + xn_over_d(size, (factor & 0xFFFF) as i32, 0x10000)
}

/// What `\get@external@font` makes of a shape at a size: the TFM name (`prefix` then
/// `suffix`) and how it is loaded (`at`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct External<'a> {
    pub prefix: &'a str,
    pub suffix: Suffix<'a>,
    pub at: At<'a>,
    /// The shape the name came from, after substitutions (`\sub@sfcnt`).
    pub shape: u32,
}

/// The part of a name a size function builds from the size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suffix<'a> {
    None,
    /// `gen`: the size's text (`\f@size`).
    Size(&'a str),
    /// `genb`: two digits of the integer part and two of the fraction (`\genb@x`).
    GenB([u8; 4]),
}

/// The `at` clause of the `\font` assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At<'a> {
    /// `at\the\@tempdimb` (`\empty@sfcnt`).
    Sp(i32),
    /// `fixed` with an optional argument: `at<text>pt`.
    Text(&'a str),
    /// `fixed` without one: the font at its design size.
    None,
}

impl External<'_> {
    /// The TFM name.
    pub fn name(&self) -> String {
        let mut s = self.prefix.to_string();
        match self.suffix {
            Suffix::None => {}
            Suffix::Size(t) => s.push_str(t),
            Suffix::GenB(d) => s.extend(d.iter().map(|&b| b as char)),
        }
        s
    }

    /// `\external@font` exactly as LaTeX writes it (the generator's self-check).
    pub fn text(&self) -> String {
        let mut s = self.name();
        match self.at {
            At::Sp(v) => {
                s.push_str(" at");
                crate::engine::engine_value::print_scaled(v, &mut s);
                s.push_str("pt");
            }
            At::Text(t) => {
                s.push_str(" at");
                s.push_str(t);
                s.push_str("pt");
            }
            At::None => {}
        }
        s
    }
}

/// `\genb@x`: `10.95` gives `1095`, `5` gives `0500`.
fn genb_digits(size: &str) -> Option<[u8; 4]> {
    let (int, frac) = size.split_once('.').unwrap_or((size, ""));
    let n: u32 = int.parse().ok().filter(|&n| n < 100)?;
    let f = frac.as_bytes();
    let d = |i: usize| f.get(i).copied().unwrap_or(b'0');
    Some([b'0' + (n / 10) as u8, b'0' + (n % 10) as u8, d(0), d(1)])
}

const ENTRY: usize = 16;
/// An entry's kinds: a single size, or a range.
const SINGLE: u8 = 0;
const RANGE: u8 = 1;

/// The NFSS table, read in place.
pub struct NfssTable<'a> {
    shapes: Phf<'a>,
    rows: U32s<'a>,
    spec_hashes: U32s<'a>,
    code: U32s<'a>,
    entries: &'a [[u8; ENTRY]],
    families: Phf<'a>,
    family_recs: U32s<'a>,
    strings: &'a [u8],
    offsets: U32s<'a>,
    id: u64,
}

#[inline]
fn le_i32(e: &[u8; ENTRY], at: usize) -> i32 {
    i32::from_le_bytes([e[at], e[at + 1], e[at + 2], e[at + 3]])
}

impl<'a> NfssTable<'a> {
    /// The table in `b`, every index checked once, so lookups never panic.
    pub fn from_blob(b: &Blob<'a>) -> Option<NfssTable<'a>> {
        let shapes = Phf::from_blob(b, "nfsh")?;
        let families = Phf::from_blob(b, "nffa")?;
        let (entries, rest) = b.section("nf.e")?.as_chunks::<ENTRY>();
        let (rows, code, family_recs, spec_hashes) = (b.u32s("nf.r")?, b.u32s("nf.c")?, b.u32s("nf.fr")?, b.u32s("nf.h")?);
        let (strings, offsets, id) = (b.section("nf.str")?, b.u32s("nf.so")?, b.u32s("nf.id")?);
        let n_str = offsets.len().checked_sub(1)?;
        let str_ok = |s: u32| (s as usize) < n_str;
        let sound = rest.is_empty()
            && rows.len() == shapes.len() + 1
            && code.len() == shapes.len()
            && spec_hashes.len() == 2 * shapes.len()
            && family_recs.len() == 3 * families.len()
            && id.len() == 2
            && rows.iter().zip(rows.iter().skip(1)).all(|(a, b)| a <= b)
            && rows.get(rows.len() - 1) as usize == entries.len()
            && offsets.iter().zip(offsets.iter().skip(1)).all(|(a, b)| a <= b)
            && offsets.get(n_str) as usize == strings.len()
            && std::str::from_utf8(strings).is_ok()
            && code.iter().all(str_ok)
            && (0..families.len()).all(|f| str_ok(family_recs.get(3 * f + 2)))
            && entries.iter().all(|e| {
                let arg = u16::from_le_bytes([e[12], e[13]]) as u32;
                let kind_ok = e[15] == SINGLE && str_ok(le_i32(e, 4) as u32) || e[15] == RANGE;
                let scale_ok = !matches!(SizeFn::from_u8(e[14]), Some(SizeFn::Fixed | SizeFn::SFixed)) || str_ok(le_i32(e, 8) as u32);
                SizeFn::from_u8(e[14]).is_some() && str_ok(arg) && kind_ok && scale_ok
            });
        sound.then(|| NfssTable { shapes, rows, spec_hashes, code, entries, families, family_recs, strings, offsets, id: (id.get(1) as u64) << 32 | id.get(0) as u64 })
    }

    /// The table's content fingerprint.
    pub fn content_id(&self) -> u64 {
        self.id
    }

    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// The ID of shape `enc/family/series/shape`.
    #[inline]
    pub fn shape(&self, key: &str) -> Option<u32> {
        self.shapes.get(key.as_bytes())
    }

    pub fn shape_name(&self, id: u32) -> &'a str {
        std::str::from_utf8(self.shapes.name(id)).unwrap_or("")
    }

    /// The hash of shape `id`'s spec text ([`spec_hash`]).
    #[inline]
    pub fn spec_hash(&self, id: u32) -> u64 {
        (self.spec_hashes.get(2 * id as usize + 1) as u64) << 32 | self.spec_hashes.get(2 * id as usize) as u64
    }

    /// Shape `id`'s loading code (empty for none).
    pub fn shape_code(&self, id: u32) -> &'a str {
        self.string(self.code.get(id as usize))
    }

    /// Family `enc+family`: its `\hyphenchar` and `\skewchar` (`NONE` when unset) and any
    /// other loading code.
    pub fn family(&self, key: &str) -> Option<(i32, i32, &'a str)> {
        let f = self.families.get(key.as_bytes())? as usize;
        let r = &self.family_recs;
        Some((r.get(3 * f) as i32, r.get(3 * f + 1) as i32, self.string(r.get(3 * f + 2))))
    }

    #[inline]
    fn string(&self, s: u32) -> &'a str {
        let (a, b) = (self.offsets.get(s as usize) as usize, self.offsets.get(s as usize + 1) as usize);
        // Checked once in `from_blob`: in bounds, and the whole pool is UTF-8 on char
        // boundaries the generator wrote.
        std::str::from_utf8(&self.strings[a..b]).unwrap_or("")
    }

    #[inline]
    fn entries(&self, shape: u32) -> &'a [[u8; ENTRY]] {
        &self.entries[self.rows.get(shape as usize) as usize..self.rows.get(shape as usize + 1) as usize]
    }

    /// `\get@external@font` (ltfsstrc.dtx): shape `shape` at the size `size` (`\f@size`'s
    /// text, `10.95`). First the entries in declaration order (`\try@size@range`): a single
    /// size equal to it or a range holding it, `lo <= size < hi`; else the single size
    /// nearest to it, the first of equals (`\try@size@substitution`), run again with that
    /// size's text. `None` where LaTeX reports the shape not found. Substitution chains
    /// (`sub`, `ssub`, `alias`) are followed, at most `DEPTH` deep.
    pub fn external(&self, shape: u32, size: &'a str) -> Option<External<'a>> {
        self.external_at(shape, size, 0)
    }

    /// The cases `\get@external@font` settles without a message or a further file: the
    /// first entry holding the size (`\try@size@range`) has a size function that only builds
    /// a name (`""`, `s`, `gen`, `sgen`, `genb`, `sgenb`) and no scale. `None` for every
    /// other case (substitution, `sub` chains, scales, `fixed`, no font): those warn, write
    /// to the log or load an `.fd` file, so LaTeX's own macro runs them.
    pub fn direct(&self, shape: u32, size: &'a str) -> Option<External<'a>> {
        let sp = decimal_sp(size)?;
        let e = self.entries(shape).iter().find(|e| match e[15] {
            SINGLE => le_i32(e, 0) == sp,
            _ => {
                let hi = le_i32(e, 4);
                sp < if hi == 0 { MAX_DIMEN } else { hi } && sp >= le_i32(e, 0)
            }
        })?;
        let plain = matches!(SizeFn::from_u8(e[14])?, SizeFn::Plain | SizeFn::S | SizeFn::Gen | SizeFn::SGen | SizeFn::GenB | SizeFn::SGenB);
        (plain && le_i32(e, 8) == 0).then(|| self.run(shape, e, size, 0)).flatten()
    }

    fn external_at(&self, shape: u32, size: &'a str, depth: u32) -> Option<External<'a>> {
        const DEPTH: u32 = 16;
        if depth > DEPTH || shape as usize >= self.shapes.len() {
            return None;
        }
        let sp = decimal_sp(size)?;
        let es = self.entries(shape);
        for e in es {
            let hit = match e[15] {
                SINGLE => le_i32(e, 0) == sp,
                _ => {
                    let hi = le_i32(e, 4);
                    sp < if hi == 0 { MAX_DIMEN } else { hi } && sp >= le_i32(e, 0)
                }
            };
            if hit {
                return self.run(shape, e, size, depth);
            }
        }
        // `\try@simples`: the nearest single size, strictly nearer than any before it.
        let mut best: Option<(i32, u32)> = None;
        for e in es.iter().filter(|e| e[15] == SINGLE) {
            let d = (le_i32(e, 0) - sp).abs();
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, le_i32(e, 4) as u32));
            }
        }
        let text = self.string(best?.1);
        // `\try@simple@size` finds `<text>` in the spec as text, the first occurrence.
        let e = es.iter().find(|e| e[15] == SINGLE && self.string(le_i32(e, 4) as u32) == text)?;
        self.run(shape, e, text, depth)
    }

    /// `\execute@size@function` for entry `e` of `shape` at size text `size`.
    fn run(&self, shape: u32, e: &[u8; ENTRY], size: &'a str, depth: u32) -> Option<External<'a>> {
        let arg = self.string(u16::from_le_bytes([e[12], e[13]]) as u32);
        let factor = le_i32(e, 8) as u32;
        let plain = |prefix: &'a str, suffix: Suffix<'a>| {
            let sp = decimal_sp(size)?;
            let at = if factor == 0 { sp } else { scale_by(factor, sp) };
            Some(External { prefix, suffix, at: At::Sp(at), shape })
        };
        match SizeFn::from_u8(e[14])? {
            SizeFn::Plain | SizeFn::S | SizeFn::SubF | SizeFn::SSubF => plain(arg, Suffix::None),
            SizeFn::Gen | SizeFn::SGen => plain(arg, Suffix::Size(size)),
            SizeFn::GenB | SizeFn::SGenB => plain(arg, Suffix::GenB(genb_digits(size)?)),
            SizeFn::Fixed | SizeFn::SFixed => {
                let opt = self.string(factor);
                Some(External { prefix: arg, suffix: Suffix::None, at: if opt.is_empty() { At::None } else { At::Text(opt) }, shape })
            }
            SizeFn::Sub | SizeFn::SSub | SizeFn::Alias => {
                // `\sub@sfcnt`: the shape `<encoding>/<arg>` at the user's size.
                let name = self.shape_name(shape);
                let enc = &name[..name.find('/')?];
                let mut key = String::with_capacity(enc.len() + 1 + arg.len());
                key.push_str(enc);
                key.push('/');
                key.push_str(arg);
                self.external_at(self.shape(&key)?, size, depth + 1)
            }
        }
    }
}

static NFSS_BLOB: &[u8] = include_bytes!("../../assets/tables/nfss.rtdb");

/// The hash of a spec's text, as `nf.h` stores it.
#[inline]
pub fn spec_hash(spec: &[u8]) -> u64 {
    crate::backend::backend_hash::hash64(0x6e66_7373, spec)
}

/// The embedded NFSS table, checked on first use; `None` if it is not a table of this build.
pub fn nfss() -> Option<&'static NfssTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<NfssTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(NFSS_BLOB).and_then(|b| NfssTable::from_blob(&b))).as_ref()
}

// ---- Generation -------------------------------------------------------------------------

/// One size of a spec, as the generator reads it.
#[cfg(any(test, feature = "gen"))]
#[derive(Clone, Debug, PartialEq)]
pub struct GenEntry {
    /// `None` for a single size; `Some((lo, hi))` texts for a range (either may be empty).
    pub range: Option<(String, String)>,
    /// A single size's text.
    pub size: String,
    pub func: SizeFn,
    pub scale: String,
    pub arg: String,
}

#[cfg(any(test, feature = "gen"))]
#[derive(Clone, Debug, Default)]
pub struct GenShape {
    pub name: String,
    /// The spec's text (the name of the control sequence LaTeX stores it as).
    pub spec: String,
    pub entries: Vec<GenEntry>,
    pub code: String,
}

#[cfg(any(test, feature = "gen"))]
#[derive(Clone, Debug, Default)]
pub struct GenFamily {
    pub name: String,
    pub hyphen: Option<i32>,
    pub skew: Option<i32>,
    pub code: String,
}

/// A size spec (`<5><6>gen*cmr<10.95>cmr10<->ssub*cmr/m/n`) as `\extract@rangefontinfo` and
/// `\extract@sizefn` read it: sizes in angle brackets, each taking the next non-empty
/// function part (`[fn*][[scale]]arg`).
#[cfg(any(test, feature = "gen"))]
pub fn parse_spec(spec: &str) -> Result<Vec<GenEntry>, String> {
    let mut out = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut rest = spec;
    while !rest.is_empty() {
        let r = rest.strip_prefix('<').ok_or_else(|| format!("expected `<` at `{rest}`"))?;
        let close = r.find('>').ok_or("unclosed `<`")?;
        pending.push(r[..close].to_string());
        rest = &r[close + 1..];
        let end = rest.find('<').unwrap_or(rest.len());
        let part = &rest[..end];
        rest = &rest[end..];
        if part.is_empty() {
            continue;
        }
        let (func, args) = match part.split_once('*') {
            Some((f, a)) => (SizeFn::named(f).ok_or_else(|| format!("size function `{f}`"))?, a),
            None => (SizeFn::Plain, part),
        };
        let (scale, arg) = match args.strip_prefix('[') {
            Some(a) => {
                let (s, a) = a.split_once(']').ok_or("unclosed `[`")?;
                (s.to_string(), a.to_string())
            }
            None => (String::new(), args.to_string()),
        };
        for size in pending.drain(..) {
            let range = size.split_once('-').map(|(a, b)| (a.to_string(), b.to_string()));
            for t in range.as_ref().map_or(vec![&size], |(a, b)| vec![a, b]) {
                if !t.is_empty() && decimal_sp(t).is_none() {
                    return Err(format!("size `{t}`"));
                }
            }
            out.push(GenEntry { range, size, func, scale: scale.clone(), arg: arg.clone() });
        }
    }
    if !pending.is_empty() {
        return Err(format!("sizes {pending:?} without a function part"));
    }
    Ok(out)
}

/// A family's loading code: `\hyphenchar\font<n>` and `\skewchar\font<n>` (with an optional
/// `=`; `<n>` decimal, `'` octal, `"` hex or `\m@ne`) are read; any other code is kept whole.
#[cfg(any(test, feature = "gen"))]
pub fn parse_family_code(code: &str) -> (Option<i32>, Option<i32>, String) {
    let toks: Vec<&str> = code.split_whitespace().collect();
    let (mut hyphen, mut skew) = (None, None);
    let mut i = 0;
    while i < toks.len() {
        let which = toks[i];
        if (which == "\\hyphenchar" || which == "\\skewchar") && toks.get(i + 1) == Some(&"\\font") {
            let mut v = toks.get(i + 2).copied().unwrap_or("");
            let mut used = 3;
            if v == "=" {
                v = toks.get(i + 3).copied().unwrap_or("");
                used = 4;
            }
            let v = v.strip_prefix('=').unwrap_or(v);
            let n = match v {
                "\\m@ne" => Some(-1),
                _ if v.starts_with('\'') => i32::from_str_radix(&v[1..], 8).ok(),
                _ if v.starts_with('"') => i32::from_str_radix(&v[1..], 16).ok(),
                _ => v.parse().ok(),
            };
            if let Some(n) = n {
                *(if which == "\\hyphenchar" { &mut hyphen } else { &mut skew }) = Some(n);
                i += used;
                continue;
            }
        }
        return (None, None, code.trim().to_string());
    }
    (hyphen, skew, String::new())
}

/// The declaration run's output: families and shapes, a later declaration of a name
/// replacing an earlier one, as `\xdef` and `\let` do.
#[cfg(any(test, feature = "gen"))]
pub fn parse_declarations(text: &str) -> Result<(Vec<GenFamily>, Vec<GenShape>), String> {
    let mut fams: std::collections::BTreeMap<String, GenFamily> = Default::default();
    let mut shapes: std::collections::BTreeMap<String, GenShape> = Default::default();
    for line in text.lines() {
        if let Some(r) = line.strip_prefix("F ") {
            let (name, code) = r.split_once(' ').unwrap_or((r, ""));
            let (hyphen, skew, code) = parse_family_code(code);
            fams.insert(name.to_string(), GenFamily { name: name.to_string(), hyphen, skew, code });
        } else if let Some(r) = line.strip_prefix("S ") {
            let (name, r) = r.split_once(' ').ok_or_else(|| format!("shape line `{line}`"))?;
            let (spec, code) = r.split_once(' ').unwrap_or((r, ""));
            let spec = spec.strip_prefix('\\').ok_or_else(|| format!("{name}: spec `{spec}`"))?;
            let entries = parse_spec(spec).map_err(|e| format!("{name}: {e}"))?;
            shapes.insert(name.to_string(), GenShape { name: name.to_string(), spec: spec.to_string(), entries, code: code.trim().to_string() });
        }
    }
    Ok((fams.into_values().collect(), shapes.into_values().collect()))
}

/// Writes the table (shape and family IDs in name order) and returns its strings' bytes.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, fams: &[GenFamily], shapes: &[GenShape]) -> Result<usize, String> {
    let mut pool: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = vec![0];
    let mut ids: std::collections::HashMap<String, u32> = Default::default();
    let mut intern = |s: &str| -> u32 {
        if let Some(&i) = ids.get(s) {
            return i;
        }
        pool.extend_from_slice(s.as_bytes());
        offsets.push(pool.len() as u32);
        let i = offsets.len() as u32 - 2;
        ids.insert(s.to_string(), i);
        i
    };
    intern("");
    let mut id = crate::backend::backend_hash::ContentHasher::new();
    let names: Vec<&[u8]> = shapes.iter().map(|s| s.name.as_bytes()).collect();
    super::data_phf::write(w, "nfsh", &names).map_err(|e| format!("{e:?}"))?;
    let fnames: Vec<&[u8]> = fams.iter().map(|f| f.name.as_bytes()).collect();
    super::data_phf::write(w, "nffa", &fnames).map_err(|e| format!("{e:?}"))?;
    let (mut rows, mut code, mut entries, mut hashes) = (vec![0u32], Vec::new(), Vec::new(), Vec::new());
    for s in shapes {
        id.bytes(s.name.as_bytes());
        let h = spec_hash(s.spec.as_bytes());
        hashes.extend_from_slice(&[h as u32, (h >> 32) as u32]);
        id.u32(h as u32);
        for e in &s.entries {
            let mut rec = [0u8; ENTRY];
            let sp = |t: &str| if t.is_empty() { 0 } else { decimal_sp(t).unwrap_or(0) };
            let (lo, hi, kind) = match &e.range {
                None => (sp(&e.size), intern(&e.size) as i32, SINGLE),
                Some((a, b)) => (sp(a), sp(b), RANGE),
            };
            let factor = match e.func {
                SizeFn::Fixed | SizeFn::SFixed => intern(&e.scale),
                _ if e.scale.is_empty() => 0,
                _ => {
                    let (i, f) = e.scale.split_once('.').unwrap_or((&e.scale, ""));
                    let i: u32 = if i.is_empty() { 0 } else { i.parse().map_err(|_| format!("{}: scale `{}`", s.name, e.scale))? };
                    i << 16 | round_decimals(f.as_bytes()) as u32
                }
            };
            let arg = u16::try_from(intern(&e.arg)).map_err(|_| "more than 65536 strings")?;
            rec[0..4].copy_from_slice(&lo.to_le_bytes());
            rec[4..8].copy_from_slice(&hi.to_le_bytes());
            rec[8..12].copy_from_slice(&factor.to_le_bytes());
            rec[12..14].copy_from_slice(&arg.to_le_bytes());
            rec[14] = e.func as u8;
            rec[15] = kind;
            id.bytes(&rec);
            id.bytes(e.size.as_bytes());
            id.bytes(e.arg.as_bytes());
            id.bytes(e.scale.as_bytes());
            entries.extend_from_slice(&rec);
        }
        rows.push((entries.len() / ENTRY) as u32);
        code.push(intern(&s.code));
        id.bytes(s.code.as_bytes());
    }
    let mut frecs = Vec::with_capacity(3 * fams.len());
    for f in fams {
        frecs.extend_from_slice(&[f.hyphen.unwrap_or(NONE) as u32, f.skew.unwrap_or(NONE) as u32, intern(&f.code)]);
        id.bytes(f.name.as_bytes());
        id.bytes(f.code.as_bytes());
        id.u32(frecs[frecs.len() - 3]);
        id.u32(frecs[frecs.len() - 2]);
    }
    let strings = pool.len();
    w.u32s("nf.r", &rows);
    w.u32s("nf.c", &code);
    w.u32s("nf.h", &hashes);
    w.bytes("nf.e", entries);
    w.u32s("nf.fr", &frecs);
    w.bytes("nf.str", pool);
    w.u32s("nf.so", &offsets);
    let h = id.finish() as u64;
    w.u32s("nf.id", &[h as u32, (h >> 32) as u32]);
    Ok(strings)
}

/// The sizes the self-check resolves every shape at: LaTeX's standard sizes, each single
/// size and range bound of the shape's spec, and sizes between and beyond them.
#[cfg(any(test, feature = "gen"))]
pub fn check_sizes(s: &GenShape) -> Vec<String> {
    let mut v: Vec<String> = ["4", "5", "6", "7", "8", "9", "9.5", "10", "10.95", "11", "12", "13", "14.4", "17.28", "20.74", "24.88", "29.86", "35.83", "50"].iter().map(|s| s.to_string()).collect();
    for e in &s.entries {
        match &e.range {
            None => v.push(e.size.clone()),
            Some((a, b)) => v.extend([a, b].into_iter().filter(|t| !t.is_empty()).cloned()),
        }
    }
    v.sort_by_key(|t| decimal_sp(t).unwrap_or(0));
    v.dedup();
    v
}

/// The self-check run: the declarations again, then LaTeX's own `\get@external@font` for each
/// (shape, size), written as `<shape> <size> <\external@font>`, or `!` where LaTeX finds no
/// font. Each case runs in a group with errors silenced and the error path cut short.
#[cfg(any(test, feature = "gen"))]
pub fn check_source(fds: &[&str], cases: &[(String, String)]) -> String {
    check_cases(declare_source(fds), cases)
}

#[cfg(any(test, feature = "gen"))]
fn check_cases(mut s: String, cases: &[(String, String)]) -> String {
    let end = s.rfind("\\immediate\\closeout").unwrap_or(s.len());
    s.truncate(end);
    s.push_str(concat!(
        "\\newwrite\\rt@ck\\immediate\\openout\\rt@ck=rtnfssck\n",
        "\\def\\rt@case#1/#2/#3/#4 #5;{\\begingroup\\def\\f@encoding{#1}\\def\\f@family{#2}\\def\\f@series{#3}\\def\\f@shape{#4}",
        "\\def\\f@size{#5}\\def\\@latex@error##1##2{}\\def\\errmessage##1{}",
        "\\def\\error@fontshape{\\def\\external@font{!}\\let\\get@external@font\\relax}",
        "\\get@external@font\\immediate\\write\\rt@ck{#1/#2/#3/#4 #5 \\external@font}\\endgroup}\n",
    ));
    for (shape, size) in cases {
        s.push_str(&format!("\\rt@case {shape} {size};\n"));
    }
    s.push_str("\\immediate\\closeout\\rt@ck\\immediate\\closeout\\rt@out\\csname @@end\\endcsname\n");
    s
}

/// Compares every case of the self-check run's output with the table: the generator fails
/// on the first difference.
#[cfg(any(test, feature = "gen"))]
pub fn check(t: &NfssTable, fams: &[GenFamily], shapes: &[GenShape], engine_out: &str) -> Result<usize, String> {
    if t.shape_count() != shapes.len() {
        return Err(format!("{} shapes in the table, {} declared", t.shape_count(), shapes.len()));
    }
    for (i, s) in shapes.iter().enumerate() {
        if t.shape(&s.name) != Some(i as u32) || t.shape_code(i as u32) != s.code {
            return Err(format!("{}: name or loading code", s.name));
        }
    }
    for f in fams {
        if t.family(&f.name) != Some((f.hyphen.unwrap_or(NONE), f.skew.unwrap_or(NONE), f.code.as_str())) {
            return Err(format!("{}: family loading code", f.name));
        }
    }
    let mut n = 0;
    for line in engine_out.lines() {
        let mut p = line.splitn(3, ' ');
        let (shape, size, want) = (p.next().unwrap_or(""), p.next().unwrap_or(""), p.next().unwrap_or("").trim_end());
        let got = t.shape(shape).and_then(|id| t.external(id, size)).map_or_else(|| "!".to_string(), |x| x.text());
        if got != want {
            return Err(format!("{shape} at {size}: LaTeX gives `{want}`, the table `{got}`"));
        }
        n += 1;
    }
    Ok(n)
}

/// The source of the declaration run (generator only): each `.fd` read on the LaTeX format,
/// with `\DeclareFontFamily` and `\DeclareFontShape@` (ltfssbas.dtx) wrapped so that each
/// writes what it declared once its own definition has run:
/// - `F <enc>+<family> <loading code>`;
/// - `S <enc>/<family>/<series>/<shape> <size spec> <shape loading code>`, the series as
///   `\DeclareFontShape@` normalises it and the size spec as the name of the control
///   sequence it made (`\csname #5\endcsname`, so expanded at declaration, as LaTeX does);
/// - `D <file>` before each file. An encoding a file names that the format lacks is declared
///   empty, as its own package would have.
#[cfg(any(test, feature = "gen"))]
pub fn declare_source(fds: &[&str]) -> String {
    let mut s = DECLARE_HEAD.to_string();
    for fd in fds {
        s.push_str(&format!("\\immediate\\write\\rt@out{{D {fd}}}\\input{{{fd}}}\n"));
    }
    s.push_str("\\immediate\\closeout\\rt@out\\csname @@end\\endcsname\n");
    s
}

/// The declaration run of a package (generator only): the packages it loads, then `D <pkg>`
/// and the package, so its own declarations are those after that line.
#[cfg(any(test, feature = "gen"))]
pub fn declare_package_source(deps: &[String], pkg: &str) -> String {
    let mut s = DECLARE_HEAD.to_string();
    for d in deps {
        s.push_str(&format!("\\usepackage{{{d}}}\n"));
    }
    s.push_str(&format!("\\immediate\\write\\rt@out{{D {pkg}}}\\usepackage{{{pkg}}}\n"));
    s.push_str("\\immediate\\closeout\\rt@out\\csname @@end\\endcsname\n");
    s
}

/// [`check_source`] after a package's declaration run instead of `.fd` files.
#[cfg(any(test, feature = "gen"))]
pub fn check_package_source(deps: &[String], pkg: &str, cases: &[(String, String)]) -> String {
    check_cases(declare_package_source(deps, pkg), cases)
}

#[cfg(any(test, feature = "gen"))]
const DECLARE_HEAD: &str = concat!(
    "\\documentclass{article}\\makeatletter\n",
    "\\newwrite\\rt@out\\immediate\\openout\\rt@out=rtnfss\n",
    "\\let\\rt@family\\DeclareFontFamily\n",
    "\\def\\DeclareFontFamily#1#2#3{\\@ifundefined{T@#1}{\\DeclareFontEncoding{#1}{}{}}{}\\rt@family{#1}{#2}{#3}",
    "\\immediate\\write\\rt@out{F #1+#2 \\detokenize{#3}}}\n",
    "\\let\\rt@shape\\DeclareFontShape@\n",
    "\\def\\DeclareFontShape@#1#2#3#4#5#6{\\rt@shape{#1}{#2}{#3}{#4}{#5}{#6}",
    "\\immediate\\write\\rt@out{S #1/#2/\\reserved@b/#4 \\expandafter\\string\\csname#5\\endcsname\\space\\detokenize{#6}}}\n",
);

#[cfg(test)]
mod tests {
    use super::super::data_blob::BlobWriter;
    use super::*;

    #[test]
    fn specs_parse_as_latex_reads_them() {
        let e = parse_spec("<5><6>gen*cmr<10.95>cmr10<-6>s*[.92]msam5<6->ssub*cmr/m/n").unwrap();
        assert_eq!(e.len(), 5);
        assert_eq!((e[0].func, e[0].arg.as_str(), e[1].size.as_str()), (SizeFn::Gen, "cmr", "6"));
        assert_eq!((e[3].func, e[3].scale.as_str(), e[3].range.clone()), (SizeFn::S, ".92", Some((String::new(), "6".into()))));
        assert_eq!(e[4].func, SizeFn::SSub);
        assert_eq!(parse_family_code("\\skewchar \\font '60 "), (None, Some(48), String::new()));
        assert_eq!(parse_family_code("\\hyphenchar \\font =-1 \\skewchar \\font 127 "), (Some(-1), Some(127), String::new()));
        assert_eq!(parse_family_code("\\qpl@set@spacing ").2, "\\qpl@set@spacing");
        assert_eq!(genb_digits("10.95"), Some(*b"1095"));
        assert_eq!(genb_digits("5"), Some(*b"0500"));
    }

    #[test]
    fn table_resolves_ranges_singles_and_substitutions() {
        let decl = "F OT1+cmr \nS OT1/cmr/m/n \\<5><6>gen*cmr<10.95>cmr10<12>cmr12 \nS OT1/cmr/m/it \\<->ssub*cmr/m/n \nS OT1/cmr/m/sc \\<-8>[.5]cmcsc8<8->fixed*[9]cmcsc10 \n";
        let (fams, shapes) = parse_declarations(decl).unwrap();
        let mut w = BlobWriter::default();
        write(&mut w, &fams, &shapes).unwrap();
        let bytes = w.finish();
        let t = NfssTable::from_blob(&Blob::new(&bytes).unwrap()).unwrap();
        let n = t.shape("OT1/cmr/m/n").unwrap();
        assert_eq!(t.external(n, "6").unwrap().text(), "cmr6 at6.0pt");
        assert_eq!(t.external(n, "10.95").unwrap().text(), "cmr10 at10.95pt");
        // No 11pt: the nearest single size, 10.95, by its text.
        assert_eq!(t.external(n, "11").unwrap().text(), "cmr10 at10.95pt");
        let it = t.shape("OT1/cmr/m/it").unwrap();
        assert_eq!(t.external(it, "5").unwrap().text(), "cmr5 at5.0pt");
        let sc = t.shape("OT1/cmr/m/sc").unwrap();
        assert_eq!(t.external(sc, "7").unwrap().text(), "cmcsc8 at3.5pt");
        assert_eq!(t.external(sc, "10").unwrap().text(), "cmcsc10 at9pt");
        assert_eq!(t.family("OT1+cmr"), Some((NONE, NONE, "")));
        assert_eq!(t.spec_hash(n), spec_hash(b"<5><6>gen*cmr<10.95>cmr10<12>cmr12"));
        // Only what LaTeX settles without a message or another file.
        assert_eq!(t.direct(n, "10.95").unwrap().text(), "cmr10 at10.95pt");
        assert_eq!(t.direct(n, "11"), None);
        assert_eq!(t.direct(it, "5"), None);
        assert_eq!(t.direct(sc, "7"), None);
    }
}
