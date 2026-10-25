//! LaTeX's kernel definitions that are values, as data (P2.2,
//! `2026-10-23-p2-native-document-path.md`): the `\chardef` and `\mathchardef` names a
//! document may use (`\&`, `\sum`, `\Gamma`, `\prime`), as the format defines them.
//!
//! `rushtex-gen class` reads them from the oracle's format at the first body token and
//! checks the written table reads back to the same list. Internal names (with `@`, `_` or
//! `:`) are left out: only native code would use them, and it keeps its own constants.
//!
//! Sections:
//! - `chars`: one line per name, `name<TAB>kind<TAB>value`, kind `c` for `\chardef` and `m`
//!   for `\mathchardef`.
//! - `ops`: the log-like operators (`\lim`, `\sin`; latex.ltx `\DeclareRobustCommand\log
//!   {\mathop{\operator@font log}\nolimits}`), one line per name, `name<TAB>word<TAB>n`
//!   with `n` 1 for `\nolimits`, plus 2 for amsopn's `\kern\z@`; the word is letters and `\,`.
//! - `delims`: fontmath.ltx's `\DeclareMathDelimiter`s (`\lbrace`, `\langle`), each a macro
//!   `\delimiter"XXXXXXX `: one line per name, `name<TAB>value`.
//! - `macros`: kernel macros whose bodies use only characters, parameters, primitives the
//!   engine runs end to end inside the native job ([`MACRO_PRIMITIVES`]), parameters and
//!   names already taken (`\frac`, `\quad`): one line per name, `name<US>flags<US>token...`
//!   with `<US>` the unit separator (0x1f), a control sequence as `C<name>`, any other token
//!   as `T<hex>`, its raw value. A robust command (`\protect\name\space`) is stored as its
//!   inner macro, protected.
//! - `mathfont`: the math version `normal` as `\mv@normal` lists it, in family order, and the
//!   math alphabets (fontmath.ltx), one per line:
//!   - `f<TAB>fam<TAB>normal<TAB>bold`: a symbol font (`\DeclareSymbolFont`), its family and
//!     its NFSS shape (`OML/cmm/m/it`) in versions `normal` and `bold`;
//!   - `s<TAB>name<TAB>fam<TAB>same`: a symbol font alphabet (`\DeclareSymbolFontAlphabet
//!     \mathcal{symbols}`), the family it selects;
//!   - `a<TAB>name<TAB>same<TAB>normal<TAB>bold`: a math alphabet (`\DeclareMathAlphabet`),
//!     given a family on its first use in math (`\document@select@group`).
//!
//!   `same` is 1 when the alphabet's encoding switch (`\M@OT1`) means what the text
//!   encoding's does, so `\use@mathgroup` skips it.
//! - `pkg` (a package's table only, `rushtex-gen natpkg`): `fam0<TAB>dep…`, the families
//!   in use before the package (its own families and the codes naming them count from
//!   there), then the packages it loads, which load first. A package's table holds only
//!   what it adds to the kernel and those packages.
//! - `nf.*` (a package's table, when it declares font shapes): its `\DeclareFontShape`s and
//!   `\DeclareFontFamily`s as a `data_nfss` table, which the native job consults before the
//!   `.fd` files' table.

use super::data_blob::Blob;

pub struct KernelTable<'a> {
    chars: &'a str,
    ops: &'a str,
    delims: &'a str,
    macros: &'a str,
    mathfonts: &'a str,
    pkg: Option<&'a str>,
    blocked: &'a str,
    lets: &'a str,
    accents: &'a str,
}

/// One entry of the math version `normal` (section `mathfont`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathFont<S> {
    Symbol { fam: u8, normal: S, bold: S },
    SymbolAlphabet { name: S, fam: u8, same: bool },
    Alphabet { name: S, same: bool, normal: S, bold: S },
}

/// Primitives a harvested kernel macro may use: math lists and horizontal material the
/// engine builds by itself, grouping, and the tests of mode. Nothing that touches what the
/// native job keeps (`\par`, vertical glue and penalties, marks, inserts, files, fonts,
/// definitions, alignments).
pub const MACRO_PRIMITIVES: [&str; 47] = [
    "over", "atop", "above", "overwithdelims", "atopwithdelims", "abovewithdelims", "left", "right", "middle", "radical", "mathchoice",
    "mathop", "mathbin", "mathrel", "mathopen", "mathclose", "mathpunct", "mathinner", "mathord", "displaystyle", "textstyle", "scriptstyle", "scriptscriptstyle",
    "mkern", "mskip", "limits", "nolimits", "displaylimits", "underline", "overline", "nonscript", "mathchar", "delimiter", "mathaccent", "fam", "vcenter",
    "relax", "begingroup", "endgroup", "hskip", "kern", "hfil", "hfill", "hss", "ifmmode", "else", "fi",
];

/// A harvested macro's token, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroToken<'a> {
    Cs(&'a str),
    Raw(u32),
}

/// A log-like operator: `\mathop{\operator@font word}`, then `\nolimits` or not.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Op<'a> {
    pub name: &'a str,
    pub word: &'a str,
    pub nolimits: bool,
    /// amsopn's `\qopname`: `\kern\z@` before the word.
    pub kern: bool,
}

/// What a name is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Def {
    Char(u32),
    MathChar(u32),
}

impl<'a> KernelTable<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<KernelTable<'a>> {
        let t = KernelTable { chars: std::str::from_utf8(b.section("chars")?).ok()?, ops: std::str::from_utf8(b.section("ops")?).ok()?, delims: std::str::from_utf8(b.section("delims")?).ok()?, macros: std::str::from_utf8(b.section("macros")?).ok()?, mathfonts: std::str::from_utf8(b.section("mathfont")?).ok()?, pkg: match b.section("pkg") {
            Some(p) => Some(std::str::from_utf8(p).ok()?),
            None => None,
        }, blocked: match b.section("blocked") {
            Some(p) => std::str::from_utf8(p).ok()?,
            None => "",
        }, lets: match b.section("lets") {
            Some(p) => std::str::from_utf8(p).ok()?,
            None => "",
        }, accents: match b.section("accents") {
            Some(p) => std::str::from_utf8(p).ok()?,
            None => "",
        } };
        (t.chars.lines().all(|l| parse(l).is_some()) && t.ops.lines().all(|l| parse_op(l).is_some()) && t.delims.lines().all(|l| parse_delim(l).is_some()) && t.macros.lines().all(|l| parse_macro(l).is_some()) && t.mathfonts.lines().all(|l| parse_math_font(l).is_some())).then_some(t)
    }

    /// A package's first own family and the packages it loads (section `pkg`).
    pub fn package(&self) -> Option<(u8, impl Iterator<Item = &'a str>)> {
        let mut f = self.pkg?.trim_end().split('\t');
        let fam0 = f.next()?.parse().ok()?;
        Some((fam0, f.filter(|d| !d.is_empty())))
    }

    /// The names a package changes that the native job does not take after it.
    pub fn blocked(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.blocked.lines()
    }

    /// Names a package `\let`s to a primitive of [`MACRO_PRIMITIVES`] (amsmath's `\DOTSI`
    /// is `\relax`): the name and the primitive's.
    pub fn lets(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.lets.lines().filter_map(|l| l.split_once('\t'))
    }

    /// amsmath's math accents (`\mathaccentV{hat}05E`): the name and its `\mathaccent` code
    /// with `\accentclass@` 7.
    pub fn accents(&self) -> impl Iterator<Item = (&'a str, u32)> + '_ {
        self.accents.lines().filter_map(|l| l.split_once('\t').and_then(|(n, v)| Some((n, v.parse().ok()?))))
    }

    /// The math version `normal`: symbol fonts in family order, and the math alphabets.
    pub fn math_fonts(&self) -> impl Iterator<Item = MathFont<&'a str>> + '_ {
        self.mathfonts.lines().filter_map(parse_math_font)
    }

    /// Every harvested macro's name, its tokens left unread.
    pub fn macro_names(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.macros.lines().filter_map(|l| l.split('\u{1f}').next())
    }

    /// Every harvested macro: name, flags and tokens.
    pub fn macros(&self) -> impl Iterator<Item = (&'a str, u8, Vec<MacroToken<'a>>)> + '_ {
        self.macros.lines().filter_map(parse_macro)
    }

    /// Every math delimiter macro and its `\delimiter` value.
    pub fn delims(&self) -> impl Iterator<Item = (&'a str, u32)> + '_ {
        self.delims.lines().filter_map(parse_delim)
    }

    /// Every log-like operator.
    pub fn ops(&self) -> impl Iterator<Item = Op<'a>> + '_ {
        self.ops.lines().filter_map(parse_op)
    }

    /// Every name and its definition.
    pub fn chars(&self) -> impl Iterator<Item = (&'a str, Def)> + '_ {
        self.chars.lines().filter_map(parse)
    }
}

fn parse(line: &str) -> Option<(&str, Def)> {
    let mut f = line.split('\t');
    let (name, kind, value) = (f.next()?, f.next()?, f.next()?.parse().ok()?);
    Some((name, if kind == "m" { Def::MathChar(value) } else { Def::Char(value) }))
}

fn parse_macro(line: &str) -> Option<(&str, u8, Vec<MacroToken<'_>>)> {
    let mut f = line.split('\u{1f}');
    let (name, flags) = (f.next()?, f.next()?.parse().ok()?);
    let toks = f.map(|t| match t.split_at_checked(1)? {
        ("C", n) => Some(MacroToken::Cs(n)),
        ("T", v) => u32::from_str_radix(v, 16).ok().map(MacroToken::Raw),
        _ => None,
    });
    Some((name, flags, toks.collect::<Option<Vec<_>>>()?))
}

fn parse_math_font(line: &str) -> Option<MathFont<&str>> {
    let f: Vec<&str> = line.split('\t').collect();
    match f[..] {
        ["f", fam, normal, bold] => Some(MathFont::Symbol { fam: fam.parse().ok()?, normal, bold }),
        ["s", name, fam, same] => Some(MathFont::SymbolAlphabet { name, fam: fam.parse().ok()?, same: same == "1" }),
        ["a", name, same, normal, bold] => Some(MathFont::Alphabet { name, same: same == "1", normal, bold }),
        _ => None,
    }
}

fn parse_delim(line: &str) -> Option<(&str, u32)> {
    let (name, v) = line.split_once('\t')?;
    Some((name, v.parse().ok()?))
}

fn parse_op(line: &str) -> Option<Op<'_>> {
    let mut f = line.split('\t');
    let (name, word, n) = (f.next()?, f.next()?, f.next()?);
    let sound = !word.is_empty() && word.replace("\\,", "").bytes().all(|b| b.is_ascii_alphabetic());
    let n: u8 = n.parse().ok()?;
    sound.then_some(Op { name, word, nolimits: n & 1 != 0, kern: n & 2 != 0 })
}

static KERNEL_BLOB: &[u8] = include_bytes!("../../assets/tables/kernel.rtdb");

/// The kernel's table, checked on first use; `None` if it is not a table of this build.
pub fn kernel() -> Option<&'static KernelTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<KernelTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(KERNEL_BLOB).and_then(|b| KernelTable::from_blob(&b))).as_ref()
}

/// The packages with a native table (`rushtex-gen natpkg`), by name.
static PACKAGES: [(&str, &[u8]); 7] = [("amsbsy", include_bytes!("../../assets/tables/pkg-amsbsy.rtdb")), ("amsfonts", include_bytes!("../../assets/tables/pkg-amsfonts.rtdb")), ("amsgen", include_bytes!("../../assets/tables/pkg-amsgen.rtdb")), ("amsmath", include_bytes!("../../assets/tables/pkg-amsmath.rtdb")), ("amsopn", include_bytes!("../../assets/tables/pkg-amsopn.rtdb")), ("amssymb", include_bytes!("../../assets/tables/pkg-amssymb.rtdb")), ("amstext", include_bytes!("../../assets/tables/pkg-amstext.rtdb"))];

/// The native table of package `name`, checked on first use; `None` when it has none.
pub fn package(name: &str) -> Option<&'static KernelTable<'static>> {
    static TABLES: [std::sync::OnceLock<Option<KernelTable<'static>>>; PACKAGES.len()] = [const { std::sync::OnceLock::new() }; PACKAGES.len()];
    let i = PACKAGES.iter().position(|p| p.0 == name)?;
    TABLES[i].get_or_init(|| Blob::new(PACKAGES[i].1).and_then(|b| KernelTable::from_blob(&b)).filter(|t| t.package().is_some())).as_ref()
}

/// The font shapes package `name` declares (an NFSS table in the same blob), if any.
pub fn package_shapes(name: &str) -> Option<&'static super::data_nfss::NfssTable<'static>> {
    static TABLES: [std::sync::OnceLock<Option<super::data_nfss::NfssTable<'static>>>; PACKAGES.len()] = [const { std::sync::OnceLock::new() }; PACKAGES.len()];
    let i = PACKAGES.iter().position(|p| p.0 == name)?;
    TABLES[i].get_or_init(|| Blob::new(PACKAGES[i].1).and_then(|b| super::data_nfss::NfssTable::from_blob(&b))).as_ref()
}

/// Every package with a native table.
pub fn packages() -> impl Iterator<Item = (&'static str, &'static KernelTable<'static>)> {
    PACKAGES.iter().filter_map(|p| package(p.0).map(|t| (p.0, t)))
}

#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, chars: &[(String, Def)], ops: &[(String, String, u8)], delims: &[(String, u32)], macros: &[(String, u8, Vec<MacroToken<'_>>)], math_fonts: &[MathFont<String>]) {
    let mut s = String::new();
    for (name, d) in chars {
        let (k, v) = match d {
            Def::Char(v) => ("c", v),
            Def::MathChar(v) => ("m", v),
        };
        s.push_str(&format!("{name}\t{k}\t{v}\n"));
    }
    w.bytes("chars", s.into_bytes());
    let mut s = String::new();
    for (name, word, flags) in ops {
        s.push_str(&format!("{name}\t{word}\t{flags}\n"));
    }
    w.bytes("ops", s.into_bytes());
    let s: String = delims.iter().map(|(name, v)| format!("{name}\t{v}\n")).collect();
    w.bytes("delims", s.into_bytes());
    let mut s = String::new();
    for (name, flags, toks) in macros {
        s.push_str(&format!("{name}\u{1f}{flags}"));
        for t in toks {
            match t {
                MacroToken::Cs(n) => s.push_str(&format!("\u{1f}C{n}")),
                MacroToken::Raw(v) => s.push_str(&format!("\u{1f}T{v:x}")),
            }
        }
        s.push('\n');
    }
    w.bytes("macros", s.into_bytes());
    let s: String = math_fonts
        .iter()
        .map(|f| match f {
            MathFont::Symbol { fam, normal, bold } => format!("f\t{fam}\t{normal}\t{bold}\n"),
            MathFont::SymbolAlphabet { name, fam, same } => format!("s\t{name}\t{fam}\t{}\n", *same as u8),
            MathFont::Alphabet { name, same, normal, bold } => format!("a\t{name}\t{}\t{normal}\t{bold}\n", *same as u8),
        })
        .collect();
    w.bytes("mathfont", s.into_bytes());
}

/// A package's `blocked` section: the names it changes that the native job does not take
/// after it, one per line.
#[cfg(any(test, feature = "gen"))]
pub fn write_blocked(w: &mut super::data_blob::BlobWriter, names: &[String]) {
    w.bytes("blocked", names.iter().map(|n| format!("{n}\n")).collect::<String>().into_bytes());
}

/// A package's `accents` section: `name<TAB>code` per line.
#[cfg(any(test, feature = "gen"))]
pub fn write_accents(w: &mut super::data_blob::BlobWriter, accents: &[(String, u32)]) {
    w.bytes("accents", accents.iter().map(|(n, v)| format!("{n}\t{v}\n")).collect::<String>().into_bytes());
}

/// A package's `lets` section: `name<TAB>primitive` per line.
#[cfg(any(test, feature = "gen"))]
pub fn write_lets(w: &mut super::data_blob::BlobWriter, lets: &[(String, String)]) {
    w.bytes("lets", lets.iter().map(|(n, p)| format!("{n}\t{p}\n")).collect::<String>().into_bytes());
}

/// A package's `pkg` section: its first own family and the packages it loads.
#[cfg(any(test, feature = "gen"))]
pub fn write_package(w: &mut super::data_blob::BlobWriter, fam0: u8, deps: &[String]) {
    let mut s = fam0.to_string();
    for d in deps {
        s.push('\t');
        s.push_str(d);
    }
    s.push('\n');
    w.bytes("pkg", s.into_bytes());
}
