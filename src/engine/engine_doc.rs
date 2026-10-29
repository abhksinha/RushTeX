//! The native document job (P2, `2026-10-23-p2-native-document-path.md`): a document the
//! pre-scan accepts runs with no format, no TeX files, no `.aux` and one pass.
//!
//! The engine starts from IniTeX's state (`Engine::default`), loads the class's state at
//! the first body token from its generated table (`data_class`), and reads only the body.
//! LaTeX's commands are native (`Meaning::Doc`), and each builds the nodes LaTeX's macros
//! would, from typed state: none of the format's macros, hooks or registers exist.
//!
//! The typesetting itself (characters, ligatures and kerns, `line_break`, the page
//! builder, `\shipout`) is the exact core, unchanged. Only `\output` is native: the page
//! builder fires it as usual, with `{\rt@output}` as the token list.

use super::engine_box::{Ctx, HBOX_GROUP, VBOX_GROUP};
use super::engine_catcode::Cat;
use super::engine_mlist::{CLOSE, INNER, OP, OPEN, ORD, PUNCT, REL};
use super::engine_eqtb::{Code, Glue, Reg, Region};
use super::engine_node::{BoxRec, Kind, Node, IGNORE_DEPTH, RUNNING};
use super::engine_pack::{BASELINE_SKIP, BOX_MAX_DEPTH, HANG_INDENT};
use super::engine_prim::Meaning;
use super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::engine_scan::{scan_dimen_ref, scan_glue};
use super::engine_token::Token;
use crate::data::data_class::{self, ClassTable};
use crate::data::data_nfss;

/// `doc!("name")`: the `Meaning::Doc` index of native `name`, computed when the code is
/// compiled.
macro_rules! doc {
    ($name:literal) => {
        const { $crate::engine::engine_doc::doc_index($name) }
    };
}

/// `param!("name")`: TeX's parameter `name`, resolved when the code is compiled.
macro_rules! param {
    ($name:literal) => {
        const { $crate::engine::engine_names::param_const($name) }
    };
}

/// `named!("name")`: the index in [`NAMED`] of `name`, computed when the code is compiled.
macro_rules! named {
    ($name:literal) => {
        const { $crate::engine::engine_doc::named_index($name) }
    };
}

#[path = "engine_tabular.rs"]
mod tabular;
#[path = "engine_ams_align.rs"]
mod ams_align;
#[path = "engine_eqnarray.rs"]
mod eqnarray;
#[path = "engine_float.rs"]
mod float;
#[path = "engine_parbox.rs"]
mod parbox;
#[path = "engine_xcolor.rs"]
mod xcolor;
#[path = "engine_fancy.rs"]
mod fancy;
#[path = "engine_url.rs"]
mod url;
#[path = "engine_hyperref.rs"]
mod hyperref;

type Native = fn(&mut Machine);

/// One step of a native command's vertical-mode work. A step that may build the page
/// (`\par`, `\penalty`, `build_page`, or a call that may do one of them) leaves the steps
/// after it behind `\rt@step` on the input, as TeX leaves the rest of a macro's tokens
/// under the output routine it fires (§1025).
#[derive(Clone, Copy)]
enum Op {
    Par,
    Penalty(i32),
    Vskip(Glue),
    Build,
    Call(Native),
}
/// A file reader: the file's bytes, or `None` when it is not found.
pub type Read<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// What a text font command changes (`\fontseries`, `\fontshape`, `\fontfamily`, `\em`).
/// The kernel's `\rmdefault`, `\sfdefault`, `\ttdefault`.
const DEFAULT_FAMILIES: [&str; 3] = ["cmr", "cmss", "cmtt"];

/// The font defaults the native job keeps as the kernel has them.
const FONT_FIXED: [&str; 10] = ["familydefault", "encodingdefault", "seriesdefault", "shapedefault", "mddefault", "bfdefault", "itdefault", "sldefault", "scdefault", "updefault"];

/// A family name as a `&'static str`: the families the pin's `.fd` files declare are few,
/// so each new name is leaked once per process and reused after.
fn family_name(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut v = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = v.iter().find(|n| **n == name) {
        return n;
    }
    let n: &'static str = Box::leak(name.to_string().into_boxed_str());
    v.push(n);
    n
}

#[derive(Clone, Copy)]
enum Change {
    Series(&'static str),
    Shape(&'static str),
    /// `\rmfamily`, `\sffamily`, `\ttfamily`: the family `\rmdefault`, `\sfdefault` or
    /// `\ttdefault` names (`Doc::families`).
    Default(u8),
    /// `\em`: `\upshape` in a slanted font, else `\itshape`.
    Em,
}

/// Commands a native package table redefines that the native job runs as Rust natives
/// (package, name, `DOC` name): the package's definition is not data, but a mechanism.
pub const PACKAGE_NATIVES: [(&str, &str, &str); 103] = [("array", "arraybackslash", "rt@arraybackslash"), ("enumitem", "setlist", "rt@setlist"), ("amsmath", "allowdisplaybreaks", "rt@allowdisplaybreaks"), ("bm", "bm", "rt@bm"), ("amsthm", "theoremstyle", "rt@thmstyle"), ("amsthm", "qed", "rt@qed"), ("amsthm", "qedsymbol", "rt@qedsymbol"), ("amsthm", "openbox", "rt@openbox"), ("amsthm", "proofname", "rt@proofname"), ("amsmath", "bigl", "rt@big1l"), ("amsmath", "bigr", "rt@big1r"), ("amsmath", "bigm", "rt@big1m"), ("amsmath", "Bigl", "rt@big15l"), ("amsmath", "Bigr", "rt@big15r"), ("amsmath", "Bigm", "rt@big15m"), ("amsmath", "biggl", "rt@big2l"), ("amsmath", "biggr", "rt@big2r"), ("amsmath", "biggm", "rt@big2m"), ("amsmath", "Biggl", "rt@big25l"), ("amsmath", "Biggr", "rt@big25r"), ("amsmath", "Biggm", "rt@big25m"), ("amsmath", "cdots", "rt@cdots"), ("amsmath", "dotsb", "rt@cdots"), ("amsmath", "dotsm", "rt@cdots"), ("amsmath", "dotsc", "rt@dotsc"), ("amsmath", "dotsi", "rt@dotsi"), ("amsmath", "dotso", "rt@dotso"), ("amsmath", "colon", "rt@colon"), ("amsmath", "big", "rt@big1"), ("amsmath", "Big", "rt@big15"), ("amsmath", "bigg", "rt@big2"), ("amsmath", "Bigg", "rt@big25"), ("amstext", "text", "rt@text"), ("amsopn", "operatorname", "rt@operatorname"), ("amsopn", "DeclareMathOperator", "rt@declmathop"), ("amsfonts", "widehat", "rt@widehat"), ("amsfonts", "widetilde", "rt@widetilde"), ("graphicx", "includegraphics", "rt@includegraphics"), ("amsmath", "eqref", "rt@eqref"), ("amsmath", "nonumber", "rt@nonumber"), ("amsmath", "notag", "rt@nonumber"), ("amsmath", "genfrac", "rt@genfrac"), ("amsmath", "dfrac", "rt@dfrac"), ("amsmath", "tfrac", "rt@tfrac"), ("amsmath", "binom", "rt@binom"), ("amsmath", "dbinom", "rt@dbinom"), ("amsmath", "tbinom", "rt@tbinom"), ("amsmath", "numberwithin", "rt@numberwithin"), ("amsmath", "pod", "rt@pod"), ("amsmath", "mod", "rt@mod"), ("amsmath", "overset", "rt@overset"), ("amsmath", "underset", "rt@underset"), ("amsmath", "overunderset", "rt@overunderset"), ("amsmath", "tag", "rt@tag"), ("amsbsy", "boldsymbol", "rt@boldsymbol"), ("amsmath", "substack", "rt@substack"), ("amsmath", "xrightarrow", "rt@xrightarrow"), ("amsmath", "xleftarrow", "rt@xleftarrow"), ("amsmath", "overrightarrow", "overrightarrow"), ("amsmath", "overleftarrow", "overleftarrow"), ("amsmath", "overleftrightarrow", "rt@overleftrightarrow"), ("amsmath", "hdotsfor", "rt@hdotsfor"), ("amsmath", "root", "root"), ("amsmath", "over", "rt@over"), ("amsmath", "atop", "rt@atop"), ("amsmath", "above", "rt@above"), ("amsmath", "overwithdelims", "rt@overwithdelims"), ("amsmath", "atopwithdelims", "rt@atopwithdelims"), ("amsmath", "abovewithdelims", "rt@abovewithdelims"), ("amsmath", "eqno", "rt@eqno"), ("amsmath", "leqno", "rt@leqno"), ("amsmath", "doteq", "doteq"), ("booktabs", "toprule", "rt@toprule"), ("booktabs", "midrule", "rt@midrule"), ("booktabs", "bottomrule", "rt@bottomrule"), ("booktabs", "specialrule", "rt@specialrule"), ("booktabs", "addlinespace", "rt@addlinespace"), ("booktabs", "cmidrule", "rt@cmidrule"), ("booktabs", "morecmidrules", "rt@morecmidrules"), ("multirow", "multirow", "rt@multirow"), ("multirow", "multirowsetup", "raggedright"), ("geometry", "geometry", "rt@geometry"), ("color", "color", "rt@color"), ("color", "textcolor", "rt@textcolor"), ("color", "definecolor", "rt@definecolor"), ("xcolor", "color", "rt@color"), ("xcolor", "textcolor", "rt@textcolor"), ("xcolor", "definecolor", "rt@definecolor"), ("url", "url", "rt@url"), ("url", "path", "rt@path"), ("url", "urlstyle", "rt@urlstyle"), ("hyperref", "href", "rt@href"), ("hyperref", "nolinkurl", "rt@nolinkurl"), ("hyperref", "hypersetup", "rt@hypersetup"), ("fancyhdr", "fancyhf", "rt@fancyhf"), ("fancyhdr", "fancyhead", "rt@fancyhead"), ("fancyhdr", "fancyfoot", "rt@fancyfoot"), ("fancyhdr", "lhead", "rt@lhead"), ("fancyhdr", "chead", "rt@chead"), ("fancyhdr", "rhead", "rt@rhead"), ("fancyhdr", "lfoot", "rt@lfoot"), ("fancyhdr", "cfoot", "rt@cfoot"), ("fancyhdr", "rfoot", "rt@rfoot")];

/// After `\tag` in a source text: `*{text}` or `{text}` as the label value amsmath gives it
/// (spaces collapsed as the tokens have them) and the bytes used, or `None` for a text the
/// pre-scan does not read (commands, groups, math).
fn tag_text(after: &str) -> Option<(String, usize)> {
    let rest = after.strip_prefix('*').unwrap_or(after);
    let body = rest.strip_prefix('{')?;
    let k = balanced(body)?;
    let raw = &body[..k];
    if raw.contains(['\\', '{', '$', '%', '~', '#']) {
        return None;
    }
    let mut text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if raw.starts_with(char::is_whitespace) {
        text.insert(0, ' ');
    }
    if raw.len() > 1 && raw.ends_with(char::is_whitespace) {
        text.push(' ');
    }
    Some((text, after.len() - rest.len() + 2 + k))
}

/// `\thesection`, `\thesubsection`, ... to `level`: the heading counters joined by `.`,
/// the section `\@Alph` after `\appendix` (`A` to `Z`, nothing for 0).
fn heading_number<T: Copy + Into<i64>>(levels: &[T], level: usize, appendix: bool) -> String {
    let mut text = String::new();
    for (k, &c) in levels[..=level].iter().enumerate() {
        if k > 0 {
            text.push('.');
        }
        match c.into() {
            n @ 1..=26 if k == 0 && appendix => text.push((b'A' + n as u8 - 1) as char),
            _ if k == 0 && appendix => {}
            n => text.push_str(&n.to_string()),
        }
    }
    text
}

/// The counters whose `\the` a preamble may give a plain prefix: `figure`, `table`,
/// `equation`.
const PREFIXED: [&str; 3] = ["figure", "table", "equation"];

/// `\renewcommand{\the<counter>}{<prefix>\arabic{<counter>}}` after the name: the prefix
/// (letters, digits, `.`, `-`), or `None` for any other text.
fn the_prefix<'a>(counter: &str, after: &'a str) -> Option<&'a str> {
    let r = after.strip_prefix('}').unwrap_or(after).trim_start().strip_prefix('{')?;
    let (prefix, r) = r.split_once("\\arabic")?;
    r.trim_start().strip_prefix('{')?.strip_prefix(counter)?.strip_prefix("}}")?;
    prefix.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-').then_some(prefix)
}

/// The preamble's prefixes of `\thefigure`, `\thetable`, `\theequation` (`S` for
/// `S\arabic{figure}`), in `PREFIXED` order; the pre-scan refuses any other form.
fn the_prefixes(src: &str) -> [String; 3] {
    let pre = &src[..src.find("\\begin{document}").unwrap_or(src.len())];
    let mut out: [String; 3] = Default::default();
    for (k, c) in PREFIXED.iter().enumerate() {
        let name = format!("\\the{c}");
        for (at, _) in pre.match_indices(&name) {
            if let Some(p) = the_prefix(c, &pre[at + name.len()..]) {
                out[k] = p.to_string();
            }
        }
    }
    out
}

/// A counter's `\the` when it is numbered within a heading level: the level's number,
/// then the counter, joined by `.` (`\thesection.\arabic{equation}`).
fn within_text<T: Copy + Into<i64>>(levels: &[T], appendix: bool, within: Option<usize>, n: impl std::fmt::Display) -> String {
    let mut text = within.map(|l| heading_number(levels, l, appendix) + ".").unwrap_or_default();
    text.push_str(&n.to_string());
    text
}

/// A package table's families moved up by `by` when other math fonts came before it: those
/// from `from` on (the table's own), in a family number or a code's family digit.
#[derive(Clone, Copy, Default)]
struct Relocate {
    from: u32,
    by: u32,
}

impl Relocate {
    fn fam(self, f: u32) -> u32 {
        if f >= self.from { f + self.by } else { f }
    }

    /// `v` with its family digit at bit `at` moved.
    fn at(self, v: u32, at: u32) -> u32 {
        let f = v >> at & 15;
        (v & !(15 << at)) | self.fam(f) << at
    }
}

/// `\tmspace`'s arguments for [`Machine::tm_space`]: negative, the `\thinmuskip`,
/// `\medmuskip` or `\thickmuskip` slot, and the em width in the kernel and with amsmath.
const TM_SPACES: [(bool, u32, &str, &str); 6] = [
    (false, super::engine_mlist::THIN_MU_SKIP, ".16667em", ".1667em"),
    (true, super::engine_mlist::THIN_MU_SKIP, ".16667em", ".1667em"),
    (false, super::engine_mlist::THIN_MU_SKIP + 1, ".2222em", ".2222em"),
    (true, super::engine_mlist::THIN_MU_SKIP + 1, ".2222em", ".2222em"),
    (false, super::engine_mlist::THIN_MU_SKIP + 2, ".2777em", ".2777em"),
    (true, super::engine_mlist::THIN_MU_SKIP + 2, ".2777em", ".2777em"),
];

/// [`Machine::mod_op`]'s commands: `\bmod`, `\pmod`, and amsmath's `\pod` and `\mod`.
const BMOD: u8 = 0;
const PMOD: u8 = 1;
const POD: u8 = 2;
const MOD: u8 = 3;

/// [`Machine::smash`]'s kinds: `\smash`, or a phantom keeping the height and depth (`V`),
/// the width (`H`) or both.
const SMASH: u8 = 0;
const PHANTOM_V: u8 = 1;
const PHANTOM_H: u8 = 2;

/// `0pt`, in place of `\z@` as a thickness (`\binom`).
const ZERO_PT: [Token; 3] = [Token::char(Cat::Other, '0'), Token::char(Cat::Letter, 'p'), Token::char(Cat::Letter, 't')];

/// Names a package changes that the native job follows in its own natives once the package
/// is loaded (`Machine::ams`): `rushtex-gen natpkg` neither blocks nor holds them.
pub const PACKAGE_FOLLOWS: [(&str, &str); 63] = [("enumerate", "enumerate"), ("mathtools", "gathered"), ("mathtools", "endgathered"), ("mathtools", "overbrace"), ("mathtools", "underbrace"), ("calc", "addtocounter"), ("calc", "addtolength"), ("calc", "addvspace"), ("calc", "appendix"), ("calc", "caption"), ("calc", "endtitlepage"), ("calc", "enumerate"), ("calc", "eqnarray"), ("calc", "equation"), ("calc", "footnote"), ("calc", "footnotemark"), ("calc", "maketitle"), ("calc", "part"), ("calc", "refstepcounter"), ("calc", "setcounter"), ("calc", "setlength"), ("calc", "stepcounter"), ("calc", "thanks"), ("calc", "thebibliography"), ("calc", "titlepage"), ("calc", "usecounter"), ("mathtools", "endalign"), ("mathtools", "endalignat"), ("mathtools", "endflalign"), ("mathtools", "endxalignat"), ("mathtools", "endxxalignat"), ("mathtools", "nonumber"), ("mathtools", "notag"), ("enumitem", "center"), ("enumitem", "endenumerate"), ("enumitem", "enditemize"), ("enumitem", "enumerate"), ("enumitem", "flushleft"), ("enumitem", "flushright"), ("enumitem", "itemize"), ("enumitem", "description"), ("enumitem", "enddescription"), ("enumitem", "trivlist"), ("float", "caption"), ("amsmath", "smash"), ("amsmath", "bmod"), ("amsmath", "pmod"), ("amsmath", ";"), ("amsmath", "!"), ("amsmath", "negthinspace"), ("amsmath", ":"), ("amsmath", "medspace"), ("amsmath", "negmedspace"), ("amsmath", "negthickspace"), ("amsmath", "thickspace"), ("amsmath", "dots"), ("amsmath", "ldots"), ("amsmath", "equation"), ("amsmath", "endequation"), ("amsmath", "["), ("amsmath", "]"), ("amsmath", ","), ("amsmath", "thinspace")];

/// `\paperwidth` and `\paperheight` of each of `data_class::PAPERS`, as article's
/// options set them (`8.5in`, `210mm`, …), in sp.
pub(super) const PAPER_SIZES: [[i32; 2]; 6] = [[40258437, 52099153], [40258437, 66308014], [34338078, 49731010], [39158276, 55380990], [27597261, 39158276], [32818364, 46616995]];

/// `\pdfpagewidth` (`\pdfpageheight` follows it).
pub const PDF_PAGE_WIDTH: u32 = 23;

/// Packages after which `\begin{document}` sets the PDF page to the paper size: those that
/// load pdftex.def (mathtools loads graphicx in its `\AtBeginDocument`).
pub const PAGE_SIZE_PACKAGES: [&str; 5] = ["graphicx", "color", "xcolor", "hyperref", "mathtools"];

/// Packages the native job runs as Rust alone, with no evaluated table: their commands
/// are [`PACKAGE_NATIVES`]. graphicx (with keyval, graphics, trig and pdftex.def).
/// amsthm (`\theoremstyle`, `\newtheorem*`, `proof`, `\qed`) is lowered the same way.
const DRIVER_PACKAGES: [&str; 15] = ["graphicx", "amsthm", "booktabs", "multirow", "array", "tabularx", "geometry", "color", "xcolor", "url", "hyperref", "fancyhdr", "microtype", "siunitx", "parskip"];
/// booktabs' registers in the order `booktabs.sty` allocates them (`\newdimen`, `\newcount`).
const BT_REGS: [(&str, Region); 17] = [("heavyrulewidth", Region::Dimen), ("lightrulewidth", Region::Dimen), ("cmidrulewidth", Region::Dimen), ("belowrulesep", Region::Dimen), ("belowbottomsep", Region::Dimen), ("aboverulesep", Region::Dimen), ("abovetopsep", Region::Dimen), ("cmidrulesep", Region::Dimen), ("cmidrulekern", Region::Dimen), ("defaultaddspace", Region::Dimen), ("@cmidla", Region::Int), ("@cmidlb", Region::Int), ("@aboverulesep", Region::Dimen), ("@belowrulesep", Region::Dimen), ("@thisruleclass", Region::Int), ("@lastruleclass", Region::Int), ("@thisrulewidth", Region::Dimen)];
/// The first hidden register of [`BT_REGS`] (the `k`th at `BT_BASE + k` in its region).
const BT_BASE: u32 = 32600;
/// [`BT_REGS`] indexes.
pub(super) const BT_HEAVY: usize = 0;
pub(super) const BT_LIGHT: usize = 1;
pub(super) const BT_CMID_WIDTH: usize = 2;
pub(super) const BT_BELOW: usize = 3;
pub(super) const BT_BELOW_BOTTOM: usize = 4;
pub(super) const BT_ABOVE: usize = 5;
pub(super) const BT_ABOVE_TOP: usize = 6;
pub(super) const BT_CMID_SEP: usize = 7;
pub(super) const BT_CMID_KERN: usize = 8;
pub(super) const BT_ADD_SPACE: usize = 9;
pub(super) const BT_AT_ABOVE: usize = 12;
pub(super) const BT_AT_BELOW: usize = 13;
pub(super) const BT_THIS_CLASS: usize = 14;
pub(super) const BT_LAST_CLASS: usize = 15;
pub(super) const BT_THIS_WIDTH: usize = 16;
/// booktabs' rules, `\toprule` to `\addlinespace`: the kinds of `bt_rule`.
pub(super) const BT_RULES: [usize; 6] = [doc!("rt@toprule"), doc!("rt@midrule"), doc!("rt@bottomrule"), doc!("rt@specialrule"), doc!("rt@addlinespace"), doc!("rt@cmidrule")];
pub(super) const BT_MORE: usize = doc!("rt@morecmidrules");
/// After a `\cmidrule`'s row: `\noalign{\futurenonspacelet\@tempa\@xcmidrule}`.
pub(super) const BT_CMID_END: usize = doc!("rt@cmidend");
/// The meaning of the active characters `^^c2` to `^^f4`, utf8.def's lead bytes.
pub(super) const UTF8_LEAD: usize = doc!("rt@utf8");
/// T1's accent slots 0 to 10: the OT1 slot of the same accent, and its command.
const T1_ACCENT_OT1: [u32; 11] = [18, 19, 94, 126, 127, 125, 23, 20, 21, 22, 95];
const T1_ACCENT_CMD: &[u8; 11] = b"`'^~\"Hrvu=.";

/// Packages that change nothing the native job reads in a document it accepts: inputenc
/// defines only `\inputencoding`, `\DeclareInputText` and `\DeclareInputMath`, and
/// with `utf8` (LaTeX's default input encoding) leaves the state as it was
/// (`rushtex-gen natpkg inputenc` and `inputenc[utf8]`).
const NO_EFFECT_PACKAGES: [&str; 1] = ["inputenc"];

/// Package options with the same effect as none, as `rushtex-gen natpkg name[option]`
/// finds (the state after it is the package's own): amsmath's defaults, `psamsfonts`
/// (the same fonts in TeX Live), inputenc's `utf8`; graphicx's `pdftex` and `final` are
/// what `graphics.cfg` and `\ExecuteOptions` choose under pdfTeX.
const SAME_OPTIONS: [(&str, &str); 16] = [
    ("xcolor", "usenames"),
    ("xcolor", "dvipsnames"),
    ("xcolor", "svgnames"),
    ("xcolor", "pdftex"),
    ("color", "usenames"),
    ("color", "dvipsnames"),
    ("color", "pdftex"),
    ("amsfonts", "psamsfonts"),
    ("amsmath", "centertags"),
    ("amsmath", "namelimits"),
    ("amsmath", "nointlimits"),
    ("amsmath", "sumlimits"),
    ("amssymb", "psamsfonts"),
    ("graphicx", "final"),
    ("graphicx", "pdftex"),
    ("inputenc", "utf8"),
];

/// `\DeclareFontSubstitution` of the encodings the native path sets (`fontdef.dtx`,
/// `t1enc.def`, `ot1enc.def`): the default family, series and shape.
const FONT_SUBSTITUTIONS: [(&str, [&str; 3]); 6] = [("OT1", ["cmr", "m", "n"]), ("T1", ["cmr", "m", "n"]), ("OML", ["cmm", "m", "it"]), ("OMS", ["cmsy", "m", "n"]), ("OMX", ["cmex", "m", "n"]), ("U", ["cmr", "m", "n"])];

/// `\wrong@fontshape` for `key` (`enc/family/series/shape`): the default shape, then the
/// default series too, then the default family too; the first that `known` takes. The
/// encoding never changes.
fn wrong_font_shape(key: &str, known: impl Fn(&str) -> bool) -> Option<String> {
    let mut p = key.split('/');
    let (enc, fam, ser) = (p.next()?, p.next()?, p.next()?);
    let [dfam, dser, dshape] = FONT_SUBSTITUTIONS.iter().find(|s| s.0 == enc)?.1;
    [format!("{enc}/{fam}/{ser}/{dshape}"), format!("{enc}/{fam}/{dser}/{dshape}"), format!("{enc}/{dfam}/{dser}/{dshape}")].into_iter().find(|k| known(k))
}

/// `\strip@pt`: the length printed as `\the` prints it, without `pt` and without a final
/// `.0`.
fn strip_pt(sp: i32) -> String {
    let mut out = String::new();
    super::engine_value::print_scaled(sp, &mut out);
    if out.ends_with(".0") {
        out.truncate(out.len() - 2);
    }
    out
}

/// Whether every option of `opts` (a comma list) has the same effect as none for `pkg`.
fn same_options(pkg: &str, opts: &str) -> bool {
    opts.split(',').map(str::trim).filter(|o| !o.is_empty()).all(|o| SAME_OPTIONS.contains(&(pkg, o)))
}

/// Whether the native job writes `name` itself, so a table's macro of that name is not
/// installed (`install`): a `DOC` native or an OT1 text symbol.
pub fn native_name(name: &str) -> bool {
    DOC.iter().any(|d| d.0 == name) || TEXT_SYMBOLS.iter().any(|s| s.0 == name)
}

/// amsmath's dots: `\cdots` (`\extrap@\@cdots`), `\dotsb@`, `\dotsc`, `\dotsi`, `\dotso@`, or `\dots`
/// choosing by its next token.
#[derive(Clone, Copy, PartialEq)]
enum Dots {
    Cdots,
    B,
    C,
    I,
    O,
    Auto,
}

/// A math alphabet's family: a symbol font's, or one given on first use.
#[derive(Clone, Copy)]
enum Alphabet {
    Fam(u8),
    Shape(&'static str, Option<u8>),
}

// Each native's name is its own: `doc!` finds the first.
const _: () = {
    let mut i = 0;
    while i < DOC_TABLE.len() {
        let mut j = i + 1;
        while j < DOC_TABLE.len() {
            assert!(!same(DOC_TABLE[i].0, DOC_TABLE[j].0), "a native named twice");
            j += 1;
        }
        i += 1;
    }
};

/// The `Meaning::Doc` index of native `name`.
const fn doc_index(name: &str) -> usize {
    let mut i = 0;
    while i < DOC_TABLE.len() {
        if same(DOC_TABLE[i].0, name) {
            return i;
        }
        i += 1;
    }
    panic!("no such native")
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut k = 0;
    while k < a.len() && a[k] == b[k] {
        k += 1;
    }
    k == a.len()
}

/// The control sequences natives reach by name, so that a document's redefinition of one
/// is honoured (`guarded-direct-lowering.md` §8): each is interned once, when the job
/// starts, and named by its index here (`named!`).
const NAMED: [&str; 166] = ["begin", "end", "\\", "name", "lbrace", "rbrace", "openbox", "par", "cr", "crcr", "arraystretch", "omit", "span", "@empty", "relax", "char", "hbox", "long", "def", "mbox", "stepcounter", "refstepcounter", "addtocounter", "label", "footnote", "displaystyle", "textstyle", "boldsymbol", "not", "DOTSB", "mathbin", "mathrel", "DOTSI", "cdotp", "ldotp", "rbrack", "}", "rangle", "rceil", "rfloor", "rgroup", "rmoustache", "right", "bigr", "biggr", "Bigr", "Biggr", "DOTSX", "{", "left", ",", ";", "quad", "qquad", "text", "]", ")", "nonumber", "notag", "rt@operatorname", "rt@reserveda", "newcommand", "hfill", "sqrtsign", "abstractname", "refname", "@preamblecmds", "qed", "eqno", "endgroup", "nobreakspace", "kern", "penalty", "interlinepenalty", "i", "j", "proofname", "qedsymbol", "setbox", "hss", "hfil", "hskip", "lvert", "rvert", "lVert", "rVert", "today", "begingroup", "vskip", "LARGE", "large", "lineskip", "ignorespaces", "prime", "mathsurround", "insert", "noindent", "vrule", "topfraction", "bottomfraction", "textfraction", "floatpagefraction", "figurename", "tablename", "over", "above", "overwithdelims", "abovewithdelims", "scriptstyle", "scriptscriptstyle", "ht", "dp", "wd", "box", "mathchoice", "ifvmode", "indent", "fi", "nonscript", "mskip", "medmuskip", "mkern", "fam", "thinmuskip", "thickmuskip", "ifdim", "aftergroup", "else", "mathop", "limits", "vbox", "baselineskip", "lineskiplimit", "raise", "mathinner", "vcenter", "halign", "everycr", "tabskip", "noalign", "cleaders", "unhbox", "copy", "relbar", "rightarrow", "leftarrow", "smash", "mathord", "prevdepth", "thickspace", "xleaders", "rootbox", "dimen", "advance", "uproot", "leftroot", "of", "vtop", "vfill", "vss", "multirowsetup", "baselinestretch", "selectfont", "unvbox", "lower", "hrule"];

/// The index in [`NAMED`] of `name`.
const fn named_index(name: &str) -> usize {
    let mut i = 0;
    while i < NAMED.len() {
        if same(NAMED[i], name) {
            return i;
        }
        i += 1;
    }
    panic!("no such name")
}

/// The native commands, by `Meaning::Doc` index. Names starting `rt@` are the steps of a
/// command that typesets material in between (read back from the input).
static DOC: [(&str, Native); 455] = DOC_TABLE;
const DOC_TABLE: [(&str, Native); 455] = [
    ("rt@output", |m| m.output()),
    ("rt@outputfoot", |m| m.output_page()),
    ("section", |m| m.section(0)),
    ("subsection", |m| m.section(1)),
    ("subsubsection", |m| m.section(2)),
    ("rt@secbox", |m| m.section_box()),
    ("rt@sectitle", |m| m.section_title()),
    ("rt@secend", |m| m.section_end()),
    ("rt@textend", |m| m.text_end(true)),
    ("rt@textendn", |m| m.text_end(false)),
    ("rt@maybeic", |m| m.maybe_ic()),
    ("emph", |m| m.text_font(Change::Em)),
    ("textbf", |m| m.text_font(Change::Series("bx"))),
    ("textmd", |m| m.text_font(Change::Series("m"))),
    ("textit", |m| m.text_font(Change::Shape("it"))),
    ("textsl", |m| m.text_font(Change::Shape("sl"))),
    ("textsc", |m| m.text_font(Change::Shape("sc"))),
    ("textup", |m| m.text_font(Change::Shape("n"))),
    ("textrm", |m| m.text_font(Change::Default(0))),
    ("textsf", |m| m.text_font(Change::Default(1))),
    ("texttt", |m| m.text_font(Change::Default(2))),
    // `\UseTextSymbol`: `\textdollar` from TS1, `\textbraceleft`/`right` from OMS.
    ("$", |m| {
        if !m.t1_symbol("TS1", 36) {
            m.text_symbol("TS1", 36)
        }
    }),
    // `\protected\def\{{\ifmmode\lbrace\else\textbraceleft\fi}`.
    ("{", |m| m.brace_symbol(named!("lbrace"), 102)),
    ("}", |m| m.brace_symbol(named!("rbrace"), 103)),
    ("_", |m| m.text_underscore()),
    ("textunderscore", |m| m.text_underscore()),
    ("LaTeX", |m| m.logo(true)),
    ("TeX", |m| m.logo(false)),
    ("ldots", |m| m.ellipsis()),
    ("dots", |m| m.dots()),
    ("textellipsis", |m| m.ellipsis()),
    (",", |m| m.thin_space()),
    ("smash", |m| m.smash(SMASH)),
    ("phantom", |m| m.smash(PHANTOM_V | PHANTOM_H)),
    ("vphantom", |m| m.smash(PHANTOM_V)),
    ("hphantom", |m| m.smash(PHANTOM_H)),
    ("mathsm@sh", |m| m.math_smash()),
    ("textbackslash", |m| m.text_symbol_command("OMS", 110)),
    ("textbar", |m| m.text_symbol_command("OMS", 106)),
    ("textbardbl", |m| m.text_symbol_command("TS1", 134)),
    ("textbraceleft", |m| m.text_symbol_command("OMS", 102)),
    ("textbraceright", |m| m.text_symbol_command("OMS", 103)),
    ("textbullet", |m| m.text_symbol_command("TS1", 136)),
    ("textdagger", |m| m.text_symbol_command("TS1", 132)),
    ("textdaggerdbl", |m| m.text_symbol_command("TS1", 133)),
    ("textparagraph", |m| m.text_symbol_command("TS1", 182)),
    ("textsection", |m| m.text_symbol_command("TS1", 167)),
    ("textcent", |m| m.text_symbol_command("TS1", 162)),
    ("textcopyright", |m| m.text_symbol_command("TS1", 169)),
    ("textdegree", |m| m.text_symbol_command("TS1", 176)),
    ("textdiv", |m| m.text_symbol_command("TS1", 246)),
    ("textonehalf", |m| m.text_symbol_command("TS1", 189)),
    ("textpm", |m| m.text_symbol_command("TS1", 177)),
    ("textregistered", |m| m.text_symbol_command("TS1", 174)),
    ("texttimes", |m| m.text_symbol_command("TS1", 214)),
    ("texttrademark", |m| m.text_symbol_command("TS1", 151)),
    ("textperiodcentered", |m| m.text_symbol_command("TS1", 183)),
    ("textless", |m| m.text_symbol_command("OML", 60)),
    ("textgreater", |m| m.text_symbol_command("OML", 62)),
    ("bmod", |m| m.mod_op(BMOD)),
    ("pmod", |m| m.mod_op(PMOD)),
    ("rt@pod", |m| m.mod_op(POD)),
    ("vdots", |m| m.vdots()),
    ("ddots", |m| m.ddots()),
    ("rt@overset", |m| m.over_under_set(1)),
    ("rt@underset", |m| m.over_under_set(2)),
    ("rt@overunderset", |m| m.over_under_set(3)),
    ("rt@mod", |m| m.mod_op(MOD)),
    ("boldmath", |m| m.math_version(1)),
    ("unboldmath", |m| m.math_version(0)),
    ("mathversion", |m| m.math_version(2)),
    ("rt@boldversion", |m| m.set_math_version(true)),
    ("rt@glbsettings", |m| m.glb_settings()),
    ("rt@boldsymbol", |m| m.boldsymbol()),
    ("rt@bm", |m| m.bm()),
    ("rt@substack", |m| m.substack()),
    ("rt@xrightarrow", |m| m.ext_arrow(false)),
    ("rt@xleftarrow", |m| m.ext_arrow(true)),
    ("overrightarrow", |m| m.over_arrow(0)),
    ("notin", |m| m.palette_relation(false)),
    ("cong", |m| m.palette_relation(true)),
    ("underbrace", |m| m.brace_over_under(true)),
    ("overbrace", |m| m.brace_over_under(false)),
    ("overleftarrow", |m| m.over_arrow(1)),
    ("rt@overleftrightarrow", |m| m.over_arrow(2)),
    ("rt@letmathcr", |m| m.let_math_cr()),
    ("rt@mathcr", |m| m.math_cr()),
    ("rt@hdotsfor", |m| m.not_native("\\hdotsfor after an entry's start")),
    ("rt@toprule", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@midrule", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@bottomrule", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@specialrule", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@addlinespace", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@cmidrule", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@morecmidrules", |m| m.not_native("a booktabs rule inside a row")),
    ("rt@cmidend", |m| m.not_native("a \\cmidrule's row not ended")),
    ("rt@cmidleaders", |m| m.cmid_leaders()),
    ("rt@utf8", |m| m.not_native("a UTF-8 lead byte not read as a character")),
    ("strut", |m| m.strut()),
    ("rt@multirow", |m| m.multirow()),
    ("rt@geometry", |m| m.geometry_command()),
    // `\color@setgroup`'s `\set@color` with `\current@color`: a push, and the pop after the
    // group.
    ("rt@url", |m| m.url_command(false, true)),
    ("rt@nolinkurl", |m| m.url_command(false, false)),
    ("rt@href", |m| m.href()),
    ("rt@hypersetup", |m| m.hypersetup()),
    ("rt@startlink", |m| m.start_link()),
    ("rt@endlink", |m| m.end_link()),
    ("rt@path", |m| m.url_command(true, false)),
    ("rt@urlstyle", |m| m.url_style()),
    ("rt@color", |m| m.color_command()),
    ("rt@textcolor", |m| m.text_color()),
    ("rt@definecolor", |m| m.define_color()),
    ("rt@setcolor", |m| {
        let c = m.current_color();
        m.color_stack(Some(&c));
        let t = [m.named_cs(named!("aftergroup")), m.doc_id(doc!("rt@resetcolor"))];
        m.back_list(&t);
    }),
    ("rt@resetcolor", |m| m.color_stack(None)),
    ("rt@multirowend", |m| m.multirow_end(false)),
    ("rt@multirowendt", |m| m.multirow_end(true)),
    ("root", |m| m.root()),
    ("buildrel", |m| m.buildrel(false)),
    ("doteq", |m| m.buildrel(true)),
    ("rt@over", |m| m.prim_frac("over")),
    ("rt@atop", |m| m.prim_frac("atop")),
    ("rt@above", |m| m.prim_frac("above")),
    ("rt@overwithdelims", |m| m.prim_frac("overwithdelims")),
    ("rt@atopwithdelims", |m| m.prim_frac("atopwithdelims")),
    ("rt@abovewithdelims", |m| m.prim_frac("abovewithdelims")),
    ("rt@eqno", |m| m.ams_eqno(false)),
    ("rt@leqno", |m| m.ams_eqno(true)),
    ("thinspace", |m| m.thin_space()),
    ("!", |m| m.tm_space(1)),
    ("negthinspace", |m| m.tm_space(1)),
    (":", |m| m.tm_space(2)),
    (">", |m| m.tm_space(2)),
    ("medspace", |m| m.tm_space(2)),
    ("negmedspace", |m| m.tm_space(3)),
    (";", |m| m.tm_space(4)),
    ("thickspace", |m| m.tm_space(4)),
    ("negthickspace", |m| m.tm_space(5)),
    ("@", |m| m.space_factor_1000()),
    ("nobreakspace", |m| m.nobreak_space()),
    ("\0~", |m| m.nobreak_space()),
    ("mbox", |m| m.mbox()),
    ("newcommand", |m| m.new_command()),
    ("renewcommand", |m| m.new_command()),
    ("title", |m| m.store(0)),
    ("author", |m| {
        m.store(1);
        if let Some(doc) = m.doc.as_mut() {
            doc.authored = true;
        }
    }),
    ("date", |m| {
        m.store(2);
        if let Some(doc) = m.doc.as_mut() {
            doc.dated = true;
        }
    }),
    ("begin", |m| m.begin_env()),
    ("end", |m| m.end_env()),
    ("(", |m| m.math_open(false)),
    (")", |m| m.math_close(false)),
    ("[", |m| m.math_open(true)),
    ("]", |m| m.math_close(true)),
    ("rt@everymath", |m| m.every_math(false)),
    ("rt@everydisplay", |m| m.every_math(true)),
    ("\0'", |m| m.math_prime()),
    ("footnote", |m| m.footnote()),
    ("rt@fnmarkfont", |m| m.footnote_mark_font()),
    ("rt@fnmarkend", |m| m.footnote_mark_end()),
    ("rt@fnsetup", |m| m.footnote_setup()),
    ("thanks", |m| m.thanks()),
    ("and", |m| m.title_and()),
    ("rt@thanksmark", |m| m.thanks_mark_end()),
    ("rt@titleend", |m| m.title_end()),
    ("rt@thanksnote", |m| m.thanks_note()),
    ("rt@fnreset", |m| m.footnote_reset()),
    ("textasteriskcentered", |m| m.text_symbol_command("TS1", 42)),
    ("rt@fnend", |m| m.footnote_end()),
    ("rt@step", |m| m.step()),
    ("rt@parahook", |m| m.para_hook()),
    ("item", |m| m.item()),
    ("\\", |m| m.newline()),
    ("rt@listpar", |m| m.list_par()),
    ("rt@endpepar", |m| m.endpe_par()),
    ("rt@propagate", |m| m.propagate()),
    ("rt@itemlabels", |m| m.item_labels()),
    ("rt@labelmark", |m| m.label_mark()),
    ("tiny", |m| m.size_command(0)),
    ("scriptsize", |m| m.size_command(1)),
    ("footnotesize", |m| m.size_command(2)),
    ("small", |m| m.size_command(3)),
    ("normalsize", |m| m.size_command(4)),
    ("large", |m| m.size_command(5)),
    ("Large", |m| m.size_command(6)),
    ("LARGE", |m| m.size_command(7)),
    ("huge", |m| m.size_command(8)),
    ("Huge", |m| m.size_command(9)),
    ("maketitle", |m| m.make_title()),
    ("appendix", |m| m.appendix()),
    ("rt@newpage", |m| m.new_page()),
    ("rt@tabular", |m| m.tabular_open()),
    ("today", |m| m.today()),
    // The options were read by the pre-scan (`class_options`) and chose the class table.
    ("documentclass", |m| {
        if m.next_is('[') {
            while m.get_token().is_some_and(|t| t != Token::char(Cat::Other, ']')) {}
        }
        let mut arg = Vec::new();
        m.read_arg(&mut arg);
        if let Some(doc) = m.doc.as_mut() {
            doc.report = arg.iter().map(|t| t.chr()).eq("report".chars());
        }
    }),
    ("makeatletter", |m| m.e.eqtb.set_catcode(b'@', Cat::Letter, false)),
    ("makeatother", |m| m.e.eqtb.set_catcode(b'@', Cat::Other, false)),
    ("document", |m| m.document()),
    ("enddocument", |m| m.end_document()),
    ("@preamblecmds", |m| m.not_native("\\@preamblecmds")),
    ("@notprerr", |m| m.not_native("\\@notprerr")),
    ("rt@stop", |m| m.halted = true),
    ("providecommand", |m| m.provide_command()),
    ("label", |m| m.label()),
    ("ref", |m| m.reference()),
    ("rt@runinend", |m| m.run_in_end()),
    ("chapter", |m| m.chapter()),
    ("rt@chapter", |m| m.chapter_head()),
    ("rt@afterheading", |m| {
        // `\@afterheading`.
        if let Some(doc) = m.doc.as_mut() {
            doc.nobreak = true;
        }
        m.set_everypar(AFTER_HEADING);
    }),
    ("rt@descfont", |m| {
        let (_, size) = m.font_key();
        let key = m.rm_key("bx/n");
        m.select_nfss(&key, size);
    }),    ("paragraph", |m| m.section(3)),
    ("rt@verbatim", |m| m.verbatim_setup()),
    ("rt@leavevmode", |m| {
        m.leave_vmode();
    }),
    ("rt@@par", |m| m.run_ops(vec![Op::Par])),
    ("pageref", |m| m.page_reference()),
    ("tableofcontents", |m| m.table_of_contents()),
    ("rt@tocsec", |m| m.toc_section_skip()),
    ("rt@bfseries", |m| m.font_change(Change::Series("bx"))),
    ("rt@tocpage", |m| m.toc_page_box()),
    ("rt@afterindent", |m| {
        if let Some(doc) = m.doc.as_mut() {
            doc.toc_afterindent.push(doc.afterindent);
            doc.afterindent = true;
        }
    }),
    ("rt@afterindentrestore", |m| {
        if let Some(doc) = m.doc.as_mut() {
            doc.afterindent = doc.toc_afterindent.pop().unwrap_or(doc.afterindent);
        }
    }),
    ("rt@nobreakfalse", |m| {
        if let Some(doc) = m.doc.as_mut() {
            doc.nobreak = false;
        }
    }),
    // ot1enc.def's `\DeclareTextAccent`s.
    ("\"", |m| m.text_accent(127)),
    ("'", |m| m.text_accent(19)),
    (".", |m| m.text_accent(95)),
    ("=", |m| m.text_accent(22)),
    ("^", |m| m.text_accent(94)),
    ("`", |m| m.text_accent(18)),
    ("~", |m| m.text_accent(126)),
    ("H", |m| m.text_accent(125)),
    ("u", |m| m.text_accent(21)),
    ("v", |m| m.text_accent(20)),
    ("r", |m| m.text_accent(23)),
    ("c", |m| m.cedilla()),
    ("rt@cedilla", |m| m.cedilla_end()),
    ("AA", |m| m.ring_letter('A')),
    ("aa", |m| m.ring_letter('a')),
    ("l", |m| m.slashed_l(false)),
    ("L", |m| m.slashed_l(true)),
    ("setlength", |m| m.set_length(false)),
    ("addtolength", |m| m.set_length(true)),
    ("setcounter", |m| m.set_counter()),
    ("pagestyle", |m| m.page_style()),
    ("thispagestyle", |m| m.this_page_style()),
    ("smallskip", |m| m.skip_amount("{3pt plus1pt minus1pt}")),
    ("medskip", |m| m.skip_amount("{6pt plus2pt minus2pt}")),
    ("bigskip", |m| m.skip_amount("{12pt plus4pt minus4pt}")),
    ("em", |m| m.font_declaration(Change::Em, false)),
    ("ensuremath", |m| m.ensure_math()),
    ("rule", |m| m.rule()),
    ("usepackage", |m| m.use_package()),
    ("fontsize", |m| m.font_size()),
    ("parbox", |m| m.parbox()),
    ("fbox", |m| m.fbox()),
    ("rt@framebox", |m| m.frame_box()),
    ("rt@xcframe", |m| m.xc_frame()),
    ("textsuperscript", |m| m.text_script('^')),
    ("textsubscript", |m| m.text_script('_')),
    ("rt@scriptfont", |m| m.script_font()),
    ("colorbox", |m| m.colorbox(false)),
    ("fcolorbox", |m| m.colorbox(true)),
    ("rt@colorbox", |m| m.colorbox_rest()),
    ("rt@fboxsepzero", |m| m.e.eqtb.set_word(FBOXSEP, 0, false)),
    ("rt@fancyhf", |m| m.fancy_hf("")),
    ("rt@fancyhead", |m| m.fancy_hf("h")),
    ("rt@fancyfoot", |m| m.fancy_hf("f")),
    ("rt@lhead", |m| m.fancy_slot(0)),
    ("rt@chead", |m| m.fancy_slot(1)),
    ("rt@rhead", |m| m.fancy_slot(2)),
    ("rt@lfoot", |m| m.fancy_slot(3)),
    ("rt@cfoot", |m| m.fancy_slot(4)),
    ("rt@rfoot", |m| m.fancy_slot(5)),
    ("rt@fancyclip", |m| m.fancy_clip()),
    ("rt@fancyskip", |m| m.fancy_skip()),
    ("rt@pbclose", |m| m.parbox_close()),
    ("rt@minipage", |m| m.minipage_open()),
    ("rt@mpclose", |m| m.minipage_close_box()),
    ("rt@widehat", |m| m.wide_accent(0x5B, 0x0362)),
    ("rt@widetilde", |m| m.wide_accent(0x5D, 0x0365)),
    ("rt@wideaccent", |m| m.wide_accent_end()),
    ("vspace", |m| m.vspace()),
    ("rt@esphack", |m| m.esphack_end()),
    ("rt@eqstep", |m| m.equation_env_step()),
    ("rt@anchor", |m| m.anchor_here()),
    ("rt@endequation", |m| m.end_equation()),
    ("rt@normalfont", |m| m.normal_font()),
    ("rt@quotation", |m| m.list_env("quotation")),
    ("cite", |m| m.cite()),
    ("bibitem", |m| m.bibitem()),
    ("rt@bibopen", |m| m.bib_open()),
    ("rt@enumtmpl", |m| m.enum_template_open()),
    ("rt@setlist", |m| m.set_list()),
    // array.sty's `\arraybackslash`: `\let\\\tabularnewline`.
    // microtype's `\leftprotrusion` where its patches put it (`\@footnotetext`).
    ("rt@leftprotrusion", |m| m.left_protrusion()),
    // A token that does nothing where the kernel has one microtype's `\leftprotrusion` meets
    // (`\abstract`'s `\fi` after `\quotation`, `\@maketitle`'s `\let` after `center`).
    ("rt@fi", |_| {}),
    ("rt@arraybackslash", |m| {
        if let Some(id) = m.doc.as_ref().map(|d| d.newline_id) {
            m.e.eqtb.set_meaning(id, Meaning::Doc(NEWLINE as u16), false);
        }
    }),
    ("rt@enitleft", |m| m.enit_left()),
    ("sqrt", |m| m.sqrt()),
    ("rt@tabu", |m| m.tabular_template(false)),
    ("rt@tabv", |m| m.tabular_template(true)),
    // Read by `align_peek` (`align_peek_native`); anywhere else not native.
    ("rt@tabend", |m| m.not_native("\\endtabular outside its alignment")),
    // `\endtabular` and `\csname endtabular*\endcsname` in tabularx's trials and its table.
    ("rt@endtabular", |m| m.tabular_end()),
    ("hline", |m| m.not_native("\\hline inside an entry")),
    ("rt@tabskip", |m| m.not_native("\\noalign outside its alignment")),
    ("tabularnewline", |m| m.newline()),
    // `\@array`'s `\par`: `\@@par` outside an alignment group, which only resets the
    // paragraph shape in a vertical list (§1070) and does nothing in an entry's.
    ("multicolumn", |m| m.not_native("\\multicolumn after an entry's start")),
    ("rt@mcbody", |m| m.multicolumn_part(false)),
    ("rt@mctail", |m| m.multicolumn_part(true)),
    ("cline", |m| m.not_native("\\cline inside an entry")),
    ("rt@cline", |m| m.cline_rule()),
    // The font declarations (latex.ltx): `\not@math@alphabet`, then the switch `\textbf`
    // and kin use; and article's `\DeclareOldFontCommand`s, `\normalfont` first.
    ("bfseries", |m| m.font_declaration(Change::Series("bx"), false)),
    ("mdseries", |m| m.font_declaration(Change::Series("m"), false)),
    ("itshape", |m| m.font_declaration(Change::Shape("it"), false)),
    ("slshape", |m| m.font_declaration(Change::Shape("sl"), false)),
    ("scshape", |m| m.font_declaration(Change::Shape("sc"), false)),
    ("upshape", |m| m.font_declaration(Change::Shape("n"), false)),
    ("rmfamily", |m| m.font_declaration(Change::Default(0), false)),
    ("sffamily", |m| m.font_declaration(Change::Default(1), false)),
    ("ttfamily", |m| m.font_declaration(Change::Default(2), false)),
    ("normalfont", |m| m.normal_font()),
    ("bf", |m| m.font_declaration(Change::Series("bx"), true)),
    ("it", |m| m.font_declaration(Change::Shape("it"), true)),
    ("sl", |m| m.font_declaration(Change::Shape("sl"), true)),
    ("sc", |m| m.font_declaration(Change::Shape("sc"), true)),
    ("rm", |m| m.font_declaration(Change::Default(0), true)),
    ("sf", |m| m.font_declaration(Change::Default(1), true)),
    ("tt", |m| m.font_declaration(Change::Default(2), true)),
    ("newpage", |m| m.new_page()),
    ("clearpage", |m| m.clear_page()),
    // Kernel macros of primitives alone (latex.ltx:570-607, 9432, 16407-16408, 18332-18343).
    ("rlap", |m| m.kernel_macro(true, "\\hbox to0pt{#1\\hss}")),
    ("llap", |m| m.kernel_macro(true, "\\hbox to0pt{\\hss#1}")),
    ("null", |m| m.kernel_macro(false, "\\hbox{}")),
    ("nobreak", |m| m.kernel_macro(false, "\\penalty10000 ")),
    ("allowbreak", |m| m.kernel_macro(false, "\\penalty0 ")),
    ("break", |m| m.kernel_macro(false, "\\penalty-10000 ")),
    ("sloppy", |m| m.kernel_macro(false, "\\tolerance9999 \\emergencystretch3em\\hfuzz.5pt\\vfuzz.5pt ")),
    ("fussy", |m| m.kernel_macro(false, "\\emergencystretch0pt\\tolerance200 \\hfuzz.1pt\\vfuzz.1pt ")),
    ("enspace", |m| {
        if !m.leave_vmode() {
            m.kernel_macro(false, "\\kern.5em ");
        }
    }),
    ("centering", |m| m.ragged_declaration(0)),
    ("raggedright", |m| m.ragged_declaration(1)),
    ("raggedleft", |m| m.ragged_declaration(2)),
    ("rt@centercr", |m| m.center_cr()),
    ("hspace", |m| m.hspace()),
    ("rt@tabpar", |m| {
        if m.mode == Mode::Vertical {
            m.normal_paragraph();
        }
    }),
    ("caption", |m| m.caption()),
    ("rt@captionmid", |m| m.caption_mid()),
    ("rt@captionend", |m| m.caption_end()),
    ("rt@floatflush", |m| m.float_flush()),
    ("rt@clearpage", |m| m.clear_page_tail()),
    ("rt@includegraphics", |m| m.include_graphics()),
    ("rt@eqref", |m| m.eqref()),
    ("rt@genfrac", |m| {
        let [l, r, thick, style] = std::array::from_fn(|_| {
            let mut a = Vec::new();
            m.read_arg(&mut a);
            a
        });
        m.genfrac(&l, &r, &thick, &style);
    }),
    ("rt@dfrac", |m| m.genfrac(&[], &[], &[], &[Token::char(Cat::Other, '0')])),
    ("rt@tfrac", |m| m.genfrac(&[], &[], &[], &[Token::char(Cat::Other, '1')])),
    ("rt@binom", |m| m.genfrac(&[Token::char(Cat::Other, '(')], &[Token::char(Cat::Other, ')')], &ZERO_PT, &[])),
    ("rt@dbinom", |m| m.genfrac(&[Token::char(Cat::Other, '(')], &[Token::char(Cat::Other, ')')], &ZERO_PT, &[Token::char(Cat::Other, '0')])),
    ("rt@tbinom", |m| m.genfrac(&[Token::char(Cat::Other, '(')], &[Token::char(Cat::Other, ')')], &ZERO_PT, &[Token::char(Cat::Other, '1')])),
    ("rt@nonumber", |m| m.nonumber()),
    ("rt@tag", |m| m.tag()),
    ("rt@dftag", |m| m.df_tag()),
    ("rt@numberwithin", |m| m.number_within()),
    ("rt@allowdisplaybreaks", |m| m.allow_display_breaks()),
    ("rt@ltxlabel", |m| m.label_now()),
    ("rt@operatorname", |m| m.operator_name()),
    ("rt@declmathop", |m| m.declare_math_operator()),
    ("rt@text", |m| m.ams_text()),
    ("rt@textsize", |m| m.text_size()),
    ("rt@checkmathfonts", |m| m.check_math_fonts()),
    ("rt@amsaccent", |m| m.ams_accent()),
    ("rt@cdots", |m| m.ams_dots(Dots::Cdots)),
    ("rt@dotsc", |m| m.ams_dots(Dots::C)),
    ("rt@dotsi", |m| m.ams_dots(Dots::I)),
    ("rt@dotso", |m| m.ams_dots(Dots::O)),
    ("rt@colon", |m| m.ams_colon()),
    ("rt@big1", |m| m.ams_big("1")),
    ("rt@big15", |m| m.ams_big("1.5")),
    ("rt@big2", |m| m.ams_big("2")),
    ("rt@big25", |m| m.ams_big("2.5")),
    ("rt@big1l", |m| m.ams_big_class("1", OPEN)),
    ("rt@big1r", |m| m.ams_big_class("1", CLOSE)),
    ("rt@big1m", |m| m.ams_big_class("1", REL)),
    ("rt@big15l", |m| m.ams_big_class("1.5", OPEN)),
    ("rt@big15r", |m| m.ams_big_class("1.5", CLOSE)),
    ("rt@big15m", |m| m.ams_big_class("1.5", REL)),
    ("rt@big2l", |m| m.ams_big_class("2", OPEN)),
    ("rt@big2r", |m| m.ams_big_class("2", CLOSE)),
    ("rt@big2m", |m| m.ams_big_class("2", REL)),
    ("rt@big25l", |m| m.ams_big_class("2.5", OPEN)),
    ("rt@big25r", |m| m.ams_big_class("2.5", CLOSE)),
    ("rt@big25m", |m| m.ams_big_class("2.5", REL)),
    ("rt@alstart", |m| m.align_start()),
    ("rt@alu", |m| m.align_u()),
    ("rt@alv", |m| m.align_v()),
    ("rt@altag", |m| m.align_tag()),
    ("rt@mlindent", |m| m.multline_indent()),
    ("rt@adu", |m| m.aligned_u()),
    ("rt@adv", |m| m.aligned_v()),
    ("rt@gdu", |m| m.gathered_u()),
    ("rt@gdv", |m| m.gathered_v()),
    ("rt@adkern", |m| m.aligned_kern()),
    ("rt@splitopen", |m| m.split_open()),
    ("rt@adgap", |m| m.not_native("amsmath's \\noalign outside its alignment")),
    ("rt@adend", |m| m.not_native("amsmath's aligned end outside its alignment")),
    ("rt@algap", |m| m.not_native("amsmath's \\noalign outside its alignment")),
    ("rt@alcr", |m| m.not_native("amsmath's \\everycr outside its alignment")),
    ("rt@alend", |m| m.not_native("amsmath's align end outside its alignment")),
    ("rt@array", |m| m.array_open()),
    ("newtheorem", |m| m.new_theorem()),
    ("rt@thmstyle", |m| m.theorem_style()),
    ("rt@qed", |m| m.qed()),
    ("rt@qedsymbol", |m| {
        let t = m.named_cs(named!("openbox"));
        m.input.back(t);
    }),
    ("rt@openbox", |m| m.open_box_symbol()),
    // `Proof`: its last letter stays a token, for a word the document's text continues.
    ("rt@proofname", |m| {
        m.build_text(b"Proo");
        m.input.back(Token::char(Cat::Letter, 'f'));
    }),
    ("rt@addpunct", |m| m.add_punct()),
    // `\popQED`'s `\endgroup`, then `\endtrivlist`.
    ("rt@endproof", |m| {
        m.end_group(SEMI_SIMPLE_GROUP);
        m.end_trivlist();
    }),
    // A kernel theorem's label: `\MakeLinkTarget{\@currentcounter}` (`\hbox{}` without
    // hyperref, whose anchor `theorem_anchor_tokens` put first) and `\hskip\labelsep`.
    ("rt@thmlabel", |m| {
        if !m.hyper() {
            m.build_hbox(|_| {});
        }
        m.label_sep();
    }),
    ("rt@space", |m| m.build_control_space()),
    // After a kernel theorem's `\item`: `\itshape\ignorespaces`.
    ("rt@thmbody", |m| {
        m.font_declaration(Change::Shape("it"), false);
        m.ignore_spaces();
    }),
    ("rt@amshead", |m| m.ams_head_open()),
    ("rt@amsheadend", |m| m.ams_head_close()),
    // amsthm's note: `\mdseries\upshape`.
    ("rt@thmnote", |m| {
        m.font_declaration(Change::Series("m"), false);
        m.font_declaration(Change::Shape("n"), false);
    }),
    // A proof's label: `\hskip\labelsep\itshape`.
    ("rt@prooflabel", |m| {
        m.label_sep();
        m.font_declaration(Change::Shape("it"), false);
    }),
    ("rt@ignorespaces", |m| m.ignore_spaces()),
    ("rt@eqahalign", |m| m.eqnarray_halign()),
    ("rt@eqau", |m| m.eqnarray_u()),
    ("rt@eqav", |m| m.eqnarray_v()),
    ("rt@eqgap", |m| m.eqnarray_gap()),
    ("rt@eqnum", |m| m.eqnarray_number()),
    ("rt@lefteqnend", |m| m.left_eqn_end()),
    ("rt@endeqnarray", |m| m.eqnarray_close()),
    ("rt@textbranch", |m| m.text_branch()),
    ("rt@thinmskip", |m| {
        let g = m.thin_mu_skip();
        m.build_mskip(g);
    }),
    ("rt@colonrest", |m| m.ams_colon_rest()),
    ("rt@colonend", |m| m.build_mskip(Glue { width: 6 << 16, stretch: 1 << 16, ..Glue::default() })),
    ("rt@bigmid", |m| m.big_middle()),
    ("rt@bigend", |m| m.big_end()),
    ("rt@mathopen", |m| m.build_math_group(OPEN, 0)),
    ("rt@casesopen", |m| m.cases_open()),
    ("rt@matrixopen", |m| m.matrix_open()),
    ("rt@arstrut", |m| m.array_strut()),
    ("rt@eqtag", |m| {
        let n = m.equation_text();
        m.build_hbox(|m| {
            m.begin_group(SIMPLE_GROUP);
            m.normal_font();
            if m.colored() {
                m.color_stack(Some(DEFAULT_COLOR));
            }
            m.eqnnum_text(&n);
            m.end_group(SIMPLE_GROUP);
            if m.colored() {
                m.color_stack(None);
            }
        });
    }),
    ("rt@amstag", |m| {
        let n = m.equation_text();
        m.build_hbox(|m| {
            m.begin_group(SIMPLE_GROUP);
            m.normal_font();
            m.equation_tag(&n);
            m.end_group(SIMPLE_GROUP);
        });
    }),
    ("rt@eqrefopen", |m| m.eqref_open()),
    ("rt@op", |m| m.math_operator()),
    ("rt@sloppy", |m| m.bib_sloppy()),
    ("rt@bibbox", |m| {
        m.input.align_state += 1;
        m.open_box(HBOX_GROUP, Ctx::Set { n: TEMP_BOX, global: false }, false, 0, false);
    }),
    ("rt@tocopen", |m| m.toc_open()),
    ("rt@tocsecopen", |m| m.toc_section_open()),
    ("rt@tocsechead", |m| m.toc_section_head()),
    ("rt@tocsectail", |m| m.toc_section_tail()),
    ("rt@toclineopen", |m| m.toc_line_open()),
    ("rt@toclinehead", |m| m.toc_line_head()),
    ("rt@toclinetail", |m| m.toc_line_tail()),
    ("rt@tocentryend", |m| m.toc_entry_end()),
    ("rt@eqrefclose", |m| m.eqref_close()),
    ("rt@eqrow", |m| m.eqnarray_row_reset()),
    ("nonumber", |m| m.nonumber()),
    // `\lefteqn{#1}`: `\rlap{$\displaystyle #1$}`.
    ("lefteqn", |m| m.left_eqn()),
    ("rt@mskip", |m| m.matrix_skip()),
];
const OUTPUT: usize = doc!("rt@output");
const OUTPUT_FOOT: usize = doc!("rt@outputfoot");
const SEC_BOX: usize = doc!("rt@secbox");
const SEC_TITLE: usize = doc!("rt@sectitle");
const SEC_END: usize = doc!("rt@secend");
const TEXT_END: usize = doc!("rt@textend");
const TEXT_END_N: usize = doc!("rt@textendn");
const MAYBE_IC: usize = doc!("rt@maybeic");
const EVERY_MATH: usize = doc!("rt@everymath");
const EVERY_DISPLAY: usize = doc!("rt@everydisplay");
const FN_MARK_FONT: usize = doc!("rt@fnmarkfont");
const FN_MARK_END: usize = doc!("rt@fnmarkend");
const FN_SETUP: usize = doc!("rt@fnsetup");
const THANKS_MARK_END: usize = doc!("rt@thanksmark");
const TITLE_END: usize = doc!("rt@titleend");
const FN_END: usize = doc!("rt@fnend");
const STEP: usize = doc!("rt@step");
const PARA_HOOK: usize = doc!("rt@parahook");
const LIST_PAR: usize = doc!("rt@listpar");
const ENDPE_PAR: usize = doc!("rt@endpepar");
const PROPAGATE: usize = doc!("rt@propagate");
const ITEM_LABELS: usize = doc!("rt@itemlabels");
const LABEL_MARK: usize = doc!("rt@labelmark");
const NEW_PAGE: usize = doc!("rt@newpage");
const TABULAR: usize = doc!("rt@tabular");
const END_TABULAR: usize = doc!("rt@endtabular");
const NOTPRERR: usize = doc!("@notprerr");
const STOP: usize = doc!("rt@stop");
const RUN_IN_END: usize = doc!("rt@runinend");
const DESC_FONT: usize = doc!("rt@descfont");
const VERBATIM: usize = doc!("rt@verbatim");
const START_LINK_NATIVE: usize = doc!("rt@startlink");
const ANCHOR: usize = doc!("rt@anchor");
const END_LINK_NATIVE: usize = doc!("rt@endlink");
const LEAVE: usize = doc!("rt@leavevmode");
const PRIM_PAR: usize = doc!("rt@@par");
const END: usize = doc!("end");
const TAB_U: usize = doc!("rt@tabu");
const TAB_V: usize = doc!("rt@tabv");
const TAB_END: usize = doc!("rt@tabend");
const HLINE: usize = doc!("hline");
/// amsmath's `align`: the display's start, the `\halign`, a field's end, the tag entry, its
/// number, the row's end, `\everycr`, the alignment's end.
const AL_START: usize = doc!("rt@alstart");
const AL_TAG: usize = doc!("rt@altag");
const AL_ALIGN_CR: usize = doc!("rt@alcr");
const AL_END: usize = doc!("rt@alend");
const ARRAY: usize = doc!("rt@array");
/// `eqnarray`'s row end (`engine_eqnarray.rs`).
const EQ_ROW: usize = doc!("rt@eqrow");
const TAB_SKIP: usize = doc!("rt@tabskip");
const MULTICOLUMN: usize = doc!("multicolumn");
const HDOTSFOR: usize = doc!("rt@hdotsfor");
const MC_BODY: usize = doc!("rt@mcbody");
const MC_TAIL: usize = doc!("rt@mctail");
const CLINE_CMD: usize = doc!("cline");
const CLINE: usize = doc!("rt@cline");
const NEWLINE: usize = doc!("\\");
const CENTER_CR: usize = doc!("rt@centercr");
const TAB_PAR: usize = doc!("rt@tabpar");
const CAPTION_MID: usize = doc!("rt@captionmid");
const CAPTION_END: usize = doc!("rt@captionend");
const FLOAT_FLUSH: usize = doc!("rt@floatflush");
const CLEAR_PAGE_TAIL: usize = doc!("rt@clearpage");
/// The font switches `switch_font` remembers: `\bfseries`, `\mdseries`, the four shapes,
/// the three families, `\em`'s and `\normalfont`.
const FONT_SWITCHES: usize = 11;

/// The user `\everypar` (LaTeX's `\everypar` token register), as the code it holds, in a
/// hidden integer register so that grouping restores it: none, `\@afterheading`'s,
/// `\@item`'s, `\@doendpe`'s, `\@xsect`'s for a run-in heading.
const EVERYPAR: Reg = Reg::register(Region::Int, 32700);
const AFTER_HEADING: i32 = 1;
const ITEM: i32 = 2;
const ENDPE: i32 = 3;
const RUN_IN: i32 = 4;
const VERB_ITEM: i32 = 5;
const MINIPAGE: i32 = 6;
/// amsthm's `\dth@everypar`, after a theorem's head.
const THM_HEAD: i32 = 7;
/// array.sty's `p` entry: `\vrule \@height \ht\@arstrutbox \@width \z@ \everypar{}`.
const P_CELL: i32 = 8;
/// microtype's `item` patch: `\@item`'s inner `\everypar{}` is `\everypar{\leftprotrusion}`.
const MT_PROT: i32 = 9;
/// `\l@nohyphenation` (language.dat's third language).
const NOHYPHENATION: i32 = 2;
/// `\@labels`, `\@tempboxa` and `\@arstrutbox`, hidden box registers.
const LABELS_BOX: u32 = 32700;
/// §463 `default_rule`: 0.4pt.
const DEFAULT_RULE: i32 = 26214;
const TEMP_BOX: u32 = 32701;
const ARSTRUT_BOX: u32 = 32702;
/// Which `\@listi` the size command set (`\@listI`, `\small`'s, `\footnotesize`'s), in a
/// hidden integer register so that grouping restores it.
const LISTI: Reg = Reg::register(Region::Int, 32701);
/// `\if@noparitem` and `\if@noparlist`, set locally by `\@trivlist`: a hidden integer
/// register of flags, restored by grouping.
const NOPAR: Reg = Reg::register(Region::Int, 32702);
const NOPAR_ITEM: i32 = 1;
const NOPAR_LIST: i32 = 2;
/// `\@currentlabel` (set locally, by `\protected@edef`), in a hidden token register.
const CURRENT_LABEL: Reg = Reg::register(Region::Toks, 32700);
/// hyperref's `\@currentHref`, the anchor a `\label` records (local, as its `\edef` is).
pub(super) const CURRENT_HREF: Reg = Reg::register(Region::Toks, 32701);
/// `\ht\strutbox` and `\dp\strutbox` (`strut_update`), hidden dimension registers.
const STRUT_HT: Reg = Reg::register(Region::Dimen, 32700);
const STRUT_DP: Reg = Reg::register(Region::Dimen, 32701);
/// `\baselineskip` before an amsmath display's `\openup\jot` (local), for hyperref's
/// `\normalbaselineskip`; 0 outside one.
const NORMAL_BASELINE: Reg = Reg::register(Region::Dimen, 32702);
/// `\normalbaselineskip` as `\size@update` sets it (`strut_update`), for
/// `\@arrayparboxrestore`'s `\baselineskip\normalbaselineskip`; a hidden glue register.
pub(super) const NORMAL_BASELINE_SKIP: Reg = Reg::register(Region::Glue, 32700);
/// The kernel's `\fboxsep` (3pt) and `\fboxrule` (.4pt), as hidden registers under their
/// names.
pub(super) const FBOXSEP: Reg = Reg::register(Region::Dimen, 32703);
pub(super) const FBOXRULE: Reg = Reg::register(Region::Dimen, 32704);

/// [`data_class::CLASS_REGS`] indexes.
/// The first of the registers holding [`data_class::CLASS_DIMENS`] (dimensions, in order).
const GEOMETRY: u32 = 32680;
/// `\columnwidth`, after the geometry: the class's `\textwidth`, then `\columnwidth\textwidth`
/// at `\begin{document}` (`\@onecolumn`'s part of the kernel's `\document`).
const COLUMN_WIDTH: Reg = Reg::register(Region::Dimen, GEOMETRY + data_class::CLASS_DIMENS.len() as u32);
const R_FOOTNOTESEP: usize = 0;
const R_LEFTMARGINI: usize = 1;
const R_LABELSEP: usize = 7;
const R_LABELWIDTH: usize = 8;
const R_LEFTMARGIN: usize = 9;
const R_RIGHTMARGIN: usize = 10;
const R_LISTPARINDENT: usize = 11;
const R_ITEMINDENT: usize = 12;
const R_TOTALLEFTMARGIN: usize = 13;
const R_LINEWIDTH: usize = 14;
const R_BEGINPARPENALTY: usize = 15;
const R_ENDPARPENALTY: usize = 16;
const R_ITEMPENALTY: usize = 17;
const R_CLUBPENALTY: usize = 18;
const R_LISTDEPTH: usize = 21;
const R_ITEMDEPTH: usize = 22;
const R_ENUMDEPTH: usize = 23;
const R_ENUMI: usize = 24;
const R_PARTOPSEP: usize = 28;
const R_TOPSEP: usize = 29;
const R_PARSEP: usize = 30;
const R_ITEMSEP: usize = 31;
const R_ATTOPSEP: usize = 32;
const R_TOPSEPADD: usize = 33;
const R_OUTERPARSKIP: usize = 34;
const R_TABCOLSEP: usize = 39;
const R_ATRIGHTSKIP: usize = 40;
const R_ARRAYCOLSEP: usize = 41;
const R_ARRAYRULEWIDTH: usize = 42;
const R_DOUBLERULESEP: usize = 43;
const R_FLOATSEP: usize = 44;
const R_TEXTFLOATSEP: usize = 45;
const R_INTEXTSEP: usize = 46;
const R_FPTOP: usize = 47;
const R_FPSEP: usize = 48;
const R_FPBOT: usize = 49;
const R_ABOVECAPTIONSKIP: usize = 50;
const R_BELOWCAPTIONSKIP: usize = 51;
const R_TOPNUMBER: usize = 52;
const R_BOTTOMNUMBER: usize = 53;
const R_TOTALNUMBER: usize = 54;
const R_FIGURE: usize = 55;

/// An open list: what its group holds as macros (`\@itemlabel`, `\makelabel`,
/// `\if@nmbrlist`, `\@listctr`, and `\\` as `\@centercr`).
#[derive(Clone, Copy)]
struct List {
    label: Label,
    make: Make,
    numbered: bool,
    counter: usize,
    centercr: bool,
    /// `\item\relax` right after the list's start (the displayed-text environments).
    auto_item: bool,
    /// `\@endpefalse` after `\endtrivlist` (amsthm's theorems and `proof`).
    endpe_false: bool,
}

/// A `\newtheorem` environment: its name, its title, the theorem whose counter it steps
/// (itself, or the one named in `\newtheorem{env}[shared]{Title}`), and the heading level
/// whose step resets the counter (`[section]`: `\@addtoreset`, numbered `\thesection.n`).
struct Theorem {
    env: String,
    title: Vec<Token>,
    counter: usize,
    within: Option<usize>,
    /// The kernel's theorem (0) or amsthm's, in the style current at `\newtheorem`:
    /// `PLAIN`, `DEFINITION`, `REMARK`.
    style: u8,
    /// `\newtheorem*`: no number.
    star: bool,
    /// `[within]` naming a theorem's counter: that theorem (`\@addtoreset`, numbered
    /// `\the<parent>.n`).
    parent: Option<usize>,
}

/// amsthm's `\theoremstyle`s.
const PLAIN: u8 = 1;
const DEFINITION: u8 = 2;
const REMARK: u8 = 3;

type NewTheorem<'a> = (&'a str, Option<&'a str>, &'a str, Option<usize>, usize, Option<&'a str>);

/// `\newtheorem`'s arguments in source text `s` (after the name): the environment, the
/// shared counter's environment, the title (any balanced text), the reset level, the length
/// read, and the counter resetting it when `[within]` is not a heading level (a theorem's,
/// for the caller to check); `None` for a form the native path does not take.
fn newtheorem_args(s: &str) -> Option<NewTheorem<'_>> {
    fn group(t: &str) -> Option<(&str, usize)> {
        let lead = t.len() - t.trim_start().len();
        let r = t[lead..].strip_prefix('{')?;
        let k = balanced(r)?;
        Some((&r[..k], lead + k + 2))
    }
    fn bracket(t: &str) -> Option<(&str, usize)> {
        let lead = t.len() - t.trim_start().len();
        let r = t[lead..].strip_prefix('[')?;
        let k = r.find(']')?;
        Some((r[..k].trim(), lead + k + 2))
    }
    let (env, mut at) = group(s)?;
    let shared = bracket(&s[at..]).map(|(n, k)| {
        at += k;
        n
    });
    let (title, k) = group(&s[at..])?;
    at += k;
    let within = if shared.is_none() {
        bracket(&s[at..]).map(|(n, k)| {
            at += k;
            n
        })
    } else {
        None
    };
    let level = within.and_then(|w| ["section", "subsection", "subsubsection"].iter().position(|l| *l == w));
    let parent = within.filter(|_| level.is_none());
    let plain = |t: &str| t.bytes().all(|c| c.is_ascii_alphanumeric() || b" -'".contains(&c));
    (plain(env) && !env.is_empty() && shared.is_none_or(plain) && parent.is_none_or(plain)).then_some((env.trim(), shared, title, level, at, parent))
}

impl Doc {
    /// `\refstepcounter` of theorem counter `owner`: the counters reset by it (`[owner]`)
    /// go to 0; the number, `\the<parent>.n` or within its level.
    fn step_theorem(&mut self, owner: usize) -> String {
        self.thm_values[owner] += 1;
        for k in 0..self.theorems.len() {
            if self.theorems[k].parent == Some(owner) {
                self.thm_values[k] = 0;
            }
        }
        let parent = self.theorems[owner].parent.map(|p| within_text(&self.counters, self.appendix, self.theorems[p].within, self.thm_values[p]) + ".");
        parent.unwrap_or_default() + &within_text(&self.counters, self.appendix, self.theorems[owner].within, self.thm_values[owner])
    }
}

/// `\makelabel`: `\@mklab` (`\hfil #1`), `\hss\llap{#1}`, or `#1` (`\trivlist`).
#[derive(Clone, Copy, PartialEq)]
enum Make {
    Mklab,
    Llap,
    Plain,
    /// `\descriptionlabel`: `\hspace\labelsep \normalfont\bfseries #1`.
    Description,
}

/// `\@itemlabel`: empty, `\labelitem<depth>`, or `\labelenum<depth>`.
#[derive(Clone, Copy, PartialEq)]
enum Label {
    Empty,
    Item(u8),
    Enum(u8),
    /// enumerate.sty's template at a depth: `Doc::enum_templates[i]`.
    Tmpl(u8, u16),
    /// `thebibliography`'s `\@biblabel{\@arabic\c@enumiv}`.
    Bib,
    /// enumitem's `label` key in `enumerate` at a depth: `Doc::enum_templates[i]`, its
    /// `\arabic*` and kin the counter; `\theenum<d>` is the whole label.
    Enit(u8, u16),
    /// enumitem's `label` key in `itemize`: `Doc::enum_templates[i]`, with no counter.
    Given(u16),
}
/// color.sty's `\default@color` with pdftex.def: `\current@color` at `\begin{document}`.
pub(super) const DEFAULT_COLOR: &str = "0 g 0 G";

/// `\current@color`, grouped as TeX groups it: 0 for [`DEFAULT_COLOR`], else one more than
/// the literal's index in `Doc::colors`.
pub(super) const CURRENT_COLOR: Reg = Reg::register(Region::Int, 32710);

/// `\footins` (`\newinsert`): its box, `\count`, `\dimen` and `\skip` register.
const FOOTINS: u32 = 253;

/// The control words a native document may use (with the macros it defines), and its
/// environments.
#[cfg(any(test, feature = "gen"))]
const WORDS: [&str; 209] = ["chapter", "dimexpr", "fboxsep", "fboxrule", "rlap", "llap", "null", "nobreak", "allowbreak", "break", "sloppy", "fussy", "enspace", "hbox", "vbox", "vtop", "raise", "lower", "vskip", "vfill", "penalty", "hrule", "vrule", "unskip", "indent", "ignorespaces", "kern", "thanks", "and", "textasteriskcentered", "clearpage", "notin", "cong", "underbrace", "overbrace", "rule", "medskip", "bigskip", "smallskip", "columnwidth", "ensuremath", "em", "thispagestyle", "c", "l", "L", "AA", "aa", "textendash", "textemdash", "textquotedblleft", "textquotedblright", "textquoteleft", "textquoteright", "textcent", "textcopyright", "textdegree", "textdiv", "textonehalf", "textpm", "textregistered", "texttimes", "texttrademark", "strut", "eqno", "leqno", "buildrel", "doteq", "root", "of", "uproot", "leftroot", "overrightarrow", "overleftarrow", "boldmath", "unboldmath", "mathversion", "nonumber", "lefteqn", "vdots", "ddots", "appendix", "textbackslash", "textbar", "textbardbl", "textbraceleft", "textbraceright", "textbullet", "textdagger", "textdaggerdbl", "textparagraph", "textsection", "textperiodcentered", "textless", "textgreater", "bmod", "pmod", "negthinspace", "medspace", "negmedspace", "thickspace", "negthickspace", "smash", "phantom", "vphantom", "hphantom", 
    "section", "subsection", "subsubsection", "emph", "textbf", "textmd", "textit", "textsl", "textsc", "textup", "textrm", "textsf", "texttt", "LaTeX", "TeX", "ldots", "dots", "textellipsis", "thinspace", "nobreakspace", "mbox", "newcommand", "renewcommand", "title", "author", "date", "begin", "end", "documentclass", "textunderscore", "footnote", "item",
    "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "maketitle", "today", "providecommand", "label", "ref", "paragraph", "pageref", "tableofcontents", "H", "u", "v", "r", "i", "j", "ss", "ae", "oe", "o", "AE", "OE", "O", "setlength", "addtolength", "setcounter", "pagestyle", "vspace", "cite", "bibitem", "sqrt", "hline", "tabularnewline", "multicolumn", "cline", "bfseries", "mdseries", "itshape", "slshape", "scshape", "upshape", "rmfamily", "sffamily", "ttfamily", "normalfont", "bf", "it", "sl", "sc", "rm", "sf", "tt", "newpage", "centering", "raggedright", "raggedleft", "hspace", "noindent", "textwidth", "textheight", "linewidth", "par", "caption",
];
/// ot1enc.def's `\DeclareTextSymbol`s: in OT1 each is its `\OT1\name`, a `\chardef`, so
/// the main loop's ligatures and kerns see it as a character.
const TEXT_SYMBOLS: [(&str, u32); 15] = [("i", 16), ("j", 17), ("ss", 25), ("ae", 26), ("oe", 27), ("o", 28), ("AE", 29), ("OE", 30), ("O", 31), ("textendash", 123), ("textemdash", 124), ("textquotedblleft", 92), ("textquotedblright", 34), ("textquoteleft", 96), ("textquoteright", 39)];
/// [`TEXT_SYMBOLS`] in T1 (`t1enc.def`'s `\DeclareTextSymbol`s), set by `\usepackage[T1]{fontenc}`.
const T1_TEXT_SYMBOLS: [(&str, u32); 15] = [("i", 25), ("j", 26), ("ss", 255), ("ae", 230), ("oe", 247), ("o", 248), ("AE", 198), ("OE", 215), ("O", 216), ("textendash", 21), ("textemdash", 22), ("textquotedblleft", 16), ("textquotedblright", 17), ("textquoteleft", 96), ("textquoteright", 39)];
/// t1enc.def's `\DeclareTextComposite`s: `(accent << 8) | base`, sorted (the accent's own
/// character, `\i` as 25), and the slot of each, side by side.
const T1_COMPOSITE_KEYS: [u16; 111] = [
    8729, 8769, 8773, 8777, 8783, 8789, 8793, 8801, 8805, 8809, 8815, 8821, 8825, 10009, 10049,
    10051, 10053, 10057, 10060, 10062, 10063, 10066, 10067, 10069, 10073, 10074, 10081, 10083,
    10085, 10089, 10092, 10094, 10095, 10098, 10099, 10101, 10105, 10106, 11801, 11849, 11866,
    11881, 11898, 18511, 18517, 18543, 18549, 24089, 24129, 24133, 24137, 24143, 24149, 24161,
    24165, 24169, 24175, 24181, 24601, 24641, 24645, 24649, 24655, 24661, 24673, 24677, 24681,
    24687, 24693, 25411, 25427, 25428, 25443, 25459, 25460, 27457, 27461, 27489, 27493, 29249,
    29269, 29281, 29301, 30017, 30023, 30049, 30055, 30275, 30276, 30277, 30284, 30286, 30290,
    30291, 30292, 30298, 30307, 30308, 30309, 30316, 30318, 30322, 30323, 30324, 30330, 32321,
    32334, 32335, 32353, 32366, 32367,
];
const T1_COMPOSITE_SLOTS: [u8; 111] = [
    239, 196, 203, 207, 214, 220, 152, 228, 235, 239, 246, 252, 184, 237, 193, 130, 201, 205, 136,
    139, 211, 143, 145, 218, 221, 153, 225, 162, 233, 237, 168, 171, 243, 175, 177, 250, 253, 185,
    25, 157, 155, 25, 187, 142, 150, 174, 182, 238, 194, 202, 206, 212, 219, 226, 234, 238, 244,
    251, 236, 192, 200, 204, 210, 217, 224, 232, 236, 242, 249, 199, 147, 149, 231, 179, 181, 129,
    134, 161, 166, 197, 151, 229, 183, 128, 135, 160, 167, 131, 132, 133, 137, 140, 144, 146, 148,
    154, 163, 164, 165, 169, 172, 176, 178, 180, 186, 195, 209, 213, 227, 241, 245,
];
/// Macros the native commands read internally (a heading's number, a list's label, a
/// footnote's size, the two-column float fractions, ...). A document defining one, or any
/// name with an `@` (no table lists all the kernel's), goes to the engine: the native would
/// not see the new text (GDL §8). `\baselinestretch` and the float fractions are read as
/// macros.
const READ_INTERNALLY: [&str; 54] = ["relbar", "thepart", "thesection", "thesubsection", "thesubsubsection", "theparagraph", "thesubparagraph", "theequation", "thefigure", "thetable", "thefootnote", "thempfootnote", "thepage", "theenumi", "theenumii", "theenumiii", "theenumiv", "labelitemi", "labelitemii", "labelitemiii", "labelitemiv", "labelenumi", "labelenumii", "labelenumiii", "labelenumiv", "labelitemfont", "contentsname", "listfigurename", "listtablename", "partname", "appendixname", "dbltopfraction", "dblfloatpagefraction", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "normalfont", "section", "item", "ref", "vspace", "textup", "bfseries", "textbullet", "textendash", "textasteriskcentered", "textperiodcentered"];
/// Commands of preamble code (`\makeatletter`, `\ifx`, `\let`, …); `\let`, `\def` and
/// `\providecommand` define their first name, as `\newcommand` does.
#[cfg(any(test, feature = "gen"))]
const PREAMBLE: [&str; 14] = ["def", "makeatletter", "makeatother", "ifx", "else", "fi", "bgroup", "egroup", "relax", "document", "enddocument", "@preamblecmds", "@notprerr", "endinput"];
/// amsmath's environments built on `array`, native once amsmath is loaded (its definitions
/// replace the kernel's `cases` and `matrix`, which the native path does not take).
const AMS_ARRAYS: [&str; 8] = ["cases", "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "smallmatrix"];
const ENVS: [&str; 35] = ["titlepage", "minipage", "eqnarray", "eqnarray*", "array", "align", "align*", "gather", "gather*", "multline", "multline*", "aligned", "gathered", "split", "equation*", "figure", "table", "figure*", "table*", "tabular", "document", "em", "quote", "quotation", "verse", "itemize", "enumerate", "center", "description", "verbatim", "flushleft", "flushright", "abstract", "equation", "thebibliography"];

/// `\normalsize` among `data_class::SIZE_COMMANDS`.
const NORMALSIZE: usize = 4;

/// `article.cls`'s `\@startsection` arguments: before-skip, after-skip, the size of
/// `\normalfont<size>\bfseries`. The indent is `\z@` and `\c@secnumdepth` 3.
struct Sec {
    before: &'static str,
    after: &'static str,
    size: usize,
}
const SECS: [Sec; 4] = [
    Sec { before: "-3.5ex plus -1ex minus -.2ex", after: "2.3ex plus .2ex", size: 6 },
    Sec { before: "-3.25ex plus -1ex minus -.2ex", after: "1.5ex plus .2ex", size: 5 },
    Sec { before: "-3.25ex plus -1ex minus -.2ex", after: "1.5ex plus .2ex", size: NORMALSIZE },
    Sec { before: "3.25ex plus 1ex minus .2ex", after: "-1em", size: NORMALSIZE },
];
/// `secnumdepth`: levels below it are numbered (`\paragraph` is not).
const SECNUMDEPTH: usize = 3;
/// `\@secpenalty`.
const SEC_PENALTY: i32 = -300;
/// `\@highpenalty`.
const HIGH_PENALTY: i32 = 301;

/// The class as loaded: its table (one per size option) and its geometry and flags after
/// the option combination's differences.
#[derive(Clone, Copy)]
struct Class {
    table: &'static ClassTable<'static>,
    vals: [i32; data_class::CLASS_DIMENS.len() + data_class::CLASS_FLAGS.len()],
}

impl Class {
    fn dimen(&self, name: &str) -> i32 {
        data_class::CLASS_DIMENS.iter().position(|n| *n == name).map_or(0, |i| self.vals[i])
    }

    fn flag(&self, name: &str) -> bool {
        data_class::CLASS_FLAGS.iter().position(|n| *n == name).is_some_and(|i| self.vals[data_class::CLASS_DIMENS.len() + i] != 0)
    }
}

/// A native document's own state: what LaTeX keeps in registers and macros, typed.
pub struct Doc {
    class: Class,
    /// `\c@page`.
    page: i32,
    /// `\normalfont\normalsize`, for the head and foot (`\reset@font\normalsize`).
    normal_font: u16,
    /// The column of the page being made, between the two steps of the output routine.
    column: Option<u32>,
    /// The control sequences of [`DOC`].
    ids: [u32; DOC.len()],
    /// The control sequences of [`NAMED`].
    named: [u32; NAMED.len()],
    /// The `\vcenter` height of the `\big` being built.
    big_pending: i32,
    /// `\if@nobreak` and `\if@afterindent` (the user `\everypar` is [`EVERYPAR`]).
    nobreak: bool,
    afterindent: bool,
    /// The list switches, all global in LaTeX: `\if@inlabel`, `\if@newlist`,
    /// `\if@noitemarg`, `\if@endpe` (`\if@noparitem` and `\if@noparlist`, set locally, are
    /// [`NOPAR`]).
    inlabel: bool,
    newlist: bool,
    noitemarg: bool,
    endpe: bool,
    /// The open lists, innermost last: what the list group holds as macros.
    lists: Vec<List>,
    /// The open `tabular`s, innermost last; the box the next one's `\@array` opens; `\cr`
    /// and `\crcr`.
    tabs: Vec<tabular::Tab>,
    tab_group: u8,
    /// The open `tabularx`'s trials (`engine_tabular`); `tab_to`, the width of the next
    /// tabular's `\halign to` (`tabular*`'s `\@halignto`).
    tx: Option<Box<tabular::Tx>>,
    tab_to: Option<i32>,
    /// tabularx's `\TX@delta`: `\hfuzz` when the package was loaded.
    tx_delta: i32,
    /// The floats (`engine_float`).
    floats: float::Floats,
    /// The lists `\\` looks through for `\@centercr` (a float's `\@parboxrestore` hides
    /// those around it).
    cr_base: usize,
    cr_id: u32,
    crcr_id: u32,
    /// [`Self::switch_font`]'s memo: per font, the font each switch reaches.
    font_switch: Vec<[u16; FONT_SWITCHES]>,
    /// `\rmdefault`, `\sfdefault`, `\ttdefault` as `\begin{document}` finds them.
    families: [&'static str; 3],
    /// [`FONT_FIXED`]'s bodies after the class loads.
    font_fixed: Vec<Option<String>>,
    /// `\arraystretch`, `\omit`, `\span`, `\\`.
    stretch_id: u32,
    newline_id: u32,
    omit_id: u32,
    span_id: u32,
    /// `\par`'s meaning outside lists (the primitive).
    par_prim: Meaning,
    par_id: u32,
    /// `\c@section`, `\c@subsection`, `\c@subsubsection`.
    counters: [i32; 3],
    /// The heading being made: its title, number box, after-skip and whether it is
    /// numbered.
    title: Vec<Token>,
    number_box: Option<u32>,
    after: Glue,
    numbered: bool,
    /// Not starred: the heading's `\addcontentsline` (a level past `secnumdepth` has no number).
    toc: bool,
    /// `\if@noskipsec`: a run-in heading waits for its paragraph.
    noskipsec: bool,
    /// `\@title`, `\@author`, `\@date`.
    stored: [Vec<Token>; 3],
    /// The size the math families were last set for (`\glb@currsize`).
    math_size: &'static str,
    /// The math versions `normal` and `bold` (`\mv@normal`, `\mv@bold`): each family's NFSS
    /// shape in each, the symbol fonts then the math alphabets in their order of first use
    /// (an alphabet's family is only in `normal`, its bold shape empty).
    fams: Vec<[&'static str; 2]>,
    /// The math alphabets, by `Meaning::Doc` index past `DOC`: name, whether `\use@mathgroup`
    /// skips its encoding switch, and its family or its shape and family once given one.
    alphabets: Vec<(&'static str, bool, Alphabet)>,
    /// The packages loaded from native tables (`data_kernel::package`).
    packages: Vec<&'static str>,
    /// booktabs' registers ([`BT_REGS`]), once it is loaded.
    pub(super) booktabs: Option<[Reg; 17]>,
    /// amsmath's display in progress: 1 for `equation` (`\@eqnswtrue`), 2 for `equation*`
    /// and `\[`, 0 outside; and `\df@label`, the key a `\label` there leaves for the end.
    ams_display: u8,
    /// amsmath's `\df@tag` from `\tag{text}` or `\tag*{text}` in the display: the text and
    /// whether it is starred (no parentheses).
    df_tag: Option<(String, bool)>,
    df_label: Option<Vec<Token>>,
    /// amsmath's `\big@size`, set with the math fonts.
    big_size: i32,
    /// The `align` being read.
    align: Option<Box<ams_align::AmsAlign>>,
    /// The `eqnarray` being read.
    eqnarray: Option<eqnarray::EqnArray>,
    /// Shapes `\wrong@fontshape` has made aliases of their substitutes (`\global\let`).
    wrong_shapes: Vec<(String, String)>,
    /// amsfonts' `\symAMSb`, and each pending wide accent: its argument and its accent.
    wide_fam: u32,
    wide: Vec<(Vec<Token>, u32)>,
    /// `\c@equation`; `\@savsk` and `\@savsf` of each `\@bsphack` awaiting its `\@esphack`.
    equation: i32,
    /// amsmath's `\numberwithin{equation}{level}`: the heading level resetting the counter
    /// and printing before it (`\theequation` is `\thesection.\arabic{equation}`).
    eq_within: Option<usize>,
    /// After `\appendix`: `\thesection` is `\@Alph\c@section`.
    appendix: bool,
    /// The preamble's prefixes of `\thefigure`, `\thetable`, `\theequation` (`the_prefixes`).
    the_prefix: [String; 3],
    sphack: Vec<(Glue, i32)>,
    /// `\c@footnote`; the footnote text between the mark and the insertion; the space
    /// factor before each mark (`\@x@sf`, -1 outside horizontal mode); `\if@nobreak` and
    /// the heading's `\everypar` outside each footnote (`\@parboxrestore` clears them).
    footnote: i32,
    footnote_text: Vec<Token>,
    /// While `\maketitle` sets the title: the `\thanks` texts so far (`\@thanks`); `\and`
    /// and `\footnote` (`\let\footnote\thanks`) act only then.
    thanks: Option<Vec<Vec<Token>>>,
    /// The footnote being set is a `\thanks` (`\@fnsymbol` mark, the title's `\@makefntext`).
    fn_symbol: bool,
    footnote_sf: Vec<i32>,
    footnote_flags: Vec<bool>,
    /// The steps left behind `\rt@step` tokens, innermost last; the heading level being
    /// started; the paragraph's indent box while the paragraph hook runs.
    steps: Vec<Vec<Op>>,
    level: usize,
    after_before: Glue,
    indent: Option<Node>,
    /// The native being run, for `leave_vmode`'s `back_input`.
    current: usize,
    /// `\item`'s optional label, between its steps.
    item_label: Option<Vec<Token>>,
    /// The `\newtheorem` environments, with each one's counter value (used for the ones
    /// that own their counter), and the `\item[...]` the next theorem's `\trivlist` ends
    /// with.
    theorems: Vec<Theorem>,
    thm_values: Vec<i32>,
    thm_item: Vec<Token>,
    /// amsthm's `\thm@style`, and the theorem whose head is being set.
    thm_style: u8,
    thm_current: usize,
    /// What follows `\@trivlist` (`\list`'s rest, `\trivlist`'s).
    trivlist_then: Native,
    /// Whether `\date` was given (else `\today`).
    /// Whether `\author` was given: the kernel's `\@author` is a warning, whose `\write`
    /// starts the title's `tabular` row (an empty entry); `\author{}` gives no row.
    authored: bool,
    dated: bool,
    /// The first construct met that the native job does not implement.
    unsupported: Option<String>,
    /// The class options' combination (`data_class::option_key`).
    options: u32,
    /// A `verbatim` environment's lines, as tokens, until its setup is done.
    verbatim: Vec<Token>,
    /// `\label`'s keys and their values, in memory (no `.aux` file).
    labels: std::collections::HashMap<String, Vec<Token>>,
    /// The values [`predict`] gives the labels from the source, for `\ref`s ahead of their
    /// `\label` (P2b); those used, with the value used, so each `\label` checks its own.
    predicted: std::collections::HashMap<String, String>,
    forward: std::collections::HashMap<String, String>,
    /// Keys `\ref` has read: a second `\label` of one of them is not native.
    used: std::collections::HashSet<String>,
    /// The labels in `\label` order (an `.aux` whatsit's `a` is its index plus one) and the
    /// page each was shipped on (`\thepage` when its `\write` would run), once known.
    label_keys: Vec<String>,
    label_pages: std::collections::HashMap<String, i32>,
    /// The contents entries [`predict`] read off the source (`None` when it could not read
    /// them all), the unstarred headings typeset so far, and each one's page once shipped.
    contents: Option<Vec<TocEntry>>,
    /// The contents entry being set (counting the levels shown).
    toc_next: usize,
    toc_seen: usize,
    toc_pages: Vec<Option<i32>>,
    /// `\tableofcontents`'s page-number boxes, each with its entry: typeset with a digit
    /// standing in, filled in before the pages ship (`finish_contents`).
    toc_boxes: Vec<(u32, usize)>,
    /// With a contents list, pages are kept (`deferred`) and shipped in order at the end;
    /// `replay` while they ship, so the page records are not taken again.
    defer: bool,
    deferred: Vec<u32>,
    /// Per shape and size a family font was selected at: the control sequence of its
    /// NFSS name and the font (`set_fam`).
    fam_fonts: std::collections::HashMap<(&'static str, &'static str), (u32, u16)>,
    /// latex.ltx's `\@eqpen`, a global count: 0 until a display's `\\` sets it.
    eqpen: i32,
    /// The `aligned` or `gathered` being read.
    aligned: Option<ams_align::AmsAligned>,
    replay: bool,
    /// `\@afterindenttrue` inside `\@dottedtocline`'s group, restored after it.
    toc_afterindent: Vec<bool>,
    /// After `\begin{document}`: the page geometry is fixed from then on.
    body: bool,
    /// `\pagestyle`: `\ps@plain` (false) or `\ps@empty` (true); `\thispagestyle{plain}`
    /// (`\@specialpage`) for the next page; whether the page being shipped has an empty foot.
    empty_style: bool,
    /// `\thispagestyle` (`\@specialpagetrue`, `\@specialstyle`): `Some(empty)` for the next
    /// page out.
    special: Option<bool>,
    /// fancyhdr's slots and marks, once the package is loaded.
    fancy: Option<Box<fancy::Fancy>>,
    /// The class is report: `\chapter` is heading level 0 and `\section` level 1.
    report: bool,
    empty_foot: bool,
    /// geometry.sty's keys and computed lengths, once it is loaded.
    geometry: Option<super::engine_geometry::Geometry>,
    /// The colour literals `\color` has set, each once ([`CURRENT_COLOR`] indexes them).
    colors: Vec<String>,
    /// `\definecolor`'s colours: name and literal.
    color_names: Vec<(String, String)>,
    /// url.sty's `\UrlBigBreakPenalty` and `\UrlBreakPenalty`: `\relpenalty` and
    /// `\binoppenalty` when it was loaded (`\mathchardef`).
    url_penalties: [i32; 2],
    /// Whether a page has been shipped (the kernel's first-page specials are done).
    shipped: bool,
    /// hyperref: each label's anchor, and the pre-scan's for labels ahead.
    label_hrefs: std::collections::HashMap<String, String>,
    predicted_hrefs: std::collections::HashMap<String, String>,
    /// The link `ref_link` set up for `rt@startlink`: attributes and anchor.
    link: Option<(usize, String)>,
    /// hyperref's options.
    hy: hyperref::HyperOpts,
    /// The URI the next `rt@startlink` links to (`\url`, `\href`).
    uri: Option<String>,
    /// The anchor `raised_link` left for `rt@anchor`.
    anchor: Option<String>,
    /// hyperref's `Hfootnote` counter (every mark; `\maketitle` does not reset it) and its
    /// `Item` counter (every numbered item).
    hfootnote: u32,
    items: u32,
    /// enumerate.sty's label templates (`enum_template`): the parts, `None` for
    /// `\theenum<d>`, and the style of `\theenum<d>` (`1`, `a`, `A`, `i`, `I`).
    enum_templates: Vec<(Vec<Option<Token>>, u8)>,
    /// enumitem's `\setlist` keys: the list (0 `list`, 1 `enumerate`, 2 `itemize`, 3
    /// `description`), the level (0 for all), the keys.
    enit_set: Vec<(u8, u8, Vec<Token>)>,
    /// The open `\parbox`es and minipages, innermost last (`parbox`).
    pbox: Vec<parbox::Saved>,
    /// The open `\colorbox`es' frame and background colours as `rt@color` tokens.
    colorboxes: Vec<(Vec<Token>, Vec<Token>)>,
    /// `\interdisplaylinepenalty` once amsmath has set it to `\@M`; only the preamble's
    /// `\allowdisplaybreaks` changes it.
    display_pen: i32,
    /// `\usepackage[T1]{fontenc}`: T1 is the text encoding (`\encodingdefault`), else OT1.
    t1: bool,
    /// A theorem's anchor, waiting for its head (`theorem_anchor_tokens`).
    thm_anchor: Option<String>,
    /// hyperref's `\Hy@linkcounter` (starred headings) and a starred heading's anchor,
    /// waiting for its title.
    link_counter: u32,
    star_anchor: Option<String>,
    /// The fonts microtype's `\pickup@font` hook has set up.
    pub(super) mt_fonts: Vec<u16>,
    /// LaTeX's registers (`data_class::CLASS_REGS`), indexed by the `R_*` constants.
    regs: [Reg; data_class::CLASS_REGS.len()],
}

/// The text the native job runs for `src`, from after `\documentclass{article}` to
/// `\end{document}`, else `None` (it runs on the engine).
///
/// Accepted today: one `\documentclass{article}`, the preamble commands of [`PREAMBLE`], and text (letters, digits, spaces,
/// punctuation, braces, `%` comments, `~`), the commands of [`WORDS`] and the macros the
/// document defines with `\newcommand`, the environments of [`ENVS`], and the control
/// symbols `\ `, `\$`, `\&`, `\#`, `\%`, `\_`, `\{`, `\}`, `\,`, `\@`.
pub fn accepts(src: &str) -> Option<&str> {
    scan(src).ok()
}

/// [`accepts`]'s scan: the body, or the first name it does not accept.
fn scan(src: &str) -> Result<&str, &str> {
    scan_with(src, &mut |_| false)
}

/// Every name, environment, option or character [`scan`] stops at, once each, in order
/// of first appearance: the native job's whole gap list for `src` (`--native-gaps`).
pub fn native_blockers(src: &str) -> Vec<&str> {
    let mut all: Vec<&str> = Vec::new();
    let end = scan_with(src, &mut |what| {
        if !all.contains(&what) {
            all.push(what);
        }
        true
    });
    if let Err(what) = end {
        if !what.is_empty() && !all.contains(&what) {
            all.push(what);
        }
    }
    all
}

/// `src` with each body `\bibliography{..}` replaced by the text of `bbl` (`\jobname.bbl`,
/// or nothing when there is no such file) and each `\bibliographystyle{..}` removed, as the
/// kernel does in one pass (`latex.ltx`: `\bibliography` is `\@input@{\jobname.bbl}`, an
/// `\InputIfFileExists` that types only "No file"; `\bibliographystyle` writes only to the
/// aux file). A `\bibliography` in the preamble is left for the pre-scan to refuse.
pub fn splice_bibliography<'a>(src: &'a str, bbl: Option<&str>) -> std::borrow::Cow<'a, str> {
    if !src.contains("\\bibliography") {
        return std::borrow::Cow::Borrowed(src);
    }
    let begin = src.find("\\begin{document}").unwrap_or(usize::MAX);
    let mut out = String::with_capacity(src.len() + bbl.map_or(0, str::len));
    let (mut i, mut from) = (0, 0);
    let b = src.as_bytes();
    while i < b.len() {
        match b[i] {
            b'%' => i += src[i..].find('\n').unwrap_or(src.len() - i),
            b'\\' if src[i + 1..].starts_with("bibliography") => {
                let n = 1 + "bibliography".len();
                let style = src[i + n..].starts_with("style");
                let name_end = i + n + if style { 5 } else { 0 };
                if b.get(name_end).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'@') || !style && i < begin {
                    i = name_end;
                    continue;
                }
                let after = src[name_end..].trim_start();
                let Some(close) = after.strip_prefix('{').and_then(|r| r.find('}')) else {
                    i = name_end;
                    continue;
                };
                let end = src.len() - after.len() + close + 2;
                out.push_str(&src[from..i]);
                // The file's last end of line and the line `\bibliography` ends give TeX two
                // spaces where the splice gives one; both fall in vertical mode, after
                // `\end{thebibliography}`.
                if !style {
                    out.push_str(bbl.map_or("", |t| t.strip_suffix('\n').unwrap_or(t)));
                }
                (i, from) = (end, end);
            }
            b'\\' => i += 2,
            _ => i += 1,
        }
    }
    out.push_str(&src[from..]);
    std::borrow::Cow::Owned(out)
}

/// The kernel's text of the UTF-8 character at the start of `s` (one `char` per byte) and
/// its length in bytes.
fn utf8_text(s: &str) -> Option<(usize, &'static str)> {
    let mut it = s.chars().map(|c| c as u32);
    let b = it.next()?;
    let more = match b {
        0xC2..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF4 => 3,
        _ => return None,
    };
    let mut c = b & (0x7F >> (more + 1));
    for _ in 0..more {
        let t = it.next().filter(|t| (0x80..0xC0).contains(t))?;
        c = c << 6 | (t & 0x3F);
    }
    // Each byte is a `char` below 256: two bytes of UTF-8 in `s`.
    let len = s.chars().take(more + 1).map(char::len_utf8).sum();
    crate::data::data_utf8::text(c).map(|t| (len, t))
}

/// The control-sequence names in a `data_utf8` text.
fn text_names(text: &str) -> impl Iterator<Item = &str> {
    text.split('\\').skip(1).map(|r| {
        let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
        &r[..n.max(r.chars().next().map_or(0, char::len_utf8))]
    })
}

const SCAN_PLAIN: u8 = 1;
const PREDICT_PLAIN: u8 = 2;

/// Per byte: `SCAN_PLAIN` when [`scan_with`] passes over it (an ASCII letter or digit, or one
/// of its text characters), `PREDICT_PLAIN` when [`predict`] does (anything but `%`, `{`,
/// `}` and `\`).
const BYTE_CLASS: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut c = 0;
    while c < 256 {
        let b = c as u8;
        if !matches!(b, b'%' | b'{' | b'}' | b'\\') {
            t[c] |= PREDICT_PLAIN;
        }
        c += 1;
    }
    let text = b" \t\r\n.,;:!?'`\"()[]*/-{}~#$^_=+<>|@";
    let mut k = 0;
    while k < text.len() {
        t[text[k] as usize] |= SCAN_PLAIN;
        k += 1;
    }
    let mut c = 0;
    while c < 128 {
        if (c as u8).is_ascii_alphanumeric() {
            t[c] |= SCAN_PLAIN;
        }
        c += 1;
    }
    t
};

/// The first index at or after `i` whose byte is not of `class` ([`BYTE_CLASS`]), or the
/// end. Eight bytes at a time (SWAR, native-architecture.md §7b): a word's bytes that may
/// leave the class are flagged with the zero-byte test; the lowest flag is a real candidate
/// (a borrow only runs upward), which the table then decides.
fn plain_end(b: &[u8], mut i: usize, class: u8) -> usize {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let zero = |x: u64| x.wrapping_sub(LO) & !x & HI;
    let is = |v: u64, c: u8| zero(v ^ (LO * c as u64));
    while let Some(chunk) = b.get(i..i + 8) {
        let v = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        let m = if class == PREDICT_PLAIN {
            is(v, b'%') | is(v, b'{') | is(v, b'}') | is(v, b'\\')
        } else {
            // Not in the scan's class: `%`, `&`, `\`, DEL, control bytes (tab, return and
            // newline are, and the table lets them through), and bytes above ASCII.
            is(v, b'%') | is(v, b'&') | is(v, b'\\') | is(v, 0x7f) | (v & HI) | (v.wrapping_sub(LO * 0x20) & !v & HI)
        };
        if m == 0 {
            i += 8;
            continue;
        }
        let k = i + (m.trailing_zeros() / 8) as usize;
        if BYTE_CLASS[b[k] as usize] & class == 0 {
            return k;
        }
        i = k + 1;
    }
    i + b.get(i..).map_or(0, |r| r.iter().take_while(|&&c| BYTE_CLASS[c as usize] & class != 0).count())
}

/// [`scan`], telling `block` each thing it does not take; `block` returning `true` goes on
/// past it (the census), `false` stops there.
fn scan_with<'a>(src: &'a str, block: &mut dyn FnMut(&'a str) -> bool) -> Result<&'a str, &'a str> {
    let mut blocked = false;
    let mut stop = |what: &'a str, blocked: &mut bool| {
        *blocked = true;
        if block(what) { Ok(()) } else { Err(what) }
    };
    let b = src.as_bytes();
    let mut defined: Vec<&str> = Vec::new();
    let mut define_next = false;
    // `\providecommand` of a defined name does nothing: it never redefines an internal.
    let mut provide = false;
    let mut class = false;
    // `\documentclass{report}`: what report.cls defines otherwise than article.cls and the
    // native job does not take yet stops (`2026-10-28-report-class-design.md`).
    let mut report = false;
    // The packages `\usepackage` loads from native tables, with the packages they load.
    let mut loaded: Vec<&'static str> = Vec::new();
    let (mut have, mut have_for) = (KERNEL_BIT, 0);
    // Open `tabular`s: `&`, `\hline` and `\tabularnewline` are taken only inside one.
    let mut tabs = 0usize;
    let mut floats = 0usize;
    // The `\newtheorem` environments.
    let mut thms: Vec<&str> = Vec::new();
    // The end of enumitem keys being read: `\arabic*` and its kin are its label's counter.
    let mut enit_end = 0;
    let mut i = 0;
    while i < b.len() {
        // A run of bytes the scan passes over, taken whole.
        if BYTE_CLASS[b[i] as usize] & SCAN_PLAIN != 0 {
            i = plain_end(b, i + 1, SCAN_PLAIN);
            continue;
        }
        match b[i] {
            b'&' if tabs > 0 => i += 1,
            b'%' => i = src[i..].find('\n').map_or(b.len(), |n| i + n + 1),
            b'\\' => {
                let rest = &src[i + 1..];
                if rest.starts_with("end{document}") || rest.starts_with("endinput") && !rest[8..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                    return if !class {
                        Err("no \\documentclass{article}")
                    } else if blocked {
                        Err("")
                    } else {
                        Ok(&src[..i])
                    };
                }
                // `\verbatim`'s text is not scanned.
                if rest.starts_with("begin{verbatim}") {
                    i += rest.find("\\end{verbatim}").ok_or("verbatim")? + 1 + "\\end{verbatim}".len();
                    continue;
                }
                let mut n = rest.bytes().take_while(|c| c.is_ascii_alphabetic() || *c == b'@').count();
                // `@` is a letter only after `\makeatletter`: an unknown name with `@` is read
                // as its letters before the `@` (or the control symbol `\@`).
                // The loaded packages' bits, recomputed only when `require` added one.
                if loaded.len() != have_for {
                    (have, have_for) = (loaded.iter().fold(KERNEL_BIT, |m, &p| m | package_bit(p)), loaded.len());
                }
                let known = |name: &str| {
                    let (table, blocked, list) = names_row(name);
                    blocked & have == 0 && (table & have != 0 || list & DRIVER_NATIVE != 0 && PACKAGE_NATIVES.iter().any(|p| p.1 == name && DRIVER_PACKAGES.contains(&p.0) && loaded.contains(&p.0)) || list & (IN_WORDS | IN_PREAMBLE) != 0) || defined.contains(&name)
                };
                if rest[..n].contains('@') && !known(&rest[..n]) {
                    n = rest[..n].find('@').unwrap_or(n);
                }
                if n == 0 {
                    if define_next && !provide {
                        stop("a definition of an internal macro", &mut blocked)?;
                    }
                    define_next = false;
                    let symbol = rest.get(..rest.chars().next().map_or(0, char::len_utf8)).unwrap_or("");
                    if !(rest.bytes().next().is_some_and(|c| b" $&#%_{},;:!>@\r\n\t()[]\\\"'.=^`~".contains(&c)) && blocked_names(symbol) & have == 0) && !known(symbol) {
                        stop(rest.get(..rest.chars().next().map_or(0, char::len_utf8)).unwrap_or(""), &mut blocked)?;
                        i += 1 + rest.chars().next().map_or(0, char::len_utf8);
                        continue;
                    }
                    i += 2;
                    continue;
                }
                let name = &rest[..n];
                if i < enit_end && matches!(name, "arabic" | "alph" | "Alph" | "roman" | "Roman") && rest[n..].starts_with('*') {
                    i += 2 + n;
                    continue;
                }
                // url.sty reads `\url{...}` and `\path{...}` with its specials other: the text is
                // not scanned (it ends at the first `}`, as `\Url@y` reads it from the file).
                // hyperref's `\href` reads its URL so too (`\hyper@normalise`).
                if matches!(name, "url" | "path") && (loaded.contains(&"url") || loaded.contains(&"hyperref")) || name == "href" && loaded.contains(&"hyperref") {
                    if let Some(k) = rest[n..].trim_start_matches(' ').strip_prefix('{').and_then(|r| r.find('}').map(|k| rest.len() - r.len() + k)) {
                        i += 1 + k + 1;
                        continue;
                    }
                }
                if tabs == 0 && matches!(name, "hline" | "tabularnewline" | "multicolumn" | "cline") || floats == 0 && name == "caption" {
                    stop(name, &mut blocked)?;
                }
                if name == "multicolumn" {
                    // `{n}{spec}`: one column `tabular::mkpream` takes.
                    let after = rest[n..].trim_start();
                    let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| r[k + 1..].trim_start())).and_then(|r| r.strip_prefix('{')).and_then(|r| balanced(r).map(|k| &r[..k]));
                    if spec.and_then(|s| tabular::spec_cols(s, loaded.contains(&"array"), loaded.contains(&"tabularx"))) != Some(1) {
                        stop("multicolumn preamble", &mut blocked)?;
                    }
                }
                if !report && name == "chapter" {
                    stop(name, &mut blocked)?;
                }
                if report && (REPORT_LATER.contains(&name) || name == "begin" && ["{abstract}", "{thebibliography}"].iter().any(|e| rest[n..].starts_with(e))) {
                    stop(name, &mut blocked)?;
                }
                if define_next {
                    let prefixed = !report && name.strip_prefix("the").filter(|c| PREFIXED.contains(c)).is_some_and(|c| {
                        // Only in the preamble, and not with amsmath's `\numberwithin`, which redefines `\theequation` in its turn.
                        !(c == "equation" && src.contains("\\numberwithin")) && !src[..i].contains("\\begin{document}") && the_prefix(c, &rest[n..]).is_some()
                    });
                    if !provide && !prefixed && (name.contains('@') || READ_INTERNALLY.contains(&name)) {
                        stop("a definition of an internal macro", &mut blocked)?;
                    }
                    define_next = false;
                    // The definition's text is read by `the_prefixes`, not scanned.
                    if prefixed {
                        let a = rest[n..].find("\\arabic").unwrap_or(0);
                        i += 1 + n + a + rest[n + a..].find("}}").unwrap_or(0) + 2;
                        continue;
                    }
                    defined.push(name);
                } else if matches!(name, "newcommand" | "renewcommand" | "providecommand" | "let" | "def" | "DeclareMathOperator") {
                    define_next = true;
                    provide = name == "providecommand";
                } else if name == "usepackage" {
                    // `\usepackage[options]{a,b}`: each a package with a native table, no options.
                    let after = rest[n..].trim_start();
                    let (opts, after) = match after.strip_prefix('[') {
                        Some(r) => r.split_once(']').map_or(("x", ""), |(o, a)| (o, a.trim_start())),
                        None => ("", after),
                    };
                    let Some(list) = after.strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0) else { return Err(name) };
                    for p in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                        if report && matches!(p, "hyperref" | "fancyhdr") {
                            // Their anchors and marks are article's.
                            stop(p, &mut blocked)?;
                        } else if p == "geometry" {
                            let opts: String = opts.lines().map(|l| l.split('%').next().unwrap_or("")).collect();
                            let mut g = super::engine_geometry::Geometry::default();
                            if opts.contains('\\') || g.keys(&opts, &mut |_| Some(0)).is_err() {
                                stop("a geometry option", &mut blocked)?;
                            }
                        } else if p == "hyperref" {
                            // `hyper_keys` reads them at the load; commands there are not native.
                            if opts.contains('\\') || opts.contains('%') {
                                stop("a hyperref option", &mut blocked)?;
                            }
                        } else if p == "fontenc" {
                            // Only `[T1]` (`font_encoding`); it loads nothing.
                            if opts.split(',').map(str::trim).filter(|o| !o.is_empty()).ne(["T1"]) {
                                stop("fontenc options other than T1", &mut blocked)?;
                            }
                            continue;
                        } else if !same_options(p, opts) {
                            stop("package options", &mut blocked)?;
                        }
                        if !require(p, &mut loaded) {
                            stop(p, &mut blocked)?;
                        }
                    }
                    i = src.len() - after.len() + 1 + list.len() + 1;
                    continue;
                } else if name == "newtheorem" {
                    // `{env}[shared]{Title}` or `{env}{Title}[level]`, with plain text;
                    // amsthm's `\newtheorem*{env}{Title}`.
                    let star = loaded.contains(&"amsthm") && rest[n..].trim_start().starts_with('*');
                    let at = if star { n + rest[n..].find('*').expect("a star") + 1 } else { n };
                    let Some((env, shared, title, _, len, parent)) = newtheorem_args(&rest[at..]).filter(|a| !star || a.1.is_none() && a.3.is_none() && a.5.is_none()).filter(|a| !report || a.3.is_none() && a.5.is_none()) else {
                        stop("newtheorem", &mut blocked)?;
                        i += 1 + n;
                        continue;
                    };
                    if ENVS.contains(&env) || AMS_ARRAYS.contains(&env) || thms.contains(&env) || defined.contains(&env) || shared.is_some_and(|s| !thms.contains(&s)) || parent.is_some_and(|s| !thms.contains(&s)) {
                        stop("newtheorem", &mut blocked)?;
                    }
                    thms.push(env);
                    // A title with commands is scanned as text from its brace on.
                    let plain = title.bytes().all(|c| c.is_ascii_alphanumeric() || b" -'".contains(&c));
                    i += 1 + at + if plain { len } else { title.as_ptr() as usize - rest[at..].as_ptr() as usize - 1 };
                    continue;
                } else if name == "theoremstyle" && !matches!(rest[n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0.trim()), Some("plain" | "definition" | "remark")) {
                    stop("theoremstyle", &mut blocked)?;
                } else if name == "documentclass" {
                    if class {
                        stop("documentclass", &mut blocked)?;
                    }
                    match class_options(&rest[n..]) {
                        Err(what) => stop(what, &mut blocked)?,
                        Ok(c) => report = c.2,
                    }
                    class = true;
                } else if matches!(name, "fbox" | "textsuperscript" | "textsubscript") {
                } else if name == "thepage" && loaded.contains(&"fancyhdr") && !src[..i].contains("\\begin{document}") {
                    // In a fancyhdr slot, typeset at output time (`fancy_tokens`).
                } else if matches!(name, "colorbox" | "fcolorbox") {
                    // xcolor's `\color@b@x` (`colorbox`); color.sty's differs.
                    if !loaded.contains(&"xcolor") {
                        stop(name, &mut blocked)?;
                    }
                } else if name == "parbox" {
                    // `\parbox[c|t|b]{width}`: no height, no inner position (`parbox`).
                    if !box_position_ok(&rest[n..]) {
                        stop("a \\parbox with a height or another position", &mut blocked)?;
                    }
                } else if name == "fontsize" {
                    // `\fontsize{size}{skip}\selectfont`, each a number of points (`font_size`).
                    let after = rest[n..].trim_start();
                    let arg = |r: &'a str| r.strip_prefix('{').and_then(|r| r.split_once('}'));
                    let next = arg(after).and_then(|(x, r)| arg(r.trim_start()).map(|(y, r)| (x, y, r.trim_start())));
                    match next {
                        Some((x, y, r)) if size_arg(x).is_some() && size_arg(y).is_some() && r.starts_with("\\selectfont") && !r[11..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '@') => {
                            i = src.len() - r.len() + 11;
                            continue;
                        }
                        _ => stop(name, &mut blocked)?,
                    }
                } else if name == "setlist" && loaded.contains(&"enumitem") {
                    // `\setlist[names]{keys}` in the preamble (`set_list`).
                    let after = rest[n..].trim_start();
                    let (names, after) = match after.strip_prefix('[') {
                        Some(r) => r.split_once(']').map_or(("?", ""), |(o, a)| (o, a.trim_start())),
                        None => ("", after),
                    };
                    let keys = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| &r[..k]));
                    let names_ok = names.split(',').map(str::trim).all(|p| p.is_empty() || matches!(p, "enumerate" | "itemize" | "description") || p.parse::<u8>().is_ok());
                    if !names_ok || !keys.is_some_and(enit_keys_ok) || src[..i].contains("\\begin{document}") {
                        stop("enumitem list options", &mut blocked)?;
                    }
                    enit_end = src.len() - after.len() + 1 + keys.map_or(0, str::len);
                } else if !known(name) {
                    stop(name, &mut blocked)?;
                } else if matches!(name, "setlength" | "addtolength") {
                    // The length (a name the native job checks when it runs) and its value,
                    // read by the engine's assignment.
                    let after = rest[n..].trim_start();
                    let target = match after.strip_prefix('{') {
                        Some(r) => r.find('}').map(|k| k + 2),
                        None => after.strip_prefix('\\').map(|r| 1 + r.bytes().take_while(|c| c.is_ascii_alphabetic() || *c == b'@').count()),
                    };
                    let Some(target) = target else { return Err(name) };
                    let skipped = rest.len() - n - after.len() + target;
                    let value = rest[n + skipped..].trim_start();
                    let Some(len) = value.strip_prefix('{').and_then(balanced) else { return Err(name) };
                    if loaded.contains(&"calc") && calc_expression(&value[1..1 + len]) {
                        stop("a calc expression", &mut blocked)?;
                    }
                    i += 1 + n + (rest.len() - n - value.len()) + len + 2;
                    continue;
                }
                // calc's `\setcounter` reads an expression; the native job reads a number.
                if matches!(name, "setcounter" | "addtocounter") && loaded.contains(&"calc") {
                    let value = rest[n..].split_once('}').and_then(|r| r.1.trim_start().strip_prefix('{')).and_then(|r| r.split_once('}')).map_or("+", |r| r.0);
                    if calc_expression(value) {
                        stop("a calc expression", &mut blocked)?;
                    }
                }
                if name == "begin" || name == "end" {
                    let env = rest[n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0).ok_or(name)?;
                    // enumitem's lists with their defaults print as the kernel's; the keys of
                    // `enit_keys_ok` are native in `enumerate` and `itemize`.
                    if name == "begin" && matches!(env, "enumerate" | "itemize" | "description") && loaded.contains(&"enumitem") {
                        let after = rest[n + env.len() + 2..].trim_start();
                        if let Some(r) = after.strip_prefix('[') {
                            let keys = bracketed(r);
                            if env == "description" || loaded.contains(&"enumerate") || !keys.is_some_and(enit_keys_ok) {
                                stop("enumitem list options", &mut blocked)?;
                            }
                            enit_end = src.len() - r.len() + keys.map_or(0, str::len);
                        }
                    }
                    if name == "begin" && env == "minipage" && !box_position_ok(&rest[n + env.len() + 2..]) {
                        stop("a minipage with a height or another position", &mut blocked)?;
                    }
                    let ams_array = AMS_ARRAYS.contains(&env) && loaded.contains(&"amsmath");
                    let proof = env == "proof" && loaded.contains(&"amsthm") || env == "tabularx" && loaded.contains(&"tabularx");
                    if !(ams_array || proof || thms.contains(&env) || ENVS.contains(&env) && blocked_names(env) & have == 0 && (!matches!(env, "equation*" | "align" | "align*" | "gather" | "gather*" | "multline" | "multline*" | "aligned" | "gathered" | "split") || loaded.contains(&"amsmath"))) && !defined.contains(&env) {
                        stop(env, &mut blocked)?;
                    }
                    if matches!(env, "figure" | "table" | "figure*" | "table*") {
                        // A float in a float or a tabular is `\@parmoderr`.
                        if name == "end" {
                            floats = floats.saturating_sub(1);
                        } else if floats > 0 || tabs > 0 {
                            stop("a float in a box", &mut blocked)?;
                        } else {
                            floats += 1;
                        }
                    }
                    // `align`'s `&` is taken as a tabular's is.
                    if (matches!(env, "tabular" | "tabularx" | "align" | "align*" | "gather" | "gather*" | "multline" | "multline*" | "aligned" | "gathered" | "split" | "array" | "eqnarray" | "eqnarray*") || ams_array) && name == "end" {
                        tabs = tabs.saturating_sub(1);
                    } else if matches!(env, "align" | "align*" | "gather" | "gather*" | "multline" | "multline*" | "aligned" | "gathered" | "split" | "eqnarray" | "eqnarray*") || ams_array {
                        tabs += 1;
                    } else if env == "array" && !defined.contains(&env) {
                        // `[pos]{spec}`: a preamble `tabular::mkpream_array` takes.
                        tabs += 1;
                        let after = rest[n + env.len() + 2..].trim_start();
                        let after = match after.strip_prefix('[') {
                            Some(r) => r.split_once(']').map_or("", |(_, a)| a.trim_start()),
                            None => after,
                        };
                        let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| &r[..k]));
                        if spec.and_then(tabular::array_spec).and_then(|s| tabular::mkpream_array(&s)).is_none() {
                            stop("array preamble", &mut blocked)?;
                        }
                    } else if env == "tabular" && !defined.contains(&env) {
                        // `[pos]{spec}`: a preamble `tabular::mkpream` takes.
                        tabs += 1;
                        let after = rest[n + env.len() + 2..].trim_start();
                        let after = match after.strip_prefix('[') {
                            Some(r) => r.split_once(']').map_or("", |(_, a)| a.trim_start()),
                            None => after,
                        };
                        let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| &r[..k]));
                        if !spec.is_some_and(|s| tabular::spec_ok(s, loaded.contains(&"array"), loaded.contains(&"tabularx"))) {
                            stop("tabular preamble", &mut blocked)?;
                        }
                    } else if env == "tabularx" && loaded.contains(&"tabularx") && !defined.contains(&env) {
                        // `{width}[pos]{spec}`.
                        tabs += 1;
                        let after = rest[n + env.len() + 2..].trim_start();
                        let after = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| r[k + 1..].trim_start())).unwrap_or("");
                        let after = match after.strip_prefix('[') {
                            Some(r) => r.split_once(']').map_or("", |(_, a)| a.trim_start()),
                            None => after,
                        };
                        let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| &r[..k]));
                        if !spec.is_some_and(|s| tabular::spec_ok(s, true, true)) {
                            stop("tabular preamble", &mut blocked)?;
                        }
                    }
                }
                i += 1 + n;
            }
            c if c.is_ascii_alphanumeric() || b" \t\r\n.,;:!?'`\"()[]*/-{}~#$^_=+<>|@".contains(&c) => i += 1,
            // A UTF-8 character (each byte is one `char` here) the kernel declares, whose text
            // names only what the native job takes.
            _ => {
                let n = src[i..].chars().next().map_or(1, char::len_utf8);
                // The loaded packages' bits, recomputed only when `require` added one.
                if loaded.len() != have_for {
                    (have, have_for) = (loaded.iter().fold(KERNEL_BIT, |m, &p| m | package_bit(p)), loaded.len());
                }
                let known = |name: &str| {
                    let (table, blocked, list) = names_row(name);
                    blocked & have == 0 && (table & have != 0 || list & DRIVER_NATIVE != 0 && PACKAGE_NATIVES.iter().any(|p| p.1 == name && DRIVER_PACKAGES.contains(&p.0) && loaded.contains(&p.0)) || list & (IN_WORDS | IN_PREAMBLE) != 0) || defined.contains(&name)
                };
                match utf8_text(&src[i..]) {
                    Some((len, text)) if text_names(text).all(|name| known(name) || name.len() == 1 && "'`^\"~=. -".contains(name)) => i += len,
                    _ => {
                        stop(src.get(i..i + n).filter(|c| c.is_ascii()).unwrap_or("a non-ASCII character"), &mut blocked)?;
                        i += n;
                    }
                }
            }
        }
    }
    Err("no \\end{document}")
}

/// An `align` body's rows: split at each `\\` outside braces; the text after the last is
/// a row too, empty after a trailing `\\`.
fn split_rows(body: &str) -> Vec<&str> {
    let (b, mut rows, mut depth, mut start, mut i) = (body.as_bytes(), Vec::new(), 0i32, 0, 0);
    // Open environments (an `array` or `cases` in a row): their `\\` are their own.
    let mut inner = 0i32;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\\' if body[i..].starts_with("\\begin{") => inner += 1,
            b'\\' if body[i..].starts_with("\\end{") => inner -= 1,
            b'\\' if b.get(i + 1) == Some(&b'\\') && depth == 0 && inner == 0 => {
                rows.push(&body[start..i]);
                i += 2;
                start = i;
                continue;
            }
            b'\\' => i += 1,
            _ => {}
        }
        i += 1;
    }
    rows.push(&body[start..]);
    rows
}

/// The text after a `[` to its `]` outside braces.
/// An enumitem `label` value `predict` can type: one `\arabic*`, `\alph*`, `\Alph*`,
/// `\roman*` or `\Roman*` with letters, digits, spaces and punctuation around it, which
/// the item's `\ref` prints whole (`enit_setup`).
fn enit_template(keys: &str) -> Option<String> {
    let v = keys.split(',').map(str::trim).find_map(|k| k.strip_prefix("label").map(str::trim_start).and_then(|r| r.strip_prefix('=')))?.trim();
    let (pre, rest) = v.split_once('\\')?;
    let (style, post) = rest.split_once('*')?;
    let plain = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || " .:;()-".contains(c));
    (matches!(style, "arabic" | "alph" | "Alph" | "roman" | "Roman") && plain(pre) && plain(post)).then(|| v.to_string())
}

fn bracketed(s: &str) -> Option<&str> {
    let mut depth = 0;
    for (k, c) in s.bytes().enumerate() {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b']' if depth == 0 => return Some(&s[..k]),
            _ => {}
        }
    }
    None
}

/// The length of a braced group's contents after its `{`, to its matching `}`.
fn balanced(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (k, c) in s.bytes().enumerate() {
        match c {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Some(k),
            b'}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// What report.cls defines otherwise than article.cls and the native job does not take yet:
/// the title page versions of `\maketitle` (and `abstract`), `\part`, `\appendix`, the
/// lists of figures and tables, `\numberwithin` (beside `\thechapter.`).
const REPORT_LATER: [&str; 7] = ["maketitle", "part", "appendix", "listoffigures", "listoftables", "numberwithin", "printindex"];

/// `\documentclass[options]{article}` after its name: the size option's index in
/// `data_class::CLASS_SIZES` and the combination of the parameter-only options
/// (`data_class::option_key`), else the first thing the native job does not take:
/// another class, an option with behaviour of its own (`twocolumn`, `titlepage`, `leqno`,
/// `fleqn`, `openbib`), an unknown one, or two of one group (`\ProcessOptions` would apply
/// them in the class's order, not the document's).
fn class_options(rest: &str) -> Result<(usize, u32, bool), &str> {
    let (opts, after) = match rest.strip_prefix('[') {
        Some(r) => r.split_once(']').ok_or("documentclass")?,
        None => ("", rest),
    };
    // `report` (openany, oneside by default) or `article`.
    let report = after.trim_start().starts_with("{report}");
    if !report && !after.trim_start().starts_with("{article}") {
        return Err("documentclass");
    }
    let (mut size, mut paper, mut orient, mut side, mut draft) = (None, None, None, None, None);
    for o in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        let slot = if let Some(i) = data_class::CLASS_SIZES.iter().position(|s| *s == o) {
            (&mut size, i)
        } else if let Some(i) = data_class::PAPERS.iter().position(|p| *p == o) {
            (&mut paper, i)
        } else {
            match o {
                "landscape" => (&mut orient, 1),
                "oneside" => (&mut side, 0),
                "twoside" => (&mut side, 1),
                "draft" => (&mut draft, 1),
                "final" => (&mut draft, 0),
                "onecolumn" | "notitlepage" => continue,
                "openany" if report => continue,
                _ => return Err(o),
            }
        };
        if slot.0.replace(slot.1).is_some() {
            return Err(o);
        }
    }
    let key = data_class::option_key(paper.unwrap_or(0), orient == Some(1), side == Some(1), draft == Some(1));
    Ok((size.unwrap_or(0), key, report))
}

/// A label's value as tokens: letters are letters, the rest other characters, as the
/// `.aux` file's `\newlabel` reads them.
/// The first character of a `\bibitem` key among the labels: no `\label` key has it.
const BIB: char = '\0';

fn label_tokens(text: &str) -> Vec<Token> {
    text.chars().map(|c| match c {
        ' ' => Token::char(Cat::Space, ' '),
        _ if c.is_ascii_alphabetic() => Token::char(Cat::Letter, c),
        _ => Token::char(Cat::Other, c),
    }).collect()
}

/// Each `\label`'s value (`\@currentlabel`) read off the body of a document [`scan`]
/// accepts, so a `\ref` ahead of its `\label` needs no second run (P2b): the counters the
/// native job's commands step (numbered headings, `enumerate`'s `\item`, `\footnote`) and
/// the groups that make `\@currentlabel` and `\@enumdepth` local (braces, environments).
/// The counters are global (`\refstepcounter`, `\usecounter`). It stops, keeping what it has,
/// at anything else that might step one: a macro the document defines, `\item[` or
/// `\footnote[`, an environment it does not know. Every predicted value is checked by its
/// `\label` when it runs (`label`), so a wrong one only sends the document to the engine.
fn predict(src: &str) -> Predicted {
    let mut out = Predicted::default();
    let Some(start) = src.find("\\begin{document}") else { return out };
    let enumitem = src[..start].contains("enumitem");
    // report: `\chapter` is level 0, `\section` level 1.
    let report = src.find("\\documentclass").is_some_and(|i| class_options(&src[i + "\\documentclass".len()..]).is_ok_and(|c| c.2));
    // A `\setlist` whose keys name a `label`.
    let set_labels = src.match_indices("\\setlist").any(|(at, _)| {
        let r = src[at + "\\setlist".len()..].trim_start();
        let r = match r.strip_prefix('[') {
            Some(r) => r.split_once(']').map_or("", |x| x.1).trim_start(),
            None => r,
        };
        r.strip_prefix('{').and_then(|r| r.split_once('}')).is_none_or(|(keys, _)| keys.contains("label"))
    });
    // Operators the preamble declares (`\DeclareMathOperator{\tr}{tr}`) number nothing.
    let operators: Vec<&str> = src[..start]
        .split("\\DeclareMathOperator")
        .skip(1)
        .filter_map(|r| r.trim_start_matches('*').trim_start().strip_prefix("{\\")?.split('}').next())
        .collect();
    // The body's definitions too: a macro used before its definition is checked as any other.
    let (quiet, loud, star_macros) = quiet_macros(src, &operators);
    // The preamble's theorems: environment, counter owner, reset level; and each counter.
    let mut thms: Vec<(&str, usize, Option<usize>, Option<usize>)> = Vec::new();
    // amsthm's `\newtheorem*` numbers nothing.
    let mut starred: Vec<&str> = Vec::new();
    for (at, _) in src[..start].match_indices("\\newtheorem") {
        let after = &src[at + "\\newtheorem".len()..];
        if let Some(r) = after.trim_start().strip_prefix('*') {
            let Some((env, ..)) = newtheorem_args(r) else { return out };
            starred.push(env);
            continue;
        }
        let Some((env, shared, _, within, _, parent)) = newtheorem_args(after) else { return out };
        let owner = match shared {
            Some(s) => match thms.iter().find(|t| t.0 == s) {
                Some(t) => t.1,
                None => return out,
            },
            None => thms.len(),
        };
        // `[theorem]`: the counter of a theorem that owns one.
        let parent = match parent {
            Some(p) => match thms.iter().position(|t| t.0 == p) {
                Some(k) if thms[k].1 == k && thms[k].3.is_none() => Some(k),
                _ => return out,
            },
            None => None,
        };
        thms.push((env, owner, within, parent));
    }
    let mut thm_values = vec![0u32; thms.len()];
    let mut pf = the_prefixes(src);
    // amsmath's `\numberwithin{equation}{level}`.
    let mut eq_within = None;
    for (at, _) in src[..start].match_indices("\\numberwithin") {
        let args: Vec<&str> = src[at + "\\numberwithin".len()..].split('}').take(2).map(|a| a.trim_start().trim_start_matches('{').trim()).collect();
        match (args.as_slice(), ["section", "subsection", "subsubsection"].iter().position(|l| args.get(1) == Some(l))) {
            (["equation", _], Some(l)) => eq_within = Some(l),
            _ => return out,
        }
    }
    // Labels and anchors are made once into `strs` and named by index there, so a group
    // opened or an anchor set copies a number, not a string.
    struct Group {
        label: u32,
        // hyperref's `\@currentHref`, set by the headings (its other anchors decline).
        href: u32,
        depth: usize,
        enumerate: bool,
        // Inside an enumitem `enumerate` whose `label` key `predict` does not model: a
        // `\label` here stops it.
        opaque: bool,
        // An enumitem `label` key it does model (`enit_template`): the item's label, in
        // `strs` (0 for none).
        template: u32,
    }
    // `\hyper@makecurrent` is global: every open group takes the new `\@currentHref`.
    let mut strs: Vec<String> = vec![String::new(), "Doc-Start".to_string()];
    let intern = |strs: &mut Vec<String>, s: String| {
        strs.push(s);
        strs.len() as u32 - 1
    };
    let set_href = |groups: &mut Vec<Group>, strs: &mut Vec<String>, href: String| {
        let id = intern(strs, href);
        for g in groups.iter_mut() {
            g.href = id;
        }
    };
    let mut groups = vec![Group { label: 0, href: 1, depth: 0, enumerate: false, opaque: false, template: 0 }];
    let (mut sec, mut enums, mut footnote, mut equation) = ([0u32; SECNUMDEPTH], [0u32; 4], 0u32, 0u32);
    let mut appendix = false;
    // The keys a `\label` gave in the display now open, which a later `\tag` renumbers
    // (amsmath writes a display's label after its tag).
    let mut display_keys: Vec<String> = Vec::new();
    let mut pending: Option<u32> = None;
    let (mut floats, mut figtab) = (Vec::new(), [0u32; 2]);
    // Inside an `equation` whose number stands.
    let mut numbered = false;
    let b = src.as_bytes();
    let next = |i: usize| src[i..].trim_start_matches([' ', '\t', '\r', '\n']).as_bytes().first().copied();
    // hyperref's footnote anchors count every mark (`Hfootnote`, never reset); its item
    // anchors every numbered item (`Item`).
    let (mut hfootnote, mut items) = (0u32, 0u32);
    // `\Hy@linkcounter`: every starred heading, thebibliography's too.
    let mut links = 0u32;
    // The anchor of the group the pending label opens (a footnote's `Hfootnote.N`).
    let mut pending_href: Option<u32> = None;
    let push = |groups: &mut Vec<Group>, pending: &mut Option<u32>, pending_href: &mut Option<u32>| {
        let top = groups.last().expect("a group");
        let href = pending_href.take().unwrap_or(top.href);
        let g = Group { label: pending.take().unwrap_or(top.label), href, depth: top.depth, enumerate: top.enumerate, opaque: top.opaque, template: top.template };
        groups.push(g);
    };
    let mut i = start + "\\begin{document}".len();
    while i < b.len() {
        // A run of bytes other than `%`, braces and backslash, taken whole.
        if BYTE_CLASS[b[i] as usize] & PREDICT_PLAIN != 0 {
            i = plain_end(b, i + 1, PREDICT_PLAIN);
            continue;
        }
        match b[i] {
            b'%' => i = src[i..].find('\n').map_or(b.len(), |n| i + n + 1),
            b'{' => {
                push(&mut groups, &mut pending, &mut pending_href);
                i += 1;
            }
            b'}' => {
                if groups.len() > 1 {
                    groups.pop();
                }
                i += 1;
            }
            b'\\' => {
                let rest = &src[i + 1..];
                let n = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
                if n == 0 {
                    if rest.starts_with('[') {
                        display_keys.clear();
                    }
                    i += 2;
                    continue;
                }
                let name = &rest[..n];
                i += 1 + n;
                if rest[n..].starts_with('@') {
                    return out;
                }
                let arg = || rest[n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0);
                // A name the preamble defines with a text that might count, even one the
                // kernel also knows (`\renewcommand{\sec}{\section{S}}`).
                // A macro of starred headings only: the link counter, as the headings step it.
                if let Some(&(_, k)) = star_macros.iter().find(|s| s.0 == name) {
                    links += k as u32;
                    set_href(&mut groups, &mut strs, format!("section*.{links}"));
                    continue;
                }
                if loud.contains(&name) {
                    return out;
                }
                match name {
                    "begin" | "end" => {
                        let Some(env) = arg() else { return out };
                        if name == "end" {
                            if groups.len() > 1 {
                                groups.pop();
                            }
                            numbered &= env != "equation";
                            if matches!(env, "figure" | "table" | "figure*" | "table*") {
                                floats.pop();
                            }
                        } else if env == "verbatim" {
                            let Some(e) = src[i..].find("\\end{verbatim}") else { return out };
                            i += e + "\\end{verbatim}".len();
                            continue;
                        } else {
                            push(&mut groups, &mut pending, &mut pending_href);
                            let g = groups.last_mut().expect("a group");
                            match env {
                                "enumerate" => {
                                    // enumitem's `label` key, here or in a `\setlist`, changes its
                                    // labels; the spacing keys do not.
                                    let keys = rest[n + env.len() + 2..].trim_start().strip_prefix('[').map(|r| r.split(']').next().unwrap_or(r));
                                    if g.depth == 4 {
                                        return out;
                                    }
                                    g.template = 0;
                                    if enumitem && (keys.is_some_and(|k| k.contains("label")) || set_labels) {
                                        match keys.filter(|_| !set_labels).and_then(enit_template) {
                                            Some(t) => g.template = intern(&mut strs, t),
                                            None => g.opaque = true,
                                        }
                                    }
                                    g.depth += 1;
                                    g.enumerate = true;
                                    enums[g.depth - 1] = 0;
                                }
                                "itemize" | "description" | "quote" | "quotation" | "verse" | "center" | "flushleft" | "flushright" | "abstract" | "titlepage" => g.enumerate = false,
                                "thebibliography" => {
                                    g.enumerate = false;
                                    links += 1;
                                    set_href(&mut groups, &mut strs, format!("section*.{links}"));
                                }
                                "equation" => {
                                    display_keys.clear();
                                    equation += 1;
                                    g.label = intern(&mut strs, pf[2].clone() + &within_text(&sec, appendix, eq_within, equation));
                                    set_href(&mut groups, &mut strs, theorem_href("equation", &sec, eq_within, equation));
                                    numbered = true;
                                }
                                "equation*" => display_keys.clear(),
                                // `align`: a number per row (each `\\`, and the last) not
                                // `\nonumber`ed; a row's `\label` takes its number. The body
                                // is read here and passed over.
                                // `\eqnarray`'s `\stepcounter`, each row's `\stepcounter` after
                                // its number, `\endeqnarray`'s `\advance\c@equation\m@ne`: a
                                // label takes the counter as it stands in its row.
                                "eqnarray" | "eqnarray*" => {
                                    let after = &src[i + 2 + env.len()..];
                                    let close = format!("\\end{{{env}}}");
                                    let Some(k) = after.find(&close) else { return out };
                                    let body = &after[..k];
                                    let arrays = body.match_indices("\\begin{").all(|(p, _)| body[p + 7..].split('}').next().is_some_and(|e| e == "array" || AMS_ARRAYS.contains(&e)));
                                    if !arrays || body.contains('@') {
                                        return out;
                                    }
                                    equation += 1;
                                    for row in split_rows(body) {
                                        let mut rest = row;
                                        while let Some(p) = rest.find("\\label{") {
                                            let r = &rest[p + 7..];
                                            let Some(e) = r.find('}') else { return out };
                                            out.labels.insert(r[..e].to_string(), pf[2].clone() + &within_text(&sec, appendix, eq_within, equation));
                                            rest = &r[e..];
                                        }
                                        if env == "eqnarray" && !row.contains("\\nonumber") && !row.contains("\\notag") {
                                            equation += 1;
                                        }
                                    }
                                    equation -= 1;
                                    groups.pop();
                                    i += 2 + env.len() + k + close.len();
                                    continue;
                                }
                                "align" | "align*" | "gather" | "gather*" | "multline" | "multline*" => {
                                    let after = &src[i + 2 + env.len()..];
                                    let close = format!("\\end{{{env}}}");
                                    let Some(k) = after.find(&close) else { return out };
                                    let body = &after[..k];
                                    let arrays = body.match_indices("\\begin{").all(|(p, _)| body[p + 7..].split('}').next().is_some_and(|e| e == "array" || AMS_ARRAYS.contains(&e)));
                                    if !arrays || body.contains('@') {
                                        return out;
                                    }
                                    // A `multline` is one equation, whatever its rows.
                                    let rows = if env.starts_with("multline") { vec![body] } else { split_rows(body) };
                                    for row in rows {
                                        // A row's `\tag{text}` replaces its number (`\tag@in@align`).
                                        let tag = match row.find("\\tag") {
                                            Some(_) if env.starts_with("multline") => return out,
                                            Some(p) => match tag_text(&row[p + 4..]) {
                                                Some((text, _)) => Some(text),
                                                None => return out,
                                            },
                                            None => None,
                                        };
                                        let numbered = !env.ends_with('*') && !row.contains("\\nonumber") && !row.contains("\\notag") && tag.is_none();
                                        if numbered {
                                            equation += 1;
                                        }
                                        let mut rest = row;
                                        while let Some(p) = rest.find("\\label{") {
                                            let r = &rest[p + 7..];
                                            let Some(e) = r.find('}') else { return out };
                                            let value = match &tag {
                                                Some(t) => t.clone(),
                                                None if numbered => pf[2].clone() + &within_text(&sec, appendix, eq_within, equation),
                                                None => return out,
                                            };
                                            out.labels.insert(r[..e].to_string(), value);
                                            out.hrefs.insert(r[..e].to_string(), theorem_href("equation", &sec, eq_within, equation));
                                            rest = &r[e..];
                                        }
                                    }
                                    groups.pop();
                                    i += 2 + env.len() + k + close.len();
                                    continue;
                                }
                                // An entry is a group; `\label` there takes the label around.
                                "em" | "document" | "tabular" | "tabularx" | "array" | "proof" | "aligned" | "gathered" | "split" | "minipage" => {}
                                e if starred.contains(&e) => {}
                                e if AMS_ARRAYS.contains(&e) => {}
                                // `\@thm`'s `\refstepcounter`, numbered `\thesection.n` when
                                // reset by a level.
                                e if thms.iter().any(|t| t.0 == e) => {
                                    let &(_, owner, ..) = thms.iter().find(|t| t.0 == e).expect("a theorem");
                                    thm_values[owner] += 1;
                                    for (k, t) in thms.iter().enumerate() {
                                        if t.3 == Some(owner) {
                                            thm_values[k] = 0;
                                        }
                                    }
                                    let parent = thms[owner].3.map(|p| within_text(&sec, appendix, thms[p].2, thm_values[p]) + ".");
                                    g.label = intern(&mut strs, parent.unwrap_or_default() + &within_text(&sec, appendix, thms[owner].2, thm_values[owner]));
                                    set_href(&mut groups, &mut strs, theorem_href(thms[owner].0, &sec, thms[owner].2, thm_values[owner]));
                                }
                                "figure" | "figure*" | "table" | "table*" => {
                                    g.enumerate = false;
                                    floats.push(env.starts_with("table") as usize);
                                }
                                _ => return out,
                            }
                        }
                        i += 2 + env.len();
                        // enumitem's keys (`[label=\alph*]`) are read by `enit_setup`, not here.
                        if name == "begin" && enumitem && matches!(env, "enumerate" | "itemize" | "description") {
                            let after = src[i..].trim_start();
                            if let Some(keys) = after.strip_prefix('[').and_then(bracketed) {
                                i = src.len() - after.len() + keys.len() + 2;
                            }
                        }
                    }
                    "chapter" if report => {
                        if next(i) == Some(b'*') {
                            continue;
                        }
                        // `\@addtoreset`: section, equation, figure, table, footnote; the
                        // pre-scan refuses theorems numbered within a counter under report.
                        sec[0] += 1;
                        sec[1..].fill(0);
                        (equation, figtab, footnote) = (0, [0, 0], 0);
                        // `\theequation`, `\thefigure`, `\thetable`: `\thechapter.` first.
                        let number = heading_number(&sec, 0, appendix);
                        pf = [format!("{number}."), format!("{number}."), format!("{number}.")];
                        let g = groups.last_mut().expect("a group");
                        g.label = intern(&mut strs, number.clone());
                        let Some(title) = heading_title(&src[i..]) else { return out };
                        if !title.is_empty() && !toc_safe(title) {
                            return out;
                        }
                        out.toc.push(TocEntry { level: 0, number, title: title.to_string() });
                    }
                    "section" | "subsection" | "subsubsection" | "paragraph" => {
                        if next(i) == Some(b'*') {
                            links += 1;
                            set_href(&mut groups, &mut strs, format!("section*.{links}"));
                            continue;
                        }
                        let level = ["section", "subsection", "subsubsection", "paragraph"].iter().position(|s| *s == name).expect("a heading") + report as usize;
                        let mut number = String::new();
                        if level < SECNUMDEPTH {
                            sec[level] += 1;
                            sec[level + 1..].fill(0);
                            if eq_within.is_some_and(|w| w >= level) {
                                equation = 0;
                            }
                            // `\@stpelt` steps a reset counter from -1, so the counters it resets go to 0 too.
                            for (t, v) in thms.iter().zip(thm_values.iter_mut()) {
                                let within = |w: Option<usize>| w.is_some_and(|w| w >= level);
                                if within(t.2) || t.3.is_some_and(|p| within(thms[p].2)) {
                                    *v = 0;
                                }
                            }
                            number = heading_number(&sec, level, appendix);
                            let g = groups.last_mut().expect("a group");
                            g.label = intern(&mut strs, number.clone());
                            set_href(&mut groups, &mut strs, ["section", "subsection", "subsubsection"].get(level).map_or(String::new(), |k| sec[..=level].iter().fold(k.to_string(), |s, c| format!("{s}.{c}"))));
                        }
                        // `\@dblarg`: the contents take the optional short title.
                        let Some(title) = heading_title(&src[i..]) else { return out };
                        if !title.is_empty() && !toc_safe(title) {
                            return out;
                        }
                        out.toc.push(TocEntry { level, number, title: title.to_string() });
                    }
                    "item" => {
                        let g = groups.last_mut().expect("a group");
                        // `\item[label]` steps no counter: `\@currentlabel` and the anchor stay.
                        if !g.enumerate || next(i) == Some(b'[') {
                            continue;
                        }
                        let d = g.depth;
                        enums[d - 1] += 1;
                        items += 1;
                        if g.template != 0 {
                            let n = enums[d - 1] as i32;
                            let t = strs[g.template as usize].clone();
                            let (pre, rest) = t.split_once('\\').unwrap_or((&t, ""));
                            let (style, post) = rest.split_once('*').unwrap_or(("", ""));
                            let num: String = match style {
                                "arabic" => n.to_string(),
                                "alph" => alph(n, b'a').collect(),
                                "Alph" => alph(n, b'A').collect(),
                                "roman" => roman(n),
                                _ => roman(n).to_uppercase(),
                            };
                            let g = groups.last_mut().expect("a group");
                            g.label = intern(&mut strs, format!("{pre}{num}{post}"));
                            set_href(&mut groups, &mut strs, format!("Item.{items}"));
                            continue;
                        }

                        // `\p@enumN\theenumN`, as `item_start` writes it.
                        let mut text = enums[0].to_string();
                        if d >= 2 {
                            if d >= 3 {
                                text.push('(');
                            }
                            text.extend(alph(enums[1] as i32, b'a'));
                            if d >= 3 {
                                text.push(')');
                                text.push_str(&roman(enums[2] as i32));
                            }
                            if d >= 4 {
                                text.extend(alph(enums[3] as i32, b'A'));
                            }
                        }
                        g.label = intern(&mut strs, text);
                        set_href(&mut groups, &mut strs, format!("Item.{items}"));
                    }
                    // `\caption`'s `\refstepcounter\@captype`, in the float's group.
                    "caption" => {
                        let Some(t) = floats.last().copied() else { return out };
                        figtab[t] += 1;
                        let g = groups.last_mut().expect("a group");
                        g.label = intern(&mut strs, format!("{}{}", pf[t], figtab[t]));
                        set_href(&mut groups, &mut strs, format!("{}.{}", ["figure", "table"][t], figtab[t]));
                    }
                    "footnote" => {
                        if next(i) == Some(b'[') {
                            return out;
                        }
                        footnote += 1;
                        hfootnote += 1;
                        pending = Some(intern(&mut strs, footnote.to_string()));
                        pending_href = Some(intern(&mut strs, format!("Hfootnote.{hfootnote}")));
                    }
                    // `\maketitle`'s `\setcounter{footnote}{0}`.
                    "maketitle" => footnote = 0,
                    // `\setcounter{section}{0}\setcounter{subsection}{0}`, `\@Alph` sections.
                    "appendix" => {
                        sec[..2].fill(0);
                        appendix = true;
                    }
                    // A label's value does not depend on the page; other counters are not
                    // followed here.
                    "setcounter" if arg() != Some("page") => return out,
                    "setlength" | "addtolength" => {
                        let after = rest[n..].trim_start();
                        let skip = match after.strip_prefix('{') {
                            Some(r) => r.find('}').map(|k| k + 2),
                            None => after.strip_prefix('\\').map(|r| 1 + r.bytes().take_while(|c| c.is_ascii_alphabetic() || *c == b'@').count()),
                        };
                        let Some(skip) = skip else { return out };
                        i += rest.len() - n - after.len() + skip;
                    }
                    "label" => {
                        let Some(key) = arg() else { return out };
                        let top = groups.last().expect("a group");
                        if top.opaque {
                            return out;
                        }
                        out.labels.insert(key.to_string(), strs[top.label as usize].clone());
                        out.hrefs.insert(key.to_string(), strs[top.href as usize].clone());
                        display_keys.push(key.to_string());
                        i += 2 + key.len();
                    }
                    // amsmath's `\tag{text}`: `\nonumber`, and the display's label the text
                    // (spaces as the tokens have them).
                    "tag" => {
                        let Some((text, used)) = tag_text(&rest[n..]) else { return out };
                        if std::mem::take(&mut numbered) {
                            equation -= 1;
                        }
                        for key in display_keys.drain(..) {
                            out.labels.insert(key, text.clone());
                        }
                        groups.last_mut().expect("a group").label = intern(&mut strs, text);
                        i += used;
                    }
                    "ref" | "eqref" => {
                        let Some(key) = arg() else { return out };
                        i += 2 + key.len();
                    }
                    // amsmath's `\nonumber` in `equation` steps the number back.
                    "nonumber" | "notag" => {
                        if std::mem::take(&mut numbered) {
                            equation -= 1;
                        }
                    }
                    _ if scan_list(name) & IN_WORDS != 0 && !matches!(name, "newcommand" | "renewcommand" | "providecommand") => {}
                    // `\def\name`: the name is `quiet` or `loud` as any defined macro.
                    "def" => {}
                    // `\fontsize{size}{skip}\selectfont` (`font_size`) numbers nothing.
                    "fontsize" | "selectfont" | "parbox" | "fbox" | "colorbox" | "fcolorbox" | "textsuperscript" | "textsubscript" => {}
                    // Kernel values, operators and math macros number nothing.
                    _ if table_names(name) != 0 => {}
                    // A package's native command (`\includegraphics`, `\operatorname`) numbers
                    // nothing.
                    _ if PACKAGE_NATIVES.iter().any(|p| p.1 == name) || operators.contains(&name) => {}
                    // A macro of the preamble whose text numbers nothing.
                    _ if quiet.contains(&name) => {}
                    _ => return out,
                }
            }
            _ => i += 1,
        }
    }
    out.complete = true;
    out
}

/// Whether [`predict`] passes `name` by without a counter changing: a native command,
/// table name, package native or declared operator that none of its own cases handles.
fn numbers_nothing(name: &str, operators: &[&str], quiet: &[&str]) -> bool {
    const COUNTING: [&str; 16] = ["begin", "end", "section", "subsection", "subsubsection", "paragraph", "item", "caption", "footnote", "maketitle", "setcounter", "setlength", "addtolength", "label", "ref", "eqref"];
    if COUNTING.contains(&name) || matches!(name, "nonumber" | "notag" | "newcommand" | "renewcommand" | "providecommand") {
        return false;
    }
    scan_list(name) & (IN_WORDS | PACKAGE_NATIVE) != 0 || table_names(name) != 0 || operators.contains(&name) || quiet.contains(&name) || matches!(name, "fontsize" | "selectfont" | "parbox" | "fbox" | "colorbox" | "fcolorbox" | "textsuperscript" | "textsubscript")
}

/// The macros the preamble defines with `\newcommand`, `\renewcommand` or
/// `\providecommand` whose text numbers nothing ([`numbers_nothing`], an earlier such macro
/// included), so that their use in the body does not stop [`predict`]. A text with an `@`
/// name, or one [`predict`] would have to read, leaves the macro out. The labels check every
/// predicted value, so a macro wrongly let through can only send the document to the engine.
/// The second list holds the other macros the preamble defines; the third, those of them
/// whose text numbers nothing but `k` starred headings (each hpdftex's `\@ssect` steps the
/// link counter, `section*.n`), with `k`.
fn quiet_macros<'a>(preamble: &'a str, operators: &[&str]) -> (Vec<&'a str>, Vec<&'a str>, Vec<(&'a str, usize)>) {
    let (mut quiet, mut loud): (Vec<&'a str>, Vec<&'a str>) = (Vec::new(), Vec::new());
    let mut starred = Vec::new();
    let mut rest = preamble;
    // The first of the commands in `rest`: one walk from backslash to backslash, not a
    // search of the rest of the text for each command.
    const DEFS: [&str; 4] = ["\\newcommand", "\\renewcommand", "\\providecommand", "\\def"];
    let first = |rest: &str| {
        let mut at = 0;
        while let Some(k) = rest[at..].find('\\') {
            at += k;
            if !matches!(rest.as_bytes().get(at + 1), Some(b'n' | b'r' | b'p' | b'd')) {
                at += 1;
                continue;
            }
            if let Some(d) = DEFS.iter().find(|d| rest[at..].starts_with(*d)) {
                return Some((at, d.len()));
            }
            at += 1;
        }
        None
    };
    while let Some(k) = first(rest) {
        let def = k.1 == 4;
        let r = rest[k.0 + k.1..].trim_start_matches('*').trim_start();
        rest = r;
        let (name, mut r) = if def {
            // `\def\name#1#2{text}`: the parameters only `#` and digits.
            let Some(r) = r.strip_prefix('\\') else { continue };
            let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
            let (name, after) = (&r[..n], &r[n..]);
            let Some(open) = after.find('{') else { continue };
            if n == 0 || !after[..open].bytes().all(|c| c == b'#' || c.is_ascii_digit()) {
                if n > 0 {
                    loud.push(name);
                }
                continue;
            }
            (name, &after[open..])
        } else {
            match r.strip_prefix("{\\").and_then(|r| r.split_once('}')) {
                Some((n, r)) => (n, r),
                None => match r.strip_prefix('\\') {
                    Some(r) => {
                        let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
                        (&r[..n], &r[n..])
                    }
                    None => continue,
                },
            }
        };
        // `[n]` and `[default]`.
        for _ in 0..2 {
            r = r.trim_start();
            if let Some(o) = r.strip_prefix('[') {
                let Some((_, a)) = o.split_once(']') else { break };
                r = a;
            }
        }
        let Some(body) = r.trim_start().strip_prefix('{') else {
            loud.push(name);
            continue;
        };
        let mut depth = 1usize;
        let Some(end) = body.bytes().position(|c| {
            match c {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            depth == 0
        }) else {
            loud.push(name);
            continue;
        };
        let text = &body[..end];
        let names = text.split('\\').skip(1).map(|t| (t.bytes().take_while(u8::is_ascii_alphabetic).count(), t));
        let ok = names.clone().all(|(n, t)| n == 0 || (!t[n..].starts_with('@') && numbers_nothing(&t[..n], operators, &quiet)));
        let star = |n: usize, t: &str| matches!(&t[..n], "section" | "subsection" | "subsubsection" | "paragraph") && t[n..].starts_with('*');
        let stars = names.clone().filter(|&(n, t)| star(n, t)).count();
        let starred_ok = stars > 0 && names.clone().all(|(n, t)| n == 0 || star(n, t) || (!t[n..].starts_with('@') && numbers_nothing(&t[..n], operators, &quiet)));
        if ok && !name.is_empty() && !name.contains('@') {
            quiet.push(name);
        } else if !name.is_empty() {
            loud.push(name);
            if starred_ok && !name.contains('@') {
                starred.push((name, stars));
            }
        }
    }
    (quiet, loud, starred)
}

/// Each `\bibitem`'s value, for a `\cite` ahead of it: in each `thebibliography`
/// (`\usecounter{enumiv}` from zero) a `\bibitem{key}` is the next number, a
/// `\bibitem[label]{key}` its label. A label with commands or braces is left out; `bibitem`
/// checks every value it uses.
fn predict_bib(body: &str, labels: &mut std::collections::HashMap<String, String>) {
    let body: String = body.lines().map(|l| {
        let mut e = 0;
        let b = l.as_bytes();
        while e < b.len() && !(b[e] == b'%' && (e == 0 || b[e - 1] != b'\\')) {
            e += 1;
        }
        &l[..e]
    }).collect::<Vec<_>>().join("\n");
    for env in body.split("\\begin{thebibliography}").skip(1) {
        let env = env.split("\\end{thebibliography}").next().unwrap_or("");
        let mut n = 0;
        for item in env.split("\\bibitem").skip(1) {
            if item.starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            let item = item.trim_start();
            let (label, rest) = match item.strip_prefix('[') {
                Some(r) => match r.split_once(']') {
                    Some((l, rest)) => (Some(l), rest.trim_start()),
                    None => continue,
                },
                None => (None, item),
            };
            let Some((key, _)) = rest.strip_prefix('{').and_then(|r| r.split_once('}')) else { continue };
            let value = match label {
                Some(l) if l.contains(['\\', '{', '}']) => continue,
                Some(l) => l.to_string(),
                None => {
                    n += 1;
                    n.to_string()
                }
            };
            labels.insert(format!("{BIB}{key}"), value);
        }
    }
}

/// hyperref's anchor of a theorem numbered by counter `env`: `env.\theH<env>`, which is
/// `\theH<level>.\arabic{env}` when a level resets it, the counters in arabic.
fn theorem_href(env: &str, sec: &[u32], within: Option<usize>, n: u32) -> String {
    let mut s = env.to_string();
    for c in within.map_or(&[][..], |w| &sec[..=w]) {
        s.push_str(&format!(".{c}"));
    }
    format!("{s}.{n}")
}

/// What [`predict`] reads off a body: the labels' values, the contents entries, and whether
/// it read the whole body (else the entries are not all there).
#[derive(Default)]
struct Predicted {
    labels: std::collections::HashMap<String, String>,
    /// hyperref's anchor each label records (`\@currentHref`).
    hrefs: std::collections::HashMap<String, String>,
    toc: Vec<TocEntry>,
    complete: bool,
}

/// An unstarred heading's `\addcontentsline{toc}`: its level (`section` is 0), number and
/// title (the short one when given), as source text.
#[derive(Clone)]
struct TocEntry {
    level: usize,
    number: String,
    title: String,
}

/// A heading's title after its name: `[short]` when given, else `{title}`; `None` when it
/// is neither. Nested braces are kept.
fn heading_title(rest: &str) -> Option<&str> {
    let rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);
    let (open, close) = if rest.starts_with('[') { (b'[', b']') } else { (b'{', b'}') };
    let b = rest.as_bytes();
    if b.first() != Some(&open) {
        return None;
    }
    let mut depth = 0;
    for (k, &c) in b.iter().enumerate().skip(1) {
        match c {
            b'{' => depth += 1,
            b'}' if depth > 0 => depth -= 1,
            c if c == close && depth == 0 => return Some(&rest[1..k]),
            b'\\' => {}
            _ => {}
        }
    }
    None
}

/// A title the contents can take from the source as the `.toc` file would give it back:
/// text, `~` and the robust commands below (a `\write` leaves them as they are); not
/// `\label` (gobbled there), `\footnote`, `\\` or a macro (expanded there).
fn toc_safe(title: &str) -> bool {
    const SAFE: [&str; 22] = ["emph", "textbf", "textmd", "textit", "textsl", "textsc", "textup", "textrm", "textsf", "texttt", "LaTeX", "TeX", "ldots", "dots", "textellipsis", "textunderscore", "thinspace", "nobreakspace", "H", "u", "v", "r"];
    let b = title.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
                if n == 0 {
                    if !b.get(i + 1).is_some_and(|c| b" &%$#_{},\"'.=^`~".contains(c)) {
                        return false;
                    }
                    i += 2;
                    continue;
                }
                if !SAFE.contains(&&title[i + 1..i + 1 + n]) {
                    return false;
                }
                i += 1 + n;
            }
            b'%' | b'#' | b'^' | b'&' | b'@' => return false,
            _ => i += 1,
        }
    }
    true
}

/// A native job's result.
pub struct Output {
    pub pdf: Vec<u8>,
    pub pages: usize,
    pub errors: Vec<String>,
}

/// Runs `src` natively when [`accepts`] does; `now` is the job's date (Unix seconds). `read`
/// finds what pdfTeX's backend reads at `\\shipout` (`pdftex.map`, encodings, Type 1
/// files); without it, fonts are written as the binary carries them.
/// `Err` (why) when the document is not one the native job accepts, by the pre-scan or, at a
/// construct it does not implement, while running: the document then runs whole on the
/// engine (§2a).
pub fn run(src: &str, now: i64, read: Option<&Read<'_>>) -> Result<Output, String> {
    let timed = std::env::var_os("RT_TIME").is_some();
    let lap = |what: &str| {
        if timed {
            eprintln!("             {:9.3}M instructions so far  {what}", instructions_retired() as f64 / 1e6);
        }
    };
    lap("the job starts");
    let body = scan(src).map_err(|what| format!("the pre-scan: {what}"))?;
    lap("pre-scan: scan");
    let src_class = src.find("\\documentclass").map(|i| &src[i + "\\documentclass".len()..]).ok_or("no \\documentclass")?;
    let (size, key, report) = class_options(src_class).map_err(|what| format!("the class options: {what}"))?;
    let class = data_class::class_size(report, size).ok_or("no class table in this build")?;
    let text = format!("{body}\\enddocument ");
    // Files come from the binary first (`carried`); `read` only for others. A font outside
    // the TFM table may have a map line `rushtex.map` lacks: the job runs again on `read`.
    let outside = std::cell::Cell::new(false);
    let none = |_: &str| None;
    let disk: &Read<'_> = read.unwrap_or(&none);
    let first = |name: &str| match carried(name) {
        Carried::File(b) => Some(b),
        Carried::Absent => None,
        Carried::Unknown => {
            if name.ends_with(".tfm") {
                outside.set(true);
            }
            disk(name)
        }
    };
    // hyperref's bookmarks need the contents entries before the body.
    let mut predicted = if body.contains("\\ref") || body.contains("\\eqref") || body.contains("\\tableofcontents") || text.contains("hyperref") { predict(body) } else { Predicted::default() };
    if body.contains("\\cite") {
        predict_bib(body, &mut predicted.labels);
    }
    lap("pre-scan: predict");
    let out = job(&text, class, key, now, &first, &predicted)?;
    if !outside.get() || read.is_none() {
        return Ok(out);
    }
    job(&text, class, key, now, disk, &predicted)
}

/// What the binary knows of a file the native job asks for.
enum Carried {
    File(Vec<u8>),
    /// The pin has no such file (a `.vf` for a font of the TFM table).
    Absent,
    Unknown,
}

/// A file from the binary (`data_embed`), else from an installed font pack (`data_pack`):
/// `pdftex.map` is `rushtex.map`, the map lines of the TFM table's fonts, with those of the
/// packs loaded; a table font's `.vf` is embedded when the pin has one.
fn carried(name: &str) -> Carried {
    use crate::data::data_pack;
    let Some(emb) = crate::data::data_embed::embedded() else { return data_pack::read(name).map_or(Carried::Unknown, Carried::File) };
    let file = if name == "pdftex.map" { "rushtex.map" } else { name };
    if let Some(mut b) = emb.read(file) {
        // The map lines of the packs this job loaded fonts from (`<pack>.map`).
        if name == "pdftex.map" {
            b.extend_from_slice(&data_pack::loaded_maps());
        }
        return Carried::File(b);
    }
    // A font pack in the cache (native-architecture.md §4b).
    if let Some(b) = data_pack::read(name) {
        return Carried::File(b);
    }
    // An expanded copy (`ecrm1000+20`, pdftex.web `expand_font_name`) is asked for by
    // `do_vf` too; no TeX tree has a `.vf` for one, so it is its base's answer.
    let table = |stem: &str| crate::data::data_tfm::fonts().is_some_and(|t| t.lookup(stem).is_some());
    let base = |stem: &str| match stem.rfind(['+', '-']) {
        Some(k) if k > 0 && stem.len() > k + 1 && stem[k + 1..].bytes().all(|b| b.is_ascii_digit()) => table(&stem[..k]),
        _ => false,
    };
    match name.strip_suffix(".vf") {
        Some(stem) if (table(stem) || base(stem)) && emb.contains("rushtex.map") => Carried::Absent,
        _ => Carried::Unknown,
    }
}

/// A counter `RT_TIME` prints beside each phase's time, when a measuring program sets one
/// (`examples/phase_count.rs` sets the process's retired instructions).
pub static PHASE_COUNTER: std::sync::OnceLock<fn() -> u64> = std::sync::OnceLock::new();

fn instructions_retired() -> u64 {
    PHASE_COUNTER.get().map_or(0, |f| f())
}

fn job(text: &str, class: &'static ClassTable<'static>, key: u32, now: i64, read: &Read<'_>, predicted: &Predicted) -> Result<Output, String> {
    // `RT_TIME=1`: the job's phases, in milliseconds, on standard error.
    let clock = std::env::var_os("RT_TIME").map(|_| std::time::Instant::now());
    let lap = |what: &str| {
        if let Some(c) = clock {
            eprintln!("{:8.3} ms  {:9.3}M instructions so far  {what}", c.elapsed().as_secs_f64() * 1e3, instructions_retired() as f64 / 1e6);
        }
    };
    lap("pre-scan (before the job)");
    let mut e = super::Engine::default();
    // `\pdfgentounicode`'s glyph names, which the format would carry.
    e.glyph_unicode.carry();
    let mut m = Machine::new(&mut e, text);
    m.read_file = Some(Box::new(read));
    m.typeset = true;
    lap("engine");
    m.load_class_with(class, key)?;
    m.fix_date_and_time(now);
    if let Some(doc) = m.doc.as_mut() {
        doc.options = key;
        doc.predicted.clone_from(&predicted.labels);
        doc.the_prefix = the_prefixes(text);
        doc.predicted_hrefs.clone_from(&predicted.hrefs);
        doc.contents = predicted.complete.then(|| predicted.toc.clone());
    }
    lap("class");
    m.run();
    lap("typeset");
    if let Some(doc) = m.doc.as_mut() {
        if let Some(what) = doc.unsupported.take() {
            return Err(format!("not native yet: {what}"));
        }
        // A forward `\ref` whose `\label` never ran: pdfLaTeX prints `??` there.
        if doc.forward.keys().any(|k| !doc.labels.contains_key(k)) {
            return Err("not native yet: a forward \\ref whose \\label never ran".into());
        }
    }
    // A TeX error in the native job is either the document's own, which the engine reports
    // as pdfLaTeX does, or a native that built something LaTeX does not: never a page.
    if let Some(first) = m.errors.first() {
        return Err(format!("not native yet: an error in the native job: {first}"));
    }
    m.finish_contents()?;
    lap("finish_contents");
    let pages = m.page.pages;
    let pdf = m.pdf();
    lap("pdf");
    Ok(Output { pdf, pages, errors: std::mem::take(&mut m.errors) })
}

/// The name of native command `i`, for `\\show` and `\\meaning`.
pub(super) fn doc_name(i: u16) -> &'static str {
    match (i as usize).checked_sub(DOC.len()) {
        Some(k) => kernel_alphabets().nth(k).unwrap_or("relax"),
        None => DOC[i as usize].0,
    }
}

/// The kernel's math alphabets (`\mathbf`, `\mathcal`), in `Meaning::Doc` order past `DOC`.
fn kernel_alphabets() -> impl Iterator<Item = &'static str> {
    use crate::data::data_kernel::MathFont;
    crate::data::data_kernel::kernel().into_iter().flat_map(|k| k.math_fonts()).filter_map(|f| match f {
        MathFont::SymbolAlphabet { name, .. } | MathFont::Alphabet { name, .. } => Some(name),
        MathFont::Symbol { .. } => None,
    })
}

impl Machine<'_, '_> {
    /// The class's state from table `t`: parameters, codes, fonts, and `\output` as the
    /// native routine. Public for `rushtex-gen class`, which checks it against the oracle.
    pub fn load_class(&mut self, t: &'static ClassTable<'static>) {
        // The base combination is in every table.
        let _ = self.load_class_with(t, 0);
    }

    /// The class's state for option combination `key` (`data_class::option_key`): the
    /// table, then the combination's differences from it.
    pub fn load_class_with(&mut self, t: &'static ClassTable<'static>, key: u32) -> Result<(), String> {
        let diffs: Vec<(u32, u32, u32)> = t.option_diffs(key).ok_or("no such option combination in the table")?.collect();
        let changed = |sec: u32, i: usize, v: u32| diffs.iter().rev().find(|d| d.0 == sec && d.1 as usize == i).map_or(v, |d| d.2);
        let eqtb = &mut self.e.eqtb;
        for (sec, region, vals) in [(0, Region::Int, &t.ints), (1, Region::Dimen, &t.dimens)] {
            for slot in 0..vals.len() {
                eqtb.set_word(Reg { region, slot: slot as u32 }, changed(sec, slot, vals.get(slot)) as i32, true);
            }
        }
        for slot in 0..t.glues.len() / 5 {
            let g = |k: usize| changed(2, 5 * slot + k, t.glues.get(5 * slot + k)) as i32;
            eqtb.set_glue(Reg { region: Region::Glue, slot: slot as u32 }, Glue { width: g(0), stretch: g(1), shrink: g(2), stretch_order: g(3) as u8, shrink_order: g(4) as u8 }, true);
        }
        for (k, kind) in [Code::Lc, Code::Uc, Code::Sf, Code::Math, Code::Del].into_iter().enumerate() {
            for c in 0..256 {
                eqtb.set_code(kind, c, t.codes.get(k * 256 + c as usize) as i32, true);
            }
        }
        for (c, &cat) in t.cats.iter().enumerate() {
            eqtb.set_catcode(c as u8, Cat::from_u8(cat).unwrap_or(Cat::Other), true);
        }
        let mut normal_font = 0;
        for f in t.fonts() {
            let id = self.find_font(f.tfm, f.size, None);
            let font = &mut self.e.fonts.list[id as usize];
            font.hyphen_char = f.hyphen_char;
            font.skew_char = f.skew_char;
            let cs = self.e.cs.intern(f.ident);
            self.e.eqtb.set_meaning(cs, Meaning::Font(id), true);
            self.e.fonts.list[id as usize].ident = cs;
            if f.current {
                self.select_font(id, true);
                normal_font = id;
            }
        }
        // The paragraph hooks and the math-font setup of LaTeX's token parameters are not
        // needed by plain text; the pre-scan refuses what would need them.
        for name in ["output", "everypar", "everymath", "everydisplay"] {
            if let Some(r) = super::engine_names::param(name) {
                self.e.eqtb.set_toks(r, Default::default(), true);
            }
        }
        let mut ids = [0u32; DOC.len()];
        for (i, (name, _)) in DOC.iter().enumerate() {
            ids[i] = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(ids[i], Meaning::Doc(i as u16), true);
        }
        let named = NAMED.map(|name| self.e.cs.intern(name));
        // utf8.def's active lead bytes (`\UTFviii@two@octets` and the rest).
        for c in 0xC2u8..=0xF4 {
            let id = self.e.cs.intern(super::engine_run::active_key(char::from(c), &mut [0; 5]));
            self.e.eqtb.set_meaning(id, Meaning::Doc(UTF8_LEAD as u16), true);
        }
        // latex.ltx: `\def\^^M{\ }`, `\let\^^I\^^M`: control space.
        if let Some(space) = self.e.cs.get(" ").map(|id| self.e.eqtb.meaning(id)) {
            for name in ["\r", "\t"] {
                let cs = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(cs, space, true);
            }
        }
        let output = self.e.toks.store(&[Token::char(Cat::BeginGroup, '{'), Token::cs(ids[OUTPUT]), Token::char(Cat::EndGroup, '}')]);
        {
            let r = param!("output");
            self.e.eqtb.set_toks(r, output, true);
        }
        // latex.ltx's frozen `\everymath` and `\everydisplay` (`\check@mathfonts`); ltpara's
        // `\everypar`, the paragraph hook.
        for (name, i) in [("everymath", EVERY_MATH), ("everydisplay", EVERY_DISPLAY), ("everypar", PARA_HOOK)] {
            let span = self.e.toks.store(&[Token::cs(ids[i])]);
            if let Some(r) = super::engine_names::param(name) {
                self.e.eqtb.set_toks(r, span, true);
            }
        }
        // The format's hyphenation of `\language` 0 (`data_hyph`).
        if let Some(h) = crate::data::data_hyph::english() {
            self.e.hyph.set_language(0, h.patterns, h.exceptions());
            // The trie built at generation time.
            if let Some(t) = &h.trie {
                self.e.hyph.set_trie(0, t.each_ref().map(|s| s.iter().collect()));
            }
        }
        // LaTeX's registers, at their register numbers (`data_class::CLASS_REGS`).
        let mut regs = [Reg { region: Region::Int, slot: 0 }; data_class::CLASS_REGS.len()];
        for (i, r) in regs.iter_mut().enumerate() {
            let (region, slot, v) = t.reg(i);
            let v: [i32; 5] = std::array::from_fn(|k| changed(4, 7 * i + 2 + k, v[k] as u32) as i32);
            *r = Reg { region: [Region::Int, Region::Dimen, Region::Glue][region.min(2) as usize], slot };
            match r.region {
                Region::Glue => self.e.eqtb.set_glue(*r, Glue { width: v[0], stretch: v[1], shrink: v[2], stretch_order: v[3] as u8, shrink_order: v[4] as u8 }, true),
                _ => self.e.eqtb.set_word(*r, v[0], true),
            }
        }
        for (name, code) in TEXT_SYMBOLS {
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::CharDef(code), true);
        }
        // The kernel's values, operators, delimiters, macros and math version.
        let (mut fams, mut alphabets) = (Vec::new(), Vec::new());
        if let Some(k) = crate::data::data_kernel::kernel() {
            self.install(k, None, &mut fams, &mut alphabets, Relocate::default());
        }
        let par_id = named[named!("par")];
        let (cr_id, crcr_id, stretch_id) = (named[named!("cr")], named[named!("crcr")], named[named!("arraystretch")]);
        let (omit_id, span_id, newline_id) = (named[named!("omit")], named[named!("span")], named[named!("\\")]);
        let par_prim = self.e.eqtb.meaning(par_id);
        let mut vals = t.class_values();
        for (i, v) in vals.iter_mut().enumerate() {
            *v = changed(3, i, *v as u32) as i32;
        }
        // The page geometry as the registers LaTeX keeps it in (`\newdimen\textwidth`), so the
        // preamble's `\setlength` and `\the\textwidth` act on them; `\begin{document}` reads
        // them back (`document`). And the named registers of `CLASS_REGS`.
        for (i, name) in data_class::CLASS_DIMENS.iter().enumerate() {
            let r = Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 };
            self.e.eqtb.set_word(r, vals[i], true);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Reg(r), true);
        }
        self.e.eqtb.set_word(COLUMN_WIDTH, vals[0], true);
        for (name, r, v) in [("fboxsep", FBOXSEP, 3 * 65536), ("fboxrule", FBOXRULE, DEFAULT_RULE)] {
            self.e.eqtb.set_word(r, v, true);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Reg(r), true);
        }
        let id = self.e.cs.intern("columnwidth");
        self.e.eqtb.set_meaning(id, Meaning::Reg(COLUMN_WIDTH), true);
        for (name, r) in data_class::CLASS_REGS.iter().zip(regs) {
            if !name.contains(' ') {
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Reg(r), true);
            }
        }
        self.doc = Some(Box::new(Doc { class: Class { table: t, vals }, page: 1, normal_font, column: None, ids, named, big_pending: 0, nobreak: false, afterindent: true, inlabel: false, newlist: false, noitemarg: false, endpe: false, lists: Vec::new(), tabs: Vec::new(), tab_group: 0, tx: None, tab_to: None, tx_delta: 0, floats: Default::default(), cr_base: 0, font_switch: Vec::new(), families: DEFAULT_FAMILIES, font_fixed: Vec::new(), cr_id, crcr_id, stretch_id, newline_id, omit_id, span_id, par_prim, par_id, counters: [0; 3], title: Vec::new(), number_box: None, after: Glue::default(), numbered: true, toc: true, noskipsec: false, stored: Default::default(), math_size: "", fams, alphabets, packages: Vec::new(), booktabs: None, ams_display: 0, df_label: None, df_tag: None, big_size: 0, align: None, eqnarray: None, wrong_shapes: Vec::new(), wide_fam: 0, wide: Vec::new(), equation: 0, eq_within: None, appendix: false, the_prefix: Default::default(), sphack: Vec::new(), footnote: 0, footnote_text: Vec::new(), thanks: None, fn_symbol: false, footnote_sf: Vec::new(), footnote_flags: Vec::new(), steps: Vec::new(), level: 0, after_before: Glue::default(), indent: None, current: 0, item_label: None, theorems: Vec::new(), thm_values: Vec::new(), thm_item: Vec::new(), thm_style: PLAIN, thm_current: 0, trivlist_then: |_| {}, dated: false, authored: false, unsupported: None, options: 0, verbatim: Vec::new(), labels: Default::default(), predicted: Default::default(), forward: Default::default(), used: Default::default(), label_keys: Vec::new(), label_pages: Default::default(), contents: None, toc_next: 0, toc_seen: 0, toc_pages: Vec::new(), toc_boxes: Vec::new(), defer: false, deferred: Vec::new(), fam_fonts: std::collections::HashMap::new(), eqpen: 0, aligned: None, replay: false, toc_afterindent: Vec::new(), body: false, empty_style: false, special: None, empty_foot: false, geometry: None, colors: Vec::new(), color_names: Vec::new(), url_penalties: [500, 700], shipped: false, label_hrefs: Default::default(), predicted_hrefs: Default::default(), link: None, hy: Default::default(), uri: None, anchor: None, hfootnote: 0, items: 0, enum_templates: Vec::new(), enit_set: Vec::new(), pbox: Vec::new(), colorboxes: Vec::new(), fancy: None, report: false, display_pen: 10000, t1: false, thm_anchor: None, link_counter: 0, star_anchor: None, mt_fonts: Vec::new(), regs }));
        self.strut_update();
        let fixed = FONT_FIXED.iter().map(|name| {
            let id = self.e.cs.intern(name);
            self.macro_body(id)
        }).collect();
        if let Some(d) = self.doc.as_mut() {
            d.font_fixed = fixed;
        }
        Ok(())
    }

    /// A macro holding a decimal factor (`\renewcommand\arraystretch{1.5}`,
    /// `\topfraction`), as a scale of dimensions: `int*v` plus §455's `xn_over_d(v, frac,
    /// 2^16)`, the fraction from §452 `round_decimals`. `default` (sign, integer part,
    /// fraction) when the name is not a macro; `None` when its text is not a number.
    fn macro_factor(&self, id: u32, default: (bool, i64, i64)) -> Option<impl Fn(i32) -> i32> {
        let (mut neg, mut int, mut frac) = default;
        if let Meaning::Macro(m) = self.e.eqtb.meaning(id) {
            (neg, frac) = (false, 0);
            let text = self.e.toks.get(m.text);
            let body = text.iter().position(|t| *t == Token::END_MATCH).filter(|&k| k == 0).map(|k| &text[k + 1..])?;
            let bytes: Vec<u8> = body.iter().map(|t| if t.cs_id().is_some() { None } else { u8::try_from(t.chr() as u32).ok() }).collect::<Option<_>>()?;
            let s = std::str::from_utf8(&bytes).ok()?.trim_matches(' ');
            let mut s = s.trim_start_matches(|c: char| {
                if c == '-' {
                    neg = !neg;
                }
                c == '-' || c == '+' || c == ' '
            });
            let digits = s.bytes().take_while(u8::is_ascii_digit).count();
            int = if digits == 0 { 0 } else { s[..digits].parse().ok()? };
            s = &s[digits..];
            if let Some(d) = s.strip_prefix(['.', ',']) {
                if !d.bytes().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                let mut a = 0i64;
                for c in d.bytes().take(17).rev() {
                    a = (a + (c - b'0') as i64 * 131072) / 10;
                }
                frac = (a + 1) / 2;
            } else if !s.is_empty() || digits == 0 {
                return None;
            }
        }
        Some(move |v: i32| {
            let r = int * v as i64 + v as i64 * frac / 65536;
            (if neg { -r } else { r }) as i32
        })
    }

    /// The class's [`data_class::CLASS_DIMENS`] and flags as loaded (for the generator's check).
    pub fn class_values(&self) -> Option<Vec<i32>> {
        self.doc.as_ref().map(|d| d.class.vals.to_vec())
    }

    pub(super) fn doc_command(&mut self, i: u16) {
        if let Some(k) = (i as usize).checked_sub(DOC.len()) {
            return self.math_alphabet(k);
        }
        if let Some((_, f)) = DOC.get(i as usize) {
            if let Some(doc) = self.doc.as_mut() {
                doc.current = i as usize;
            }
            f(self);
        }
    }

    /// LaTeX's `\@makecol` with no floats and no footnotes: `\box255` becomes the column,
    /// a `\vbox` to `\textheight` (`\@colht`).
    ///
    /// - `\@outputbox@removebskip`: the box is repacked (`\boxmaxdepth\@maxdepth`), its last
    ///   glue taken off when its stretch is infinite;
    /// - the `space-footnotes-floats` socket's `\@outputbox@reinsertbskip`: repacked again with
    ///   `\@backup@outputbox@depth`, then that glue back;
    /// - `\@make@normalcolbox`: `\unvbox`, `\vskip-\dp`, `\@textbottom`.
    ///
    /// Then, as `\@outputpage`'s first step, the foot's page number is typeset from tokens
    /// (the main loop's ligatures and kerns), and `\rt@outputfoot` finishes the page.
    fn output(&mut self) {
        // `\let\par\@@par`.
        if let Some((id, m)) = self.doc.as_ref().map(|d| (d.par_id, d.par_prim)) {
            self.e.eqtb.set_meaning(id, m, false);
        }
        let p = self.e.eqtb.word(param!("outputpenalty"));
        if p < -10000 {
            return self.special_output(p);
        }
        self.output_column();
    }

    fn output_column(&mut self) {
        self.free_mid();
        let Some(doc) = self.doc.as_ref() else { return };
        let class = doc.class;
        let bottom = doc.floats.has_bottom();
        let Some(b) = self.e.eqtb.box_reg(255) else { return };
        let max_depth = class.dimen("@maxdepth");
        let start = self.cur.len();
        let r = self.e.nodes.boxes[b as usize];
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.eqtb.replace_box(255, None);
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
        let mut removed = None;
        if let Some(n) = self.cur.last().copied().filter(|n| self.cur.len() > start && n.kind == Kind::Glue) {
            let g = self.e.nodes.glue(&n);
            if g.stretch_order > 0 {
                self.cur.pop();
                removed = Some(g);
                self.e.nodes.flush_node(n);
            }
        }
        let a = self.vpack(start, 0, false, max_depth);
        let mut depth = a.depth;
        self.cur.extend_from_slice(self.e.nodes.get(a.list));
        self.e.nodes.free_block(a.list);
        // `footnotes-floats-legacy`: with footnotes or bottom floats, `\@outputbox@append{}`
        // first, and the removed glue goes back without `\@backup@outputbox@depth`.
        let footins = self.e.eqtb.box_reg(FOOTINS);
        if footins.is_some() || bottom {
            let a = self.vpack(start, 0, false, max_depth);
            depth = a.depth;
            self.cur.extend_from_slice(self.e.nodes.get(a.list));
            self.e.nodes.free_block(a.list);
        }
        if let Some(g) = removed {
            if depth > 0 && footins.is_none() && !bottom {
                let w = depth.min(self.dimen_param(super::engine_page::MAX_DEPTH));
                let n = self.e.nodes.glue_node(Glue { width: -w, ..Glue::default() }, 0);
                self.cur.push(n);
            }
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
            let c = self.vpack(start, 0, false, max_depth);
            depth = c.depth;
            self.cur.extend_from_slice(self.e.nodes.get(c.list));
            self.e.nodes.free_block(c.list);
        }
        // `\@outputbox@appendfootnotes`: `\vskip\skip\footins`, article's `\footnoterule`
        // (`\kern-3\p@ \hrule\@width.4\columnwidth \kern2.6\p@`), `\unvbox\footins`.
        if let Some(f) = footins {
            let skip = self.e.eqtb.glue(Reg::register(Region::Glue, FOOTINS));
            let n = self.e.nodes.glue_node(skip, 0);
            self.cur.push(n);
            // `\color@begingroup \normalcolor \footnoterule \unvbox\footins \color@endgroup`.
            let colored = self.colored();
            if colored {
                self.color_stack(Some(DEFAULT_COLOR));
            }
            let k = self.dimen_of("-3pt");
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
            let w = scan_dimen_ref(".4\\textwidth", 0, 0, &|_| Some(Glue { width: class.dimen("textwidth"), ..Glue::default() })).unwrap_or(0);
            let rule = self.e.nodes.rule_node([w, 26214, 0]);
            self.cur.push(rule);
            let k = self.dimen_of("2.6pt");
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
            let r = self.e.nodes.boxes[f as usize];
            self.cur.extend_from_slice(self.e.nodes.get(r.list));
            self.e.eqtb.replace_box(FOOTINS, None);
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(f);
            if colored {
                self.color_stack(None);
            }
            let c = self.vpack(start, 0, false, max_depth);
            depth = c.depth;
            self.cur.extend_from_slice(self.e.nodes.get(c.list));
            self.e.nodes.free_block(c.list);
        }
        let depth = self.attach_floats(start, depth);
        let n = self.e.nodes.glue_node(Glue { width: -depth, ..Glue::default() }, 0);
        self.cur.push(n);
        if class.flag("raggedbottom") {
            // `\vskip \z@ \@plus.0001fil`: .0001 is 7sp (§452 `round_decimals`).
            let n = self.e.nodes.glue_node(Glue { stretch: 7, stretch_order: 1, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        let d = self.dimen_param(super::engine_pack::BOX_MAX_DEPTH);
        let column = self.vpack(start, class.dimen("textheight"), true, d);
        let column = self.e.nodes.add_box(BoxRec { vertical: true, ..column });
        if let Some(doc) = self.doc.as_mut() {
            doc.column = Some(column);
        }
        self.start_foot();
    }

    /// `\@outputpage` of the column in `doc.column`, up to the foot's page number, which is
    /// typeset from tokens; `\rt@outputfoot` finishes the page.
    fn start_foot(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let (page, font, special) = (doc.page, doc.normal_font, doc.special);
        // `\@outputpage`: `\if@specialpage \global\@specialpagefalse \@nameuse{ps@\@specialstyle}
        // \fi`; `\ps@empty`'s `\@oddfoot` is `\@empty`.
        let empty = self.doc.as_mut().is_some_and(|d| {
            d.empty_foot = d.special.take().unwrap_or(d.empty_style);
            d.empty_foot
        });
        let this = self.doc.as_mut().and_then(|d| d.fancy.as_mut()).is_some_and(|f| std::mem::take(&mut f.this));
        let fancy = (this || !empty && special.is_none()) && self.fancy_page(this);
        self.push_nest(Mode::Horizontal, true, 1000);
        if fancy {
            let mut t = self.fancy_tokens(page);
            t.push(self.doc_id(OUTPUT_FOOT));
            return self.back_list(&t);
        }
        if empty {
            let t = self.doc_id(OUTPUT_FOOT);
            return self.input.back(t);
        }
        // `\@thefoot` of `\ps@plain`: `\reset@font\hfil\thepage\hfil`, in the output group.
        self.select_font(font, false);
        let n = self.e.nodes.glue_node(Glue { stretch: 65536, stretch_order: 1, ..Glue::default() }, 0);
        self.cur.push(n);
        let mut list: Vec<Token> = page.to_string().chars().map(|c| Token::char(Cat::Other, c)).collect();
        list.push(self.doc_id(OUTPUT_FOOT));
        self.back_list(&list);
    }

    /// LaTeX's `\@outputpage` for `\ps@plain`, one column, one side: the page box
    ///
    /// `\vbox{\vskip\topmargin \moveright\oddsidemargin\vbox{head \vskip\headsep column foot}}`
    ///
    /// with the head a `\vbox to\headheight` of depth 0, the column after `\lineskip` glue
    /// (`\baselineskip`, `\lineskip` and `\lineskiplimit` are 0) and the foot at
    /// `\baselineskip\footskip`. Then `\stepcounter{page}`.
    /// LaTeX's shipout background (`\__shipout_add_background_box:n`), there once a package
    /// fills `\@kernel@before@shipout@background` (pdftex.def's page colour, empty here): the
    /// page `b` repacked to its height plus depth after a box of size zero holding
    /// `\kern-1in\vbox to\z@{\kern-1in\hbox to\z@{\hss}\vss}`, with `\kern\z@` at its end,
    /// then given its height and depth back. Its glue is set as before. Without `picture`,
    /// the kernel's first-page specials (`\__shipout_add_firstpage_specials:`, there once
    /// `shipout/firstpage` has code): the same with an empty box of size zero.
    fn page_background(&mut self, b: BoxRec, d: i32, picture: bool) -> BoxRec {
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let one_inch = 4736286;
        let start = self.cur.len();
        if !picture {
            let tmp = self.hpack(start, 0, false);
            let tmp = self.e.nodes.add_box(tmp);
            self.cur.push(Node::new(Kind::Box, tmp as i32));
            return self.background_rest(start, b, d);
        }
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let h = self.hpack(start, 0, true);
        let h = self.e.nodes.add_box(h);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -one_inch) });
        self.cur.push(Node::new(Kind::Box, h as i32));
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let v = self.vpack(start, 0, true, d);
        let v = self.e.nodes.add_box(BoxRec { vertical: true, ..v });
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -one_inch) });
        self.cur.push(Node::new(Kind::Box, v as i32));
        let mut tmp = self.hpack(start, 0, false);
        (tmp.width, tmp.height, tmp.depth) = (0, 0, 0);
        let tmp = self.e.nodes.add_box(tmp);
        self.cur.push(Node::new(Kind::Box, tmp as i32));
        self.background_rest(start, b, d)
    }

    /// The end of `\__shipout_add_background_box:n`: the page's list after the box, `\kern\z@`,
    /// packed to the page's height plus depth, then given them back.
    fn background_rest(&mut self, start: usize, b: BoxRec, d: i32) -> BoxRec {
        self.cur.extend_from_slice(self.e.nodes.get(b.list));
        self.e.nodes.free_block(b.list);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
        let mut r = self.vpack(start, 0, false, d);
        (r.height, r.depth) = (b.height, b.depth);
        r
    }

    fn output_page(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(column) = doc.column.take() else { return };
        doc.page += 1;
        let class = doc.class;
        let empty = doc.empty_foot;
        let fancy = doc.fancy.as_mut().is_some_and(|f| std::mem::take(&mut f.page));
        let width = class.dimen("textwidth");
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        // The foot: `\hb@xt@\textwidth{\hfil 1\hfil}` in `\color@hbox` (empty for `\ps@empty`);
        // fancyhdr's foot ends in its own `\hss`.
        if !empty && !fancy {
            let n = self.e.nodes.glue_node(fil, 0);
            self.cur.push(n);
        }
        let start = self.start as usize;
        let inner = self.hpack(start, width, true);
        self.pop_nest();
        let foot = self.wrap_hbox(inner);
        // The head: `\vbox to\headheight{\vfil \color@hbox{\hb@xt@\textwidth{}}}`.
        let start = self.cur.len();
        let line = match self.e.eqtb.box_reg(fancy::HEAD_BOX).filter(|_| fancy) {
            Some(b) => {
                self.e.eqtb.replace_box(fancy::HEAD_BOX, None);
                let r = self.e.nodes.boxes[b as usize];
                self.e.nodes.free_box(b);
                r
            }
            None => self.hpack(start, width, true),
        };
        let head_line = self.wrap_hbox(line);
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        self.cur.push(Node::new(Kind::Box, head_line as i32));
        let d = self.dimen_param(super::engine_pack::BOX_MAX_DEPTH);
        let head = self.vpack(start, class.dimen("headheight"), true, d);
        let head = self.e.nodes.add_box(BoxRec { vertical: true, depth: 0, ..head });
        // The inner `\vbox`, with interline glue as `append_to_vlist` makes it.
        let saved = self.aux;
        let start = self.cur.len();
        self.aux = super::engine_node::IGNORE_DEPTH;
        let zero = Glue::default();
        let params = [super::engine_pack::BASELINE_SKIP, super::engine_pack::LINE_SKIP];
        let old: Vec<Glue> = params.iter().map(|&p| self.glue_param(p)).collect();
        let old_limit = self.dimen_param(super::engine_pack::LINE_SKIP_LIMIT);
        for p in params {
            self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: p }, zero, false);
        }
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: super::engine_pack::LINE_SKIP_LIMIT }, 0, false);
        self.append_to_vlist(head);
        let n = self.e.nodes.glue_node(Glue { width: class.dimen("headsep"), ..zero }, 0);
        self.cur.push(n);
        self.append_to_vlist(column);
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: super::engine_pack::BASELINE_SKIP }, Glue { width: class.dimen("footskip"), ..zero }, false);
        self.append_to_vlist(foot);
        let page_box = self.vpack(start, 0, false, d);
        // `\@outputpage`: `\if@twoside \ifodd\count\z@` (the page counter, stepped after
        // `\shipout`; `output_page` has stepped it already).
        let page = self.doc.as_ref().map_or(1, |d| d.page - 1);
        let margin = class.dimen(if class.flag("twoside") && page % 2 == 0 { "evensidemargin" } else { "oddsidemargin" });
        let page_box = self.e.nodes.add_box(BoxRec { vertical: true, shift: margin, ..page_box });
        // The shipped box.
        let start = self.cur.len();
        self.aux = super::engine_node::IGNORE_DEPTH;
        let n = self.e.nodes.glue_node(Glue { width: class.dimen("topmargin"), ..zero }, 0);
        self.cur.push(n);
        self.append_to_vlist(page_box);
        let mut shipped = self.vpack(start, 0, false, d);
        // The kernel's `\shipout`, as the engine's boxes show it: `shipout/before` (hyperref's
        // page anchor), the background (pdftex.def), then the first-page specials (hyperref
        // fills `shipout/firstpage`), each box put before the last.
        if self.hyper() {
            let top = class.dimen("topmargin");
            shipped = self.page_anchor(shipped, page, top, margin, i32::MAX >> 1);
        }
        if self.doc.as_ref().is_some_and(|d| d.packages.iter().any(|p| matches!(*p, "graphicx" | "color" | "xcolor"))) {
            shipped = self.page_background(shipped, d, true);
        }
        if self.hyper() && self.doc.as_mut().is_some_and(|d| !std::mem::replace(&mut d.shipped, true)) {
            shipped = self.page_background(shipped, d, false);
        }
        let shipped = self.e.nodes.add_box(BoxRec { vertical: true, ..shipped });
        self.aux = saved;
        for (p, g) in params.into_iter().zip(old) {
            self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: p }, g, false);
        }
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: super::engine_pack::LINE_SKIP_LIMIT }, old_limit, false);
        if self.doc.as_ref().is_some_and(|d| d.defer) {
            // A contents list waits for the pages: the page records now, the shipping at the
            // end (`finish_contents`), in order.
            self.out_whats(shipped, false, &mut [].iter());
            self.page.dead_cycles = 0;
            if let Some(doc) = self.doc.as_mut() {
                doc.deferred.push(shipped);
            }
        } else {
            self.ship_out(shipped);
        }
        self.after_opcol();
    }

    fn set_int_named(&mut self, r: Reg, v: i32) {
        self.e.eqtb.set_word(r, v, false);
    }

    /// A construct the native job does not implement: the job stops, and the document runs
    /// on the engine instead.
    pub(super) fn not_native(&mut self, what: &str) {
        if let Some(doc) = self.doc.as_mut() {
            doc.unsupported.get_or_insert_with(|| what.to_string());
        }
        self.halted = true;
    }

    /// A `\write` to LaTeX's `.aux` file: an inert whatsit, kept for its place in the list
    /// (glue after it is a legal breakpoint, §868, §973).
    /// `\protected@write` ends with `\if@nobreak\ifvmode\nobreak\fi\fi`.
    /// The inert whatsit of a `\protected@write\@auxout`; `entry` is a label's index plus
    /// one (0 for none), so shipout can record its page (`shipped_entry`).
    fn aux_write(&mut self, entry: i32) {
        self.cur.push(Node { kind: Kind::Whatsit, sub: super::engine_whatsit::WRITE, font: super::engine_whatsit::NULL_STREAM, a: entry, b: 0 });
        if self.mode == Mode::Vertical && self.doc.as_ref().is_some_and(|d| d.nobreak) {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
    }

    fn doc_id(&self, i: usize) -> Token {
        Token::cs(self.doc.as_ref().map_or(0, |d| d.ids[i]))
    }

    /// The control sequence number of [`NAMED`]`[i]`.
    fn named_id(&self, i: usize) -> u32 {
        self.doc.as_ref().map_or(0, |d| d.named[i])
    }

    /// The control sequence [`NAMED`]`[i]`, as the document may have redefined it.
    fn named_cs(&self, i: usize) -> Token {
        Token::cs(self.doc.as_ref().map_or(0, |d| d.named[i]))
    }

    /// The paragraph hook every paragraph runs after `new_graf` (ltpara.dtx,
    /// `\g__para_standard_everypar_tl`): the indent box is taken, the empty paragraph ended,
    /// and the paragraph started again with `\parskip` zero and `\noindent`, then the box
    /// put back. Then the user's `\everypar`: `\@afterheading`'s, kept as typed flags.
    fn para_hook(&mut self) {
        // `\box_gset_to_last:N \g_para_indent_box`.
        let indent = (self.cur.len() > self.start as usize).then(|| self.cur[self.cur.len() - 1]).filter(|n| n.kind == Kind::Box);
        if indent.is_some() {
            self.cur.pop();
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.indent = indent;
        }
        // `\group_begin: \tex_par:D \group_end:`: the list is empty, so `end_graf` is
        // `pop_nest` (its `normal_paragraph` is undone by the group), then `build_page`.
        // `\tex_noindent:D` with `\parskip` zero (no minipages yet) and `\everypar` empty
        // builds the page again; then `\@kernel@before@para@begin`.
        self.pop_nest();
        self.run_ops(vec![Op::Build, Op::Call(|m| m.start_graf(false, Glue::default(), false)), Op::Call(|m| m.para_begin())]);
    }

    /// The end of the paragraph hook: `\@afterheading`'s `\everypar`, kept as typed flags,
    /// and `\__para_handle_indent:`.
    fn para_begin(&mut self) {
        let indent = self.doc.as_mut().and_then(|d| d.indent.take());
        if let Some(b) = indent {
            self.cur.push(b);
        }
        // The user's `\everypar`.
        match self.everypar() {
            AFTER_HEADING => {
                // `\@afterheading`: `\if@nobreak \@nobreakfalse \clubpenalty\@M
                // \if@afterindent \else {\setbox\z@\lastbox}\fi \else \clubpenalty
                // \@clubpenalty \everypar{}\fi`.
                let Some(doc) = self.doc.as_mut() else { return };
                if doc.nobreak {
                    doc.nobreak = false;
                    let drop = !doc.afterindent;
                    self.set_int_named(param!("clubpenalty"), 10000);
                    if drop {
                        self.drop_last_box();
                    }
                } else {
                    let v = self.rw(R_CLUBPENALTY);
                    self.set_int_named(param!("clubpenalty"), v);
                    self.set_everypar(0);
                }
            }
            ITEM => self.item_everypar(),
            MT_PROT => self.left_protrusion(),
            THM_HEAD => self.theorem_everypar(),
            // `\@setminipage`'s `\everypar{\@minipagefalse\everypar{}}`.
            MINIPAGE => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.floats.minipage = false;
                }
                self.set_everypar(0);
            }
            P_CELL => {
                let ht = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
                self.build_rule([0, ht, RUNNING]);
                self.set_everypar(0);
            }
            // `\@verbatim`'s `\everypar{\the\everypar \unpenalty}`.
            VERB_ITEM => {
                self.item_everypar();
                if self.tail().is_some_and(|t| t.kind == Kind::Penalty) {
                    self.cur.pop();
                }
            }
            RUN_IN => self.run_in(),
            ENDPE => {
                // `\@doendpe`: `{\setbox\z@\lastbox}\everypar{}\@endpefalse`.
                self.drop_last_box();
                self.set_everypar(0);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
            }
            _ => {}
        }
    }

    /// `{\setbox\z@\lastbox}`: the last box of the list, if it is one, dies. Whether there was one.
    fn drop_last_box(&mut self) -> bool {
        if self.cur.len() > self.start as usize && self.cur[self.cur.len() - 1].kind == Kind::Box {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
            return true;
        }
        false
    }

    /// The user `\everypar`, as the code it holds.
    fn everypar(&self) -> i32 {
        self.e.eqtb.word(EVERYPAR)
    }

    fn set_everypar(&mut self, kind: i32) {
        self.e.eqtb.set_word(EVERYPAR, kind, false);
    }

    fn nopar(&self) -> i32 {
        self.e.eqtb.word(NOPAR)
    }

    fn set_nopar(&mut self, flags: i32) {
        self.e.eqtb.set_word(NOPAR, flags, false);
    }

    /// LaTeX register `i` (`data_class::CLASS_REGS`): an integer or dimension.
    fn rw(&self, i: usize) -> i32 {
        self.doc.as_ref().map_or(0, |d| self.e.eqtb.word(d.regs[i]))
    }

    fn rg(&self, i: usize) -> Glue {
        self.doc.as_ref().map_or(Glue::default(), |d| self.e.eqtb.glue(d.regs[i]))
    }

    fn set_rw(&mut self, i: usize, v: i32, global: bool) {
        if let Some(r) = self.doc.as_ref().map(|d| d.regs[i]) {
            self.e.eqtb.set_word(r, v, global);
        }
    }

    fn set_rg(&mut self, i: usize, g: Glue) {
        if let Some(r) = self.doc.as_ref().map(|d| d.regs[i]) {
            self.e.eqtb.set_glue(r, g, false);
        }
    }

    /// Runs `ops` in order; at the first that may build the page, the rest wait behind
    /// `\rt@step`.
    fn run_ops(&mut self, mut ops: Vec<Op>) {
        ops.reverse();
        self.run_reversed(ops);
    }

    fn run_reversed(&mut self, mut ops: Vec<Op>) {
        while let Some(op) = ops.pop() {
            if let Op::Vskip(g) = op {
                self.vskip(g);
                continue;
            }
            if !ops.is_empty() {
                if let Some(doc) = self.doc.as_mut() {
                    doc.steps.push(ops);
                }
                let t = self.doc_id(STEP);
                self.input.back(t);
            }
            match op {
                Op::Par => self.par(),
                Op::Penalty(p) => self.vpenalty(p),
                Op::Build => self.maybe_build_page(),
                Op::Call(f) => f(self),
                Op::Vskip(_) => {}
            }
            return;
        }
    }

    fn step(&mut self) {
        if let Some(ops) = self.doc.as_mut().and_then(|d| d.steps.pop()) {
            self.run_reversed(ops);
        }
    }

    /// `\par` (§1094): a paragraph ends; on the main vertical list, the page is built.
    fn par(&mut self) {
        match self.mode {
            Mode::Horizontal if !self.inner => self.end_graf(),
            Mode::Vertical => self.normal_paragraph(),
            _ => {}
        }
        self.maybe_build_page();
    }

    /// `\penalty` in vertical mode (§1103).
    fn vpenalty(&mut self, p: i32) {
        self.cur.push(Node::new(Kind::Penalty, p));
        self.maybe_build_page();
    }

    fn vskip(&mut self, g: Glue) {
        let n = self.e.nodes.glue_node(g, 0);
        self.cur.push(n);
    }

    /// `\lastskip` (§424).
    fn last_skip(&self) -> Glue {
        match self.last_item(Kind::Glue) {
            Some(n) => self.e.nodes.glue(&n),
            None => self.page_last_glue().unwrap_or_default(),
        }
    }

    /// The current font's em and ex, for `scan_glue`.
    fn em_ex(&self) -> (i64, i64) {
        let p = self.e.fonts.params(self.cur_font());
        (p.get(5).copied().unwrap_or(0) as i64, p.get(4).copied().unwrap_or(0) as i64)
    }

    fn glue_of(&self, text: &str) -> Glue {
        let (em, ex) = self.em_ex();
        scan_glue(text, em, ex, &|_| None).unwrap_or_default()
    }

    /// `\addpenalty` (latex.ltx): a penalty before the last skip, the skip kept.
    fn add_penalty(&mut self, p: i32, ops: &mut Vec<Op>) {
        if self.doc.as_ref().is_some_and(|d| d.nobreak || d.floats.minipage) {
            return;
        }
        let last = self.last_skip();
        if last.width == 0 {
            return ops.push(Op::Penalty(p));
        }
        let a = last;
        let pd = self.aux;
        let max = self.dimen_param(super::engine_page::MAX_DEPTH);
        let mut b = a;
        b.width += if pd > max { max } else if pd == IGNORE_DEPTH { 0 } else { pd };
        ops.extend([Op::Vskip(neg(b)), Op::Penalty(p)]);
        if a.width != b.width {
            ops.push(Op::Vskip(glue_add(b, neg(a))));
        }
        ops.push(Op::Vskip(a));
    }

    /// `\addvspace` with `\@xaddvskip` (latex.ltx): the larger of the last skip and `g`.
    fn add_vspace(&mut self, g: Glue) {
        if self.doc.as_ref().is_some_and(|d| d.floats.minipage) {
            return;
        }
        let last = self.last_skip();
        if last.width == 0 {
            return self.vskip(g);
        }
        if last.width < g.width {
            self.vskip(neg(last));
            self.vskip(g);
        } else if g.width < 0 && last.width >= 0 {
            self.vskip(neg(last));
            self.vskip(glue_add(g, last));
        }
    }

    /// A macro argument: one token, or a braced group without its braces.
    pub(super) fn read_arg(&mut self, out: &mut Vec<Token>) {
        out.clear();
        let Some(t) = self.next_unblank() else { return };
        if t.cat() != Some(Cat::BeginGroup) {
            return out.push(t);
        }
        let mut depth = 1;
        while let Some(t) = self.get_token() {
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
            out.push(t);
        }
    }

    /// The next token that is not a space, unexpanded.
    fn next_unblank(&mut self) -> Option<Token> {
        loop {
            let t = self.get_token()?;
            if t.cat() != Some(Cat::Space) {
                return Some(t);
            }
        }
    }

    /// `\@ifnextchar c`: consumes `c` when it comes next (after spaces).
    pub(super) fn next_is(&mut self, c: char) -> bool {
        let Some(t) = self.next_unblank() else { return false };
        if t == Token::char(Cat::Other, c) {
            return true;
        }
        self.input.back(t);
        false
    }

    /// report's `\chapter` (`openany`): `\clearpage \thispagestyle{plain}`, then the rest
    /// (`chapter_head`).
    fn chapter(&mut self) {
        let mut t = vec![self.doc_id(doc!("clearpage")), self.doc_id(doc!("thispagestyle"))];
        t.extend(self.tex_tokens("{plain}"));
        t.push(self.doc_id(doc!("rt@chapter")));
        self.back_list(&t);
    }

    /// `\global\@topnum\z@ \@afterindentfalse \secdef\@chapter\@schapter`. `\@chapter`:
    /// `\refstepcounter{chapter}` (its `\@addtoreset` list: section, equation, figure, table,
    /// footnote, and below them), `\addcontentsline{toc}`, `\chaptermark` (nothing under
    /// `plain`), `\addtocontents{lof}` and `{lot}` (two more `.aux` writes); then
    /// `\@makechapterhead` as tokens and `\@afterheading`. `\@schapter` only the head
    /// (`\@makeschapterhead`).
    fn chapter_head(&mut self) {
        self.fl().topnum = 0;
        let star = self.next_is('*');
        let mut title = Vec::new();
        if !star && self.next_is('[') {
            self.bracket_arg();
        }
        self.read_arg(&mut title);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.afterindent = false;
        doc.level = 0;
        doc.numbered = !star;
        doc.toc = !star;
        let mut number = String::new();
        if !star {
            doc.counters[0] += 1;
            doc.counters[1..].fill(0);
            doc.equation = 0;
            doc.footnote = 0;
            for t in 0..doc.theorems.len() {
                let within = |k: usize| doc.theorems[k].within.is_some();
                if within(t) || doc.theorems[t].parent.is_some_and(within) {
                    doc.thm_values[t] = 0;
                }
            }
            number = heading_number(&doc.counters, 0, doc.appendix);
            // `\theequation`, `\thefigure`, `\thetable`: `\thechapter.` first.
            doc.the_prefix = std::array::from_fn(|_| format!("{number}."));
            self.store_current_label(&number);
            self.set_rw(R_FIGURE, 0, true);
            self.set_rw(R_FIGURE + 1, 0, true);
            self.contents_write();
            self.aux_write(0);
            self.aux_write(0);
        }
        let mut t = vec![self.doc_id(doc!("vspace"))];
        t.extend(self.tex_tokens("*{50pt}{\\parindent0pt"));
        t.extend([self.doc_id(doc!("raggedright")), self.doc_id(doc!("normalfont"))]);
        if !star {
            t.extend([self.doc_id(doc!("huge")), self.doc_id(doc!("bfseries"))]);
            t.extend(self.tex_tokens(&format!("Chapter {number}")));
            t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
            t.extend(self.tex_tokens("\\vskip20pt"));
        }
        t.extend(self.tex_tokens("\\interlinepenalty10000 "));
        t.extend([self.doc_id(doc!("Huge")), self.doc_id(doc!("bfseries"))]);
        t.append(&mut title);
        t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
        t.extend(self.tex_tokens("\\vskip40pt}"));
        t.push(self.doc_id(doc!("rt@afterheading")));
        self.back_list(&t);
    }

    /// `\section` and its kin: `\@startsection` (latex.ltx) with article's arguments.
    ///
    /// The heading is `\@sect`'s: in a group, `\normalfont<size>\bfseries`, the number box
    /// `\hbox{\hskip\z@ <number>\quad}` (`\@hangfrom`), `\hangindent` its width,
    /// `\noindent`, the box, `\interlinepenalty\@M`, the title, `\par`. Then `\@xsect`. The
    /// table of contents line and the mark have no effect on the page and are not made.
    fn section(&mut self, level: usize) {
        // `\@ifstar`, then `\@dblarg`'s optional short title (for the contents only).
        let star = self.next_is('*');
        let mut title = self.doc.as_mut().map(|d| std::mem::take(&mut d.title)).unwrap_or_default();
        if !star && self.next_is('[') {
            let mut depth = 0;
            while let Some(t) = self.get_token() {
                match t.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => depth -= 1,
                    _ if depth == 0 && t == Token::char(Cat::Other, ']') => break,
                    _ => {}
                }
            }
        }
        self.read_arg(&mut title);
        let Some(doc) = self.doc.as_mut() else { return };
        // report's `\section` is level 1, under `\chapter`.
        let level = level + doc.report as usize;
        doc.title = title;
        doc.numbered = !star && level < SECNUMDEPTH;
        doc.toc = !star;
        doc.level = level;
        let noskipsec = doc.noskipsec;
        if doc.packages.contains(&"hyperref") && ((!doc.numbered && !star) || doc.contents.is_none() || level > 2) {
            return self.not_native("hyperref: an unnumbered heading, or headings without their bookmarks");
        }
        if doc.numbered {
            // `\refstepcounter`'s `\@currentlabel`: the number as `\@sect` steps it.
            let mut c = doc.counters;
            c[level] += 1;
            c[level + 1..].fill(0);
            let text = heading_number(&c, level, doc.appendix);
            self.store_current_label(&text);
        }
        // `\@startsection`: `\if@noskipsec \leavevmode \fi \par`.
        if noskipsec {
            return self.not_native("a heading right after a run-in heading");
        }
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Call(|m| m.section_skip())]);
    }

    /// `\@startsection` after its `\par`: `\@afterindent`, `\addpenalty\@secpenalty`,
    /// `\addvspace<before>`, then `\@sect`.
    fn section_skip(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let level = doc.level;
        let sec = &SECS[level - doc.report as usize];
        let mut before = self.glue_of(sec.before);
        let afterindent = before.width >= 0;
        if !afterindent {
            before = neg(before);
        }
        let after = self.glue_of(sec.after);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.afterindent = afterindent;
        doc.after = after;
        if doc.nobreak {
            self.set_everypar(0);
            return self.section_head();
        }
        // parskip's `\advance\@tempskipa-\parskip` before the `\addvspace`.
        if self.parskip() {
            before = glue_add(before, neg(self.e.eqtb.glue(param!("parskip"))));
        }
        let mut ops = Vec::new();
        self.add_penalty(SEC_PENALTY, &mut ops);
        ops.push(Op::Call(|m| {
            let before = m.doc.as_ref().map(|d| d.after_before).unwrap_or_default();
            m.add_vspace(before);
            m.section_head();
        }));
        if let Some(doc) = self.doc.as_mut() {
            doc.after_before = before;
        }
        self.run_ops(ops);
    }

    /// `\@sect` or `\@ssect`: a display heading, or a run-in one (`after` not positive):
    /// `\def\@svsechd{…}`, then `\@xsect`'s `\@nobreakfalse \global\@noskipsectrue
    /// \everypar{…}\ignorespaces`. The title waits in `title` for the paragraph.
    fn section_head(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.after.width > 0 {
            return self.section_start();
        }
        if doc.numbered {
            return self.not_native("a numbered run-in heading");
        }
        doc.nobreak = false;
        doc.noskipsec = true;
        self.set_everypar(RUN_IN);
        self.ignore_spaces();
    }

    /// `\@xsect`'s `\everypar` at the paragraph after a run-in heading: `\global
    /// \@noskipsecfalse {\setbox\z@\lastbox} \clubpenalty\@M \begingroup \@svsechd
    /// \endgroup \unskip \hskip -<after>`, where `\@svsechd` is `{font}{\hskip\z@\relax
    /// title}` and, unstarred, the `\addcontentsline`.
    fn run_in(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if !doc.noskipsec {
            let v = self.rw(R_CLUBPENALTY);
            self.set_int_named(param!("clubpenalty"), v);
            return self.set_everypar(0);
        }
        doc.noskipsec = false;
        let mut list = std::mem::take(&mut doc.title);
        let size = SECS[doc.level - doc.report as usize].size;
        self.drop_last_box();
        self.set_int_named(param!("clubpenalty"), 10000);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_size(size);
        let key = self.rm_key("bx/n");
        self.select_nfss(&key, self.size_row(size).fsize);
        // `\hskip\z@`: a new specification, not `zero_glue` (protrusion stops at it).
        let mut g = self.e.nodes.glue_node(Glue::default(), 0);
        g.font = 1;
        self.cur.push(g);
        list.push(self.doc_id(RUN_IN_END));
        self.back_list(&list);
    }

    fn run_in_end(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.doc.as_ref().is_some_and(|d| d.toc) {
            self.contents_write();
        }
        self.unskip();
        let Some(level) = self.doc.as_ref().map(|d| d.level - d.report as usize) else { return };
        let g = neg(self.glue_of(SECS[level].after));
        let g = self.e.nodes.glue_node(g, 0);
        self.cur.push(g);
    }

    /// `\@sect`: `\refstepcounter`, whose `\stepcounter` resets the levels below, then the
    /// heading's group and number box.
    fn section_start(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let level = doc.level;
        let sec = &SECS[level - doc.report as usize];
        let mut number = String::new();
        if doc.numbered {
            doc.counters[level] += 1;
            doc.counters[level + 1..].fill(0);
            // `\@addtoreset`: theorems and equations numbered within this level or a lower one.
            if doc.eq_within.is_some_and(|w| w >= level) {
                doc.equation = 0;
            }
            // `\@stpelt` steps a reset counter from -1, so the counters it resets go to 0 too.
            for t in 0..doc.theorems.len() {
                let within = |k: usize| doc.theorems[k].within.is_some_and(|w| w >= level);
                if within(t) || doc.theorems[t].parent.is_some_and(within) {
                    doc.thm_values[t] = 0;
                }
            }
            number = heading_number(&doc.counters, level, doc.appendix);
            if doc.packages.contains(&"hyperref") {
                let counters = doc.counters;
                self.heading_anchor(level, &counters);
            }
        } else if doc.packages.contains(&"hyperref") {
            // hpdftex's `\@ssect`: `\Hy@MakeCurrentHrefAuto{section*}`, the anchor first in
            // the title (`section_title`).
            doc.link_counter += 1;
            let name = format!("section*.{}", doc.link_counter);
            doc.star_anchor = Some(name.clone());
            self.set_current_href(&name);
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_size(sec.size);
        let key = self.rm_key("bx/n");
        self.select_nfss(&key, self.size_row(sec.size).fsize);
        // `\setbox\@tempboxa\hbox{{\hskip\z@\relax\@svsec}}`.
        self.push_nest(Mode::Horizontal, true, 1000);
        // `\hskip\z@`: a new specification, not `zero_glue` (protrusion stops at it).
        let mut g = self.e.nodes.glue_node(Glue::default(), 0);
        g.font = 1;
        self.cur.push(g);
        let mut list: Vec<Token> = number.chars().map(|c| Token::char(Cat::Other, c)).collect();
        list.push(self.doc_id(SEC_BOX));
        self.back_list(&list);
    }

    /// The number box's `\quad`, the box, `\hangindent`, then `\noindent`.
    fn section_box(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.numbered) {
            let (em, _) = self.em_ex();
            let g = self.e.nodes.glue_node(Glue { width: em as i32, ..Glue::default() }, 0);
            self.cur.push(g);
        }
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: HANG_INDENT }, b.width, false);
        let b = self.e.nodes.add_box(b);
        if let Some(doc) = self.doc.as_mut() {
            doc.number_box = Some(b);
        }
        let t = self.doc_id(SEC_TITLE);
        self.input.back(t);
        self.new_graf(false);
    }

    /// `\box\@tempboxa\interlinepenalty\@M`, then the title.
    fn section_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let b = doc.number_box.take();
        let mut list = std::mem::take(&mut doc.title);
        if let Some(f) = doc.fancy.as_mut() {
            f.title.clone_from(&list);
        }
        // `\Hy@@SectionAnchor`: `\leavevmode\hbox to0pt{\kern-\Hy@SectionHShift
        // \Hy@raisedlink{\hyper@anchorstart{…}\hyper@anchorend}\hss}` (the indent is 0).
        if let Some(name) = doc.star_anchor.take() {
            let mut t = self.tex_tokens("\\hbox to0pt{\\kern0pt ");
            t.extend(self.text_raised_link(name));
            t.extend(self.tex_tokens("\\hss}"));
            t.append(&mut list);
            list = t;
        }
        if let Some(b) = b {
            self.cur.push(Node::new(Kind::Box, b as i32));
            self.aux = 1000;
        }
        self.set_int_named(param!("interlinepenalty"), 10000);
        list.push(self.doc_id(SEC_END));
        self.back_list(&list);
        list.clear();
        if let Some(doc) = self.doc.as_mut() {
            doc.title = list;
        }
    }

    /// `\@@par\endgroup`, then `\@xsect`: `\par\nobreak\vskip<after>\@afterheading
    /// \ignorespaces`.
    fn section_end(&mut self) {
        self.run_ops(vec![Op::Par, Op::Call(|m| m.section_after())]);
    }

    fn section_after(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        // `\addcontentsline{toc}`: an unstarred heading's `\write` to the `.aux` file, after
        // hyperref's bookmark `\protected@write` to the `.out` file (entry 0: nothing at
        // shipout).
        if self.doc.as_ref().is_some_and(|d| d.toc) {
            // fancyhdr's `\sectionmark`, before `\addcontentsline`.
            self.fancy_marks();
            if self.doc.as_ref().is_some_and(|d| d.unsupported.is_some()) {
                return;
            }
            if self.hyper() {
                self.aux_write(0);
            }
            self.contents_write();
        }
        let Some(after) = self.doc.as_ref().map(|d| d.after) else { return };
        let mut ops = Vec::new();
        if after.width > 0 {
            // parskip's `\advance\@tempskipa-\parskip` before `\@xsect`'s `\vskip`.
            let skip = if self.parskip() { glue_add(after, neg(self.e.eqtb.glue(param!("parskip")))) } else { after };
            ops.extend([self.cur_par(), Op::Penalty(10000), Op::Vskip(skip)]);
        }
        ops.push(Op::Call(|m| {
            if m.doc.as_ref().is_some_and(|d| d.after.width > 0) {
                // `\@afterheading`.
                if let Some(doc) = m.doc.as_mut() {
                    doc.nobreak = true;
                }
                m.set_everypar(AFTER_HEADING);
            }
            if let Some(x) = m.next_non_blank() {
                m.input.back(x.t);
            }
        }));
        self.run_ops(ops);
    }

    /// `\@setfontsize`'s `\size@update`: `\baselineskip` from the size table, then
    /// `\baselineskip\f@linespread\baselineskip`, the glue coerced to its width and scaled by
    /// `\baselinestretch` as `\set@fontsize` expanded it.
    fn set_size(&mut self, size: usize) {
        let mut g = glue_words(self.size_row(size).baselineskip);
        // The kernel table's `\baselinestretch` is empty: `\baselineskip\baselineskip`.
        let id = self.named_id(named!("baselinestretch"));
        let empty = matches!(self.e.eqtb.meaning(id), Meaning::Macro(m) if self.e.toks.get(m.text).len() == 1);
        if !empty {
            let Some(stretch) = self.macro_factor(id, (false, 1, 0)) else {
                return self.not_native("a \\baselinestretch that is not a number");
            };
            g = Glue { width: stretch(g.width), ..Glue::default() };
        }
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, g, false);
        self.strut_update();
    }

    /// `\fontsize{#1}{#2}\selectfont` (latex.ltx `\set@fontsize`, `\selectfont`): `\f@size`
    /// is `#1` without `pt`, the current shape is selected at it, then `\size@update`:
    /// `\baselineskip` `#2` scaled by `\baselinestretch` as `set_size` does, and `\strutbox`.
    /// A size the shape's `.fd` entries do not hold (`\try@size@substitution` renames the
    /// font and warns) declines, as does a size whose math the job would need.
    fn font_size(&mut self) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut b);
        let select = self.named_id(named!("selectfont"));
        if self.next_unblank().and_then(|t| t.cs_id()) != Some(select) {
            return self.not_native("\\fontsize without \\selectfont after it");
        }
        let text = |t: &[Token]| -> String { t.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect() };
        let (Some(size), Some(skip)) = (size_arg(&text(&a)), size_arg(&text(&b))) else {
            return self.not_native("a \\fontsize argument that is not a number of points");
        };
        let size = size_name(&strip_pt(size));
        let (key, _) = self.font_key();
        if !self.size_held(&key, size) {
            return self.not_native("a font size its shape does not have");
        }
        self.select_nfss(&key, size);
        let mut g = Glue { width: skip, ..Glue::default() };
        let id = self.named_id(named!("baselinestretch"));
        let empty = matches!(self.e.eqtb.meaning(id), Meaning::Macro(m) if self.e.toks.get(m.text).len() == 1);
        if !empty {
            let Some(stretch) = self.macro_factor(id, (false, 1, 0)) else {
                return self.not_native("a \\baselinestretch that is not a number");
            };
            g = Glue { width: stretch(g.width), ..Glue::default() };
        }
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, g, false);
        self.strut_update();
    }

    /// Whether the `.fd` entries of shape `key` hold `size` without a substitution: a
    /// single size equal to it or a range containing it.
    fn size_held(&self, key: &str, size: &str) -> bool {
        let packages = self.doc.as_ref().map(|d| d.packages.clone()).unwrap_or_default();
        let t = packages.iter().rev().filter_map(|p| crate::data::data_kernel::package_shapes(p)).find(|t| t.shape(key).is_some()).or_else(data_nfss::nfss).filter(|t| t.shape(key).is_some());
        t.and_then(|t| t.shape(key).map(|s| t.holds(s, size))).unwrap_or(false)
    }

    /// Size command `i` of the class's size option (`data_class::SIZE_COMMANDS`).
    fn size_row(&self, i: usize) -> data_class::SizeRow<'static> {
        match &self.doc {
            Some(d) => d.class.table.size(i),
            None => data_class::article().map(|t| t.size(i)).expect("the article table"),
        }
    }

    /// `\DeclareMathSizes` for text size `size`: the text, script and scriptscript sizes
    /// (`\tf@size`, `\sf@size`, `\ssf@size`); a size no command sets keeps itself.
    fn math_sizes(&self, size: &'static str) -> (&'static str, &'static str, &'static str) {
        (0..data_class::SIZE_COMMANDS.len()).map(|i| self.size_row(i)).find(|r| r.fsize == size).map_or((size, size, size), |r| (r.fsize, r.sf, r.ssf))
    }

    /// The current font's NFSS name `enc/family/series/shape` and size, from its identifier
    /// (`\font@name`, `\OT1/cmr/m/n/10`).
    fn font_key(&self) -> (String, &'static str) {
        let f = self.cur_font();
        let name = self.e.cs.name(self.e.fonts.list[f as usize].ident);
        match name.rsplit_once('/') {
            Some((key, size)) if key.matches('/').count() == 3 => (key.to_string(), static_size(size)),
            _ => (self.rm_key("m/n"), self.size_row(NORMALSIZE).fsize),
        }
    }

    /// `\selectfont` of `key` (`enc/family/series/shape`) at `size`: the font
    /// `\get@external@font` names, loaded once and known by its `\font@name` from then on.
    fn select_nfss(&mut self, key: &str, size: &'static str) {
        self.select_nfss_cs(key, size);
        // microtype's `\pickup@font` hook, from `\begin{document}` on (`\MT@setup@`).
        if self.doc.as_ref().is_some_and(|d| d.body) && self.microtype() {
            let f = self.cur_font();
            self.microtype_setup(key, f);
        }
    }

    /// [`Self::select_nfss`], and the control sequence `key/size` whose meaning is now the
    /// font selected; `None` when that name stays undefined (the first selection of a
    /// substituted shape) or the shape is undefined.
    fn select_nfss_cs(&mut self, key: &str, size: &'static str) -> Option<u32> {
        // `key/size`, made on the stack: a selection of a loaded shape allocates nothing.
        let mut stack = [0u8; 96];
        let long;
        let name: &str = if key.len() + 1 + size.len() <= stack.len() {
            let (k, s) = (key.len(), size.len());
            stack[..k].copy_from_slice(key.as_bytes());
            stack[k] = b'/';
            stack[k + 1..k + 1 + s].copy_from_slice(size.as_bytes());
            std::str::from_utf8(&stack[..k + 1 + s]).expect("two strings and a slash")
        } else {
            long = format!("{key}/{size}");
            &long
        };
        let cs = self.e.cs.intern(name);
        if let Meaning::Font(f) = self.e.eqtb.meaning(cs) {
            self.select_font(f, false);
            return Some(cs);
        }
        // A shape a loaded package declares, else the `.fd` files' (`data_nfss`).
        let packages = self.doc.as_ref().map(|d| d.packages.clone()).unwrap_or_default();
        let table = |k: &str| packages.iter().rev().filter_map(|p| crate::data::data_kernel::package_shapes(p)).find(|t| t.shape(k).is_some()).or_else(data_nfss::nfss).filter(|t| t.shape(k).is_some());
        // `\wrong@fontshape`: an undefined shape takes the encoding's default shape, then
        // its default series, then its default family (`\DeclareFontSubstitution`). The
        // first time, the font is the substitute's (`\font@name` is its name) and the shape
        // becomes an alias of it, so the asked-for name stays undefined; a later selection
        // finds the alias and loads the font under the asked-for name (`\extract@font`).
        let aliased = self.doc.as_ref().and_then(|d| d.wrong_shapes.iter().find(|w| w.0 == key)).map(|w| w.1.clone());
        let (key, first) = match (table(key), aliased) {
            (Some(_), _) => (key.to_string(), false),
            (None, Some(k)) => (k, false),
            // A text family no table holds: LaTeX would read its `.fd` file, which the native
            // job does not have (`rushtex-gen nfss` pins them). Other encodings in a text
            // family (`OMS/cmtt` for `\{`) often have none, and LaTeX substitutes.
            (None, None) if matches!(key.split('/').next(), Some("OT1" | "T1")) && key.split('/').nth(1).is_some_and(|fam| {
                let family = format!("{}+{fam}", key.split('/').next().unwrap_or(""));
                packages.iter().filter_map(|p| crate::data::data_kernel::package_shapes(p)).chain(data_nfss::nfss()).all(|t| t.family(&family).is_none())
            }) =>
            {
                self.not_native("a font family whose .fd file the native tables do not hold");
                return None;
            }
            (None, None) => match wrong_font_shape(key, |k| table(k).is_some()) {
                Some(k) => {
                    if let Some(d) = self.doc.as_mut() {
                        d.wrong_shapes.push((key.to_string(), k.clone()));
                    }
                    (k, true)
                }
                None => {
                    self.error(format!("Font shape `{key}' undefined"));
                    return None;
                }
            },
        };
        let cs = if first { self.e.cs.intern(&format!("{key}/{size}")) } else { cs };
        if first {
            if let Meaning::Font(f) = self.e.eqtb.meaning(cs) {
                self.select_font(f, false);
                return None;
            }
        }
        let key = key.as_str();
        let t = table(key)?;
        let Some(ext) = t.shape(key).and_then(|s| t.external(s, size)) else {
            self.error(format!("Font shape `{key}' undefined"));
            return None;
        };
        let at = match ext.at {
            data_nfss::At::Sp(v) => v,
            data_nfss::At::Text(v) => data_nfss::decimal_sp(v).unwrap_or(-1000),
            data_nfss::At::None => -1000,
        };
        // A substitution (`ssub`) names the font after the shape it came to.
        let real = t.shape_name(ext.shape);
        let real_cs = if real == key { cs } else { self.e.cs.intern(&format!("{real}/{size}")) };
        let f = match self.e.eqtb.meaning(real_cs) {
            // `\do@subst@correction` loads a substitute again under its own name, which
            // names the font after it (§1260), even when it was loaded before.
            Meaning::Font(f) => {
                self.e.fonts.list[f as usize].ident = real_cs;
                f
            }
            _ => {
                let f = self.find_font(&ext.name(), at, Some(real_cs));
                let mut parts = real.split('/');
                let family = format!("{}+{}", parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                if let Some((hyphen, skew, _)) = t.family(&family).or_else(|| data_nfss::nfss()?.family(&family)) {
                    let font = &mut self.e.fonts.list[f as usize];
                    if hyphen != data_nfss::NONE {
                        font.hyphen_char = hyphen;
                    }
                    if skew != data_nfss::NONE {
                        font.skew_char = skew;
                    }
                }
                self.e.fonts.list[f as usize].ident = real_cs;
                self.e.eqtb.set_meaning(real_cs, Meaning::Font(f), true);
                f
            }
        };
        self.e.eqtb.set_meaning(cs, Meaning::Font(f), true);
        self.select_font(f, false);
        (!first).then_some(cs)
    }

    fn font_change(&mut self, c: Change) {
        let slot = match c {
            Change::Series("bx") => 0,
            Change::Series("m") => 1,
            Change::Shape("it") => 2,
            Change::Shape("sl") => 3,
            Change::Shape("sc") => 4,
            Change::Shape("n") => 5,
            Change::Default(0) => 6,
            Change::Default(1) => 7,
            Change::Default(2) => 8,
            Change::Em => 9,
            _ => FONT_SWITCHES,
        };
        self.switch_font(slot, |m| m.font_change_by_name(c));
    }

    /// The font switch `slot` from the current font: the font it reached before, else
    /// `select` run once and its result kept. A switch's result is a function of the font
    /// it starts from (its NFSS key, size and slant) and of the loaded packages' shapes, so
    /// the memo is a row of [`FONT_SWITCHES`] font numbers per font (one cache line),
    /// cleared when a package loads.
    fn switch_font(&mut self, slot: usize, select: impl FnOnce(&mut Self)) {
        let from = self.cur_font();
        if slot < FONT_SWITCHES {
            let hit = self.doc.as_ref().and_then(|d| d.font_switch.get(from as usize)).map_or(0, |row| row[slot]);
            if hit != 0 {
                return self.select_font(hit, false);
            }
        }
        select(self);
        let to = self.cur_font();
        if let Some(doc) = self.doc.as_mut().filter(|_| slot < FONT_SWITCHES && to != 0) {
            if doc.font_switch.len() <= from as usize {
                doc.font_switch.resize(from as usize + 1, [0; FONT_SWITCHES]);
            }
            doc.font_switch[from as usize][slot] = to;
        }
    }

    /// The family `\rmdefault` (0), `\sfdefault` (1) or `\ttdefault` (2) names.
    fn family(&self, k: u8) -> &'static str {
        self.doc.as_ref().map_or(DEFAULT_FAMILIES[k as usize], |d| d.families[k as usize])
    }

    /// `\encodingdefault/\rmdefault/<series/shape>`: the key `\normalfont` and the headings start from.
    fn rm_key(&self, series_shape: &str) -> String {
        format!("{}/{}/{series_shape}", self.encoding(), self.family(0))
    }

    /// `\encodingdefault`: `T1` after `\usepackage[T1]{fontenc}`, else `OT1`.
    fn encoding(&self) -> &'static str {
        if self.t1() {
            "T1"
        } else {
            "OT1"
        }
    }

    pub(super) fn t1(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.t1)
    }

    /// The preamble's `\rmdefault`, `\sfdefault` and `\ttdefault` become `Doc::families`.
    /// The other font defaults (`\familydefault`, `\encodingdefault`, the series and shape
    /// defaults) are read nowhere natively, so a document that changes one goes to the engine.
    fn font_defaults(&mut self) {
        let fixed = self.doc.as_ref().map(|d| d.font_fixed.clone()).unwrap_or_default();
        if FONT_FIXED.iter().zip(&fixed).any(|(name, body)| {
            let id = self.e.cs.intern(name);
            self.macro_body(id) != *body
        }) {
            return self.not_native("a changed font default");
        }
        for (k, name) in ["rmdefault", "sfdefault", "ttdefault"].into_iter().enumerate() {
            let id = self.e.cs.intern(name);
            let Some(b) = self.macro_body(id) else { continue };
            if b.is_empty() || !b.bytes().all(|c| c.is_ascii_alphanumeric()) {
                return self.not_native("a font default that is not a family name");
            }
            if b != DEFAULT_FAMILIES[k] {
                let fam = family_name(&b);
                if let Some(d) = self.doc.as_mut() {
                    d.families[k] = fam;
                    d.font_switch.clear();
                }
            }
        }
    }

    /// A parameterless macro's text, its control sequences written `\name`, `\@empty` dropped.
    fn macro_body(&self, id: u32) -> Option<String> {
        let Meaning::Macro(m) = self.e.eqtb.meaning(id) else { return None };
        let text = self.e.toks.get(m.text);
        let body = text.iter().position(|t| *t == Token::END_MATCH).filter(|&k| k == 0).map(|k| &text[k + 1..])?;
        // `\@empty` expands to nothing (the kernel writes `\mddefault` as `m\@empty`).
        let empty = self.named_id(named!("@empty"));
        Some(body.iter().filter_map(|t| match t.cs_id() {
            Some(c) if c == empty => None,
            Some(c) => Some(format!("\\{}", self.e.cs.name(c))),
            None => Some(t.chr().to_string()),
        }).collect())
    }

    fn font_change_by_name(&mut self, c: Change) {
        let (key, size) = self.font_key();
        let fam = |k: u8| self.family(k);
        let mut p: Vec<&str> = key.split('/').collect();
        match c {
            Change::Series(s) => p[2] = s,
            Change::Shape(s) => p[3] = s,
            Change::Default(k) => p[1] = fam(k),
            Change::Em => p[3] = if self.e.fonts.params(self.cur_font()).first().is_some_and(|&s| s > 0) { "n" } else { "it" },
        }
        self.select_nfss(&p.join("/"), size);
    }

    /// `\hspace{#1}`: `\hskip #1\relax`; `\hspace*{#1}`: `\vrule \@width\z@ \nobreak
    /// \hskip #1\hskip\z@skip`. In vertical mode `\hskip` starts a paragraph first.
    fn hspace(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let star = self.next_is('*');
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if star {
            self.build_rule([0, RUNNING, RUNNING]);
            self.build_penalty(10000);
        }
        let relax = self.named_cs(named!("relax"));
        arg.push(relax);
        self.back_list(&arg);
        let (g, fresh) = self.scan_glue_spec(false);
        if self.get_token() != Some(relax) {
            return self.not_native("\\hspace of more than glue");
        }
        self.build_glue(g, fresh);
        if star {
            self.build_glue(Glue::default(), false);
        }
    }

    /// A font declaration (`\bfseries`), or with `old` an old one (`\bf`: `\normalfont
    /// \bfseries`). In math `\not@math@alphabet` is an error and `\bf` a math alphabet,
    /// neither native.
    fn font_declaration(&mut self, c: Change, old: bool) {
        if self.mode == Mode::Math {
            return self.not_native("a font declaration in math");
        }
        if old {
            self.normal_font();
        }
        self.font_change(c);
    }

    /// `\DeclareTextFontCommand` (ltfntcmd.dtx): `\hmode@bgroup`, `\text@command`, the
    /// switch, `\check@icl`, the text, `\check@icr`, `\egroup`.
    fn text_font(&mut self, c: Change) {
        // `\hmode@bgroup`'s `\leavevmode` (before the argument is read: the native runs again).
        if self.leave_vmode() {
            return;
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        self.begin_group(SIMPLE_GROUP);
        // `\text@command`: no corrections for an empty or blank argument (`\nocorr` is not
        // defined in a native document).
        let corr = !(arg.is_empty() || arg.len() == 1 && arg[0].cat() == Some(Cat::Space));
        self.font_change(c);
        arg.push(self.doc_id(if corr { TEXT_END } else { TEXT_END_N }));
        self.back_list(&arg);
        if corr {
            self.maybe_ic();
        }
    }

    /// `\check@icr` (`\aftergroup\maybe@ic` outside vertical mode), then `\egroup`.
    fn text_end(&mut self, corr: bool) {
        if corr && self.mode != Mode::Vertical {
            let level = self.e.eqtb.level();
            let t = self.doc_id(MAYBE_IC);
            self.after_group.push((level, t));
        }
        self.end_group(SIMPLE_GROUP);
    }

    /// `\maybe@ic`: an italic correction unless the font is slanted or a `\nocorrlist`
    /// character (`,` `.`) comes next; `\sw@slant` and `\fix@penalty` put it before a last
    /// skip and penalty.
    fn maybe_ic(&mut self) {
        if self.e.fonts.params(self.cur_font()).first().is_some_and(|&s| s > 0) {
            return;
        }
        let Some(t) = self.get_token() else { return };
        self.input.back(t);
        if t == Token::char(Cat::Other, ',') || t == Token::char(Cat::Other, '.') {
            return;
        }
        let skip = self.last_item(Kind::Glue).filter(|n| self.e.nodes.glue(n).width != 0);
        if let Some(n) = skip {
            self.cur.pop();
            let g = self.e.nodes.glue(&n);
            self.e.nodes.flush_node(n);
            self.fix_penalty();
            self.vskip(g);
        } else {
            self.fix_penalty();
        }
    }

    fn fix_penalty(&mut self) {
        match self.last_item(Kind::Penalty).filter(|n| n.a != 0) {
            Some(n) => {
                self.cur.pop();
                self.append_italic_correction();
                self.cur.push(n);
            }
            None => self.append_italic_correction(),
        }
    }

    /// `\UseTextSymbol{enc}`: `{\fontencoding{enc}\selectfont\char c}`.
    fn brace_symbol(&mut self, delim: usize, c: u8) {
        if self.mode == Mode::Math {
            let t = self.named_cs(delim);
            return self.input.back(t);
        }
        if !self.t1_symbol("OMS", c) {
            self.text_symbol("OMS", c);
        }
    }

    /// In T1, the symbols `t1enc.def` declares in the encoding itself (`\textbackslash`,
    /// `\textbar`, the braces, `<`, `>`, `\textsection`, `\textdollar`,
    /// `\textunderscore`): the character, as a `\chardef` typesets it. `false` when the
    /// document is OT1 or the symbol stays in its default encoding (TS1).
    fn t1_symbol(&mut self, enc: &str, c: u8) -> bool {
        if !self.t1() || self.mode == Mode::Math {
            return false;
        }
        let t1 = match (enc, c) {
            ("OMS", 110) => 92,
            ("OMS", 106) => 124,
            ("OMS", 102) => 123,
            ("OMS", 103) => 125,
            ("OML", 60) => 60,
            ("OML", 62) => 62,
            ("TS1", 167) => 159,
            ("TS1", 36) => 36,
            ("OT1", 95) => 95,
            _ => return false,
        };
        self.back_list(&[Token::char(Cat::Other, char::from(t1))]);
        true
    }

    /// latex.ltx's `\DeclareTextSymbolDefault{\textbackslash}{OMS}` and its kin (OMS for
    /// the backslash, bars and braces, OML for `<` and `>`, TS1 for the daggers, bullet,
    /// section and paragraph signs): in OT1 text the default encoding's character, at the current family, series and shape (NFSS
    /// substitutes as it does for `\{`). In a formula LaTeX errs, which is not native.
    fn text_symbol_command(&mut self, enc: &str, c: u8) {
        if self.mode == Mode::Math {
            return self.not_native("a text symbol in a formula");
        }
        if self.t1_symbol(enc, c) {
            return;
        }
        self.text_symbol(enc, c);
    }

    fn text_symbol(&mut self, enc: &str, c: u8) {
        // The group a `{` would open: its `}` comes back as a token, which lowers the
        // alignment state as it is read (§324) inside a `tabular` cell.
        self.begin_group(SIMPLE_GROUP);
        self.input.align_state += 1;
        let (key, size) = self.font_key();
        let rest = key.split_once('/').map_or("cmr/m/n", |k| k.1);
        self.select_nfss(&format!("{enc}/{rest}"), size);
        let mut list = vec![self.named_cs(named!("char"))];
        list.extend(c.to_string().chars().map(|d| Token::char(Cat::Other, d)));
        list.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&list);
    }

    /// OT1's `\textunderscore`: `\leavevmode\kern.06em\vbox{\hrule\@width.3em}`.
    fn text_underscore(&mut self) {
        // T1's `\DeclareTextSymbol{\textunderscore}{T1}{95}`.
        if self.t1_symbol("OT1", 95) {
            return;
        }
        if self.leave_vmode() {
            return;
        }
        let (em, ex) = self.em_ex();
        let dimen = |t: &str| scan_dimen_ref(t, em, ex, &|_| None).unwrap_or(0);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, dimen(".06em")) });
        self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
        let rule = self.e.nodes.rule_node([dimen(".3em"), 26214, 0]);
        self.cur.push(rule);
        let start = self.start as usize;
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let b = self.vpack(start, 0, false, d);
        self.pop_nest();
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.cur.push(Node::new(Kind::Box, b as i32));
        self.aux = 1000;
    }

    /// `\leavevmode` at a native's start. In vertical mode the native's token goes back and
    /// a paragraph starts (§1090 `back_input; new_graf`), so `\everypar` (the paragraph
    /// hook) runs before the native does: the caller returns when this is true.
    pub(super) fn leave_vmode(&mut self) -> bool {
        if self.mode != Mode::Vertical {
            return false;
        }
        let t = self.doc_id(self.doc.as_ref().map_or(0, |d| d.current));
        self.input.back(t);
        self.new_graf(true);
        true
    }

    /// Character `c` of font `f` on its own (no ligature or kern with what follows), with
    /// its space factor (§1034).
    fn char_alone(&mut self, f: u16, c: u8) {
        self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
        self.adjust_space_factor(c);
    }

    /// `\kern` in horizontal mode: an explicit kern.
    fn hkern(&mut self, w: i32) {
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
    }

    /// A dimension in the current font's units (`-.36em`, `.5ex`).
    fn dimen_of(&self, text: &str) -> i32 {
        let (em, ex) = self.em_ex();
        scan_dimen_ref(text, em, ex, &|_| None).unwrap_or(0)
    }

    /// `\TeX` (ltlogos.dtx): `T\kern-.1667em\lower.5ex\hbox{E}\kern-.125emX\@`. `\LaTeX`
    /// first: `L\kern-.36em`, an `A` in the script size (`\sf@size`) in a `\vbox` to the
    /// height of `T`, `\kern-.15em`.
    fn logo(&mut self, latex: bool) {
        if self.leave_vmode() {
            return;
        }
        let f = self.cur_font();
        if latex {
            self.char_alone(f, b'L');
            let k = self.dimen_of("-.36em");
            self.hkern(k);
            let height = self.e.fonts.dims(f, b'T').map_or(0, |d| d[1]);
            // `\hbox{\check@mathfonts\fontsize\sf@size\z@\math@fontsfalse\selectfont A}`.
            let (key, size) = self.font_key();
            let script = self.math_sizes(size).1;
            self.begin_group(SIMPLE_GROUP);
            self.select_nfss(&key, script);
            let small = self.cur_font();
            self.end_group(SIMPLE_GROUP);
            let start = self.cur.len();
            self.cur.push(Node { kind: Kind::Char, sub: 0, font: small, a: b'A' as i32, b: 0 });
            let a = self.hpack(start, 0, false);
            let a = self.e.nodes.add_box(a);
            // `\vbox to\ht\z@{\hbox{…}\vss}`.
            self.push_nest(Mode::Vertical, true, IGNORE_DEPTH);
            self.append_to_vlist(a);
            let vss = self.e.nodes.glue_node(Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() }, 0);
            self.cur.push(vss);
            let start = self.start as usize;
            let d = self.dimen_param(BOX_MAX_DEPTH);
            let v = self.vpack(start, height, true, d);
            self.pop_nest();
            let v = self.e.nodes.add_box(BoxRec { vertical: true, ..v });
            self.cur.push(Node::new(Kind::Box, v as i32));
            let k = self.dimen_of("-.15em");
            self.hkern(k);
        }
        self.char_alone(f, b'T');
        let k = self.dimen_of("-.1667em");
        self.hkern(k);
        let start = self.cur.len();
        self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: b'E' as i32, b: 0 });
        let e = self.hpack(start, 0, false);
        let shift = self.dimen_of(".5ex");
        let e = self.e.nodes.add_box(BoxRec { shift, ..e });
        self.cur.push(Node::new(Kind::Box, e as i32));
        let k = self.dimen_of("-.125em");
        self.hkern(k);
        self.char_alone(f, b'X');
        // `\@`.
        self.aux = 1000;
    }

    /// OT1's `\textellipsis`: `.\kern\fontdimen3\font` three times.
    fn ellipsis(&mut self) {
        // amsmath: `\ldots` in math is `\mathellipsis`, `\dots` the dots its next token
        // calls for (`\mdots@`).
        if self.mode == Mode::Math && self.ams() {
            return self.math_dots(false, false);
        }
        if self.leave_vmode() {
            return;
        }
        let f = self.cur_font();
        let stretch = self.e.fonts.params(f).get(2).copied().unwrap_or(0);
        for _ in 0..3 {
            self.char_alone(f, b'.');
            self.hkern(stretch);
        }
    }

    /// `\dots`: `\ldots`; after amsmath in math, the dots its next token calls for.
    fn dots(&mut self) {
        if self.mode == Mode::Math && self.ams() {
            return self.ams_dots(Dots::Auto);
        }
        self.ellipsis()
    }

    /// `\,` outside math: `\leavevmode@ifvmode\kern+.16667em`.
    fn thin_space(&mut self) {
        self.tm_space(0);
    }

    /// `\tmspace{sign}{muskip}{em}`: `\ifmmode\mskip sign muskip\else
    /// \leavevmode@ifvmode\kern sign em\fi`, of [`TM_SPACES`]`[k]` (`\,`, `\!`, `\:`,
    /// `\negmedspace`, `\;`, `\negthickspace`). amsmath's thin spaces are `.1667em`.
    fn tm_space(&mut self, k: usize) {
        let (minus, slot, em, ams_em) = TM_SPACES[k];
        if self.mode == Mode::Math {
            let g = self.e.eqtb.glue(Reg { region: Region::Glue, slot });
            return self.build_mskip(if minus { neg(g) } else { g });
        }
        if self.leave_vmode() {
            return;
        }
        let k = self.dimen_of(if self.ams() { ams_em } else { em });
        self.hkern(if minus { -k } else { k });
    }

    /// `\@`: `\spacefactor\@m{}`.
    fn space_factor_1000(&mut self) {
        if self.mode == Mode::Horizontal {
            self.aux = 1000;
        }
    }

    /// `~`, `\nobreakspace`: `\leavevmode\nobreak\ `.
    fn nobreak_space(&mut self) {
        if self.leave_vmode() {
            return;
        }
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let sf = std::mem::replace(&mut self.aux, 1000);
        self.append_space();
        self.aux = sf;
    }

    /// `\mbox`: `\leavevmode\hbox{#1}`.
    fn mbox(&mut self) {
        // `\leavevmode` first: the command runs again in horizontal mode, its argument
        // still in the input.
        if self.leave_vmode() {
            return;
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut list = Vec::with_capacity(arg.len() + 3);
        list.push(self.named_cs(named!("hbox")));
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.extend_from_slice(&arg);
        list.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&list);
    }

    /// `\newcommand{\name}[n]{text}` is `\long\def\name#1..#n{text}` (`\@yargdef`); the
    /// starred form is not `\long`. The engine's own `\def` reads the text.
    fn new_command(&mut self) {
        let long = !self.next_is('*');
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let Some(&cs) = name.first() else { return };
        let mut n = 0u8;
        if self.next_is('[') {
            while let Some(t) = self.get_token() {
                match t.chr() {
                    ']' => break,
                    c @ '0'..='9' => n = n * 10 + (c as u8 - b'0'),
                    _ => {}
                }
            }
            if self.next_is('[') {
                return self.not_native("\\newcommand with a default argument");
            }
        }
        let mut list = Vec::with_capacity(4 + 2 * n as usize);
        if long {
            list.push(self.named_cs(named!("long")));
        }
        list.push(self.named_cs(named!("def")));
        list.push(cs);
        for k in 1..=n {
            list.push(Token::char(Cat::Param, '#'));
            list.push(Token::char(Cat::Other, (b'0' + k) as char));
        }
        self.back_list(&list);
    }

    /// amsopn's `\operatorname{word}`: `\qopname\newmcodes@ o{word}`, i.e. `\mathop{` with
    /// `'`, `*`, `.`, `-`, `/` and `:` given their text mathcodes, `\kern\z@\operator@font
    /// word}\nolimits@`; starred, with `\nmlimits@` (`\displaylimits`, the default).
    fn operator_name(&mut self) {
        let star = self.next_is('*');
        let mut word = Vec::new();
        self.read_arg(&mut word);
        if self.mode != Mode::Math {
            return self.not_native("\\operatorname outside math");
        }
        // `\nolimits` (limits 2) after the group: nothing between reads the noad's limits.
        self.build_math_group(OP, if star { 0 } else { 2 });
        for (c, v) in [(b'\'', 39), (b'*', 42), (b'.', 0x613a), (b'-', 45), (b'/', 47), (b':', 0x603a)] {
            self.e.eqtb.set_code(Code::Math, c as u32, v, false);
        }
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
        self.set_cur_fam(0);
        word.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&word);
    }

    /// amstext's `\text{…}`: `\mbox` outside math; in math `\text@`, a `\mathchoice` of
    /// four `\hbox{{\everymath{style}\let\f@size<size>\selectfont …}}`, the text at the
    /// current size in display and text style, at `\sf@size` and `\ssf@size` in the script
    /// styles, then `\check@mathfonts`. `\everymath` is LaTeX's user register, after the
    /// frozen one's `\check@mathfonts`. The script copies' `\firstchoice@false` only stops
    /// counter steps, which a counter in the text would need.
    fn ams_text(&mut self) {
        if self.mode != Mode::Math {
            let t = self.named_cs(named!("mbox"));
            return self.input.back(t);
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let counters = [named!("stepcounter"), named!("refstepcounter"), named!("addtocounter"), named!("label"), named!("footnote")].map(|n| self.named_id(n));
        if arg.iter().any(|t| t.cs_id().is_some_and(|c| counters.contains(&c))) {
            return self.not_native("a counter in \\text");
        }
        // `{\mathchoice`, then each branch's `{` before its box ([`Self::text_branch`]).
        self.build_math_group(ORD, 0);
        self.build_math_choice();
        let close = Token::char(Cat::EndGroup, '}');
        let branch = self.doc_id(doc!("rt@textbranch"));
        let mut t = Vec::with_capacity(4 * (arg.len() + 4) + 2);
        for k in 0..4 {
            if k > 0 {
                t.push(Token::char(Cat::BeginGroup, '{'));
            }
            t.extend([branch, Token::char(Cat::Other, char::from(b'0' + k))]);
            t.extend_from_slice(&arg);
            t.extend([close, close, close]);
        }
        t.extend([self.doc_id(doc!("rt@checkmathfonts")), close]);
        self.back_list(&t);
    }

    /// A `\text` branch, its number the digit after it (a `\text` inside the text sets its
    /// own branches in between): `\hbox{{\everymath{\rt@everymath\displaystyle}`
    /// (`\textstyle` in the others) and the size, the display and text branches at the
    /// current size, the script ones at `\sf@size` and `\ssf@size`.
    fn text_branch(&mut self) {
        let k = self.get_token().map_or(0, |t| t.chr() as u8 - b'0');
        // The box's group and the inner one, both closed by `}` tokens.
        self.input.align_state += 2;
        self.open_box(HBOX_GROUP, Ctx::Shift(0), false, 0, false);
        self.begin_group(SIMPLE_GROUP);
        let style = if k == 0 { self.named_cs(named!("displaystyle")) } else { self.named_cs(named!("textstyle")) };
        let every = [self.doc_id(EVERY_MATH), style];
        self.set_every_math(&every);
        self.text_size_of(k.saturating_sub(1));
    }

    /// amsmath's `\mathaccentV`: with no accent in the argument (`\macc@depth` 1), the
    /// plain `\mathaccent"7XYZ{arg}`; the argument is first set in a box that is dropped,
    /// which leaves nothing. An accent anywhere in the argument, through macros too, takes
    /// the nested path (`\macc@nested`), not native.
    fn ams_accent(&mut self) {
        let mut code = Vec::new();
        self.read_arg(&mut code);
        let code: u32 = code.iter().map(|t| t.chr()).collect::<String>().parse().unwrap_or(0);
        if self.mode != Mode::Math {
            return self.error("Please use \\mathaccent for accents in math mode");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if self.has_accent(&arg, 8) {
            return self.not_native("a math accent over an accent (amsmath's \\macc@nested)");
        }
        // `\accentclass@`: 0 inside an OML or OMS alphabet (amsmath's `\noaccents@`), so
        // the accent keeps its own family; 7 elsewhere.
        let fam = self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot: super::engine_noad::CUR_FAM });
        let plain = usize::try_from(fam).ok().and_then(|f| self.doc.as_ref()?.fams.get(f).copied()).is_some_and(|[k, _]| k.starts_with("OML/") || k.starts_with("OMS/"));
        let code = if plain { code & 0xfff } else { code };
        self.build_math_accent(code);
        arg.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&arg);
    }

    /// Whether `toks`, or the bodies of the macros it names (to `depth` levels), name
    /// `\rt@amsaccent` or `\mathaccent`; a macro too deep counts as one.
    fn has_accent(&self, toks: &[Token], depth: u8) -> bool {
        let names = [self.e.cs.get("rt@amsaccent"), self.e.cs.get("mathaccent"), self.e.cs.get("rt@widehat"), self.e.cs.get("rt@widetilde")];
        toks.iter().filter_map(|t| t.cs_id()).any(|c| {
            names.contains(&Some(c))
                || match self.e.eqtb.meaning(c) {
                    Meaning::Macro(m) => depth == 0 || self.has_accent(self.e.toks.get(m.text), depth - 1),
                    Meaning::Doc(i) => DOC.get(i as usize).is_some_and(|d| matches!(d.0, "rt@amsaccent" | "rt@widehat" | "rt@widetilde")),
                    _ => false,
                }
        })
    }

    /// amsmath's dots in math (`\cdots`, `\dotsc`, `\dotsi`, `\dotso`, and `\dots` by
    /// the next token, `\mdots@@`). The next token is looked at as `\futurelet` sees it.
    fn ams_dots(&mut self, kind: Dots) {
        if self.mode != Mode::Math {
            return self.not_native("amsmath's dots outside math");
        }
        let next = self.get_token();
        if let Some(t) = next {
            self.input.back(t);
        }
        let is = |m: &Self, i: usize| next.is_some_and(|t| t.cs_id() == Some(m.named_id(i)));
        let ch = |c: char| next.is_some_and(|t| t.cs_id().is_none() && t.chr() == c && t.cat() == Some(Cat::Other));
        let kind = match kind {
            Dots::Auto => {
                if is(self, named!("boldsymbol")) || next.is_some_and(|t| matches!(t.cs_id().map(|c| self.e.eqtb.meaning(c)), Some(Meaning::Doc(_)))) && !self.extra_known(next) {
                    return self.not_native("\\dots before a command the native job does not classify");
                }
                if ch(',') {
                    Dots::C
                } else if is(self, named!("not")) || "+=<>-*:".chars().any(ch) {
                    Dots::B
                } else {
                    match next.and_then(|t| t.cs_id()).map(|c| self.e.eqtb.meaning(c)) {
                        Some(Meaning::MathCharDef(v)) if matches!(v >> 12, 2 | 3) => Dots::B,
                        Some(Meaning::Macro(m)) if m.flags & super::engine_prim::PROTECTED == 0 => {
                            let body = self.e.toks.get(m.text);
                            let first = body.iter().skip_while(|t| t.cs_id().is_none() && body[0] == Token::END_MATCH && **t == Token::END_MATCH).find(|t| t.cs_id().is_some() || **t != Token::END_MATCH);
                            let first = first.and_then(|t| t.cs_id());
                            if first.is_some_and(|c| self.named_in(c, &[named!("not"), named!("DOTSB"), named!("mathbin"), named!("mathrel")])) {
                                Dots::B
                            } else if first == Some(self.named_id(named!("DOTSI"))) {
                                Dots::I
                            } else {
                                Dots::O
                            }
                        }
                        _ => Dots::O,
                    }
                }
            }
            k => k,
        };
        let extra = self.dots_extra(next);
        match kind {
            // `\@cdots` (`\dotsb@`); `\cdots` itself adds `\,` before `,;.` or what `\extra@`
            // takes (`\extrap@`).
            Dots::Cdots => self.math_dots(true, ch(',') || ch(';') || ch('.') || extra),
            Dots::B => self.math_dots(true, false),
            Dots::I => {
                let g = self.thin_mu_skip();
                self.build_mskip(neg(g));
                self.math_dots(true, false);
            }
            Dots::C => self.math_dots(false, ch(';') || ch('.') || extra),
            _ => self.math_dots(false, extra),
        }
    }

    /// `\mathinner{\cdotp\cdotp\cdotp}` or `\mathellipsis`, then `\,` (`\mskip\thinmuskip`).
    fn math_dots(&mut self, centred: bool, thin: bool) {
        let p = if centred { self.named_cs(named!("cdotp")) } else { self.named_cs(named!("ldotp")) };
        self.build_math_group(INNER, 0);
        let mut t = vec![p, p, p, Token::char(Cat::EndGroup, '}')];
        if thin {
            t.push(self.doc_id(doc!("rt@thinmskip")));
        }
        self.back_list(&t);
    }

    /// amsmath's `\extra@`: the next token a right delimiter (`\rightdelim@`), `$`, or a
    /// macro starting `\DOTSX`. Before `&` the look-ahead meets the next template: an
    /// `array`'s begins with the `$` ending the entry's formula.
    fn dots_extra(&self, next: Option<Token>) -> bool {
        let Some(t) = next else { return false };
        // An entry's end: the kernel's `$`; array.sty's `\textonly@unskip`, a protected macro.
        if t.cs_id().is_some_and(|c| self.e.eqtb.meaning(c) == Meaning::Doc(TAB_V as u16)) {
            return self.doc.as_ref().and_then(|d| d.tabs.last()).is_some_and(|t| t.math && !t.pkg);
        }
        match t.cs_id() {
            None => matches!(t.chr(), ')' | ']') && t.cat() == Some(Cat::Other) || t.cat() == Some(Cat::MathShift),
            Some(c) => {
                let right = [named!("rbrack"), named!("}"), named!("rbrace"), named!("rangle"), named!("rceil"), named!("rfloor"), named!("rgroup"), named!("rmoustache"), named!("right"), named!("bigr"), named!("biggr"), named!("Bigr"), named!("Biggr")];
                let dotsx = self.named_id(named!("DOTSX"));
                self.named_in(c, &right) || matches!(self.e.eqtb.meaning(c), Meaning::Macro(m) if m.flags & super::engine_prim::PROTECTED == 0 && self.e.toks.get(m.text).iter().find_map(|x| x.cs_id()) == Some(dotsx))
            }
        }
    }

    /// Whether a native the dots test may meet is one it knows (`\right`, `\}`).
    fn extra_known(&self, next: Option<Token>) -> bool {
        let known = [named!("}"), named!("right"), named!("{"), named!("left"), named!(","), named!(";"), named!("quad"), named!("qquad"), named!("text"), named!("\\"), named!("end"), named!("]"), named!(")"), named!("label"), named!("nonumber"), named!("notag")];
        next.and_then(|t| t.cs_id()).is_some_and(|c| self.named_in(c, &known))
    }

    /// Whether control sequence `c` is one of the [`NAMED`] entries `list`.
    fn named_in(&self, c: u32, list: &[usize]) -> bool {
        list.iter().any(|&i| self.named_id(i) == c)
    }

    /// amsmath's `\colon`: `\nobreak\mskip2mu\mathpunct{}\nonscript\mkern-\thinmuskip{:}
    /// \mskip6muplus1mu\relax`.
    fn ams_colon(&mut self) {
        self.build_penalty(10000);
        self.build_mskip(Glue { width: 2 << 16, ..Glue::default() });
        self.build_math_group(PUNCT, 0);
        let t = [Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@colonrest"))];
        self.back_list(&t);
    }

    /// `\colon` after its `\mathpunct{}`: `\nonscript\mkern-\thinmuskip{:}`, then
    /// `\mskip6mu plus1mu` after the group (`\rt@colonend`).
    fn ams_colon_rest(&mut self) {
        self.build_nonscript();
        let w = self.thin_mu_skip().width;
        self.build_mkern(-w);
        self.build_math_group(ORD, 0);
        let t = [Token::char(Cat::Other, ':'), Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@colonend"))];
        self.back_list(&t);
    }

    /// amsmath's `\big` family, `\bBigg@{factor}{delim}`: `{\@mathmeasure\z@{\nulldelimiterspace
    /// \z@}{\left delim\vcenter to factor\big@size{}\right.}\box\z@}`, the measuring math
    /// with `\mathsurround` 0 and no `\everymath`.
    fn ams_big(&mut self, factor: &str) {
        if self.mode != Mode::Math {
            return self.not_native("amsmath's \\big outside math");
        }
        self.big_open(factor, ORD);
    }

    /// amsmath's `\genfrac{l}{r}{thickness}{style}{num}{den}` (pdfTeX's): `\@genfrac
    /// \@mathstyle{style}\@@over` (`\@@above` for a thickness, `...withdelims` for
    /// delimiters), which is `{style{\begingroup num\endgroup\@@over l r thickness\relax
    /// den}}`; `\@mathstyle` is `\relax` for no style, else `\ifcase` of `\displaystyle`,
    /// `\textstyle`, `\scriptstyle`, `\scriptscriptstyle` (and past 3). `\dfrac`, `\tfrac`,
    /// `\binom`, `\dbinom`, `\tbinom` are `\genfrac` with their arguments (`\binom`'s `\z@`
    /// read as `0pt`).
    fn genfrac(&mut self, l: &[Token], r: &[Token], thick: &[Token], style: &[Token]) {
        let style = if style.is_empty() {
            self.named_cs(named!("relax"))
        } else {
            let digits: String = style.iter().map(|t| if t.cs_id().is_none() { t.chr() } else { 'x' }).collect();
            let Ok(n) = digits.trim().parse::<u32>() else { return self.not_native("a \\genfrac style that is not a number") };
            self.named_cs([named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")][n.min(3) as usize])
        };
        let prim = match (thick.is_empty(), l.is_empty() && r.is_empty()) {
            (true, true) => named!("over"),
            (true, false) => named!("overwithdelims"),
            (false, true) => named!("above"),
            (false, false) => named!("abovewithdelims"),
        };
        let (mut num, mut den) = (Vec::new(), Vec::new());
        self.read_arg(&mut num);
        self.read_arg(&mut den);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![open, style, open, self.named_cs(named!("begingroup"))];
        t.extend(num);
        t.extend([self.named_cs(named!("endgroup")), self.named_cs(prim)]);
        t.extend(l.iter().chain(r).chain(thick));
        t.push(self.named_cs(named!("relax")));
        t.extend(den);
        t.extend([close, close]);
        self.back_list(&t);
    }

    /// latex.ltx's `\smash` and `\ph@nt` (`\phantom`, `\vphantom`, `\hphantom`), and
    /// amsmath's `\smash[t|b|tb]`. In math `\mathpalette` gives a `\mathchoice` of
    /// `\setbox\z@\hbox{$\m@th style{arg}$}` (then `{}` for `\smash`) and the finish; else
    /// `\setbox\z@\hbox{arg}` and the finish. `\finsm@sh` is `\ht\z@\z@ \dp\z@\z@
    /// \leavevmode@ifvmode\box\z@`, `t` or `b` keeping one; `\finph@nt` is `\setbox\tw@
    /// \null`, the kept dimensions copied, `\leavevmode@ifvmode\box\tw@`.
    fn smash(&mut self, kind: u8) {
        let (mut top, mut bottom) = (true, true);
        if kind == SMASH && self.ams() && self.next_is('[') {
            let opt: String = self.bracket_arg().iter().map(|t| t.chr()).collect();
            match opt.trim() {
                "t" => bottom = false,
                "b" => top = false,
                "tb" | "bt" => {}
                _ => return self.not_native("\\smash with that option"),
            }
            // amsmath's `\edef\finsm@sh`, local, is what `\relbar`'s `\mathsm@sh` finishes
            // with until the group ends: kept as the meaning of `\finsm@sh`, a `\chardef` of
            // the kept dimensions (bit 1 the height, bit 2 the depth).
            let id = self.e.cs.intern("finsm@sh");
            self.e.eqtb.set_meaning(id, Meaning::CharDef(top as u32 | (bottom as u32) << 1), false);
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let fin = self.smash_finish(kind, top, bottom);
        let mut t = Vec::new();
        if kind == SMASH {
            t.push(self.named_cs(named!("relax")));
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        if self.mode == Mode::Math {
            t.push(self.named_cs(named!("mathchoice")));
            for style in [named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")] {
                t.push(open);
                t.extend(self.smash_box(Some(self.named_cs(style)), &arg, kind == SMASH));
                t.extend(fin.iter().copied());
                t.push(close);
            }
        } else {
            t.extend(self.smash_box(None, &arg, false));
            t.extend(fin);
        }
        self.back_list(&t);
    }

    /// The modulo commands, as their tokens (`\operator@font` is `\mathgroup\symoperators`,
    /// family 0; `\allowbreak` is `\penalty\z@`):
    /// - `\bmod`: `\nonscript\mskip-\medmuskip\mkern5mu\mathbin{\operator@font mod}
    ///   \penalty900\mkern5mu\nonscript\mskip-\medmuskip`, the same in both;
    /// - latex.ltx's `\pmod{n}`: `\allowbreak\mkern18mu({\operator@font mod}\,\,n)`;
    /// - amsmath's `\pod{n}`: `\allowbreak\if@display\mkern18mu\else\mkern8mu\fi(n)`, its
    ///   `\pmod{n}` `\pod{{\operator@font mod}\mkern6mu n}`, and `\mod{n}`
    ///   `\allowbreak\if@display\mkern18mu\else\mkern12mu\fi{\operator@font mod}\,\,n`.
    ///
    /// `\if@display` is true inside a display formula, at any depth (`\everydisplay`).
    fn mod_op(&mut self, kind: u8) {
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let word = |w: &str| w.chars().map(ch).collect::<Vec<_>>();
        let (open, close, space) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::Space, ' '));
        let operator_mod = [open, self.named_cs(named!("fam")), ch('0'), space, ch('m'), ch('o'), ch('d'), close];
        let mut t = Vec::new();
        if kind == BMOD {
            let skip = [self.named_cs(named!("nonscript")), self.named_cs(named!("mskip")), ch('-'), self.named_cs(named!("medmuskip"))];
            t.extend(skip);
            t.push(self.named_cs(named!("mkern")));
            t.extend(word("5mu"));
            t.push(self.named_cs(named!("mathbin")));
            t.extend(operator_mod);
            t.push(self.named_cs(named!("penalty")));
            t.extend(word("900"));
            t.push(space);
            t.push(self.named_cs(named!("mkern")));
            t.extend(word("5mu"));
            t.extend(skip);
            return self.back_list(&t);
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let ams = self.ams();
        let display = self.mode == Mode::Math && !self.inner || self.nest.iter().any(|l| l.mode == Mode::Math && !l.inner);
        t.push(self.named_cs(named!("penalty")));
        t.extend([ch('0'), space, self.named_cs(named!("mkern"))]);
        t.extend(word(match (ams, kind, display) {
            (false, _, _) | (true, _, true) => "18mu",
            (true, MOD, false) => "12mu",
            (true, _, false) => "8mu",
        }));
        let thin = self.named_cs(named!(","));
        match (ams, kind) {
            (false, _) => {
                t.push(ch('('));
                t.extend(operator_mod);
                t.extend([thin, thin]);
                t.extend(arg);
                t.push(ch(')'));
            }
            (true, MOD) => {
                t.extend(operator_mod);
                t.extend([thin, thin]);
                t.extend(arg);
            }
            (true, _) => {
                t.push(ch('('));
                if kind == PMOD {
                    t.extend(operator_mod);
                    t.push(self.named_cs(named!("mkern")));
                    t.extend(word("6mu"));
                }
                t.extend(arg);
                t.push(ch(')'));
            }
        }
        self.back_list(&t);
    }

    /// fontmath.ltx's `\vdots`: `\vbox{\baselineskip4\p@ \lineskiplimit\z@ \kern6\p@
    /// \hbox{.}\hbox{.}\hbox{.}}`.
    fn vdots(&mut self) {
        let cs = |i: usize| self.named_cs(i);
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let (open, close, space) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::Space, ' '));
        let mut t = vec![cs(named!("vbox")), open, cs(named!("baselineskip"))];
        t.extend("4pt".chars().map(ch));
        t.extend([space, cs(named!("lineskiplimit"))]);
        t.extend(ZERO_PT);
        t.extend([space, cs(named!("kern"))]);
        t.extend("6pt".chars().map(ch));
        t.push(space);
        for _ in 0..3 {
            t.extend([cs(named!("hbox")), open, ch('.'), close]);
        }
        t.push(close);
        self.back_list(&t);
    }

    /// fontmath.ltx's `\ddots`: `\mathinner{\mkern1mu\raise7\p@\vbox{\kern7\p@\hbox{.}}
    /// \mkern2mu\raise4\p@\hbox{.}\mkern2mu\raise\p@\hbox{.}\mkern1mu}`.
    fn ddots(&mut self) {
        let cs = |i: usize| self.named_cs(i);
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let (open, close, space) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::Space, ' '));
        let dot = [cs(named!("hbox")), open, ch('.'), close];
        let mkern = |t: &mut Vec<Token>, n: char| {
            t.extend([cs(named!("mkern")), ch(n), ch('m'), ch('u'), space]);
        };
        let raise = |t: &mut Vec<Token>, n: char| {
            t.extend([cs(named!("raise")), ch(n), ch('p'), ch('t'), space]);
        };
        let mut t = vec![cs(named!("mathinner")), open];
        mkern(&mut t, '1');
        raise(&mut t, '7');
        t.extend([cs(named!("vbox")), open, cs(named!("kern")), ch('7'), ch('p'), ch('t'), space]);
        t.extend(dot);
        t.push(close);
        mkern(&mut t, '2');
        raise(&mut t, '4');
        t.extend(dot);
        mkern(&mut t, '2');
        raise(&mut t, '1');
        t.extend(dot);
        mkern(&mut t, '1');
        t.push(close);
        self.back_list(&t);
    }

    /// amsmath's `\overset{a}{b}` (1), `\underset{a}{b}` (2) and `\overunderset{a}{c}{b}`
    /// (3): `\binrel@{b}\binrel@@{\mathop{\kern\z@ b}\limits^{a}}` (`_` for under).
    /// See [`Self::binrel_tokens`].
    fn over_under_set(&mut self, kind: u8) {
        let (mut a, mut c, mut b) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut a);
        if kind == 3 {
            self.read_arg(&mut c);
        }
        self.read_arg(&mut b);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = self.binrel_tokens(&b);
        t.extend([open, self.named_cs(named!("mathop")), open, self.named_cs(named!("kern"))]);
        t.extend(ZERO_PT);
        t.push(Token::char(Cat::Space, ' '));
        t.extend(b.iter().copied());
        t.extend([close, self.named_cs(named!("limits"))]);
        let script = |t: &mut Vec<Token>, cat: Cat, c: char, arg: &[Token]| {
            t.extend([Token::char(cat, c), open]);
            t.extend(arg.iter().copied());
            t.push(close);
        };
        match kind {
            1 => script(&mut t, Cat::Superscript, '^', &a),
            2 => script(&mut t, Cat::Subscript, '_', &a),
            _ => {
                script(&mut t, Cat::Superscript, '^', &a);
                script(&mut t, Cat::Subscript, '_', &c);
            }
        }
        t.push(close);
        self.back_list(&t);
    }

    /// amsmath's `\substack{rows}`: `\subarray{c}rows\endsubarray`. A `\vcenter` with
    /// `\baselineskip` the `\fontdimen10` plus `\fontdimen12` of family 2's script font,
    /// `\lineskip` and `\lineskiplimit` three times family 3's script `\fontdimen8`, around
    /// `\ialign{\hfil$\m@th\scriptstyle#$\hfil\cr rows\crcr}`. Each `\\`, `\\*`, `\\[dim]`
    /// is amsmath's `\math@cr` (`\Let@`): `\cr\noalign{\vskip dim\relax}`, `dim` 0pt when not
    /// given. An `&` in the rows is not native.
    fn substack(&mut self) {
        if self.mode != Mode::Math {
            return self.not_native("\\substack outside math");
        }
        let mut rows = Vec::new();
        self.read_arg(&mut rows);
        if rows.iter().any(|t| t.cat() == Some(Cat::AlignTab)) {
            return self.not_native("an & in \\substack");
        }
        let newline = self.named_id(named!("\\"));
        let other = |t: Option<&Token>, c: char| t.is_some_and(|t| *t == Token::char(Cat::Other, c));
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut body = Vec::with_capacity(rows.len());
        let mut i = 0;
        while let Some(&r) = rows.get(i) {
            i += 1;
            if r.cs_id() != Some(newline) {
                body.push(r);
                continue;
            }
            if other(rows.get(i), '*') {
                i += 1;
            }
            let mut skip = &ZERO_PT[..];
            if other(rows.get(i), '[') {
                let Some(j) = rows[i..].iter().position(|t| other(Some(t), ']')) else { return self.not_native("an unclosed \\\\[") };
                skip = &rows[i + 1..i + j];
                i += j + 1;
            }
            body.extend([self.named_cs(named!("cr")), self.named_cs(named!("noalign")), open, self.named_cs(named!("vskip"))]);
            body.extend(skip.iter().copied());
            body.extend([self.named_cs(named!("relax")), close]);
        }
        let param = |m: &Self, fam: u32, n: usize| match m.e.eqtb.meaning(super::FAM_BASE_ID + 16 + fam) {
            Meaning::Font(f) => m.e.fonts.params(f).get(n - 1).copied().unwrap_or(0),
            _ => 0,
        };
        let (baseline, lineskip) = (param(self, 2, 10) + param(self, 2, 12), 3 * param(self, 3, 8));
        let cs = |i: usize| self.named_cs(i);
        let (shift, space) = (Token::char(Cat::MathShift, '$'), Token::char(Cat::Space, ' '));
        let mut t = vec![cs(named!("vcenter")), open, cs(named!("baselineskip"))];
        Self::sp_tokens(baseline, &mut t);
        t.extend([space, cs(named!("lineskip"))]);
        Self::sp_tokens(lineskip, &mut t);
        t.extend([space, cs(named!("lineskiplimit")), cs(named!("lineskip"))]);
        self.ialign_tokens(&mut t);
        t.extend([cs(named!("hfil")), shift, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        t.extend([cs(named!("scriptstyle")), Token::char(Cat::Param, '#'), shift, cs(named!("hfil")), cs(named!("cr"))]);
        t.extend(body);
        t.extend([cs(named!("crcr")), close, close]);
        self.back_list(&t);
    }

    /// amsmath's `\xrightarrow[below]{above}` (0) and `\xleftarrow` (1): `\ext@arrow` with
    /// `\rightarrowfill@` or `\leftarrowfill@` and the kerns `0359` or `3095`.
    /// `\mathrel{\mathop{\setbox0\hbox{fill}\setbox2\vbox{\m@th\hbox{$\scriptstyle\mkern a mu
    /// {below}\mkern b mu$}\hbox{…above…}\copy0}\hbox to\wd2{\unhbox0}}\limits^{…}_{…}}`,
    /// the fill `\arrowfill@`'s `$\m@th…\displaystyle L\mkern-7mu\cleaders\hbox{$\displaystyle
    /// \mkern-2mu\relbar\mkern-2mu$}\hfill\mkern-7mu R$`, each script only when its argument
    /// is not empty (`\@ifnotempty`).
    fn ext_arrow(&mut self, left: bool) {
        if self.mode != Mode::Math {
            return self.not_native("an amsmath extensible arrow outside math");
        }
        let mut below = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        // A delimited argument loses the braces around all of it (§400).
        if below.len() >= 2 && below[0].cat() == Some(Cat::BeginGroup) && below[below.len() - 1].cat() == Some(Cat::EndGroup) && below[1..below.len() - 1].iter().try_fold(0i32, |d, t| match t.cat() {
            Some(Cat::BeginGroup) => Some(d + 1),
            Some(Cat::EndGroup) => (d > 0).then(|| d - 1),
            _ => Some(d),
        }) == Some(0) {
            below = below[1..below.len() - 1].to_vec();
        }
        let mut above = Vec::new();
        self.read_arg(&mut above);
        let blank = |a: &[Token]| !a.is_empty() && a.iter().all(|t| t.cat() == Some(Cat::Space));
        if blank(&below) || blank(&above) {
            return self.not_native("an extensible arrow's argument of spaces");
        }
        let cs = |i: usize| self.named_cs(i);
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
        let mkern = |t: &mut Vec<Token>, v: &str| self.mkern_tokens(t, v);
        let (a, b) = if left { ("9", "5") } else { ("5", "9") };
        let mut t = vec![cs(named!("mathrel")), open, cs(named!("mathop")), open, cs(named!("setbox")), ch('0'), cs(named!("hbox")), open];
        self.arrow_fill(&mut t, if left { 1 } else { 0 }, named!("displaystyle"));
        t.extend([close, cs(named!("setbox")), ch('2'), cs(named!("vbox")), open, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        for arg in [&below, &above] {
            t.extend([cs(named!("hbox")), open, shift, cs(named!("scriptstyle"))]);
            mkern(&mut t, a);
            t.push(open);
            t.extend(arg.iter().copied());
            t.push(close);
            mkern(&mut t, b);
            t.extend([shift, close]);
        }
        t.extend([cs(named!("copy")), ch('0'), close, cs(named!("hbox")), ch('t'), ch('o'), cs(named!("wd")), ch('2'), open, cs(named!("unhbox")), ch('0'), close, close, cs(named!("limits"))]);
        for (arg, cat, c) in [(&above, Cat::Superscript, '^'), (&below, Cat::Subscript, '_')] {
            if arg.is_empty() {
                continue;
            }
            t.extend([Token::char(cat, c), open]);
            if left {
                mkern(&mut t, "3");
            }
            t.extend(arg.iter().copied());
            if !left {
                mkern(&mut t, "3");
            }
            t.push(close);
        }
        t.push(close);
        self.back_list(&t);
    }

    /// amsgen's `\ex@` (`\compute@ex@` in `\every@math@size`), at the current size: 1.5pt
    /// above 20pt, else 1pt plus or minus 1pt less `\vfuzz`, which starts at 1pt and is
    /// multiplied by .97 once per point of twice the size's distance from 10pt (less 1000sp).
    fn ams_ex(&mut self) -> i32 {
        const PT: i32 = 1 << 16;
        let (_, size) = self.font_key();
        let Some(f) = super::engine_graphics::times(size, PT) else { return PT };
        if f > 20 * PT {
            return PT * 3 / 2;
        }
        let d = 2 * (10 * PT - f);
        let (minus, mut d) = (d > 0, d.abs() - 1000);
        let mut vfuzz = PT;
        while d > 0 {
            vfuzz = super::engine_graphics::times(".97", vfuzz).unwrap_or(vfuzz);
            d -= PT;
        }
        let d = PT - vfuzz;
        if minus { PT - d } else { PT + d }
    }

    /// `\Let@` for `smallmatrix`: `\\` is [`Self::math_cr`] until the `\vcenter` ends.
    fn let_math_cr(&mut self) {
        let id = self.named_id(named!("\\"));
        self.e.eqtb.set_meaning(id, Meaning::Doc(doc!("rt@mathcr") as u16), false);
    }

    /// amsmath's `\math@cr` with `\math@cr@@@` the `\cr` (`\restore@math@cr`): `\@eqpen` set
    /// globally (`\interdisplaylinepenalty`, 10000 under amsmath, as is `\\*`'s), then
    /// `\cr\noalign{\vskip dim\relax}`, `dim` from `[dim]` or 0pt. The look-ahead counts as
    /// inside braces (`\iffalse{\fi`), so an `&` after it is not taken as the cell's end.
    fn math_cr(&mut self) {
        self.input.align_state += 1;
        self.next_is('*');
        let skip = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        self.input.align_state -= 1;
        if let Some(doc) = self.doc.as_mut() {
            doc.eqpen = 10000;
        }
        let mut t = vec![self.named_cs(named!("cr")), self.named_cs(named!("noalign")), Token::char(Cat::BeginGroup, '{'), self.named_cs(named!("vskip"))];
        match skip {
            Some(arg) => t.extend(arg),
            None => t.extend(ZERO_PT),
        }
        t.extend([self.named_cs(named!("relax")), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&t);
    }

    /// amsmath's `\hdotsfor[k]{n}` as an entry's first token: the column count `n` and
    /// `\hdots@for`'s text, `\m@th\dotsspace@1.5mu\mkern-k\dotsspace@\xleaders\hbox{$\m@th
    /// \mkern k\dotsspace@.\mkern k\dotsspace@$}\hfill\mkern-k\dotsspace@`, with `k` times
    /// 1.5mu written out (`print_scaled`).
    pub(super) fn hdots_for(&mut self) -> Option<(String, Vec<Token>)> {
        let k = if self.next_is('[') { self.bracket_arg() } else { vec![Token::char(Cat::Other, '1')] };
        let mut n = Vec::new();
        self.read_arg(&mut n);
        let digits = |a: &[Token]| a.iter().all(|t| t.cs_id().is_none() && (t.chr().is_ascii_digit() || t.chr() == '.')).then(|| a.iter().map(|t| t.chr()).collect::<String>());
        let (Some(k), Some(n)) = (digits(&k), digits(&n)) else {
            self.not_native("an \\hdotsfor with a computed count or spacing");
            return None;
        };
        let Some(v) = super::engine_graphics::times(&k, 3 << 15) else {
            self.not_native("an \\hdotsfor spacing out of range");
            return None;
        };
        let mut mu = String::new();
        super::engine_value::print_scaled(v, &mut mu);
        let cs = |i: usize| self.named_cs(i);
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
        let (minus, plus) = (format!("-{mu}"), mu.clone());
        let mut t = vec![cs(named!("mathsurround"))];
        t.extend(ZERO_PT);
        self.mkern_tokens(&mut t, &minus);
        t.extend([cs(named!("xleaders")), cs(named!("hbox")), open, shift, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        self.mkern_tokens(&mut t, &plus);
        t.push(ch('.'));
        self.mkern_tokens(&mut t, &plus);
        t.extend([shift, close, cs(named!("hfill"))]);
        self.mkern_tokens(&mut t, &minus);
        Some((n, t))
    }

    /// `\mkern<v>mu`.
    fn mkern_tokens(&self, t: &mut Vec<Token>, v: &str) {
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        t.push(self.named_cs(named!("mkern")));
        t.extend(v.chars().map(ch));
        t.extend([ch('m'), ch('u')]);
    }

    /// `\ialign` up to its `{`: `\everycr{}\tabskip0pt\halign{`.
    fn ialign_tokens(&self, t: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        t.extend([self.named_cs(named!("everycr")), open, close, self.named_cs(named!("tabskip"))]);
        t.extend(ZERO_PT);
        t.extend([self.named_cs(named!("halign")), open]);
    }

    /// amsmath's `\rightarrowfill@` (0), `\leftarrowfill@` (1) and `\leftrightarrowfill@` (2)
    /// in `style`, `\arrowfill@`: `$\m@th\thickmuskip0mu\medmuskip\thickmuskip\thinmuskip
    /// \thickmuskip\relax style L\mkern-7mu\cleaders\hbox{$style\mkern-2mu\relbar\mkern-2mu$}
    /// \hfill\mkern-7mu R$`.
    fn arrow_fill(&self, t: &mut Vec<Token>, kind: u8, style: usize) {
        let cs = |i: usize| self.named_cs(i);
        let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
        let (l, r) = match kind {
            0 => (named!("relbar"), named!("rightarrow")),
            1 => (named!("leftarrow"), named!("relbar")),
            _ => (named!("leftarrow"), named!("rightarrow")),
        };
        t.extend([shift, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        t.push(cs(named!("thickmuskip")));
        t.extend([Token::char(Cat::Other, '0'), Token::char(Cat::Letter, 'm'), Token::char(Cat::Letter, 'u')]);
        t.extend([cs(named!("medmuskip")), cs(named!("thickmuskip")), cs(named!("thinmuskip")), cs(named!("thickmuskip")), cs(named!("relax")), cs(style)]);
        // amsmath's `\relbar` is `\mathrel{\mathpalette\mathsm@sh\std@minus}`; the style
        // is known here, so `\mathsm@sh` is called with it, without the `\mathchoice`: the
        // choice would become a style node of the same style and the chosen list (§731),
        // and a style node adds no spacing (§762). The pre-scan sends a document that
        // redefines `\relbar` to the engine (`READ_INTERNALLY`).
        let relbar = |t: &mut Vec<Token>| t.extend([cs(named!("mathrel")), open, self.doc_id(doc!("mathsm@sh")), cs(style), open, Token::cs(self.e.cs.get("std@minus").unwrap_or(0)), close, close]);
        if l == named!("relbar") { relbar(t) } else { t.push(cs(l)) }
        self.mkern_tokens(t, "-7");
        t.extend([cs(named!("cleaders")), cs(named!("hbox")), open, shift, cs(style)]);
        self.mkern_tokens(t, "-2");
        relbar(t);
        self.mkern_tokens(t, "-2");
        t.extend([shift, close, cs(named!("hfill"))]);
        self.mkern_tokens(t, "-7");
        if r == named!("relbar") { relbar(t) } else { t.push(cs(r)) }
        t.push(shift);
    }

    /// `\overrightarrow{a}` (0), `\overleftarrow` (1) and amsmath's `\overleftrightarrow` (2).
    /// With amsmath, `\mathpalette{\overarrow@\rightarrowfill@}{a}`: in each style
    /// `\vbox{\ialign{#\crcr fill\crcr\noalign{\nointerlineskip}$\m@th\hfil style a\hfil$\crcr}}`.
    /// Without, fontmath's `\vbox{\m@th\ialign{#\crcr\rightarrowfill\crcr\noalign{\kern-\p@
    /// \nointerlineskip}$\hfil\displaystyle{a}\hfil$\crcr}}`, the fill
    /// `$\m@th\smash-\mkern-7mu\cleaders\hbox{$\mkern-2mu\smash-\mkern-2mu$}\hfill\mkern-7mu
    /// \mathord\rightarrow$` (`\mathord\leftarrow` first for the left one).
    fn over_arrow(&mut self, kind: u8) {
        if self.mode != Mode::Math {
            return self.not_native("an over-arrow outside math");
        }
        let mut a = Vec::new();
        self.read_arg(&mut a);
        let cs = |i: usize| self.named_cs(i);
        let (open, close, shift, param) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'), Token::char(Cat::Param, '#'));
        let nointerlineskip = |t: &mut Vec<Token>| {
            t.push(cs(named!("prevdepth")));
            t.extend("-1000".chars().map(|c| Token::char(Cat::Other, c)));
            t.extend([Token::char(Cat::Letter, 'p'), Token::char(Cat::Letter, 't'), Token::char(Cat::Space, ' ')]);
        };
        let mut t = Vec::new();
        if self.ams() {
            t.push(cs(named!("mathchoice")));
            for style in [named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")] {
                t.extend([open, cs(named!("vbox")), open]);
                self.ialign_tokens(&mut t);
                t.extend([param, cs(named!("crcr"))]);
                self.arrow_fill(&mut t, kind, style);
                t.extend([cs(named!("crcr")), cs(named!("noalign")), open]);
                nointerlineskip(&mut t);
                t.extend([close, shift, cs(named!("mathsurround"))]);
                t.extend(ZERO_PT);
                t.extend([cs(named!("hfil")), cs(style)]);
                t.extend(a.iter().copied());
                t.extend([cs(named!("hfil")), shift, cs(named!("crcr")), close, close, close]);
            }
        } else {
            if kind == 2 {
                return self.not_native("\\overleftrightarrow without amsmath");
            }
            let smash = |m: &Self, t: &mut Vec<Token>| t.extend([m.named_cs(named!("smash")), Token::char(Cat::Other, '-')]);
            t.extend([cs(named!("vbox")), open, cs(named!("mathsurround"))]);
            t.extend(ZERO_PT);
            self.ialign_tokens(&mut t);
            t.extend([param, cs(named!("crcr")), shift, cs(named!("mathsurround"))]);
            t.extend(ZERO_PT);
            if kind == 0 {
                smash(self, &mut t);
            } else {
                t.extend([cs(named!("mathord")), cs(named!("leftarrow"))]);
            }
            self.mkern_tokens(&mut t, "-7");
            t.extend([cs(named!("cleaders")), cs(named!("hbox")), open, shift]);
            self.mkern_tokens(&mut t, "-2");
            smash(self, &mut t);
            self.mkern_tokens(&mut t, "-2");
            t.extend([shift, close, cs(named!("hfill"))]);
            self.mkern_tokens(&mut t, "-7");
            if kind == 0 {
                t.extend([cs(named!("mathord")), cs(named!("rightarrow"))]);
            } else {
                smash(self, &mut t);
            }
            t.extend([shift, cs(named!("crcr")), cs(named!("noalign")), open, cs(named!("kern"))]);
            t.extend("-1pt".chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)));
            nointerlineskip(&mut t);
            t.extend([close, shift, cs(named!("hfil")), cs(named!("displaystyle")), open]);
            t.extend(a.iter().copied());
            t.extend([close, cs(named!("hfil")), shift, cs(named!("crcr")), close, close]);
        }
        self.back_list(&t);
    }

    /// fontmath.ltx's `\notin` (`\mathrel{\m@th\mathpalette\c@ncel\in}`, with `\c@ncel` an
    /// `\ooalign` of `\hfil s\mkern1mu/\hfil` over `s\in`) and `\cong`
    /// (`\mathrel{\mathpalette\@vereq\sim}`: `\lower.5\p@\vbox` of `\sim` over `=`), each
    /// style's branch of `\mathpalette` written out. `\oalign`'s `\leavevmode` does nothing
    /// in math.
    fn palette_relation(&mut self, cong: bool) {
        if self.mode != Mode::Math {
            return self.not_native("a math relation outside math");
        }
        let mut text = String::from(if cong { "\\mathrel{\\mathchoice" } else { "\\mathrel{\\mathsurround0pt\\mathchoice" });
        for st in ["\\displaystyle", "\\textstyle", "\\scriptstyle", "\\scriptscriptstyle"] {
            if cong {
                text += &format!("{{\\lower.5pt\\vbox{{\\lineskiplimit16383.99999pt\\lineskip-.5pt\\everycr{{}}\\tabskip0pt\\halign{{$\\mathsurround0pt{st}\\hfil#\\hfil$\\crcr\\sim\\crcr=\\crcr}}}}}}");
            } else {
                text += &format!("{{\\mathsurround0pt\\lineskiplimit-16383.99999pt\\vtop{{\\baselineskip0pt\\lineskip.25ex\\everycr{{}}\\tabskip0pt\\halign{{#\\crcr$\\hfil{st}\\mkern1mu/\\hfil$\\crcr${st}\\in$\\crcr\\crcr}}}}}}");
            }
        }
        text.push('}');
        let t = self.tex_tokens(&text);
        self.back_list(&t);
    }

    /// fontmath.ltx's `\underbrace{a}` (`\mathop{\vtop{\m@th\ialign{##\crcr $\hfil\displaystyle{a}
    /// \hfil$\crcr \noalign{\kern3\p@\nointerlineskip}\upbracefill\crcr\noalign{\kern3\p@}}}}
    /// \limits`) and `\overbrace{a}` (a `\vbox`, `\downbracefill` above), the fills
    /// `$\m@th\setbox\z@\hbox{$\braceld$}` and the four brace pieces with `\leaders\vrule`.
    /// mathtools's (mathtools.sty:441-450) kern `.7\fontdimen5\textfont2` next to the
    /// argument and `.5\fontdimen5\textfont2` outside the brace instead of `3\p@`.
    fn brace_over_under(&mut self, under: bool) {
        if self.mode != Mode::Math {
            return self.not_native("a brace outside math");
        }
        let mut a = Vec::new();
        self.read_arg(&mut a);
        let fill = |l: &str, r: &str, l2: &str, r2: &str| format!("$\\mathsurround0pt\\setbox0\\hbox{{$\\braceld$}}\\{l}\\leaders\\vrule height\\ht0 depth0pt\\hfill\\{r}\\{l2}\\leaders\\vrule height\\ht0 depth0pt\\hfill\\{r2}$");
        let (near, far) = if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"mathtools")) { (".7\\fontdimen5\\textfont2 ", ".5\\fontdimen5\\textfont2 ") } else { ("3pt", "3pt") };
        let (head, tail) = if under {
            ("\\mathop{\\vtop{\\mathsurround0pt\\everycr{}\\tabskip0pt\\halign{#\\crcr$\\hfil\\displaystyle{".to_string(), format!("}}\\hfil$\\crcr\\noalign{{\\kern{near}\\prevdepth-1000pt}}{}\\crcr\\noalign{{\\kern{far}}}}}}}}}\\limits", fill("bracelu", "bracerd", "braceld", "braceru")))
        } else {
            (format!("\\mathop{{\\vbox{{\\mathsurround0pt\\everycr{{}}\\tabskip0pt\\halign{{#\\crcr\\noalign{{\\kern{far}}}{}\\crcr\\noalign{{\\kern{near}\\prevdepth-1000pt}}$\\hfil\\displaystyle{{", fill("braceld", "braceru", "bracelu", "bracerd")), "}\\hfil$\\crcr}}}\\limits".to_string())
        };
        let mut t = self.tex_tokens(&head);
        t.extend(a);
        let tail = self.tex_tokens(&tail);
        t.extend(tail);
        self.back_list(&t);
    }

    /// amsbsy's `\binrel@{b}`: `b` measured in text style between empty atoms with
    /// `\thinmuskip` 0, `\medmuskip` −1mu, `\thickmuskip` 1mu, less `b` alone; then
    /// `\binrel@@` is `\mathbin` for a negative width, `\mathrel` for a positive one, else
    /// `\relax`. As tokens: `\begingroup\setbox0\hbox{…}\ifdim\wd0<0pt \aftergroup\mathbin
    /// \else\ifdim\wd0>0pt \aftergroup\mathrel\fi\fi\endgroup`, the class landing before
    /// the `{` that follows (where `\relax` would make no difference).
    fn binrel_tokens(&self, b: &[Token]) -> Vec<Token> {
        let cs = |i: usize| self.named_cs(i);
        let (open, close, shift, space) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'), Token::char(Cat::Space, ' '));
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let mut t = vec![cs(named!("begingroup")), cs(named!("setbox")), ch('0'), cs(named!("hbox")), open];
        for (skip, v) in [(named!("thinmuskip"), "0mu"), (named!("medmuskip"), "-1mu"), (named!("thickmuskip"), "1mu")] {
            t.push(cs(skip));
            t.extend(v.chars().map(ch));
            t.push(space);
        }
        t.extend([cs(named!("setbox")), ch('2'), cs(named!("hbox")), open, shift]);
        t.extend(b.iter().copied());
        t.push(cs(named!("mathsurround")));
        t.extend(ZERO_PT);
        t.extend([shift, close, cs(named!("kern")), ch('-'), cs(named!("wd")), ch('2'), space, shift, open, close]);
        t.extend(b.iter().copied());
        t.extend([open, close, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        t.extend([shift, close]);
        for (rel, class) in [('<', named!("mathbin")), ('>', named!("mathrel"))] {
            t.extend([cs(named!("ifdim")), cs(named!("wd")), ch('0'), ch(rel)]);
            t.extend(ZERO_PT);
            t.extend([space, cs(named!("aftergroup")), cs(class)]);
            if rel == '<' {
                t.push(cs(named!("else")));
            }
        }
        t.extend([cs(named!("fi")), cs(named!("fi")), cs(named!("endgroup"))]);
        t
    }

    /// latex.ltx's `\mathsm@sh style{arg}`, which amsmath's `\relbar` reaches through
    /// `\mathpalette`: `\setbox\z@\hbox{$\m@th style{arg}$}{}\finsm@sh`.
    fn math_smash(&mut self) {
        let keep = match self.e.eqtb.meaning(self.e.cs.intern("finsm@sh")) {
            Meaning::CharDef(k) => k,
            _ => 3,
        };
        let Some(style) = self.get_token() else { return };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut t = self.smash_box(Some(style), &arg, true);
        t.extend(self.smash_finish(SMASH, keep & 1 != 0, keep & 2 != 0));
        self.back_list(&t);
    }

    /// `\setbox\z@\hbox{arg}`, or in math `\setbox\z@\hbox{$\m@th style{arg}$}` and, for
    /// `\mathsm@sh`, an empty `{}`.
    fn smash_box(&self, style: Option<Token>, arg: &[Token], empty: bool) -> Vec<Token> {
        let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
        let mut t = vec![self.named_cs(named!("setbox")), Token::char(Cat::Other, '0'), self.named_cs(named!("hbox")), open];
        match style {
            Some(style) => {
                t.extend([shift, self.named_cs(named!("mathsurround"))]);
                t.extend(ZERO_PT);
                t.extend([style, open]);
                t.extend(arg.iter().copied());
                t.extend([close, shift, close]);
                if empty {
                    t.extend([open, close]);
                }
            }
            None => {
                t.extend(arg.iter().copied());
                t.push(close);
            }
        }
        t
    }

    /// `\finsm@sh` (`\ht\z@\z@ \dp\z@\z@ \leavevmode@ifvmode\box\z@`, `t` or `b` keeping
    /// one) or `\finph@nt` (`\setbox\tw@\null`, the kept dimensions copied,
    /// `\leavevmode@ifvmode\box\tw@`).
    fn smash_finish(&self, kind: u8, top: bool, bottom: bool) -> Vec<Token> {
        let cs = |i: usize| self.named_cs(i);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let (zero, two, eq, space) = (Token::char(Cat::Other, '0'), Token::char(Cat::Other, '2'), Token::char(Cat::Other, '='), Token::char(Cat::Space, ' '));
        let mut fin = Vec::new();
        let b = if kind == SMASH {
            for (keep, dim) in [(top, named!("ht")), (bottom, named!("dp"))] {
                if keep {
                    fin.extend([cs(dim), zero, eq]);
                    fin.extend(ZERO_PT);
                    fin.push(space);
                }
            }
            zero
        } else {
            fin.extend([cs(named!("setbox")), two, cs(named!("hbox")), open, close]);
            if kind & PHANTOM_V != 0 {
                fin.extend([cs(named!("ht")), two, eq, cs(named!("ht")), zero, space, cs(named!("dp")), two, eq, cs(named!("dp")), zero, space]);
            }
            if kind & PHANTOM_H != 0 {
                fin.extend([cs(named!("wd")), two, eq, cs(named!("wd")), zero, space]);
            }
            two
        };
        fin.extend([cs(named!("ifvmode")), cs(named!("indent")), cs(named!("fi")), cs(named!("box")), b, space]);
        fin
    }

    /// `\{` and `\}` in a formula are `\lbrace` and `\rbrace` (`\ifmmode\lbrace\else
    /// \textbraceleft\fi`): what `\left` and `\right` read in their place.
    pub(in crate::engine) fn math_delimiter_alias(&self, t: Token) -> Option<Token> {
        let (open, close) = (self.named_id(named!("{")), self.named_id(named!("}")));
        match t.cs_id() {
            Some(c) if c == open => Some(self.named_cs(named!("lbrace"))),
            Some(c) if c == close => Some(self.named_cs(named!("rbrace"))),
            _ => None,
        }
    }

    /// The delimiter argument read, then `\bBigg@`'s group (a noad of `kind`): `\setbox\z@
    /// \hbox{\everymath{}\m@th$\nulldelimiterspace\z@ \left delim`, the rest in
    /// [`Self::big_middle`] and [`Self::big_end`].
    fn big_open(&mut self, factor: &str, kind: u8) {
        let mut delim = Vec::new();
        self.read_arg(&mut delim);
        // `\{` and `\}` in math are `\lbrace` and `\rbrace`, which `\left` reads.
        let (open, close) = (self.named_id(named!("{")), self.named_id(named!("}")));
        for d in delim.iter_mut() {
            match d.cs_id() {
                Some(c) if c == open => *d = self.named_cs(named!("lbrace")),
                Some(c) if c == close => *d = self.named_cs(named!("rbrace")),
                _ => {}
            }
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.big_pending = super::engine_graphics::times(factor, doc.big_size).unwrap_or(0);
        self.build_math_group(kind, 0);
        self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
        self.set_every_math(&[]);
        self.math_surround_zero();
        self.enter_text_math();
        self.null_delimiter_space_zero();
        let mut t = Vec::with_capacity(delim.len() + 2);
        t.push(self.named_cs(named!("left")));
        t.extend(delim);
        t.push(self.doc_id(doc!("rt@bigmid")));
        self.back_list(&t);
    }

    /// `\vcenter to factor\big@size{}`, then `\right.`.
    fn big_middle(&mut self) {
        let size = self.doc.as_ref().map_or(0, |d| d.big_pending);
        self.open_box(super::engine_noad::VCENTER_GROUP, Ctx::Shift(0), true, size, false);
        self.package();
        let t = [self.named_cs(named!("right")), Token::char(Cat::Other, '.'), self.doc_id(doc!("rt@bigend"))];
        self.back_list(&t);
    }

    /// `$}\box\z@}`; the group's `}` is this native, not a token.
    fn big_end(&mut self) {
        self.after_math();
        self.package();
        self.build_box_register(0);
        self.input.align_state -= 1;
        self.end_math_group();
    }

    /// `\bigl` and kin: `\mathopen\big` (`\mathclose`, `\mathrel` for `r`, `m`).
    fn ams_big_class(&mut self, factor: &str, kind: u8) {
        if self.mode != Mode::Math {
            return self.not_native("amsmath's \\big outside math");
        }
        self.big_open(factor, kind);
    }

    /// `\let\f@size\f@size` (`0`), `\sf@size` (`1`) or `\ssf@size` (`2`), then
    /// `\selectfont`: the current font's shape at that size of the current size.
    fn text_size(&mut self) {
        let k = self.get_token().map_or(0, |t| t.chr() as u8 - b'0');
        self.text_size_of(k);
    }

    fn text_size_of(&mut self, k: u8) {
        let (key, size) = self.font_key();
        let sizes = self.math_sizes(size);
        let size = [sizes.0, sizes.1, sizes.2][k.min(2) as usize];
        self.select_nfss(&key, size);
    }

    /// amsopn's `\DeclareMathOperator{\name}{word}` (starred: with limits):
    /// `\DeclareRobustCommand{\name}{\qopname\newmcodes@ o{word}}`, here a protected
    /// macro of `\operatorname{word}`. A name already defined is LaTeX's error.
    fn declare_math_operator(&mut self) {
        let star = self.next_is('*');
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let mut word = Vec::new();
        self.read_arg(&mut word);
        let Some(&cs) = name.first().filter(|_| name.len() == 1) else { return self.not_native("\\DeclareMathOperator's name") };
        if self.meaning(cs) != Meaning::Undefined {
            return self.not_native("\\DeclareMathOperator of a defined name");
        }
        let Some(id) = cs.cs_id() else { return self.not_native("\\DeclareMathOperator's name") };
        let mut t = vec![Token::END_MATCH];
        t.push(self.named_cs(named!("rt@operatorname")));
        if star {
            t.push(Token::char(Cat::Other, '*'));
        }
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(word);
        t.push(Token::char(Cat::EndGroup, '}'));
        let text = self.e.toks.store(&t);
        self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: super::engine_prim::PROTECTED }), false);
    }

    /// `\providecommand`: `\newcommand` when the name is undefined, else the definition is
    /// read and dropped (`\@reserveda`).
    fn provide_command(&mut self) {
        let star = self.next_is('*');
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let Some(&cs) = name.first() else { return };
        let target = if self.meaning(cs) == Meaning::Undefined { cs } else { self.named_cs(named!("rt@reserveda")) };
        let mut list = Vec::with_capacity(4);
        list.push(self.named_cs(named!("newcommand")));
        if star {
            list.push(Token::char(Cat::Other, '*'));
        }
        list.push(target);
        self.back_list(&list);
    }

    fn set_current_label(&mut self, text: &str) {
        // hyperref's `\refstepcounter` makes an anchor (`hyperref.rs`): not ported yet here.
        if self.hyper() {
            return self.not_native("hyperref: an anchor");
        }
        self.store_current_label(text);
    }

    /// `\@currentlabel`, for a site whose hyperref anchor is made where it is.
    pub(super) fn store_current_label(&mut self, text: &str) {
        let span = self.e.toks.store(&label_tokens(text));
        self.e.eqtb.set_toks(CURRENT_LABEL, span, false);
    }

    /// `\label{key}`: `\@bsphack`, the key's value (`\@currentlabel`) kept in memory, the
    /// `.aux` write's whatsit, `\@esphack`.
    fn label(&mut self) {
        // amsmath's `\label@in@display`: `\gdef\df@label{key}`, written after the tag.
        if self.doc.as_ref().is_some_and(|d| d.ams_display == 3 || d.ams_display != 0 && self.mode == Mode::Math && !self.inner) {
            let mut key = Vec::new();
            self.read_arg(&mut key);
            let Some(doc) = self.doc.as_mut() else { return };
            if doc.df_label.is_some() || doc.ams_display == 2 && doc.df_tag.is_none() {
                return self.not_native("a second \\label, or one in equation*");
            }
            doc.df_label = Some(key);
            return;
        }
        self.label_now();
    }

    /// `\ltx@label`: LaTeX's `\label`, also where amsmath's display stores the key.
    fn label_now(&mut self) {
        let mut key = Vec::new();
        self.read_arg(&mut key);
        self.label_key(&key);
    }

    /// `\label` of `key`.
    fn label_key(&mut self, key: &[Token]) {
        let saved = (self.mode == Mode::Horizontal).then(|| (self.last_skip(), self.aux));
        let key: String = key.iter().map(|t| t.chr()).collect();
        // In `eqnarray`, `\@currentlabel` is the macro `\p@equation\theequation`: the
        // counter as it stands, which the rows step inside their number's box.
        let value = match self.doc.as_ref().filter(|d| d.eqnarray.is_some()) {
            Some(_) => label_tokens(&self.equation_text()),
            None => self.e.toks.get(self.e.eqtb.toks(CURRENT_LABEL)).to_vec(),
        };
        if !self.defined_once(&key, &value) {
            return;
        }
        let href: String = self.e.toks.get(self.e.eqtb.toks(CURRENT_HREF)).iter().map(|t| t.chr()).collect();
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.packages.contains(&"hyperref") {
            let href = if href.is_empty() { "Doc-Start".to_string() } else { href };
            if doc.forward.contains_key(&key) && doc.predicted_hrefs.get(&key) != Some(&href) {
                return self.not_native("hyperref: a label's anchor the pre-scan predicted differently");
            }
            doc.label_hrefs.insert(key.clone(), href);
        }
        doc.label_keys.push(key.clone());
        let entry = doc.label_keys.len() as i32;
        doc.labels.insert(key, value);
        self.aux_write(entry);
        // `\@esphack`: `\spacefactor\@savsf \ifdim\@savsk>\z@ \nobreak \hskip\z@skip
        // \ignorespaces \fi`.
        // In vertical mode, `\if@nobreak\nobreak\else\if@noskipsec\nobreak\fi\fi`.
        if self.mode == Mode::Vertical && self.doc.as_ref().is_some_and(|d| d.nobreak || d.noskipsec) {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
        if let Some((skip, sf)) = saved.filter(|_| self.mode == Mode::Horizontal) {
            self.aux = sf;
            if skip.width > 0 {
                self.cur.push(Node::new(Kind::Penalty, 10000));
                let g = self.e.nodes.glue_node(Glue::default(), 0);
                self.cur.push(g);
                self.ignore_spaces();
            }
        }
    }

    /// `\ref{key}`: `\@setref`, the label's value then `\null` (the `\newlabel`'s fields
    /// past the two it has are `\@empty`). A key not yet labelled (a forward reference) takes
    /// the value [`predict`] read off the source, as pdfLaTeX's final run takes the `.aux`
    /// file's; its `\label` checks it. A key with no value is not native.
    fn reference(&mut self) {
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let key: String = key.iter().map(|t| t.chr()).collect();
        let Some(doc) = self.doc.as_mut() else { return };
        let mut v = match doc.labels.get(&key) {
            Some(v) => v.clone(),
            None => match doc.predicted.get(&key) {
                Some(p) => {
                    let v = label_tokens(p);
                    doc.forward.insert(key.clone(), p.clone());
                    v
                }
                None => return self.not_native("a \\ref with no \\label"),
            },
        };
        if doc.packages.contains(&"hyperref") {
            return self.ref_link(&key, v);
        }
        doc.used.insert(key);
        v.extend([self.named_cs(named!("hbox")), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&v);
    }

    /// pdfLaTeX's final run reads every `\newlabel` and `\bibcite` from the `.aux` file
    /// before the text: a `\ref` or `\cite` gets the last value. A key already read, or
    /// read ahead of its definition with a value the pre-scan got wrong, is not native.
    fn defined_once(&mut self, key: &str, value: &[Token]) -> bool {
        let Some(doc) = self.doc.as_ref() else { return false };
        let text: String = value.iter().map(|t| t.chr()).collect();
        if doc.labels.contains_key(key) && doc.used.contains(key) {
            self.not_native("a \\label or \\bibitem defined twice");
            return false;
        }
        if doc.forward.get(key).is_some_and(|p| *p != text) {
            self.not_native("a \\label or \\bibitem the pre-scan numbered differently");
            return false;
        }
        true
    }

    /// `\cite[#1]{#2}` (`\@citex`): `\leavevmode`, then `\@cite` on the keys, `[{…, #1}]`.
    /// Each key, its leading spaces dropped (`\@firstofone`), is `\hbox{\b@key}`
    /// (`\@cite@ofmt`), the keys separated by `,\penalty\@m\ `. `\b@key` is the
    /// `\bibitem`'s value, kept in memory (the key `BIB` and the name), or the pre-scan's
    /// for a `\bibitem` ahead. An undefined key (pdfLaTeX's bold `?`) is not native.
    fn cite(&mut self) {
        // `\leavevmode` first (`\@citex`): the paragraph starts, then `\cite` again.
        if self.leave_vmode() {
            return;
        }
        if self.mode == Mode::Math {
            return self.not_native("\\cite in math");
        }
        let opt = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let keys: String = arg.iter().map(|t| t.chr()).collect();
        if keys.trim().is_empty() || arg.iter().any(|t| t.cat().is_none()) {
            return self.not_native("a \\cite with a blank key or commands in its keys");
        }
        // Each key's label, as `\bibcite` stored it (letters, others, spaces).
        let mut labels: Vec<Vec<u8>> = Vec::new();
        let mut names = Vec::new();
        let hyper = self.hyper();
        for key in keys.split(',') {
            if hyper {
                names.push(format!("cite.{}", key.trim_start_matches(' ')));
            }
            let key = format!("{BIB}{}", key.trim_start_matches(' '));
            let Some(doc) = self.doc.as_mut() else { return };
            let v = match doc.labels.get(&key) {
                Some(v) => v.clone(),
                None => match doc.predicted.get(&key) {
                    Some(p) => {
                        doc.forward.insert(key.clone(), p.clone());
                        label_tokens(p)
                    }
                    None => return self.not_native("a \\cite with no \\bibitem"),
                },
            };
            doc.used.insert(key);
            let Some(bytes) = v.iter().map(|t| u8::try_from(t.chr() as u32).ok()).collect::<Option<Vec<u8>>>() else {
                return self.not_native("a \\cite label outside 8-bit characters");
            };
            labels.push(bytes);
        }
        // `\@cite{#1}{#2}`: `[{#1\if@tempswa , #2\fi}]`, with `\@citex`'s `#1` each label
        // in an `\hbox` and `,\penalty\@m\ ` between them. The group holds no assignment
        // unless the note does, so it is entered only for the note.
        self.build_text(b"[");
        for (k, label) in labels.iter().enumerate() {
            if k > 0 {
                self.build_text(b",");
                self.build_penalty(1000);
                self.build_control_space();
            }
            // hyperref's `\bibcite`: `\b@key` is `\hyper@@link[cite]{}{cite.key}{label}`.
            match names.get(k) {
                Some(name) => {
                    let name = name.clone();
                    self.build_hbox(|m| {
                        if let Some(doc) = m.doc.as_mut() {
                            doc.link = Some((hyperref::CITE, name));
                        }
                        m.start_link();
                        m.build_text(label);
                        m.end_link();
                    });
                }
                None => self.build_hbox(|m| m.build_text(label)),
            }
        }
        // `]` stays a token: it forms a word with the document's characters after it.
        let close = Token::char(Cat::Other, ']');
        match opt {
            Some(note) => {
                self.build_text(b",");
                self.append_space();
                self.begin_group(SIMPLE_GROUP);
                self.input.align_state += 1;
                let mut t = note;
                t.push(Token::char(Cat::EndGroup, '}'));
                t.push(close);
                self.back_list(&t);
            }
            None => self.back_list(&[close]),
        }
    }

    /// `\bibitem[#1]{#2}` (`\@lbibitem`): `\item[\@biblabel{#1}\hfill]`, `\bibcite{#2}{#1}`
    /// kept in memory. `\bibitem{#1}` (`\@bibitem`): `\item`, `\bibcite{#1}{\the\value
    /// {\@listctr}}`. Each `\bibcite` is an `\immediate\write`: no node. A `#1` with
    /// commands or braces (written expanded) is not native.
    fn bibitem(&mut self) {
        let opt = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut key = Vec::new();
        self.read_arg(&mut key);
        // hyperref's `\@bibitem` and `\@lbibitem`: the anchor `cite.key`, raised in the
        // label of `\bibitem[…]`, vertical after `\item` of `\bibitem{…}` (`item_rest`).
        let anchor = self.hyper().then(|| key.iter().map(|t| t.chr()).fold("cite.".to_string(), |mut s, c| {
            s.push(c);
            s
        }));
        let key: String = std::iter::once(BIB).chain(key.iter().map(|t| t.chr())).collect();
        let Some(list) = self.doc.as_ref().and_then(|d| d.lists.last()).copied().filter(|l| l.label == Label::Bib) else {
            return self.not_native("a \\bibitem outside thebibliography");
        };
        if self.mode == Mode::Math {
            return self.error("\\item is not allowed in math mode");
        }
        let value = match &opt {
            Some(o) if o.iter().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) => {
                return self.not_native("a \\bibitem label with commands or braces");
            }
            Some(o) => o.clone(),
            None => label_tokens(&(self.rw(list.counter) + 1).to_string()),
        };
        if !self.defined_once(&key, &value) {
            return;
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.labels.insert(key, value);
        let raised = match (&anchor, &opt) {
            (Some(name), Some(_)) => self.text_raised_link(name.clone()),
            _ => Vec::new(),
        };
        if let (Some(name), None) = (anchor, &opt) {
            if let Some(doc) = self.doc.as_mut() {
                doc.anchor = Some(name);
            }
        }
        let label = opt.map(|o| {
            let mut l = raised;
            l.push(Token::char(Cat::Other, '['));
            l.extend(o);
            l.extend([Token::char(Cat::Other, ']'), self.named_cs(named!("hfill"))]);
            l
        });
        if label.is_none() {
            if let Some(doc) = self.doc.as_mut() {
                doc.noitemarg = true;
            }
        }
        self.item_start(label);
    }

    /// `thebibliography`'s `\list` after `\section*{\refname}` (`\@mkboth` gobbles under
    /// the plain style): `\settowidth\labelwidth{\@biblabel{#1}}` (the width of
    /// `\@tempboxa`), `\leftmargin\labelwidth \advance\leftmargin\labelsep`,
    /// `\usecounter{enumiv}`; after `\@trivlist`, `\sloppy`, `\clubpenalty4000
    /// \@clubpenalty\clubpenalty \widowpenalty4000 \sfcode`\.\@m`.
    fn bib_open(&mut self) {
        let w = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        self.e.eqtb.replace_box(TEMP_BOX, None);
        let list = List { label: Label::Bib, make: Make::Mklab, numbered: true, counter: R_ENUMI + 3, centercr: false, auto_item: false, endpe_false: false };
        if !self.list_open(list) {
            return;
        }
        self.set_rw(R_LABELWIDTH, w, false);
        let sep = self.rw(R_LABELSEP);
        self.set_rw(R_LEFTMARGIN, w + sep, false);
        self.set_rw(list.counter, 0, true);
        self.set_rw(R_CLUBPENALTY, 4000, false);
        // Read after `\@trivlist`'s steps, which go on the input above them.
        let t = self.doc_id(doc!("rt@sloppy"));
        self.input.back(t);
        self.trivlist(|m| m.list_tail());
    }

    /// `\sloppy` (`\tolerance9999 \emergencystretch3em\hfuzz.5pt\vfuzz\hfuzz`), then
    /// `\clubpenalty4000 \widowpenalty4000 \sfcode`\.\@m`.
    fn bib_sloppy(&mut self) {
        use super::engine_linebreak::{CLUB_PENALTY, EMERGENCY_STRETCH, TOLERANCE, WIDOW_PENALTY};
        use super::engine_pack::{HFUZZ, VFUZZ};
        let em3 = self.glue_of("3em").width;
        self.set_local(Region::Int, TOLERANCE, 9999);
        self.set_local(Region::Dimen, EMERGENCY_STRETCH, em3);
        self.set_local(Region::Dimen, HFUZZ, 1 << 15);
        self.set_local(Region::Dimen, VFUZZ, 1 << 15);
        self.set_local(Region::Int, CLUB_PENALTY, 4000);
        self.set_local(Region::Int, WIDOW_PENALTY, 4000);
        self.e.eqtb.set_code(Code::Sf, b'.' as u32, 1000, false);
    }

    /// `\sqrt` (`\@ifnextchar[\@sqrt\sqrtsign`): without `[`, `\sqrtsign`, the kernel's
    /// `\radical`. `\@sqrt[#1]` is `\root #1\of`, which needs `\rootbox` and `\dimen@`:
    /// not native yet.
    fn sqrt(&mut self) {
        if self.next_is('[') {
            // `\@sqrt[#1]{\root #1\of}`.
            let index = self.bracket_arg();
            let mut radicand = Vec::new();
            self.read_arg(&mut radicand);
            return self.root_tokens(index, radicand);
        }
        let t = self.named_cs(named!("sqrtsign"));
        self.input.back(t);
    }

    /// `\root index\of radicand`.
    fn root(&mut self) {
        let of = self.named_id(named!("of"));
        let mut index = Vec::new();
        let mut depth = 0;
        loop {
            let Some(t) = self.get_token() else { return };
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ if depth == 0 && t.cs_id() == Some(of) => break,
                _ => {}
            }
            index.push(t);
        }
        let mut radicand = Vec::new();
        self.read_arg(&mut radicand);
        self.root_tokens(index, radicand);
    }

    /// The root's tokens. The kernel's `\root`: `\setbox\rootbox\hbox{$\m@th\scriptscriptstyle
    /// {index}$}\mathpalette\r@@t{radicand}`, `\r@@t` in each style `\setbox\z@\hbox{$\m@th
    /// style\sqrtsign{radicand}$}\dimen@\ht\z@ \advance\dimen@-\dp\z@ \mkern5mu\raise.6\dimen@
    /// \copy\rootbox\mkern-10mu\box\z@`. amsmath's reads a leading `\uproot{u}` and
    /// `\leftroot{l}` (either order), puts it all in `\bgroup…\egroup`, and its `\r@@t` also
    /// raises by 1.667 times `u`mu and kerns by `l`mu.
    fn root_tokens(&mut self, mut index: Vec<Token>, radicand: Vec<Token>) {
        if self.mode != Mode::Math {
            return self.not_native("\\root outside math");
        }
        let ams = self.ams();
        let (mut up, mut left) = (None, None);
        if ams {
            let (u, l) = (self.named_id(named!("uproot")), self.named_id(named!("leftroot")));
            let mut i = 0;
            while let Some(t) = index.get(i) {
                let slot = match t.cs_id() {
                    Some(c) if c == u && up.is_none() => &mut up,
                    Some(c) if c == l && left.is_none() => &mut left,
                    _ => break,
                };
                // The argument: a group or one token, then one optional space.
                let (arg, used) = match index.get(i + 1) {
                    Some(t) if t.cat() == Some(Cat::BeginGroup) => {
                        let Some(k) = index[i + 2..].iter().position(|t| t.cat() == Some(Cat::EndGroup)) else { break };
                        (index[i + 2..i + 2 + k].to_vec(), k + 3)
                    }
                    Some(&t) => (vec![t], 2),
                    None => break,
                };
                let text: String = arg.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
                let Ok(v) = text.trim().parse::<i32>() else { return self.not_native("an \\uproot or \\leftroot that is not a number") };
                *slot = Some(v);
                i += used;
                if index.get(i).is_some_and(|t| t.cat() == Some(Cat::Space)) {
                    i += 1;
                }
            }
            index.drain(..i);
        }
        let (up, left) = (up.unwrap_or(0), left.unwrap_or(0));
        let cs = |i: usize| self.named_cs(i);
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let word = |t: &mut Vec<Token>, w: &str| t.extend(w.chars().map(ch));
        let (open, close, shift, space) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'), Token::char(Cat::Space, ' '));
        let mut t = Vec::new();
        if ams {
            t.push(open);
        }
        t.extend([cs(named!("setbox")), cs(named!("rootbox")), cs(named!("hbox")), open, shift, cs(named!("mathsurround"))]);
        t.extend(ZERO_PT);
        t.extend([cs(named!("scriptscriptstyle")), open]);
        t.extend(index);
        t.extend([close, shift, close, cs(named!("mathchoice"))]);
        for style in [named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")] {
            t.extend([open, cs(named!("setbox")), ch('0'), cs(named!("hbox")), open, shift, cs(named!("mathsurround"))]);
            t.extend(ZERO_PT);
            t.extend([cs(style), cs(named!("sqrtsign")), open]);
            t.extend(radicand.iter().copied());
            t.extend([close, shift, close, cs(named!("dimen")), ch('0'), space, cs(named!("ht")), ch('0'), space, cs(named!("advance")), cs(named!("dimen")), ch('0'), space, ch('-'), cs(named!("dp")), ch('0'), space]);
            if ams {
                t.extend([cs(named!("setbox")), ch('1'), cs(named!("hbox")), open, shift, cs(named!("mathsurround"))]);
                t.extend(ZERO_PT);
                t.extend([cs(style), cs(named!("mskip"))]);
                word(&mut t, &format!("{up}mu"));
                t.extend([shift, close, cs(named!("advance")), cs(named!("dimen")), ch('0'), space]);
                word(&mut t, "by1.667");
                t.extend([cs(named!("wd")), ch('1'), space]);
                self.mkern_tokens(&mut t, &format!("{}", -left));
            }
            self.mkern_tokens(&mut t, "5");
            t.push(cs(named!("raise")));
            word(&mut t, ".6");
            t.extend([cs(named!("dimen")), ch('0'), space, cs(named!("copy")), cs(named!("rootbox"))]);
            self.mkern_tokens(&mut t, "-10");
            if ams {
                self.mkern_tokens(&mut t, &format!("{left}"));
            }
            t.extend([cs(named!("box")), ch('0'), close]);
        }
        if ams {
            t.push(close);
        }
        self.back_list(&t);
    }

    /// `\buildrel above\over rel` (`\mathrel{\mathop{\kern\z@ rel}\limits^{above}}`), and
    /// `\doteq` (`\buildrel\textstyle.\over=`; amsmath's is the same after its `\DOTSB`).
    fn buildrel(&mut self, doteq: bool) {
        let (mut above, mut rel) = (Vec::new(), Vec::new());
        if doteq {
            above.extend([self.named_cs(named!("textstyle")), Token::char(Cat::Other, '.')]);
            rel.push(Token::char(Cat::Other, '='));
        } else {
            let over = self.named_id(named!("over"));
            let mut depth = 0;
            loop {
                let Some(t) = self.get_token() else { return };
                match t.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => depth -= 1,
                    _ if depth == 0 && t.cs_id() == Some(over) => break,
                    _ => {}
                }
                above.push(t);
            }
            self.read_arg(&mut rel);
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.named_cs(named!("mathrel")), open, self.named_cs(named!("mathop")), open, self.named_cs(named!("kern"))];
        t.extend(ZERO_PT);
        t.push(Token::char(Cat::Space, ' '));
        t.extend(rel);
        t.extend([close, self.named_cs(named!("limits")), Token::char(Cat::Superscript, '^'), open]);
        t.extend(above);
        t.extend([close, close]);
        self.back_list(&t);
    }

    /// amsmath's `\primfrac`: the plain fraction command `name` (`\over`, `\atop`, ...) made
    /// the primitive again, globally (its warning is in the log only), then run.
    fn prim_frac(&mut self, name: &str) {
        let Some(p) = super::engine_prim::prim(name) else { return };
        let id = self.e.cs.intern(name);
        self.e.eqtb.set_meaning(id, Meaning::Prim(p), true);
        self.input.back(Token::cs(id));
    }

    /// amsmath's `\eqno` and `\leqno`: the primitive, with both made `\relax` until the
    /// group ends.
    fn ams_eqno(&mut self, left: bool) {
        let name = if left { "leqno" } else { "eqno" };
        let Some(p) = super::engine_prim::prim(name) else { return };
        let saved = self.e.cs.intern(if left { "rt@@leqno" } else { "rt@@eqno" });
        self.e.eqtb.set_meaning(saved, Meaning::Prim(p), true);
        for n in ["eqno", "leqno"] {
            let id = self.e.cs.intern(n);
            self.e.eqtb.set_meaning(id, Meaning::Prim(super::engine_prim::Prim::Relax), false);
        }
        self.input.back(Token::cs(saved));
    }

    /// `\pageref{key}`: `\@setref` on the `\newlabel`'s second field, the page then `\null`.
    /// Only a label already shipped has its page; one on the current page or ahead is not
    /// native yet (P2b.2: its width moves line breaks).
    fn page_reference(&mut self) {
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let key: String = key.iter().map(|t| t.chr()).collect();
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(&page) = doc.label_pages.get(&key) else { return self.not_native("a \\pageref to a page not yet shipped") };
        if doc.packages.contains(&"hyperref") {
            return self.ref_link(&key, label_tokens(&page.to_string()));
        }
        doc.used.insert(key);
        let mut v = label_tokens(&page.to_string());
        v.extend([self.named_cs(named!("hbox")), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&v);
    }

    /// At shipout, an `.aux` whatsit's entry: the label's `\thepage`. `output_page` has
    /// already stepped the page counter (`\stepcounter{page}` follows `\shipout`).
    pub(super) fn shipped_entry(&mut self, entry: i32) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.replay {
            return;
        }
        if entry < 0 {
            if let Some(p) = doc.toc_pages.get_mut((-entry) as usize - 1) {
                *p = Some(doc.page - 1);
            }
        } else if let Some(key) = doc.label_keys.get(entry as usize - 1) {
            doc.label_pages.insert(key.clone(), doc.page - 1);
        }
    }

    /// An unstarred heading's `\addcontentsline{toc}`: the `.aux` whatsit carries the
    /// entry (negative), and the heading is checked against [`predict`]'s entry.
    fn contents_write(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let k = doc.toc_seen;
        doc.toc_seen += 1;
        doc.toc_pages.push(None);
        if doc.defer && doc.contents.as_ref().and_then(|t| t.get(k)).is_none_or(|e| e.level != doc.level) {
            return self.not_native("a heading the pre-scan did not see");
        }
        self.aux_write(-(k as i32 + 1));
    }

    /// article's `\tableofcontents`: `\section*{\contentsname}` (`\@mkboth` gobbles its
    /// arguments with `\ps@plain`), then `\@starttoc{toc}`: in a group, each entry as the
    /// `.toc` file's `\contentsline` would typeset it (`\l@section`, `\@dottedtocline` for
    /// the levels to `tocdepth` 3), then `\@nobreakfalse`. The entries come from the source
    /// ([`predict`]), so no `.toc` file and no second run: their numbers and titles do not
    /// depend on layout, and each page number sits in a box `\@pnumwidth` wide whose height
    /// is a digit's (all digits share it), so a digit stands in until the pages are known
    /// and nothing is laid out again (`finish_contents`).
    fn table_of_contents(&mut self) {
        if self.fancy_mkboth() {
            return;
        }
        if self.hyper() {
            return self.not_native("hyperref: \\tableofcontents");
        }
        if self.parskip() {
            return self.not_native("parskip: \\tableofcontents");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        let entries = match doc.contents.take() {
            Some(t) if !doc.defer && t.iter().any(|e| e.level < 3) => t,
            _ => return self.not_native("a \\tableofcontents the pre-scan cannot read whole"),
        };
        doc.defer = true;
        doc.contents = Some(entries);
        doc.toc_next = 0;
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        // report's: `\chapter*{\contentsname …}`; its `\l@chapter`, `\l@section` and
        // `\l@subsection` set a level as article's do the same level (`tocdepth` 2 there).
        let head = if self.doc.as_ref().is_some_and(|d| d.report) { doc!("chapter") } else { doc!("section") };
        let mut t = vec![self.doc_id(head), Token::char(Cat::Other, '*'), open];
        t.extend(label_tokens("Contents"));
        t.extend([close, self.doc_id(doc!("rt@tocopen"))]);
        self.back_list(&t);
    }

    /// The contents' group, then its first entry.
    fn toc_open(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.toc_entry();
    }

    /// The next entry to the depth shown, as the `.toc` file's `\contentsline` sets it:
    /// its opening natives, the title's tokens, its closing natives; after the last,
    /// `\@nobreakfalse` and the group's end.
    fn toc_entry(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let next = doc.contents.as_ref().and_then(|c| c.iter().filter(|e| e.level < 3).nth(doc.toc_next)).map(|e| (e.level, e.title.clone()));
        let Some((level, title)) = next else {
            doc.nobreak = false;
            return self.end_group(SEMI_SIMPLE_GROUP);
        };
        let title = {
            let mut t = Vec::new();
            self.title_tokens(&title, &mut t);
            t
        };
        let mut t = Vec::with_capacity(title.len() + 5);
        if level == 0 {
            t.extend([self.doc_id(doc!("rt@tocsec")), self.doc_id(doc!("rt@tocsecopen"))]);
            t.extend(title);
            t.extend([self.doc_id(doc!("rt@tocsectail")), self.named_cs(named!("par")), self.doc_id(doc!("rt@tocentryend"))]);
        } else {
            t.push(self.doc_id(doc!("rt@toclineopen")));
            t.extend(title);
            t.extend([self.doc_id(doc!("rt@toclinetail")), self.named_cs(named!("par")), self.doc_id(doc!("rt@tocentryend"))]);
        }
        self.back_list(&t);
    }

    /// The entry being set.
    fn toc_current(&self) -> Option<(usize, String)> {
        let doc = self.doc.as_ref()?;
        doc.contents.as_ref()?.iter().filter(|e| e.level < 3).nth(doc.toc_next).map(|e| (e.level, e.number.clone()))
    }

    fn set_skip(&mut self, slot: u32, g: Glue) {
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot }, g, false);
    }

    fn skip(&self, slot: u32) -> Glue {
        self.e.eqtb.glue(Reg { region: Region::Glue, slot })
    }

    /// `\advance\leftskip w\hskip-\leftskip\hbox to w{number\hfil}`.
    fn toc_number(&mut self, w: i32, number: &str) {
        use super::engine_linebreak::LEFT_SKIP;
        let left = self.skip(LEFT_SKIP);
        let left = Glue { width: left.width + w, ..left };
        self.set_skip(LEFT_SKIP, left);
        self.build_glue(neg(left), false);
        self.build_hbox_to(w, |m| {
            m.build_text(number.as_bytes());
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
        });
        // microtype's `toc` patch: `\numberline` ends with `\leftprotrusion`.
        if self.microtype() && !number.is_empty() {
            self.left_protrusion();
        }
    }

    /// `\hbox to1.55em{\hss0\kern-1pt\kern1pt}` (`\hfil` for `\hss` in the lower
    /// levels), the page number's stand-in, then `\rt@tocpage`. With color, the lower
    /// levels' `\@dottedtocline` (`\hfil\normalfont \normalcolor #5`) pushes inside the box
    /// and pops after it.
    fn toc_page(&mut self, hss: bool) {
        let w = self.glue_of("1.55em").width;
        let fil = Glue { stretch: 1 << 16, stretch_order: 1, shrink: if hss { 1 << 16 } else { 0 }, shrink_order: if hss { 1 } else { 0 }, ..Glue::default() };
        let colored = !hss && self.colored();
        self.build_hbox_to(w, |m| {
            m.build_glue(fil, false);
            if colored {
                m.color_stack(Some(DEFAULT_COLOR));
            }
            m.build_text(b"0");
            m.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -(1 << 16)) });
            m.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 1 << 16) });
        });
        self.toc_page_box();
        if colored {
            self.color_stack(None);
        }
    }

    /// `\l@section` after its skip: `\begingroup\parindent\z@\rightskip\@pnumwidth
    /// \parfillskip-\@pnumwidth`, then [`Self::toc_section_head`].
    fn toc_section_open(&mut self) {
        use super::engine_linebreak::{PAR_FILL_SKIP, RIGHT_SKIP};
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_local(Region::Dimen, super::engine_pack::PAR_INDENT, 0);
        let w = self.glue_of("1.55em").width;
        self.set_skip(RIGHT_SKIP, Glue { width: w, ..Glue::default() });
        self.set_skip(PAR_FILL_SKIP, Glue { width: -w, ..Glue::default() });
        let t = self.doc_id(doc!("rt@tocsechead"));
        self.input.back(t);
    }

    /// `\leavevmode\bfseries` and the number in its `1.5em` box.
    fn toc_section_head(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let w = self.glue_of("1.5em").width;
        self.font_change(Change::Series("bx"));
        let Some((_, number)) = self.toc_current() else { return };
        self.toc_number(w, &number);
    }

    /// After the title: `\nobreak\hfil\nobreak` and the page box.
    fn toc_section_tail(&mut self) {
        self.build_penalty(10000);
        self.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
        self.build_penalty(10000);
        self.toc_page(true);
    }

    /// `\@dottedtocline`: `\vskip\z@\@plus.2\p@ {\leftskip indent\rightskip\@tocrmarg
    /// \parfillskip-\rightskip \parindent indent\@afterindenttrue \interlinepenalty\@M
    /// \leavevmode`, then [`Self::toc_line_head`].
    fn toc_line_open(&mut self) {
        use super::engine_linebreak::{INTER_LINE_PENALTY, LEFT_SKIP, PAR_FILL_SKIP, RIGHT_SKIP};
        let Some((level, _)) = self.toc_current() else { return };
        self.build_glue(Glue { stretch: 13107, ..Glue::default() }, false);
        self.begin_group(SIMPLE_GROUP);
        let indent = self.glue_of(if level == 1 { "1.5em" } else { "3.8em" }).width;
        let right = self.glue_of("2.55em").width;
        self.set_skip(LEFT_SKIP, Glue { width: indent, ..Glue::default() });
        self.set_skip(RIGHT_SKIP, Glue { width: right, ..Glue::default() });
        self.set_skip(PAR_FILL_SKIP, Glue { width: -right, ..Glue::default() });
        self.set_local(Region::Dimen, super::engine_pack::PAR_INDENT, indent);
        if let Some(doc) = self.doc.as_mut() {
            doc.toc_afterindent.push(doc.afterindent);
            doc.afterindent = true;
        }
        self.set_local(Region::Int, INTER_LINE_PENALTY, 10000);
        let t = self.doc_id(doc!("rt@toclinehead"));
        self.input.back(t);
    }

    /// `\advance\leftskip width\null\nobreak\hskip-\leftskip {\hbox to width{number\hfil}`;
    /// the title's group closes in [`Self::toc_line_tail`].
    fn toc_line_head(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let Some((level, number)) = self.toc_current() else { return };
        let w = self.glue_of(if level == 1 { "2.3em" } else { "3.2em" }).width;
        use super::engine_linebreak::LEFT_SKIP;
        let left = self.skip(LEFT_SKIP);
        let left = Glue { width: left.width + w, ..left };
        self.set_skip(LEFT_SKIP, left);
        self.build_hbox(|_| {});
        self.build_penalty(10000);
        self.build_glue(neg(left), false);
        self.begin_group(SIMPLE_GROUP);
        self.build_hbox_to(w, |m| {
            m.build_text(number.as_bytes());
            m.build_glue(Glue { stretch: 1 << 16, stretch_order: 1, ..Glue::default() }, false);
        });
        // microtype's `toc` patch: `\numberline` ends with `\leftprotrusion`.
        if self.microtype() && !number.is_empty() {
            self.left_protrusion();
        }
    }

    /// `}\nobreak\leaders\hbox{$\m@th\mkern4.5mu\hbox{.}\mkern4.5mu$}\hfill\nobreak` and
    /// the page box.
    fn toc_line_tail(&mut self) {
        self.end_group(SIMPLE_GROUP);
        self.build_penalty(10000);
        // `$`, `\everymath` (the math fonts' check), `\m@th`.
        self.build_leaders(Glue { stretch: 1 << 16, stretch_order: 2, ..Glue::default() }, |m| {
            m.push_text_math();
            m.check_math_fonts();
            m.math_surround_zero();
            m.build_mkern(9 << 15);
            m.build_hbox(|m| m.build_text(b"."));
            m.build_mkern(9 << 15);
            m.after_math();
        });
        self.build_penalty(10000);
        self.toc_page(false);
    }

    /// After the entry's `\par`: its group's end (`\endgroup`, or `}\@afterindentrestore`),
    /// then the next entry.
    fn toc_entry_end(&mut self) {
        let Some((level, _)) = self.toc_current() else { return };
        if level == 0 {
            // report's `\l@chapter`: `\par\penalty\@highpenalty\endgroup`.
            if self.doc.as_ref().is_some_and(|d| d.report) {
                self.build_penalty(HIGH_PENALTY);
            }
            self.end_group(SEMI_SIMPLE_GROUP);
        } else {
            self.end_group(SIMPLE_GROUP);
            if let Some(doc) = self.doc.as_mut() {
                doc.afterindent = doc.toc_afterindent.pop().unwrap_or(doc.afterindent);
            }
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.toc_next += 1;
        }
        self.toc_entry();
    }

    /// `\l@section`'s `\addpenalty\@secpenalty \addvspace{1.0em \@plus\p@}`; report's
    /// `\l@chapter` `\addpenalty{-\@highpenalty} \vskip 1.0em \@plus\p@`.
    fn toc_section_skip(&mut self) {
        let mut ops = Vec::new();
        if self.doc.as_ref().is_some_and(|d| d.report) {
            self.add_penalty(-HIGH_PENALTY, &mut ops);
            ops.push(Op::Call(|m| {
                let g = m.glue_of("1.0em plus 1pt");
                m.build_glue(g, false);
            }));
            return self.run_ops(ops);
        }
        self.add_penalty(SEC_PENALTY, &mut ops);
        ops.push(Op::Call(|m| {
            let g = m.glue_of("1.0em plus 1pt");
            m.add_vspace(g);
        }));
        self.run_ops(ops);
    }

    /// The page-number box just appended: kept with its entry for `finish_contents`.
    fn toc_page_box(&mut self) {
        let Some(&Node { kind: Kind::Box, a, .. }) = self.cur.last() else { return self.not_native("a contents page box") };
        let Some(doc) = self.doc.as_mut() else { return };
        let n = doc.toc_boxes.len();
        let entry = doc.contents.as_ref().and_then(|t| t.iter().enumerate().filter(|(_, e)| e.level < 3).nth(n)).map(|(i, _)| i);
        match entry {
            Some(i) => doc.toc_boxes.push((a as u32, i)),
            None => self.not_native("a contents page box"),
        }
    }

    /// After the text: each contents page-number box gets its entry's page, packed again to
    /// its width (the stand-in digit becomes the number's digits; Computer Modern has no
    /// kerns between digits), then the kept pages ship in order.
    pub(super) fn finish_contents(&mut self) -> Result<(), String> {
        let Some(doc) = self.doc.as_mut() else { return Ok(()) };
        if !doc.defer {
            return Ok(());
        }
        if doc.contents.as_ref().is_none_or(|t| t.len() != doc.toc_seen) {
            return Err("not native yet: headings the pre-scan did not see".into());
        }
        let boxes = std::mem::take(&mut doc.toc_boxes);
        let pages = doc.toc_pages.clone();
        // Where each page-number box sits, found in one walk of the kept pages.
        let lines = self.lines_holding(boxes.iter().map(|&(b, _)| b));
        for (b, entry) in boxes {
            let page = pages.get(entry).copied().flatten().ok_or("not native yet: a contents entry never shipped")?;
            let old = self.e.nodes.boxes[b as usize];
            let start = self.cur.len();
            let mut list = self.e.nodes.get(old.list).to_vec();
            self.e.nodes.expand_words(&mut list);
            for n in list {
                if n.kind == Kind::Char {
                    self.cur.extend(page.to_string().bytes().map(|c| Node { a: c as i32, ..n }));
                } else {
                    self.cur.push(n);
                }
            }
            let rec = self.hpack(start, old.width, true);
            self.e.nodes.free_block(old.list);
            self.e.nodes.boxes[b as usize] = BoxRec { shift: old.shift, ..rec };
            if (rec.height, rec.depth) != (old.height, old.depth) {
                self.refit_contents_line(lines.get(&b).copied())?;
            }
        }
        let Some(doc) = self.doc.as_mut() else { return Ok(()) };
        doc.replay = true;
        for b in std::mem::take(&mut doc.deferred) {
            self.ship_out(b);
        }
        Ok(())
    }

    /// A contents page box whose height or depth changed with its digits (Times' digits
    /// differ in height, Computer Modern's do not): its line is packed again (§649, at the
    /// same width, so its glue set is unchanged) and the interline glue above and below the
    /// line takes up the difference, as §679 would have computed it from the final line;
    /// on a page's first line the `\topskip` glue above does (§1001). A line whose
    /// neighbours are not `\baselineskip` or `\topskip` glue, or whose new glue would fall
    /// below `\lineskiplimit` (so §679 would have chosen `\lineskip`), is not native.
    fn refit_contents_line(&mut self, at: Option<(u32, usize, u32)>) -> Result<(), String> {
        const NOT: &str = "not native yet: a contents line whose height changes";
        let (g, i, line) = at.ok_or(NOT)?;
        let old = self.e.nodes.boxes[line as usize];
        if old.vertical {
            return Err(NOT.into());
        }
        let start = self.cur.len();
        let list = self.e.nodes.get(old.list).to_vec();
        self.cur.extend(list);
        let rec = self.hpack(start, old.width, true);
        self.e.nodes.free_block(old.list);
        self.e.nodes.boxes[line as usize] = BoxRec { shift: old.shift, ..rec };
        let limit = self.dimen_param(super::engine_pack::LINE_SKIP_LIMIT);
        let skip = super::engine_pack::BASELINE_SKIP as u8 + 1;
        let top = super::engine_page::TOP_SKIP as u8 + 1;
        let list = self.e.nodes.boxes[g as usize].list;
        let nodes = self.e.nodes.get(list);
        // The glue before the line takes the height's change, the glue after it the depth's.
        let above = (rec.height - old.height, i.checked_sub(1));
        let below = (rec.depth - old.depth, nodes[i + 1..].iter().position(|n| n.kind == Kind::Box).map(|k| i + k));
        for (side, (delta, at)) in [above, below].into_iter().enumerate() {
            if delta == 0 {
                continue;
            }
            if side == 1 && at.is_none() {
                self.refit_last_line(g, i, delta)?;
                continue;
            }
            let n = at.map(|k| self.e.nodes.get(list)[k]).ok_or(NOT)?;
            // The first line of a page: §1001's `\topskip` glue, `\topskip` less the box's
            // height (0 when the box is taller). The page's total is unchanged while the
            // glue stays positive, so the break stands.
            if side == 0 && n.kind == Kind::Glue && n.sub == top {
                let glue = &mut self.e.nodes.glues[n.a as usize];
                if glue.width <= 0 || glue.width < delta {
                    return Err(NOT.into());
                }
                glue.width -= delta;
                continue;
            }
            if n.kind != Kind::Glue || n.sub != skip {
                return Err(NOT.into());
            }
            let glue = &mut self.e.nodes.glues[n.a as usize];
            glue.width -= delta;
            if glue.width < limit {
                return Err(NOT.into());
            }
        }
        Ok(())
    }

    /// The last box of page body `g` (at `i`) took a new depth (`delta` more): the column's
    /// `\vskip` of the old depth negated right after it takes the new one; with only glue,
    /// kerns and penalties after it (the contents ended by `\newpage`), the depth goes into the natural
    /// height before the glue that follows (§670), and when the body is set by infinite glue
    /// that all lies after the line, only that glue stretches, so nothing above moves. The
    /// body is then packed again at its height, as §1017 `vpackage` would pack it from the
    /// final line. Finite glue setting, infinite glue above the line, or a body that would
    /// become overfull (§1007 would then have broken the page earlier) is not native.
    fn refit_last_line(&mut self, g: u32, i: usize, delta: i32) -> Result<(), String> {
        const NOT: &str = "not native yet: a contents line whose height changes";
        let body = self.e.nodes.boxes[g as usize];
        // The column's `\vskip-\dimen@` after its last line (`\@makecol`, `output_column`)
        // is the line's depth negated: it takes the new depth.
        if let Some(n) = self.e.nodes.get(body.list).get(i + 1).copied().filter(|n| n.kind == Kind::Glue && n.sub == 0) {
            let glue = &mut self.e.nodes.glues[n.a as usize];
            if glue.width < 0 && glue.width + delta <= 0 && *glue == (Glue { width: glue.width, ..Glue::default() }) {
                glue.width -= delta;
            }
        }
        let nodes = self.e.nodes.get(body.list);
        let order = body.glue_order;
        let tail = nodes[i + 1..].iter().all(|n| matches!(n.kind, Kind::Glue | Kind::Kern | Kind::Penalty));
        let infinite_above = nodes[..i].iter().any(|n| n.kind == Kind::Glue && {
            let glue = self.e.nodes.glue(n);
            glue.stretch_order == order && glue.stretch != 0
        });
        if !body.vertical || !tail || body.glue_sign != super::engine_node::Sign::Stretching || order == 0 || infinite_above {
            return Err(NOT.into());
        }
        let start = self.cur.len();
        let list = nodes.to_vec();
        self.cur.extend(list);
        let rec = self.vpack(start, body.height, true, super::engine_value::MAX_DIMEN);
        if (rec.glue_sign, rec.glue_order, rec.width, rec.depth) != (super::engine_node::Sign::Stretching, order, body.width, body.depth) {
            return Err(NOT.into());
        }
        self.e.nodes.free_block(body.list);
        self.e.nodes.boxes[g as usize] = BoxRec { shift: body.shift, ..rec };
        Ok(())
    }

    /// For each box of `wanted`, the first box in the kept pages' trees (in order, depth
    /// first) whose list holds it: its parent list's box, its index there, and the box. One
    /// walk for all of them, ended when each is found.
    fn lines_holding(&self, wanted: impl Iterator<Item = u32>) -> std::collections::HashMap<u32, (u32, usize, u32)> {
        let mut found: std::collections::HashMap<u32, Option<(u32, usize, u32)>> = wanted.map(|b| (b, None)).collect();
        let mut left = found.len();
        let roots = self.doc.as_ref().map_or(&[][..], |d| &d.deferred[..]);
        let mut stack: Vec<(u32, usize)> = Vec::new();
        for &root in roots {
            if left == 0 {
                break;
            }
            stack.push((root, 0));
            // Each frame: a box and the next index of its list to visit.
            while let Some(&mut (g, ref mut at)) = stack.last_mut() {
                let nodes = self.e.nodes.get(self.e.nodes.boxes[g as usize].list);
                let Some(k) = nodes[*at..].iter().position(|n| n.kind == Kind::Box).map(|k| *at + k) else {
                    stack.pop();
                    continue;
                };
                *at = k + 1;
                let c = nodes[k].a as u32;
                for m in self.e.nodes.get(self.e.nodes.boxes[c as usize].list) {
                    if m.kind == Kind::Box {
                        if let Some(slot @ None) = found.get_mut(&(m.a as u32)) {
                            *slot = Some((g, k, c));
                            left -= 1;
                        }
                    }
                }
                stack.push((c, 0));
            }
            stack.clear();
        }
        found.into_iter().filter_map(|(b, at)| Some((b, at?))).collect()
    }

    /// A contents title from the source, lexed as the document's text is: inside braces, so
    /// it starts mid-line as in the `.toc` file's `\numberline {1}Title`, with `%` ending the
    /// last line.
    /// `out` must be empty: the title's braces are taken off its ends.
    fn title_tokens(&mut self, title: &str, out: &mut Vec<Token>) {
        debug_assert!(out.is_empty());
        let text = format!("{{{title}}}%");
        let mut input = super::engine_input::Input::new(&text);
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::engine_names::END_LINE_CHAR });
        while let Some(t) = input.next(&self.e.toks, &self.e.eqtb, &mut self.e.cs, elc) {
            out.push(t);
        }
        out.pop();
        if !out.is_empty() {
            out.remove(0);
        }
    }

    /// `\title`, `\author`, `\date`: the argument, for `\maketitle`.
    fn store(&mut self, i: usize) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if let Some(doc) = self.doc.as_mut() {
            doc.stored[i] = arg;
        }
    }

    /// An environment's name: the argument's characters.
    fn env_name(&mut self) -> String {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        arg.iter().map(|t| t.chr()).collect()
    }

    /// `\begin{name}`: `\begingroup` and the environment's start.
    fn begin_env(&mut self) {
        let name = self.env_name();
        if let Some(k) = self.theorem_index(&name) {
            return if self.doc.as_ref().is_some_and(|d| d.theorems[k].style != 0) { self.ams_theorem_begin(k) } else { self.theorem_begin(k) };
        }
        if name == "proof" && self.amsthm() {
            return self.proof_begin();
        }
        match name.as_str() {
            "document" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                self.document();
            }
            // The classes' `titlepage` in one column: `\newpage \thispagestyle{empty}
            // \setcounter{page}\@ne`.
            "titlepage" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                let mut t = vec![self.doc_id(NEW_PAGE), self.doc_id(doc!("thispagestyle"))];
                t.extend(self.tex_tokens("{empty}"));
                t.push(self.doc_id(doc!("setcounter")));
                t.extend(self.tex_tokens("{page}{1}"));
                self.back_list(&t);
            }
            "em" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                self.font_change(Change::Em);
            }
            "quote" | "quotation" | "verse" | "itemize" | "enumerate" | "center" | "flushleft" | "flushright" | "description" => {
                // `\begin`: `\@endpefalse` inside the group.
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                if let Some(i) = ["center", "flushleft", "flushright"].iter().position(|n| *n == name) {
                    self.center(i);
                } else {
                    self.list_env(&name);
                }
            }
            "verbatim" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                self.verbatim();
            }
            // article.cls (no `titlepage`, one column): `\small`, the centred bold
            // `\abstractname` with `\vspace{-.5em}\vspace{\z@}`, then `\quotation` without a
            // group of its own.
            "abstract" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
                let mut t = vec![self.doc_id(doc!("small")), self.doc_id(doc!("begin")), open];
                t.extend(label_tokens("center"));
                t.extend([close, open, self.doc_id(doc!("rt@bfseries")), self.named_cs(named!("abstractname")), self.doc_id(doc!("vspace")), open]);
                t.extend(label_tokens("-.5em"));
                t.extend([close, self.doc_id(doc!("vspace")), open]);
                t.extend(label_tokens("0pt"));
                t.extend([close, close, self.doc_id(doc!("end")), open]);
                t.extend(label_tokens("center"));
                t.extend([close, self.doc_id(doc!("rt@quotation")), self.doc_id(doc!("rt@fi"))]);
                self.back_list(&t);
            }
            // article.cls: `\section*{\refname}`, then the list (`bib_open`).
            "thebibliography" => {
                if self.fancy_mkboth() {
                    return;
                }
                let mut arg = Vec::new();
                self.read_arg(&mut arg);
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
                let mut t = vec![self.doc_id(doc!("section")), Token::char(Cat::Other, '*'), open, self.named_cs(named!("refname")), close, self.doc_id(doc!("rt@bibbox")), Token::char(Cat::Other, '[')];
                t.extend(arg);
                t.extend([Token::char(Cat::Other, ']'), close, self.doc_id(doc!("rt@bibopen"))]);
                self.back_list(&t);
            }
            "figure" | "table" | "figure*" | "table*" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                self.float_open(if name.starts_with("figure") { 1 } else { 2 });
            }
            // `\minipage` inside `\begin`'s group (`\@endpefalse`).
            "minipage" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let t = self.doc_id(doc!("rt@minipage"));
                self.input.back(t);
            }
            // tabularx's `\tabularx` inside `\begin`'s group.
            "tabularx" if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"tabularx")) => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                self.tabularx_begin();
            }
            // `\tabular` inside `\begin`'s group (`\@endpefalse`).
            "tabular" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let t = self.doc_id(TABULAR);
                self.input.back(t);
            }
            // `\equation`: `\dollardollar@begin` (`$$`), then `\refstepcounter{equation}`
            // inside the display.
            "equation" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let ams = self.ams();
                let mut t = Vec::new();
                if ams {
                    self.ams_display(1);
                }
                // amsmath: `\mathdisplay` (`$$`), then `\incr@eqnum\mathopen{}`.
                let dollar = Token::char(Cat::MathShift, '$');
                t.extend([dollar, dollar, self.doc_id(doc!("rt@eqstep"))]);
                if ams {
                    t.extend([self.doc_id(doc!("rt@mathopen")), Token::char(Cat::EndGroup, '}')]);
                }
                self.back_list(&t);
            }
            "align" | "align*" if self.ams() => self.align_begin(name == "align*"),
            "gather" | "gather*" if self.ams() => self.gather_begin(name == "gather*"),
            "multline" | "multline*" if self.ams() => self.multline_begin(name == "multline*"),
            "aligned" | "gathered" if self.ams() => self.aligned_begin(name == "gathered"),
            "split" if self.ams() => self.split_begin(),
            "eqnarray" | "eqnarray*" => self.eqnarray_begin(name == "eqnarray*"),
            // `\array` in a formula, inside `\begin`'s group.
            "array" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let t = self.doc_id(ARRAY);
                self.input.back(t);
            }
            // amsmath's `\env@cases` and `\env@matrix`: `\array` with their own
            // specification, a delimiter before (`\left\lbrace`, `\left(`, ...), and a
            // matrix's `\hskip -\arraycolsep` (`*\c@MaxMatrixCols c` is ten columns).
            "cases" | "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" if self.ams() => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                if name == "matrix" {
                    return self.matrix_open();
                }
                let mut t = vec![self.named_cs(named!("left"))];
                t.push(self.matrix_fence(&name, true));
                t.push(self.doc_id(if name == "cases" { doc!("rt@casesopen") } else { doc!("rt@matrixopen") }));
                self.back_list(&t);
            }
            // amsmath's `smallmatrix`: `\null\,\vcenter\bgroup\Let@\restore@math@cr
            // \baselineskip6\ex@ \lineskip1.5\ex@ \lineskiplimit\lineskip \ialign\bgroup
            // \hfil$\m@th\scriptstyle#$\hfil&&\thickspace\hfil$\m@th\scriptstyle#$\hfil\crcr`.
            "smallmatrix" if self.ams() => {
                if self.mode != Mode::Math {
                    return self.not_native("smallmatrix outside math");
                }
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let ex = self.ams_ex();
                let cs = |i: usize| self.named_cs(i);
                let (open, close, shift, space, param) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'), Token::char(Cat::Space, ' '), Token::char(Cat::Param, '#'));
                let mut t = vec![cs(named!("hbox")), open, close, cs(named!(",")), cs(named!("vcenter")), open, self.doc_id(doc!("rt@letmathcr")), cs(named!("baselineskip"))];
                Self::sp_tokens(6 * ex, &mut t);
                t.extend([space, cs(named!("lineskip"))]);
                Self::sp_tokens(super::engine_graphics::times("1.5", ex).unwrap_or(0), &mut t);
                t.extend([space, cs(named!("lineskiplimit")), cs(named!("lineskip"))]);
                self.ialign_tokens(&mut t);
                let cell = |m: &Self, t: &mut Vec<Token>| {
                    t.extend([m.named_cs(named!("hfil")), shift, m.named_cs(named!("mathsurround"))]);
                    t.extend(ZERO_PT);
                    t.extend([m.named_cs(named!("scriptstyle")), param, shift, m.named_cs(named!("hfil"))]);
                };
                cell(self, &mut t);
                t.extend([Token::char(Cat::AlignTab, '&'), Token::char(Cat::AlignTab, '&'), cs(named!("thickspace"))]);
                cell(self, &mut t);
                t.push(cs(named!("crcr")));
                self.back_list(&t);
            }
            // amsmath's `equation*`: `\mathdisplay` with `\@eqnswfalse`.
            "equation*" if self.ams() => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                self.ams_display(2);
                let d = Token::char(Cat::MathShift, '$');
                self.back_list(&[d, d]);
            }
            // `\begingroup \csname name\endcsname`.
            _ => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                let t = Token::cs(self.e.cs.intern(&name));
                self.input.back(t);
            }
        }
    }

    /// `\document`: `\endgroup` (the `\begin` group), the `\@onlypreamble` commands made
    /// `\@notprerr` (`\@preamblecmds` among them), the kernel's write to the `.aux` file,
    /// `\ignorespaces`.
    fn document(&mut self) {
        if self.groups.last() == Some(&SEMI_SIMPLE_GROUP) {
            self.end_group(SEMI_SIMPLE_GROUP);
        }
        if self.hyper() {
            self.hyper_colors();
        }
        self.begin_document_values();
        let n = self.named_id(named!("@preamblecmds"));
        self.e.eqtb.set_meaning(n, Meaning::Doc(NOTPRERR as u16), true);
        self.aux_write(0);
        // xcolor ends with `\color{black}`: a push (no pop at group level 0) on the main list
        // in the preamble, after the kernel's `\write`, which the native job makes here.
        if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"xcolor")) {
            self.color_stack(Some(DEFAULT_COLOR));
        }
        if self.hyper() {
            self.hyper_begin_document();
        }
        self.ignore_spaces();
    }

    /// What `\document` computes from the preamble's values (the class table holds them as
    /// they are after `\document` with the class's own geometry): the geometry read back from
    /// its registers, `\vsize\textheight`, `\columnwidth\textwidth`, `\hsize\columnwidth`,
    /// `\linewidth\hsize`, `\normalsize` (its `\baselineskip`, display skips and `\@listi`)
    /// and `\global\@maxdepth\maxdepth`.
    fn begin_document_values(&mut self) {
        // mathtools's `\AtBeginDocument{\RequirePackage{graphicx}}` (mathtools.sty:1383).
        if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"mathtools")) {
            self.load_package("graphicx");
        }
        // parskip's `\AtBeginDocument` patch of amsthm's `\deferred@thm@head`.
        if self.parskip() && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"amsthm")) {
            return self.not_native("parskip with amsthm");
        }
        // siunitx's `\AddToHook{begindocument/before}{\RequirePackage{color}}`
        // (siunitx.sty:4696); xcolor stands for color once loaded.
        if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"siunitx") && !d.packages.contains(&"xcolor")) {
            self.load_package("color");
        }
        let geometry: Vec<i32> = (0..data_class::CLASS_DIMENS.len()).map(|i| self.e.eqtb.word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 })).collect();
        let max_depth = self.dimen_param(super::engine_page::MAX_DEPTH);
        self.e.eqtb.set_word(COLUMN_WIDTH, geometry[0], true);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.body = true;
        doc.class.vals[..geometry.len()].copy_from_slice(&geometry);
        let i = data_class::CLASS_DIMENS.iter().position(|n| *n == "@maxdepth").expect("@maxdepth");
        doc.class.vals[i] = max_depth;
        let (width, height) = (doc.class.dimen("textwidth"), doc.class.dimen("textheight"));
        // pdftex.def's `\AtBeginDocument`: with `\mag` 1000 and no stock size, the PDF page is
        // `\paperwidth` by `\paperheight`, the class's paper option's.
        // geometry's `\AtBeginDocument` sets them from its `\paperwidth` the same way.
        // hyperref's hpdftex.def sets them as pdftex.def does.
        if let Some(g) = doc.geometry {
            let [pw, ph] = g.paper;
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: PDF_PAGE_WIDTH }, pw, false);
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: PDF_PAGE_WIDTH + 1 }, ph, false);
        } else if doc.packages.iter().any(|p| PAGE_SIZE_PACKAGES.contains(p)) {
            let [mut pw, mut ph] = PAPER_SIZES[(doc.options & 7) as usize];
            if doc.options & 8 != 0 {
                (pw, ph) = (ph, pw);
            }
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: PDF_PAGE_WIDTH }, pw, false);
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: PDF_PAGE_WIDTH + 1 }, ph, false);
        }
        // microtype's `\MT@setup@`: `\pdfadjustspacing` and `\pdfprotrudechars`.
        if doc.packages.contains(&"microtype") {
            let (adjust, protrude) = super::engine_microtype::microtype_ints();
            self.set_int_named(param!("pdfadjustspacing"), adjust);
            self.set_int_named(param!("pdfprotrudechars"), protrude);
        }
        self.font_defaults();
        self.set_dimen_named(param!("vsize"), height);
        self.set_dimen_named(param!("hsize"), width);
        self.floats_begin(height);
        self.set_rw(R_LINEWIDTH, width, false);
        // `\normalsize` keeps the current family; `\begin{document}`'s `\normalfont`
        // (LaTeX's begin-document hook) starts the body in `\rmdefault`.
        let key = if self.family(0) == DEFAULT_FAMILIES[0] { self.font_key().0 } else { self.rm_key("m/n") };
        self.apply_size(NORMALSIZE, &key);
        let f = self.cur_font();
        if let Some(d) = self.doc.as_mut() {
            d.normal_font = f;
        }
    }

    /// `\setlength#1#2` (`#1 #2\relax`) and `\addtolength#1#2` (`\advance#1 #2\relax`), done
    /// by the engine's own assignment. `#1` must be a parameter or a register the native job
    /// keeps; the page geometry only in the preamble (after `\begin{document}` LaTeX has
    /// already derived `\hsize` and the rest from it).
    fn set_length(&mut self, add: bool) {
        let mut target = Vec::new();
        self.read_arg(&mut target);
        let [t] = target[..] else { return self.not_native("\\setlength of more than one token") };
        let Some(id) = t.cs_id() else { return self.not_native("\\setlength of a character") };
        let reg = match self.e.eqtb.meaning(id) {
            Meaning::Reg(r) => Some(r),
            Meaning::Unset => super::engine_names::param(self.e.cs.name(id)),
            _ => None,
        };
        let Some(reg) = reg.filter(|r| matches!(r.region, Region::Dimen | Region::Glue)) else {
            return self.not_native("\\setlength of something not a length");
        };
        let geometry = reg.region == Region::Dimen && (GEOMETRY..GEOMETRY + data_class::CLASS_DIMENS.len() as u32).contains(&reg.slot);
        if geometry && self.doc.as_ref().is_some_and(|d| d.body) {
            return self.not_native("the page geometry set in the document body");
        }
        let mut value = Vec::new();
        self.read_arg(&mut value);
        // `#1 #2\relax`: the assignment reads the value; what it leaves, and the `\relax`,
        // run after it.
        value.push(self.named_cs(named!("relax")));
        self.back_list(&value);
        if add {
            self.register_apply(super::engine_prim::Prim::Advance, reg, false);
        } else {
            self.assign_register(reg, false);
        }
    }

    /// `\setcounter{#1}{#2}`: `\global\c@#1 #2\relax` (no counters reset). The page counter
    /// and the heading and footnote counters the native job keeps; others not yet.
    fn set_counter(&mut self) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: String = name.iter().map(|t| t.chr()).collect();
        if !matches!(name.as_str(), "page" | "section" | "subsection" | "subsubsection" | "footnote" | "topnumber" | "bottomnumber" | "totalnumber" | "figure" | "table") {
            return self.not_native("\\setcounter of a counter the native job does not keep");
        }
        let mut value = Vec::new();
        self.read_arg(&mut value);
        let relax = self.named_cs(named!("relax"));
        value.push(relax);
        self.back_list(&value);
        let n = self.scan_int();
        if let Some(t) = self.get_token().filter(|&t| t != relax) {
            self.input.back(t);
        }
        if let Some(i) = ["topnumber", "bottomnumber", "totalnumber", "figure", "table"].iter().position(|c| *c == name) {
            return self.set_rw(R_TOPNUMBER + i, n, true);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        // report keeps the chapter at level 0.
        let at = |k: usize| k + doc.report as usize;
        match name.as_str() {
            "page" => doc.page = n,
            "footnote" => doc.footnote = n,
            "section" => doc.counters[at(0)] = n,
            "subsection" => doc.counters[at(1)] = n,
            _ if doc.report => self.not_native("\\setcounter{subsubsection} under report"),
            _ => doc.counters[2] = n,
        }
    }

    /// `\pagestyle{plain}` or `{empty}`, in the preamble or at the body's outer level (in a
    /// group it is local to it, which the native job does not keep).
    fn page_style(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let style: String = arg.iter().map(|t| t.chr()).collect();
        // In no group but `document`'s (which the native job does not open).
        let outer = self.groups.is_empty();
        let Some(doc) = self.doc.as_mut() else { return };
        match style.as_str() {
            "plain" | "empty" if (!doc.body || outer) && doc.fancy.as_ref().is_none_or(|f| !f.on) => doc.empty_style = style == "empty",
            "fancy" if !doc.body && doc.fancy.is_some() => self.fancy_on(),
            _ => self.not_native("\\pagestyle other than plain or empty, or in the body"),
        }
    }

    /// `\thispagestyle{#1}` (`\global\@specialpagetrue\gdef\@specialstyle{#1}`): the style of
    /// the next page out, which `\@outputpage` applies in its group.
    fn this_page_style(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let style: String = arg.iter().map(|t| t.chr()).collect();
        let Some(doc) = self.doc.as_mut() else { return };
        match style.as_str() {
            "plain" | "empty" => {
                doc.special = Some(style == "empty");
                if let Some(f) = doc.fancy.as_mut() {
                    f.this = false;
                }
            }
            "fancy" if doc.fancy.is_some() => {
                doc.special = None;
                if let Some(f) = doc.fancy.as_mut() {
                    f.this = true;
                }
            }
            _ => self.not_native("\\thispagestyle other than plain, empty or fancy"),
        }
    }

    /// `\smallskip`, `\medskip`, `\bigskip`: `\vspace\smallskipamount` and kin, the size
    /// files' values (`size1x.clo`: the same at 10, 11 and 12pt). The pre-scan refuses the
    /// amounts' names, so they keep these values.
    fn skip_amount(&mut self, glue: &str) {
        let mut t = vec![self.doc_id(doc!("vspace"))];
        t.extend(self.tex_tokens(glue));
        self.back_list(&t);
    }

    /// `\ensuremath{#1}`: `#1` in math, else `$\m@th#1$` (`\m@th` is `\mathsurround\z@`).
    fn ensure_math(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if self.mode == Mode::Math {
            return self.back_list(&arg);
        }
        let dollar = Token::char(Cat::MathShift, '$');
        let mut t = vec![dollar];
        t.extend(self.tex_tokens("\\mathsurround0pt\\relax"));
        t.extend(arg);
        t.push(dollar);
        self.back_list(&t);
    }

    /// `\rule[#1]{#2}{#3}` (`\@rule`): `\leavevmode\hbox{\vrule\@width#2\@height#3+#1
    /// \@depth-#1}`, each length read as `\setlength` reads it (`#1\relax`); the
    /// `\@tempdim`s are set inside the box's group.
    fn rule(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let raise = if self.next_is('[') {
            let arg = self.bracket_arg();
            self.dimen_arg(arg)
        } else {
            0
        };
        let (mut w, mut h) = (Vec::new(), Vec::new());
        self.read_arg(&mut w);
        self.read_arg(&mut h);
        let (w, h) = (self.dimen_arg(w), self.dimen_arg(h));
        let t = self.tex_tokens(&format!("\\hbox{{\\vrule width{w}sp height{}sp depth{}sp}}", h.saturating_add(raise), raise.saturating_neg()));
        self.back_list(&t);
    }

    /// `\enddocument`: `\clearpage` (its `\newpage`), and the job's end.
    fn end_document(&mut self) {
        let list = [self.doc_id(NEW_PAGE), self.doc_id(FLOAT_FLUSH), self.doc_id(STOP)];
        self.back_list(&list);
    }

    /// A kernel macro whose body is primitives and its one argument (`#1`, when `arg`),
    /// as TeX text for `tex_tokens`.
    fn kernel_macro(&mut self, arg: bool, body: &str) {
        let mut a = Vec::new();
        if arg {
            self.read_arg(&mut a);
        }
        let mut t = Vec::new();
        for (k, part) in body.split("#1").enumerate() {
            if k > 0 {
                t.extend(a.iter().copied());
            }
            let p = self.tex_tokens(part);
            t.extend(p);
        }
        self.back_list(&t);
    }

    /// `\clearpage` (latex.ltx:20414) in one column, where `\@dbltopnum` is never `-1`:
    /// `\newpage`, then the rest.
    fn clear_page(&mut self) {
        let t = [self.doc_id(NEW_PAGE), self.doc_id(CLEAR_PAGE_TAIL)];
        self.back_list(&t);
    }

    /// `\clearpage` after its `\newpage`: `\@@write\m@ne{}\vbox{}\penalty -\@Mi`; the output
    /// routine's `\@doclearpage` sets the floats on float pages and gives the write back.
    fn clear_page_tail(&mut self) {
        self.cur.push(Node { kind: Kind::Whatsit, sub: super::engine_whatsit::WRITE, font: super::engine_whatsit::NULL_STREAM, a: 0, b: 0 });
        self.run_ops(vec![Op::Call(|m| m.empty_vbox(false)), Op::Penalty(-10001)]);
    }

    /// `\clearpage`'s `\vbox{}\penalty-\@Mi` after its `\newpage`, when floats wait: the
    /// output routine's `\@doclearpage` puts them on float pages. (Its `\write\m@ne{}` and
    /// the box go with the page `\@doclearpage` drops.)
    fn float_flush(&mut self) {
        if !self.doc.as_ref().is_some_and(|d| d.floats.pending()) {
            return;
        }
        self.run_ops(vec![Op::Call(|m| m.empty_vbox(false)), Op::Penalty(-10001)]);
    }

    /// `\end{name}`: the environment's end and `\endgroup`.
    fn end_env(&mut self) {
        let name = self.env_name();
        // `\@endtheorem`: `\endtrivlist` (amsthm's then `\@endpefalse`).
        if self.theorem_index(&name).is_some() {
            return self.end_trivlist();
        }
        // amsthm's `\endproof`: `\popQED` (`\begingroup \qed \relax\endgroup`), `\endtrivlist
        // \@endpefalse`.
        if name == "proof" && self.amsthm() {
            self.begin_group(SEMI_SIMPLE_GROUP);
            let t = [self.named_cs(named!("qed")), self.doc_id(doc!("rt@endproof"))];
            return self.back_list(&t);
        }
        match name.as_str() {
            "document" => self.end_document(),
            "em" => self.end_group(SEMI_SIMPLE_GROUP),
            // `\endtitlepage`: `\newpage`, then `\setcounter{page}\@ne` when one-sided; and
            // `\end`'s `\endgroup`.
            "titlepage" => {
                let mut t = vec![self.doc_id(NEW_PAGE)];
                if !self.doc.as_ref().is_some_and(|d| d.class.flag("twoside")) {
                    t.push(self.doc_id(doc!("setcounter")));
                    t.extend(self.tex_tokens("{page}{1}"));
                }
                t.push(self.named_cs(named!("endgroup")));
                self.back_list(&t);
            }
            "quote" | "quotation" | "verse" | "itemize" | "enumerate" | "description" | "abstract" | "thebibliography" => self.end_list(),
            // `\endequation`: `\eqno \hbox{\@eqnnum}\dollardollar@end\@ignoretrue`, with
            // `\@eqnnum` `{\normalfont \normalcolor (\theequation)}`.
            "equation" | "equation*" if self.in_split_display() => self.align_end(),
            "equation" | "equation*" if self.ams() => self.end_ams_display(),
            "align" | "align*" | "gather" | "gather*" if self.in_align() => self.align_end(),
            "multline" | "multline*" if self.in_align() => self.multline_end(),
            "aligned" | "gathered" | "split" if self.in_aligned() => self.aligned_end(),
            "eqnarray" | "eqnarray*" if self.in_eqnarray() => self.eqnarray_end(),
            "equation" => {
                let dollar = Token::char(Cat::MathShift, '$');
                let mut t = Vec::new();
                // hyperref's `\endequation`: `\mathclose{\Hy@raisedlink{\hyper@anchorend}}`
                // before the kernel's.
                if self.hyper() {
                    t = self.raised_link(None);
                }
                t.extend([self.named_cs(named!("eqno")), self.doc_id(doc!("rt@eqtag")), dollar, dollar, self.doc_id(doc!("rt@endequation"))]);
                self.back_list(&t);
            }
            // `\endverbatim`: `\if@newlist \leavevmode\fi \endtrivlist`; its last line ended
            // in vertical mode, so `\leavevmode` is not needed.
            "center" | "flushleft" | "flushright" | "verbatim" => self.end_trivlist(),
            "tabular" => self.tabular_end(),
            // `\endminipage` (`\par` first) and `\end`'s `\endgroup`.
            "minipage" => {
                let list = [self.named_cs(named!("par")), self.doc_id(doc!("rt@mpclose")), self.named_cs(named!("endgroup"))];
                self.back_list(&list);
            }
            // `\endarray` (`\crcr\egroup\egroup`), what amsmath puts after it, and
            // `\end`'s `\endgroup`.
            "array" | "cases" | "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" if self.in_tabular() => {
                let crcr = Token::cs(self.doc.as_ref().map_or(0, |d| d.crcr_id));
                let mut list = vec![crcr, self.doc_id(TAB_END)];
                match name.as_str() {
                    "array" => {}
                    "cases" => list.extend([self.named_cs(named!("right")), Token::char(Cat::Other, '.')]),
                    "matrix" => list.push(self.doc_id(doc!("rt@mskip"))),
                    _ => list.extend([self.doc_id(doc!("rt@mskip")), self.named_cs(named!("right")), self.matrix_fence(&name, false)]),
                }
                list.push(self.named_cs(named!("endgroup")));
                self.back_list(&list);
            }
            // `\endsmallmatrix` (`\crcr\egroup\egroup\,`) and `\end`'s `\endgroup`.
            "smallmatrix" if self.ams() => {
                let close = Token::char(Cat::EndGroup, '}');
                let list = [self.named_cs(named!("crcr")), close, close, self.named_cs(named!(",")), self.named_cs(named!("endgroup"))];
                self.back_list(&list);
            }
            "figure" | "table" | "figure*" | "table*" => self.float_close(),
            // `\csname endname\endcsname \endgroup`.
            _ => {
                let list = [Token::cs(self.e.cs.intern(&format!("end{name}"))), self.named_cs(named!("endgroup"))];
                self.back_list(&list);
            }
        }
    }

    /// article's list environments: `\list{label}{setup}`, then `\item\relax` for the
    /// displayed-text ones.
    fn list_env(&mut self, name: &str) {
        if self.list_in_nested_minipage() {
            return self.not_native("a list in a minipage in a list");
        }
        let (em, _) = self.em_ex();
        let em = em as i32;
        let mut list = List { label: Label::Empty, make: Make::Mklab, numbered: false, counter: 0, centercr: false, auto_item: true, endpe_false: false };
        match name {
            "itemize" | "enumerate" => {
                // `\@itemdepth`/`\@enumdepth`, advanced locally.
                let r = if name == "itemize" { R_ITEMDEPTH } else { R_ENUMDEPTH };
                let d = self.rw(r);
                if d > 3 {
                    return self.error("Too deeply nested");
                }
                self.set_rw(r, d + 1, false);
                list.make = Make::Llap;
                list.auto_item = false;
                if name == "itemize" {
                    list.label = Label::Item(d as u8 + 1);
                } else {
                    // enumerate.sty: `\@ifnextchar[{\@@enum@}{\@enum@}`.
                    if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"enumerate")) && self.next_is('[') {
                        return self.enum_template();
                    }
                    // `\usecounter\@enumctr`.
                    list.label = Label::Enum(d as u8 + 1);
                    list.numbered = true;
                    list.counter = R_ENUMI + d as usize;
                    self.set_rw(list.counter, 0, true);
                }
            }
            "verse" => list.centercr = true,
            "description" => {
                list.make = Make::Description;
                list.auto_item = false;
            }
            _ => {}
        }
        if !self.list_open(list) {
            return;
        }
        if matches!(name, "itemize" | "enumerate") && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"enumitem")) {
            return self.enit_setup(name == "enumerate");
        }
        match name {
            // `\rightmargin\leftmargin`.
            "quote" => {
                let l = self.rw(R_LEFTMARGIN);
                self.set_rw(R_RIGHTMARGIN, l, false);
            }
            // `\listparindent 1.5em \itemindent\listparindent \rightmargin\leftmargin
            // \parsep\z@\@plus\p@`.
            "quotation" => {
                let i = em + em / 2;
                self.set_rw(R_LISTPARINDENT, i, false);
                self.set_rw(R_ITEMINDENT, i, false);
                let l = self.rw(R_LEFTMARGIN);
                self.set_rw(R_RIGHTMARGIN, l, false);
                let g = self.glue_of("0pt plus 1pt");
                self.set_rg(R_PARSEP, g);
            }
            // `\itemsep\z@ \itemindent -1.5em \listparindent\itemindent
            // \rightmargin\leftmargin \advance\leftmargin 1.5em`.
            "verse" => {
                self.set_rg(R_ITEMSEP, Glue::default());
                let i = -(em + em / 2);
                self.set_rw(R_ITEMINDENT, i, false);
                self.set_rw(R_LISTPARINDENT, i, false);
                let l = self.rw(R_LEFTMARGIN);
                self.set_rw(R_RIGHTMARGIN, l, false);
                self.set_rw(R_LEFTMARGIN, l - i, false);
            }
            // `\labelwidth\z@ \itemindent-\leftmargin`; enumitem's `style=standard` makes
            // `\enit@calcleft` take the item indent: `\labelsep+\labelindent-\leftmargin`
            // (`\labelindent` 0). Its `\setlist` keys for description are not read yet.
            "description" => {
                self.set_rw(R_LABELWIDTH, 0, false);
                let l = self.rw(R_LEFTMARGIN);
                let enumitem = self.doc.as_ref().is_some_and(|d| d.packages.contains(&"enumitem"));
                if enumitem && self.doc.as_ref().is_some_and(|d| d.enit_set.iter().any(|s| s.0 == 0 || s.0 == 3)) {
                    return self.not_native("enumitem: \\setlist keys for description");
                }
                let i = if enumitem { self.rw(R_LABELSEP) - l } else { -l };
                self.set_rw(R_ITEMINDENT, i, false);
            }
            _ => {}
        }
        self.trivlist(|m| m.list_tail());
    }

    /// `\begin{verbatim}`: `\@verbatim \frenchspacing \@vobeyspaces \@xverbatim`. The text
    /// up to `\end{verbatim}` is read as it is in the file, not through catcode changes
    /// and active characters; each line becomes what LaTeX makes of it. `\@verbatim` is
    /// `\trivlist \item\relax`, then the setup in `verbatim_setup`.
    fn verbatim(&mut self) {
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::engine_names::END_LINE_CHAR });
        let Some(text) = self.input.raw_until("\\end{verbatim}", elc) else {
            return self.not_native("verbatim without its end in the same file");
        };
        let mut lines: Vec<&str> = text.split('\n').collect();
        // The rest of the `\begin` line and the text before `\end{verbatim}` on its line.
        if lines.len() < 2 || !lines[0].is_empty() || !lines[lines.len() - 1].is_empty() {
            return self.not_native("verbatim text on the \\begin or \\end line");
        }
        if text.chars().any(|c| !(' '..='~').contains(&c) && c != '\n') {
            return self.not_native("verbatim with a tab or non-ASCII character");
        }
        lines.pop();
        // Each line after the first: its characters, then `^^M` as `\par` (`\leavevmode
        // \null \@@par \penalty\interlinepenalty`). Spaces are `\nobreakspace{}`
        // (`\@vobeyspaces`); ``\`<>,'-`` are `\leavevmode\kern\z@\char` (`\@noligs`).
        let mut list = Vec::with_capacity(text.len() * 2);
        let id = |m: &Self, name: usize| m.named_cs(name);
        let (nbsp, kern, leave, hbox, par, penalty, ilp) = (id(self, named!("nobreakspace")), id(self, named!("kern")), self.doc_id(LEAVE), id(self, named!("hbox")), self.doc_id(PRIM_PAR), id(self, named!("penalty")), id(self, named!("interlinepenalty")));
        for line in &lines[1..] {
            for c in line.chars() {
                match c {
                    ' ' => list.push(nbsp),
                    // `\leavevmode\kern\z@\char`: a line may start with one.
                    '`' | '<' | '>' | ',' | '\'' | '-' => {
                        list.extend([leave, kern, Token::char(Cat::Other, '0'), Token::char(Cat::Letter, 'p'), Token::char(Cat::Letter, 't'), Token::char(Cat::Other, c)]);
                    }
                    c if c.is_ascii_alphabetic() => list.push(Token::char(Cat::Letter, c)),
                    c => list.push(Token::char(Cat::Other, c)),
                }
            }
            list.extend([leave, hbox, Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), par, penalty, ilp]);
        }
        // `\end{verbatim}`, read with the text.
        list.push(id(self, named!("end")));
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.extend("verbatim".chars().map(|c| Token::char(Cat::Letter, c)));
        list.push(Token::char(Cat::EndGroup, '}'));
        if let Some(doc) = self.doc.as_mut() {
            doc.verbatim = list;
        }
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: false, auto_item: true, endpe_false: false });
        }
        self.trivlist(|m| {
            for r in [R_LABELWIDTH, R_LEFTMARGIN, R_ITEMINDENT] {
                m.set_rw(r, 0, false);
            }
            if let Some(doc) = m.doc.as_mut() {
                doc.noitemarg = true;
            }
            let t = m.doc_id(VERBATIM);
            m.input.back(t);
            m.item_start(None);
        });
    }

    /// `\@verbatim` after `\item\relax`: `\vskip\parskip` (no minipage), `\leftskip
    /// \@totalleftmargin \rightskip\z@skip \parindent\z@ \parfillskip\@flushglue
    /// \parskip\z@skip \@@par \language\l@nohyphenation`, `\verbatim@font`, `\everypar`
    /// with `\unpenalty` added, `\frenchspacing` (no space factor is used: every space is
    /// `\ `); then the lines.
    fn verbatim_setup(&mut self) {
        if !self.doc.as_ref().is_some_and(|d| d.floats.minipage) {
            let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
            let g = self.e.nodes.glue_node(parskip, 0);
            self.cur.push(g);
        }
        let left = self.rw(R_TOTALLEFTMARGIN);
        self.set_glue_named(param!("leftskip"), Glue { width: left, ..Glue::default() });
        self.set_glue_named(param!("rightskip"), Glue::default());
        self.set_dimen_named(param!("parindent"), 0);
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        self.set_glue_named(param!("parskip"), Glue::default());
        self.run_ops(vec![Op::Par, Op::Call(|m| {
            m.set_int_named(param!("language"), NOHYPHENATION);
            let (_, size) = m.font_key();
            m.select_nfss(&format!("{}/cmtt/m/n", m.encoding()), size);
            if m.everypar() == ITEM {
                m.set_everypar(VERB_ITEM);
            } else if m.everypar() == MT_PROT {
                return m.not_native("microtype: verbatim in an item's later paragraph");
            }
            // microtype's `verbatim` patch: `\microtypesetup{activate=false}` at the end of
            // `\@verbatim` (`\pdfprotrudechars` and `\pdfadjustspacing` 0; its `\selectfont`
            // finds the font set up).
            if m.microtype() {
                m.set_int_named(param!("pdfprotrudechars"), 0);
                m.set_int_named(param!("pdfadjustspacing"), 0);
            }
            let list = m.doc.as_mut().map(|d| std::mem::take(&mut d.verbatim)).unwrap_or_default();
            m.back_list(&list);
        })]);
    }

    /// A text accent of OT1 (`\DeclareTextAccent`, code `code`) on its argument:
    /// `\add@accent` (latex.ltx), `\hmode@bgroup`, the base's space factor from
    /// `\hbox{#2}`, `\accent#1 #2\egroup`, `\spacefactor` restored. The base is one letter
    /// or `\i`/`\j`; ot1enc.def's composites put `\`, `\'`, `\^`, `\"` and `\=` over
    /// `\i` for `i`, and `\r A` raises the ring over `A` by `.67(\ht\hbox{!}-1ex)`. `\.i`
    /// (a composite to `\i` itself) and math mode (`\ddot` and kin) are not native yet.
    fn text_accent(&mut self, code: u32) {
        if self.mode == Mode::Math {
            return self.not_native("a text accent in math");
        }
        if self.leave_vmode() {
            return;
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if self.t1() {
            return self.t1_accent(code, &arg);
        }
        let base = match arg[..] {
            [t] if t.cat() == Some(Cat::Letter) => match t.chr() {
                'i' if code == 95 => return self.not_native("\\.i"),
                'i' if matches!(code, 18 | 19 | 94 | 127 | 22) => 16,
                // ot1enc.def's `\DeclareTextCompositeCommand{\r}{OT1}{A}`.
                'A' if code == 23 => {
                    let t = self.tex_tokens("\\setbox0\\hbox{!}\\dimen0\\ht0\\advance\\dimen0-1ex\\hbox to0pt{\\raise.67\\dimen0\\hbox{\\char23}\\hss}A");
                    return self.back_list(&t);
                }
                c => c as u32,
            },
            [t] => match t.cs_id() {
                Some(c) if c == self.named_id(named!("i")) && code != 95 => 16,
                Some(c) if c == self.named_id(named!("j")) => 17,
                _ => return self.not_native("an accent on more than a letter"),
            },
            _ => return self.not_native("an accent on more than a letter"),
        };
        // `\hbox{#2}` starts at space factor 1000 (§1034).
        let sf = match self.e.eqtb.code(Code::Sf, base) {
            0 => 1000,
            k => k,
        };
        // `{\accent code \char base}`, a group with no assignment.
        let f = self.cur_font();
        self.build_accent(code as u8, f, Some(base as u8));
        self.aux = sf;
    }

    /// A text accent in T1 (`t1enc.def`), `code` its OT1 slot: a composite
    /// (`\DeclareTextComposite`) is the precomposed character as a letter token
    /// (`\@text@composite`), else `\add@accent` with the T1 accent as in OT1.
    fn t1_accent(&mut self, code: u32, arg: &[Token]) {
        let Some(slot) = T1_ACCENT_OT1.iter().position(|&c| c == code) else { return self.not_native("an accent T1 has not") };
        const CMD: &[u8; 11] = T1_ACCENT_CMD;
        let base = match arg {
            // `\DeclareTextCompositeCommand{\=}{T1}{i}{\=\i}`.
            [t] if t.cat() == Some(Cat::Letter) && t.chr() == 'i' && CMD[slot] == b'=' => 25,
            [t] if t.cat() == Some(Cat::Letter) && t.chr().is_ascii_alphabetic() => t.chr() as u32,
            [t] => match t.cs_id() {
                Some(c) if c == self.named_id(named!("i")) => 25,
                Some(c) if c == self.named_id(named!("j")) => 26,
                _ => return self.not_native("an accent on more than a letter"),
            },
            _ => return self.not_native("an accent on more than a letter"),
        };
        let key = (CMD[slot] as u16) << 8 | base as u16;
        if let Ok(k) = T1_COMPOSITE_KEYS.binary_search(&key) {
            let c = char::from(T1_COMPOSITE_SLOTS[k]);
            return self.back_list(&[Token::char(Cat::Letter, c)]);
        }
        let sf = match self.e.eqtb.code(Code::Sf, base) {
            0 => 1000,
            k => k,
        };
        let f = self.cur_font();
        self.build_accent(slot as u8, f, Some(base as u8));
        self.aux = sf;
    }

    /// ot1enc.def's `\c`: `\leavevmode\setbox\z@\hbox{#1}`, then (`cedilla_end`) `\accent24
    /// #1` when `\ht\z@` is 1ex, else the cedilla `\ooalign`ed under the box.
    fn cedilla(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("a text accent in math");
        }
        if self.leave_vmode() {
            return;
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        // T1: a composite letter, or `\textcommabelow`/`\textcommaabove` (not native).
        if let ([t], true) = (&arg[..], self.t1()) {
            if t.cat() == Some(Cat::Letter) {
                if "gGKkLlNnRr".contains(t.chr()) {
                    return self.not_native("\\c with a comma accent (T1)");
                }
                if let Ok(k) = T1_COMPOSITE_KEYS.binary_search(&((b'c' as u16) << 8 | t.chr() as u16)) {
                    let c = char::from(T1_COMPOSITE_SLOTS[k]);
                    return self.back_list(&[Token::char(Cat::Letter, c)]);
                }
            }
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = self.tex_tokens("\\setbox0\\hbox");
        t.push(open);
        t.extend(&arg);
        t.extend([close, self.doc_id(doc!("rt@cedilla")), open]);
        t.extend(arg);
        t.push(close);
        self.back_list(&t);
    }

    /// The `\ifdim\ht\z@=1ex` of `\c`: `\accent24` over a letter as tall as 1ex.
    fn cedilla_end(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let f = self.cur_font();
        let ex = self.e.fonts.list[f as usize].params(&self.e.fonts.info)[4];
        let ht = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
        // The cedilla: OT1 24, T1 11 (`t1enc.def`).
        let cedilla = if self.t1() { 11 } else { 24 };
        if ht != ex {
            // `{\ooalign{\unhbox\z@\crcr\hidewidth\char24\hidewidth}}`, with `\ooalign`,
            // `\oalign`, `\ialign`, `\maxdimen` and `\hideskip` as latex.ltx defines them.
            let t = self.tex_tokens(&format!("{{\\lineskiplimit-16383.99999pt\\vtop{{\\baselineskip0pt\\lineskip.25ex\\everycr{{}}\\tabskip0pt\\halign{{#\\crcr\\unhbox0\\crcr\\hskip-1000pt plus1fill\\char{cedilla}\\hskip-1000pt plus1fill\\crcr}}}}}}"));
            return self.back_list(&t);
        }
        match arg[..] {
            [t] if t.cat() == Some(Cat::Letter) => self.build_accent(cedilla, f, Some(t.chr() as u8)),
            _ => self.not_native("\\c under more than a letter"),
        }
    }

    /// latex.ltx's `\def\aa{\r a}` and `\def\AA{\r A}`.
    fn ring_letter(&mut self, c: char) {
        let t = [self.doc_id(doc!("r")), Token::char(Cat::Letter, c)];
        self.back_list(&t);
    }

    /// ot1enc.def's `\l` (`\hmode@bgroup\@xxxii l\egroup`) and `\L` (`\leavevmode
    /// \setbox\z@\hbox{L}\hb@xt@\wd\z@{\hss\@xxxii L}`); `\@xxxii` is `\chardef`32.
    fn slashed_l(&mut self, capital: bool) {
        if self.mode == Mode::Math {
            return self.not_native("\\l or \\L in math");
        }
        if self.leave_vmode() {
            return;
        }
        // T1's `\DeclareTextSymbol{\L}{T1}{138}` and `{\l}{T1}{170}`.
        if self.t1() {
            let c = char::from(if capital { 138 } else { 170 });
            return self.back_list(&[Token::char(Cat::Letter, c)]);
        }
        let t = self.tex_tokens(if capital { "\\setbox0\\hbox{L}\\hbox to\\wd0{\\hss\\char32 L}" } else { "{\\char32 l}" });
        self.back_list(&t);
    }

    /// `\newtheorem{env}{Title}[within]` or `\newtheorem{env}[shared]{Title}` (`\@ynthm`,
    /// `\@xnthm`, `\@othm`): the environment and its counter.
    fn new_theorem(&mut self) {
        let ams = self.amsthm();
        let star = ams && self.next_is('*');
        let text = |arg: &[Token]| -> String { arg.iter().map(|t| t.chr()).collect::<String>().trim().to_string() };
        let mut env = Vec::new();
        self.read_arg(&mut env);
        let shared = if !star && self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut title = Vec::new();
        self.read_arg(&mut title);
        let within = if !star && shared.is_none() && self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let env = text(&env);
        let shared = shared.map(|s| text(&s));
        let within = within.map(|w| text(&w));
        let Some(doc) = self.doc.as_mut() else { return };
        let k = doc.theorems.len();
        let counter = match &shared {
            Some(s) => match doc.theorems.iter().position(|t| t.env == *s) {
                Some(c) => doc.theorems[c].counter,
                None => return self.not_native("\newtheorem sharing a counter that is not a theorem's"),
            },
            None => k,
        };
        let mut parent = None;
        let within = match within.as_deref() {
            None => None,
            Some(w) => match ["section", "subsection", "subsubsection"].iter().position(|l| *l == w) {
                Some(l) => Some(l),
                // A theorem's counter: one it owns, not shared (`\c@<env>`).
                None => match doc.theorems.iter().position(|t| t.env == w) {
                    Some(p) if doc.theorems[p].counter == p && doc.theorems[p].parent.is_none() && !doc.packages.contains(&"hyperref") => {
                        parent = Some(p);
                        None
                    }
                    _ => return self.not_native("\\newtheorem numbered within that counter"),
                },
            },
        };
        let style = if ams { doc.thm_style } else { 0 };
        doc.theorems.push(Theorem { env, title, counter, within, style, star, parent });
        doc.thm_values.push(0);
    }

    fn amsthm(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"amsthm"))
    }

    /// amsthm's `\theoremstyle{plain}`, `{definition}` or `{remark}`.
    fn theorem_style(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let style = match name.trim() {
            "plain" => PLAIN,
            "definition" => DEFINITION,
            "remark" => REMARK,
            _ => return self.not_native("a \\theoremstyle other than plain, definition and remark"),
        };
        if let Some(doc) = self.doc.as_mut() {
            doc.thm_style = style;
        }
    }

    /// amsthm's `\@thm`: `\ifhmode\unskip\unskip\par\fi`, then the list.
    fn ams_theorem_begin(&mut self, k: usize) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.endpe = false;
        doc.thm_current = k;
        if self.mode == Mode::Horizontal {
            self.unskip();
            self.unskip();
            let par = self.cur_par();
            return self.run_ops(vec![par, Op::Call(|m| m.ams_theorem_list())]);
        }
        self.ams_theorem_list();
    }

    /// `\normalfont \trivlist`.
    fn ams_theorem_list(&mut self) {
        self.normal_font();
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: true });
        self.trivlist(|m| m.ams_theorem_head());
    }

    /// The rest of `\trivlist` and `\@thm`: `\thm@space@setup` and the style (`\th@plain`
    /// `\itshape`, `\th@definition` `\normalfont`, `\th@remark` half the space), `\@topsep`
    /// and `\@topsepadd`, `\refstepcounter`, then `\@begintheorem`'s head in `\@labels`
    /// (`\deferred@thm@head`): `\normalfont`, the head font, the title, `\textup{n}`, `{\mdseries
    /// \upshape(note)}`, `.`, `\hskip\thm@headsep`.
    fn ams_theorem_head(&mut self) {
        for r in [R_LABELWIDTH, R_LEFTMARGIN, R_ITEMINDENT] {
            self.set_rw(r, 0, false);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        let k = doc.thm_current;
        let (style, star, owner) = (doc.theorems[k].style, doc.theorems[k].star, doc.theorems[k].counter);
        let title = doc.theorems[k].title.clone();
        let mut number = None;
        if !star {
            number = Some(doc.step_theorem(owner));
        }
        // `\divide\thm@preskip\tw@` divides each part.
        let mut pre = self.rg(R_TOPSEP);
        if style == REMARK {
            pre = Glue { width: pre.width / 2, stretch: pre.stretch / 2, shrink: pre.shrink / 2, ..pre };
        }
        self.set_rg(R_ATTOPSEP, pre);
        self.set_rg(R_TOPSEPADD, pre);
        if style == PLAIN {
            self.font_declaration(Change::Shape("it"), false);
        } else {
            self.normal_font();
        }
        if let Some(n) = &number {
            self.theorem_label(owner, n);
        }
        let note = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("rt@amshead"))];
        let anchor = self.theorem_anchor_tokens();
        t.extend(anchor);
        t.extend(title);
        if let Some(n) = number {
            t.extend([Token::char(Cat::Space, ' '), self.doc_id(doc!("textup")), open]);
            t.extend(label_tokens(&n));
            t.push(close);
        }
        if let Some(note) = note {
            t.extend([Token::char(Cat::Space, ' '), open, self.doc_id(doc!("rt@thmnote")), Token::char(Cat::Other, '(')]);
            t.extend(note);
            t.extend([Token::char(Cat::Other, ')'), close]);
        }
        t.push(self.doc_id(doc!("rt@amsheadend")));
        let Some(doc) = self.doc.as_mut() else { return };
        doc.thm_item = t;
        // `\if@inlabel \indent \par \fi`.
        let mut ops = Vec::new();
        if doc.inlabel {
            ops.push(Op::Call(|m| {
                if m.mode == Mode::Vertical {
                    m.new_graf(true);
                } else {
                    let b = m.e.nodes.add_box(BoxRec { width: m.dimen_param(super::engine_pack::PAR_INDENT), ..BoxRec::default() });
                    m.cur.push(Node::new(Kind::Box, b as i32));
                }
            }));
            ops.push(self.cur_par());
        }
        ops.push(Op::Call(|m| m.ams_theorem_skips()));
        self.run_ops(ops);
    }

    /// The head's box opened (`\setbox\@labels\hbox{`), `\normalfont` and the style's head
    /// font.
    fn ams_head_open(&mut self) {
        let remark = self.doc.as_ref().is_some_and(|d| d.theorems[d.thm_current].style == REMARK);
        self.open_box(HBOX_GROUP, Ctx::Set { n: LABELS_BOX, global: false }, false, 0, false);
        // The head is `\item`'s label: `\sbox`'s `\color@setgroup`.
        if self.colored() {
            let c = self.current_color();
            self.color_stack(Some(&c));
        }
        self.normal_font();
        self.font_declaration(if remark { Change::Shape("it") } else { Change::Series("bx") }, false);
    }

    /// The head's end: `\the\thm@headpunct` (`.`), `\hskip\thm@headsep` (5pt plus1pt
    /// minus1pt), the box's `}`, then `\ignorespaces`. The look-ahead of `\textup`'s
    /// `\maybe@ic` meets this native, not the period, so the italic correction stays.
    fn ams_head_close(&mut self) {
        self.build_text(b".");
        self.build_glue(Glue { width: 5 << 16, stretch: 1 << 16, shrink: 1 << 16, ..Glue::default() }, true);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        self.ignore_spaces();
    }

    /// `\deferred@thm@head`'s space: `\@nbitem` after a heading, else `\addpenalty
    /// \@beginparpenalty \addvspace\@topsep \addvspace{-\parskip}`; then `\@inlabeltrue`,
    /// `\everypar\dth@everypar`, and the head's box.
    fn ams_theorem_skips(&mut self) {
        let mut ops = Vec::new();
        if self.doc.as_ref().is_some_and(|d| d.nobreak) {
            let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
            let a = glue_add(self.rg(R_OUTERPARSKIP), neg(parskip));
            self.add_vspace(a);
        } else {
            let p = self.rw(R_BEGINPARPENALTY);
            self.add_penalty(p, &mut ops);
            ops.push(Op::Call(|m| {
                let t = m.rg(R_ATTOPSEP);
                m.add_vspace(t);
                let parskip = m.glue_param(super::engine_pack::PAR_SKIP);
                m.add_vspace(neg(parskip));
            }));
        }
        ops.push(Op::Call(|m| {
            m.set_everypar(THM_HEAD);
            let Some(doc) = m.doc.as_mut() else { return };
            doc.inlabel = true;
            let t = std::mem::take(&mut doc.thm_item);
            m.back_list(&t);
        }));
        self.run_ops(ops);
    }

    /// `\dth@everypar`: `\@minipagefalse \global\@newlistfalse \@noparitemfalse`; in a label,
    /// `{\setbox\z@\lastbox \ifvoid\z@ \kern-\itemindent \fi}\unhbox\@labels`; then the club
    /// penalty, as `\@item`'s.
    fn theorem_everypar(&mut self) {
        let f = self.nopar() & !NOPAR_ITEM;
        self.set_nopar(f);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.newlist = false;
        doc.floats.minipage = false;
        if doc.inlabel {
            doc.inlabel = false;
            if !self.drop_last_box() {
                let k = -self.rw(R_ITEMINDENT);
                self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
            }
            self.unhbox_labels();
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.nobreak {
            doc.nobreak = false;
            self.set_int_named(param!("clubpenalty"), 10000);
        } else {
            let v = self.rw(R_CLUBPENALTY);
            self.set_int_named(param!("clubpenalty"), v);
            self.set_everypar(0);
        }
    }

    /// amsthm's `proof`: `[#1]` (`\proofname`), `\par \pushQED{\qed} \normalfont
    /// \topsep6\p@\@plus6\p@ \trivlist \item[\hskip\labelsep \itshape #1\@addpunct{.}]
    /// \ignorespaces`.
    fn proof_begin(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        let label = if self.next_is('[') { self.bracket_arg() } else { vec![self.named_cs(named!("proofname"))] };
        let mut t = vec![self.doc_id(doc!("item")), Token::char(Cat::Other, '['), self.doc_id(doc!("rt@prooflabel"))];
        t.extend(label);
        t.extend([self.doc_id(doc!("rt@addpunct")), Token::char(Cat::Other, ']'), self.doc_id(doc!("rt@ignorespaces"))]);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.thm_item = t;
        let par = self.cur_par();
        self.run_ops(vec![
            par,
            Op::Call(|m| {
                m.normal_font();
                m.set_rg(R_TOPSEP, Glue { width: 6 << 16, stretch: 6 << 16, ..Glue::default() });
                let parskip = m.glue_param(super::engine_pack::PAR_SKIP);
                m.set_rg(R_PARSEP, parskip);
                let Some(doc) = m.doc.as_mut() else { return };
                doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: true });
                m.trivlist(|m| m.theorem_item());
            }),
        ]);
    }

    /// amsthm's `\@addpunct{.}`: `\ifhmode \ifnum\spacefactor>\@m \else.\fi\fi`.
    fn add_punct(&mut self) {
        if self.mode == Mode::Horizontal && self.aux <= 1000 {
            self.input.back(Token::char(Cat::Other, '.'));
        }
    }

    /// amsthm's `\qed`: in a formula `\quad\hbox{\qedsymbol}`, else `\leavevmode\unskip
    /// \penalty9999 \hbox{}\nobreak\hfill \quad\hbox{\qedsymbol}`.
    fn qed(&mut self) {
        if self.mode != Mode::Math {
            if self.leave_vmode() {
                return;
            }
            self.unskip();
            self.build_penalty(9999);
            self.build_hbox(|_| {});
            self.build_penalty(10000);
            self.build_glue(super::engine_box::fil_glue(super::engine_prim::Prim::Hfill), false);
        }
        self.build_quad();
        // `\hbox{`, closed by a `}` token.
        self.input.align_state += 1;
        self.open_box(HBOX_GROUP, Ctx::Shift(0), false, 0, false);
        let t = [self.named_cs(named!("qedsymbol")), Token::char(Cat::EndGroup, '}')];
        self.back_list(&t);
    }

    /// amsthm's `\openbox`: `\leavevmode \hbox to.77778em{\hfil\vrule \vbox to.675em{\hrule
    /// width.6em\vfil\hrule}\vrule\hfil}`.
    fn open_box_symbol(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let (em, ex) = self.em_ex();
        let dimen = |t: &str| scan_dimen_ref(t, em, ex, &|_| None).unwrap_or(0);
        let hfil = |m: &mut Self| m.build_glue(super::engine_box::fil_glue(super::engine_prim::Prim::Hfil), false);
        self.open_box(HBOX_GROUP, Ctx::Shift(0), true, dimen(".77778em"), false);
        hfil(self);
        self.build_rule([DEFAULT_RULE, RUNNING, RUNNING]);
        self.open_box(VBOX_GROUP, Ctx::Shift(0), true, dimen(".675em"), false);
        self.build_rule([dimen(".6em"), DEFAULT_RULE, 0]);
        self.build_glue(super::engine_box::fil_glue(super::engine_prim::Prim::Vfil), false);
        self.build_rule([RUNNING, DEFAULT_RULE, 0]);
        self.package();
        self.build_rule([DEFAULT_RULE, RUNNING, RUNNING]);
        hfil(self);
        self.package();
    }

    /// `\hskip\labelsep`.
    fn label_sep(&mut self) {
        let w = self.rw(R_LABELSEP);
        self.build_glue(Glue { width: w, ..Glue::default() }, true);
    }

    fn theorem_index(&self, name: &str) -> Option<usize> {
        self.doc.as_ref()?.theorems.iter().position(|t| t.env == name)
    }

    /// `\begin{env}` of a `\newtheorem`: `\@thm`'s `\refstepcounter`, then `\@begintheorem`
    /// or `\@opargbegintheorem`: `\trivlist \item[\MakeLinkTarget{..}\hskip\labelsep
    /// {\bfseries Title\ n\ (note)}]\itshape`, and `\ignorespaces`. Without hyperref,
    /// `\MakeLinkTarget` in the label's box leaves `\smash{}`, an empty box.
    fn theorem_begin(&mut self, k: usize) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.endpe = false;
        let owner = doc.theorems[k].counter;
        let number = doc.step_theorem(owner);
        let title = doc.theorems[k].title.clone();
        self.theorem_label(owner, &number);
        let note = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let space = self.doc_id(doc!("rt@space"));
        let mut t = vec![self.doc_id(doc!("item")), Token::char(Cat::Other, '[')];
        let anchor = self.theorem_anchor_tokens();
        t.extend(anchor);
        t.extend([self.doc_id(doc!("rt@thmlabel")), Token::char(Cat::BeginGroup, '{'), self.doc_id(doc!("bfseries"))]);
        t.extend(title);
        t.push(space);
        t.extend(label_tokens(&number));
        if let Some(note) = note {
            t.extend([space, Token::char(Cat::Other, '(')]);
            t.extend(note);
            t.push(Token::char(Cat::Other, ')'));
        }
        t.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::Other, ']'), self.doc_id(doc!("rt@thmbody"))]);
        // `\trivlist`: `\parsep\parskip`, then `\@trivlist`.
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.thm_item = t;
        doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: false });
        self.trivlist(|m| m.theorem_item());
    }

    /// A theorem's `\refstepcounter`: `\@currentlabel`, and with hyperref its anchor name
    /// (`theorem_href`), placed by `theorem_anchor_tokens`.
    fn theorem_label(&mut self, owner: usize, number: &str) {
        if !self.hyper() {
            return self.set_current_label(number);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.appendix {
            return self.not_native("hyperref: a theorem in the appendix");
        }
        let counters: Vec<u32> = doc.counters.iter().map(|&c| c as u32).collect();
        let name = theorem_href(&doc.theorems[owner].env, &counters, doc.theorems[owner].within, doc.thm_values[owner] as u32);
        self.store_current_label(number);
        self.set_current_href(&name);
        if let Some(doc) = self.doc.as_mut() {
            doc.thm_anchor = Some(name);
        }
    }

    /// hyperref's `\Hy@theorem@makelinktarget`: the anchor waits for the next `\item` and is
    /// the first thing in its label, a raised link then `\kern\z@` (the engine's boxes).
    fn theorem_anchor_tokens(&mut self) -> Vec<Token> {
        let Some(name) = self.doc.as_mut().and_then(|d| d.thm_anchor.take()) else { return Vec::new() };
        let mut t = self.text_raised_link(name);
        t.extend(self.tex_tokens("\\kern0pt "));
        t
    }

    /// After `\@trivlist`: `\labelwidth\z@ \leftmargin\z@ \itemindent\z@`, then the
    /// theorem's `\item[...]`.
    fn theorem_item(&mut self) {
        for r in [R_LABELWIDTH, R_LEFTMARGIN, R_ITEMINDENT] {
            self.set_rw(r, 0, false);
        }
        let t = self.doc.as_mut().map(|d| std::mem::take(&mut d.thm_item)).unwrap_or_default();
        self.back_list(&t);
    }

    /// `\begin{center}`, `\begin{flushleft}` and `\begin{flushright}` (`kind` 0, 1, 2):
    /// `\trivlist`, then `\centering`, `\raggedright` or `\raggedleft`, then `\item\relax`.
    fn center(&mut self, kind: usize) {
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: true, auto_item: true, endpe_false: false });
        }
        let then: Native = match kind {
            0 => |m| m.ragged(0),
            1 => |m| m.ragged(1),
            _ => |m| m.ragged(2),
        };
        self.trivlist(then);
    }

    /// After `\trivlist`: the alignment of [`Self::center`]'s `kind`, then `\item\relax`.
    fn ragged(&mut self, kind: usize) {
        for r in [R_LABELWIDTH, R_LEFTMARGIN, R_ITEMINDENT] {
            self.set_rw(r, 0, false);
        }
        self.ragged_skips(kind);
        if let Some(doc) = self.doc.as_mut() {
            doc.noitemarg = true;
        }
        self.item_start(None);
    }

    /// `\centering`, `\raggedright`, `\raggedleft` as declarations: their skips, and
    /// `\let\\\@centercr`.
    fn ragged_declaration(&mut self, kind: usize) {
        self.ragged_skips(kind);
        if let Some(id) = self.doc.as_ref().map(|d| d.newline_id) {
            self.e.eqtb.set_meaning(id, Meaning::Doc(CENTER_CR as u16), false);
        }
    }

    /// The skips of [`Self::center`]'s `kind` (`0` `\centering`, `1` `\raggedright`, `2`
    /// `\raggedleft`).
    fn ragged_skips(&mut self, kind: usize) {
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        match kind {
            // `\centering`: `\rightskip\@flushglue \leftskip\@flushglue
            // \finalhyphendemerits\z@ \parindent\z@ \parfillskip\z@skip`.
            0 => {
                self.set_glue_named(param!("rightskip"), fil);
                self.set_glue_named(param!("leftskip"), fil);
                self.set_glue_named(param!("parfillskip"), Glue::default());
            }
            // `\raggedright`: `\@rightskip\@flushglue \rightskip\@rightskip
            // \finalhyphendemerits\z@ \leftskip\z@skip \parindent\z@`.
            1 => {
                self.set_rg(R_ATRIGHTSKIP, fil);
                self.set_glue_named(param!("rightskip"), fil);
                self.set_glue_named(param!("leftskip"), Glue::default());
            }
            // `\raggedleft`: `\rightskip\z@skip \leftskip\@flushglue
            // \finalhyphendemerits\z@ \parindent\z@ \parfillskip\z@skip`.
            _ => {
                self.set_glue_named(param!("rightskip"), Glue::default());
                self.set_glue_named(param!("leftskip"), fil);
                self.set_glue_named(param!("parfillskip"), Glue::default());
            }
        }
        self.set_int_named(param!("finalhyphendemerits"), 0);
        self.set_dimen_named(param!("parindent"), 0);
    }

    /// `\list`'s start, before its setup argument: `\@listdepth`, the margins zero, and
    /// `\@list<depth>` (size10.clo's `\@listI` to `\@listvi`).
    fn list_open(&mut self, list: List) -> bool {
        let depth = self.rw(R_LISTDEPTH);
        if depth > 5 {
            self.error("Too deeply nested");
            return false;
        }
        self.set_rw(R_LISTDEPTH, depth + 1, true);
        for r in [R_RIGHTMARGIN, R_LISTPARINDENT, R_ITEMINDENT] {
            self.set_rw(r, 0, false);
        }
        let depth = depth as usize + 1;
        let margin = self.rw(R_LEFTMARGINI + depth - 1);
        self.set_rw(R_LEFTMARGIN, margin, false);
        if depth > 1 {
            let w = margin - self.rw(R_LABELSEP);
            self.set_rw(R_LABELWIDTH, w, false);
        }
        // `\@listi` (as the last size command defined it) to `\@listvi`, from the class table.
        let (mask, glues) = match depth {
            1 => {
                let k = self.e.eqtb.word(LISTI);
                self.size_row(if k > 0 { k as usize - 1 } else { NORMALSIZE }).listi
            }
            _ => self.doc.as_ref().map_or((0, [[0; 5]; 4]), |d| d.class.table.list(depth)),
        };
        for (k, r) in [R_TOPSEP, R_PARSEP, R_ITEMSEP, R_PARTOPSEP].into_iter().enumerate() {
            if mask & 1 << k != 0 {
                self.set_rg(r, glue_words(glues[k]));
            }
        }
        // parskip's `\@listI` (`\normalsize`'s `\@listi`), `\@listii` and `\@listiii`.
        if self.parskip() && depth <= 3 && (depth > 1 || matches!(self.e.eqtb.word(LISTI), 0) || self.e.eqtb.word(LISTI) == NORMALSIZE as i32 + 1) {
            self.parskip_list();
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(list);
        }
        true
    }

    /// parskip.sty (v2.0h, no options) as it loads: `\parskip=.5\baselineskip plus 2pt`,
    /// `\parindent` 0, `\partopsep` 0 and its `\@listi` run once.
    fn parskip_load(&mut self) {
        let b = self.e.eqtb.glue(param!("baselineskip")).width;
        let g = Glue { width: crate::backend::scaled::xn_over_d(b, 0x8000, 0x10000), stretch: 2 << 16, ..Glue::default() };
        self.set_glue_named(param!("parskip"), g);
        self.set_dimen_named(param!("parindent"), 0);
        self.set_rg(R_PARTOPSEP, Glue::default());
        self.parskip_list();
    }

    /// parskip's lists: `\topsep\z@ \parsep\parskip \itemsep\z@`.
    fn parskip_list(&mut self) {
        let p = self.e.eqtb.glue(param!("parskip"));
        self.set_rg(R_TOPSEP, Glue::default());
        self.set_rg(R_PARSEP, p);
        self.set_rg(R_ITEMSEP, Glue::default());
    }

    pub(super) fn parskip(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"parskip"))
    }

    /// `\@trivlist`: `\@topsepadd`, and `\unskip\par` in horizontal mode; then `then`.
    fn trivlist(&mut self, then: Native) {
        if let Some(doc) = self.doc.as_mut() {
            doc.trivlist_then = then;
        }
        let mut add = self.rg(R_TOPSEP);
        if self.mode == Mode::Vertical {
            add = glue_add(add, self.rg(R_PARTOPSEP));
            self.set_rg(R_TOPSEPADD, add);
            return self.trivlist_rest();
        }
        self.set_rg(R_TOPSEPADD, add);
        self.unskip();
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Call(|m| m.trivlist_rest())]);
    }

    /// The rest of `\@trivlist`.
    fn trivlist_rest(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.inlabel {
            self.set_nopar(NOPAR_ITEM | NOPAR_LIST);
        } else {
            if doc.newlist {
                self.error("Something's wrong--perhaps a missing \\item");
            }
            let f = self.nopar() & !NOPAR_LIST;
            self.set_nopar(f);
            let a = self.rg(R_TOPSEPADD);
            self.set_rg(R_ATTOPSEP, a);
        }
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        let t = glue_add(self.rg(R_ATTOPSEP), parskip);
        self.set_rg(R_ATTOPSEP, t);
        // `\leftskip\z@skip \rightskip\@rightskip \parfillskip\@flushglue`.
        self.set_glue_named(param!("leftskip"), Glue::default());
        let right = self.rg(R_ATRIGHTSKIP);
        self.set_glue_named(param!("rightskip"), right);
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        // `\@setpar`, `\global\@newlisttrue`, `\@outerparskip\parskip`.
        let Some(doc) = self.doc.as_mut() else { return };
        doc.newlist = true;
        let id = doc.par_id;
        let then = doc.trivlist_then;
        self.e.eqtb.set_meaning(id, Meaning::Doc(LIST_PAR as u16), false);
        self.set_rg(R_OUTERPARSKIP, parskip);
        then(self);
    }

    /// The rest of `\list`: `\parskip\parsep`, `\parindent\listparindent`, `\linewidth`,
    /// `\@totalleftmargin`, `\parshape`; then `\item\relax` or `\ignorespaces`.
    fn list_tail(&mut self) {
        let last = self.doc.as_ref().and_then(|d| d.lists.last().copied());
        let parsep = self.rg(R_PARSEP);
        self.set_glue_named(param!("parskip"), parsep);
        let i = self.rw(R_LISTPARINDENT);
        self.set_dimen_named(param!("parindent"), i);
        let w = self.rw(R_LINEWIDTH) - self.rw(R_RIGHTMARGIN) - self.rw(R_LEFTMARGIN);
        self.set_rw(R_LINEWIDTH, w, false);
        let t = self.rw(R_TOTALLEFTMARGIN) + self.rw(R_LEFTMARGIN);
        self.set_rw(R_TOTALLEFTMARGIN, t, false);
        self.e.eqtb.set_par_shape(vec![[t, w]], false);
        if last.is_some_and(|l| l.auto_item) {
            if let Some(doc) = self.doc.as_mut() {
                doc.noitemarg = true;
            }
            return self.item_start(None);
        }
        self.ignore_spaces();
    }

    /// `\ignorespaces`.
    fn ignore_spaces(&mut self) {
        if let Some(x) = self.next_non_blank() {
            self.input.back(x.t);
        }
    }

    /// `\unskip`.
    fn unskip(&mut self) {
        if self.tail().is_some_and(|t| t.kind == Kind::Glue) {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
    }

    /// The step `\par` is now: LaTeX's `\par` in a list, after one, or the primitive.
    fn cur_par(&self) -> Op {
        match self.doc.as_ref().map(|d| self.e.eqtb.meaning(d.par_id)) {
            Some(Meaning::Doc(i)) if i as usize == LIST_PAR => Op::Call(|m| m.list_par()),
            Some(Meaning::Doc(i)) if i as usize == ENDPE_PAR => Op::Call(|m| m.endpe_par()),
            _ => Op::Par,
        }
    }

    /// `\@trivlist`'s `\par`: nothing before the first `\item`, else `{\@@par}`, whose group
    /// keeps the list's `\parshape`.
    fn list_par(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.newlist) {
            return;
        }
        self.run_ops(vec![Op::Call(|m| m.begin_group(SIMPLE_GROUP)), Op::Par, Op::Call(|m| m.end_group(SIMPLE_GROUP))]);
    }

    /// `\@doendpe`'s `\par`: `\@restorepar\clubpenalty\@clubpenalty\@endpefalse\everypar{}
    /// \par`.
    fn endpe_par(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.endpe = false;
        let m = if doc.lists.is_empty() { doc.par_prim } else { Meaning::Doc(LIST_PAR as u16) };
        let id = doc.par_id;
        self.e.eqtb.set_meaning(id, m, false);
        let v = self.rw(R_CLUBPENALTY);
        self.set_int_named(param!("clubpenalty"), v);
        self.set_everypar(0);
        self.input.back(Token::cs(id));
    }

    /// `\@endpetrue`: global, and again after the group in a simple or semi-simple group
    /// (`\propagate@doendpe`).
    fn endpe_true(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = true;
        }
        if matches!(self.groups.last(), Some(&SIMPLE_GROUP) | Some(&SEMI_SIMPLE_GROUP)) {
            let t = self.doc_id(PROPAGATE);
            let level = self.e.eqtb.level();
            self.after_group.push((level, t));
        }
    }

    /// `\propagate@doendpe`: `\if@endpe \@doendpe \fi`.
    fn propagate(&mut self) {
        if !self.doc.as_ref().is_some_and(|d| d.endpe) {
            return;
        }
        // `\@doendpe`: `\@endpetrue`, `\par` and `\everypar` redefined.
        self.endpe_true();
        if let Some(id) = self.doc.as_ref().map(|d| d.par_id) {
            self.e.eqtb.set_meaning(id, Meaning::Doc(ENDPE_PAR as u16), false);
        }
        self.set_everypar(ENDPE);
    }

    /// `\endlist`: `\@listdepth` back, `\endtrivlist`, then `\end`'s `\endgroup`.
    fn end_list(&mut self) {
        let d = self.rw(R_LISTDEPTH);
        self.set_rw(R_LISTDEPTH, d - 1, true);
        self.end_trivlist();
    }

    /// `\endtrivlist`, then `\end`'s `\endgroup`.
    fn end_trivlist(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.inlabel {
            // `\leavevmode \global\@inlabelfalse`.
            return self.run_ops(vec![
                Op::Call(|m| {
                    if m.mode == Mode::Vertical {
                        m.new_graf(true);
                    }
                }),
                Op::Call(|m| {
                    if let Some(doc) = m.doc.as_mut() {
                        doc.inlabel = false;
                    }
                    m.end_list_par();
                }),
            ]);
        }
        self.end_list_par();
    }

    fn end_list_par(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.newlist {
            doc.newlist = false;
            self.error("Something's wrong--perhaps a missing \\item");
        }
        if self.mode == Mode::Horizontal {
            self.unskip();
            let par = self.cur_par();
            return self.run_ops(vec![par, Op::Call(|m| m.end_list_skip())]);
        }
        self.end_list_skip();
    }

    /// `\endtrivlist`'s skips and `\@endparenv` (`\addpenalty\@endparpenalty
    /// \addvspace\@topsepadd\@endpetrue`), then the group's end.
    fn end_list_skip(&mut self) {
        let mut ops = Vec::new();
        if self.nopar() & NOPAR_LIST == 0 {
            let last = self.last_skip();
            if last.width > 0 {
                self.vskip(neg(last));
                let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
                let a = glue_add(glue_add(last, parskip), neg(self.rg(R_OUTERPARSKIP)));
                self.vskip(a);
            }
            let p = self.rw(R_ENDPARPENALTY);
            self.add_penalty(p, &mut ops);
            ops.push(Op::Call(|m| {
                let g = m.rg(R_TOPSEPADD);
                m.add_vspace(g);
                m.endpe_true();
            }));
        }
        ops.push(Op::Call(|m| {
            if let Some(doc) = m.doc.as_mut() {
                if doc.lists.pop().is_some_and(|l| l.endpe_false) {
                    doc.endpe = false;
                }
            }
            m.end_group(SEMI_SIMPLE_GROUP);
        }));
        self.run_ops(ops);
    }

    /// `\item`: `\@ifnextchar[`, else `\@noitemargtrue \@item[\@itemlabel]`.
    fn item(&mut self) {
        if self.mode == Mode::Math {
            return self.error("\\item is not allowed in math mode");
        }
        if self.doc.as_ref().is_none_or(|d| d.lists.is_empty()) {
            return self.error("Lonely \\item--perhaps a missing list environment");
        }
        let label = if self.next_is('[') {
            let mut l = Vec::new();
            let mut depth = 0;
            while let Some(t) = self.get_token() {
                match t.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => depth -= 1,
                    _ if depth == 0 && t == Token::char(Cat::Other, ']') => break,
                    _ => {}
                }
                l.push(t);
            }
            Some(l)
        } else {
            if let Some(doc) = self.doc.as_mut() {
                doc.noitemarg = true;
            }
            None
        };
        self.item_start(label);
    }

    /// `\@item`'s vertical part: the previous item's end, `\@beginparpenalty` and `\@topsep`
    /// before the first item, `\@itempenalty` and `\itemsep` before the others.
    fn item_start(&mut self, label: Option<Vec<Token>>) {
        let nopar = self.nopar();
        let Some(doc) = self.doc.as_mut() else { return };
        doc.item_label = label;
        if nopar & NOPAR_ITEM != 0 {
            // `\@donoparitem`.
            self.set_nopar(nopar & !NOPAR_ITEM);
            let lm = self.rw(R_LEFTMARGIN);
            self.push_nest(Mode::Horizontal, true, 1000);
            let g = self.e.nodes.glue_node(Glue { width: -lm, ..Glue::default() }, 0);
            self.cur.push(g);
            self.unhbox_labels();
            let g = self.e.nodes.glue_node(Glue { width: lm, ..Glue::default() }, 0);
            self.cur.push(g);
            self.set_labels();
            if !self.doc.as_ref().is_some_and(|d| d.floats.minipage) {
                let last = self.last_skip();
                self.vskip(neg(last));
                let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
                let a = glue_add(glue_add(last, self.rg(R_OUTERPARSKIP)), neg(parskip));
                self.vskip(a);
            }
            return self.item_rest();
        }
        let mut ops = Vec::new();
        if doc.inlabel {
            ops.push(Op::Call(|m| {
                if m.mode == Mode::Vertical {
                    m.new_graf(true);
                } else {
                    let b = m.e.nodes.add_box(BoxRec { width: m.dimen_param(super::engine_pack::PAR_INDENT), ..BoxRec::default() });
                    m.cur.push(Node::new(Kind::Box, b as i32));
                }
            }));
            ops.push(self.cur_par());
        }
        ops.push(Op::Call(|m| {
            if m.mode == Mode::Horizontal {
                m.unskip();
                m.unskip();
                let par = m.cur_par();
                return m.run_ops(vec![par, Op::Call(|m| m.item_skips())]);
            }
            m.item_skips();
        }));
        self.run_ops(ops);
    }

    fn item_skips(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let mut ops = Vec::new();
        if doc.newlist {
            if doc.nobreak {
                // `\@nbitem`.
                let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
                let a = glue_add(self.rg(R_OUTERPARSKIP), neg(parskip));
                self.add_vspace(a);
            } else {
                let p = self.rw(R_BEGINPARPENALTY);
                self.add_penalty(p, &mut ops);
                ops.push(Op::Call(|m| {
                    let t = m.rg(R_ATTOPSEP);
                    m.add_vspace(t);
                    let parskip = m.glue_param(super::engine_pack::PAR_SKIP);
                    m.add_vspace(neg(parskip));
                }));
            }
        } else {
            let p = self.rw(R_ITEMPENALTY);
            self.add_penalty(p, &mut ops);
            ops.push(Op::Call(|m| {
                let s = m.rg(R_ITEMSEP);
                m.add_vspace(s);
            }));
        }
        ops.push(Op::Call(|m| {
            if let Some(doc) = m.doc.as_mut() {
                doc.inlabel = true;
            }
            m.item_rest();
        }));
        self.run_ops(ops);
    }

    /// `\@item`'s end: `\everypar`, the counter, `\sbox\@tempboxa{\makelabel{#1}}`, then
    /// `\@labels` (`\rt@itemlabels`).
    fn item_rest(&mut self) {
        self.set_everypar(ITEM);
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(list) = doc.lists.last().copied() else { return };
        let given = doc.item_label.take();
        if doc.noitemarg {
            doc.noitemarg = false;
            if list.numbered {
                // `\refstepcounter\@listctr`: the counters within it reset.
                let v = self.rw(list.counter) + 1;
                self.set_rw(list.counter, v, true);
                for r in list.counter + 1..R_ENUMI + 4 {
                    self.set_rw(r, 0, true);
                }
                // `\@currentlabel`: `\p@enum<d>\theenum<d>`; in `thebibliography`
                // `\p@enumiv` is empty and `\theenumiv` `\@arabic\c@enumiv`.
                let bib = list.label == Label::Bib;
                let k = list.counter - R_ENUMI;
                let the = |m: &Self, j: usize| enum_form(m.enum_style(j as u8 + 1), m.rw(R_ENUMI + j));
                let mut text = if bib { v.to_string() } else { the(self, 0) };
                if k >= 1 && !bib {
                    if k >= 2 {
                        text.push('(');
                    }
                    text.push_str(&the(self, 1));
                    if k >= 2 {
                        text.push(')');
                        text.push_str(&the(self, 2));
                    }
                    if k >= 3 {
                        text.push_str(&the(self, 3));
                    }
                }
                // enumitem's `label`: `\theenum<d>` is `{label}` and `\p@enum<d>` empty.
                let enit = !bib && self.doc.as_ref().is_some_and(|d| d.lists.iter().any(|l| matches!(l.label, Label::Enit(..))));
                let toks = if enit { self.enit_current(k) } else { Vec::new() };
                // hyperref's `\item`: `\stepcounter{Item}`, the anchor `Item.N` where the
                // kernel's `\refstepcounter` runs (after the item's skip, in vertical mode).
                if enit && !self.hyper() {
                    let span = self.e.toks.store(&toks);
                    self.e.eqtb.set_toks(CURRENT_LABEL, span, false);
                } else if enit {
                    if self.mode != Mode::Vertical {
                        return self.not_native("hyperref: an item not in vertical mode");
                    }
                    let Some(doc) = self.doc.as_mut() else { return };
                    doc.items += 1;
                    let name = format!("Item.{}", doc.items);
                    let span = self.e.toks.store(&toks);
                    self.e.eqtb.set_toks(CURRENT_LABEL, span, false);
                    self.set_current_href(&name);
                    self.vertical_anchor(name);
                } else if bib && self.hyper() {
                    self.store_current_label(&text);
                } else if self.hyper() {
                    if self.mode != Mode::Vertical {
                        return self.not_native("hyperref: an item not in vertical mode");
                    }
                    let Some(doc) = self.doc.as_mut() else { return };
                    doc.items += 1;
                    let name = format!("Item.{}", doc.items);
                    self.store_current_label(&text);
                    self.set_current_href(&name);
                    self.vertical_anchor(name);
                } else {
                    self.set_current_label(&text);
                }
            }
        }
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let mut t = vec![self.named_cs(named!("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Other, '='), self.named_cs(named!("hbox")), open]);
        // `\sbox`'s `\color@setgroup` (`\begingroup\set@color`) and, after the label,
        // `\color@endgroup`: the pop comes inside the box.
        let colored = self.colored();
        if colored {
            t.extend([self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@setcolor"))]);
        }
        // `\makelabel`: `\hss\llap{#1}` (itemize, enumerate) or `\@mklab` (`\hfil #1`).
        let enumitem = self.doc.as_ref().is_some_and(|d| d.packages.contains(&"enumitem"));
        match list.make {
            Make::Llap => {
                // enumitem's `\enit@align@right`: `\nobreak\hss\llap{#1}` (enumitem.sty:503).
                if enumitem {
                    t.push(self.named_cs(named!("penalty")));
                    t.extend("10000 ".chars().map(|c| Token::char(if c == ' ' { Cat::Space } else { Cat::Other }, c)));
                }
                t.extend([self.named_cs(named!("hss")), self.named_cs(named!("hbox"))]);
                self.word_tokens("to", &mut t);
                t.push(Token::char(Cat::Other, '0'));
                self.word_tokens("pt", &mut t);
                t.extend([open, self.named_cs(named!("hss"))]);
            }
            Make::Mklab => t.push(self.named_cs(named!("hfil"))),
            Make::Plain => {}
            // enumitem's `\makelabel`: `\labelsep\z@`, then `\descriptionlabel` of
            // `\enit@align@left`'s `\nobreak#1\hfil` (enumitem.sty:505, 1474).
            Make::Description => {
                t.push(self.named_cs(named!("hskip")));
                Self::sp_tokens(if enumitem { 0 } else { self.rw(R_LABELSEP) }, &mut t);
                t.push(self.doc_id(DESC_FONT));
                if enumitem {
                    t.push(self.named_cs(named!("penalty")));
                    t.extend("10000 ".chars().map(|c| Token::char(if c == ' ' { Cat::Space } else { Cat::Other }, c)));
                }
            }
        }
        // hyperref's `\@bibitem`: `\Hy@raisedlink{\hyper@anchorstart{cite.key}…}` in
        // vertical mode after `\H@item` (`\@lbibitem`'s is in the given label).
        let bib_anchor = list.label == Label::Bib && given.is_none() && self.doc.as_ref().is_some_and(|d| d.anchor.is_some());
        match given {
            Some(l) => t.extend(l),
            None => self.item_label(list, &mut t),
        }
        if list.make == Make::Description && enumitem {
            t.push(self.named_cs(named!("hfil")));
        }
        if list.make == Make::Llap {
            t.push(close);
        }
        if colored {
            t.push(self.named_cs(named!("endgroup")));
        }
        t.extend([close, self.doc_id(ITEM_LABELS)]);
        if bib_anchor {
            t.push(self.doc_id(ANCHOR));
        }
        self.back_list(&t);
    }

    /// `\@itemlabel`: `\labelitem<d>` is the mark (`\rt@labelmark`); `\labelenum<d>` is
    /// `\theenum<d>.`, `(\theenumii)`, in the current font.
    fn item_label(&mut self, list: List, t: &mut Vec<Token>) {
        let n = self.rw(list.counter);
        let o = |c| Token::char(Cat::Other, c);
        match list.label {
            Label::Empty => {}
            Label::Item(_) => t.push(self.doc_id(LABEL_MARK)),
            Label::Enum(1) => {
                t.extend(n.to_string().chars().map(o));
                t.push(o('.'));
            }
            Label::Enum(2) => {
                t.push(o('('));
                t.extend(alph(n, b'a').map(|c| Token::char(Cat::Letter, c)));
                t.push(o(')'));
            }
            Label::Enum(3) => {
                t.extend(roman(n).chars().map(|c| Token::char(Cat::Letter, c)));
                t.push(o('.'));
            }
            Label::Enum(_) => {
                t.extend(alph(n, b'A').map(|c| Token::char(Cat::Letter, c)));
                t.push(o('.'));
            }
            Label::Tmpl(_, i) | Label::Given(i) => self.enum_template_tokens(i as usize, n, t),
            // `\enit@normlabel` keeps `\@itemlabel` in braces.
            Label::Enit(_, i) => {
                t.push(Token::char(Cat::BeginGroup, '{'));
                self.enum_template_tokens(i as usize, n, t);
                t.push(Token::char(Cat::EndGroup, '}'));
            }
            Label::Bib => {
                t.push(o('['));
                t.extend(n.to_string().chars().map(o));
                t.push(o(']'));
            }
        }
    }

    /// enumerate.sty's `\@@enum@[#1]` (enumerate.sty:61), after `\@enumdepth` is advanced:
    /// each `A`, `a`, `i`, `I`, `1` of the template outside braces is `\theenum<d>` in that
    /// style (the last one sets it), the rest is copied; `\labelenum<d>` is the template.
    /// `\settowidth\leftmargin<d>{\the\@enLab\hspace{\labelsep}}` with the counter at 7
    /// is measured in `\@tempboxa`, then `enum_template_open` runs `\@enum@`.
    fn enum_template(&mut self) {
        let mut arg = self.bracket_arg();
        // A delimited argument loses the braces around all of it.
        if arg.first().and_then(|t| t.cat()) == Some(Cat::BeginGroup) {
            let mut depth = 0;
            let end = arg.iter().position(|t| {
                match t.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => depth -= 1,
                    _ => {}
                }
                depth == 0
            });
            if end == Some(arg.len() - 1) {
                arg = arg[1..arg.len() - 1].to_vec();
            }
        }
        let (mut parts, mut style, mut depth) = (Vec::new(), 0, 0);
        for t in arg {
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ => {}
            }
            let c = ['A', 'a', 'i', 'I'].into_iter().find(|&c| t == Token::char(Cat::Letter, c)).or((t == Token::char(Cat::Other, '1')).then_some('1'));
            match c {
                Some(c) if depth == 0 => {
                    style = c as u8;
                    parts.push(None);
                }
                _ => parts.push(Some(t)),
            }
        }
        if style == 0 {
            return self.not_native("enumerate: a label template without a counter");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.enum_templates.push((parts, style));
        let i = doc.enum_templates.len() - 1;
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let mut t = vec![self.named_cs(named!("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Other, '='), self.named_cs(named!("hbox")), open, open]);
        self.enum_template_tokens(i, 7, &mut t);
        t.extend([close, self.named_cs(named!("hskip"))]);
        Self::sp_tokens(self.rw(R_LABELSEP), &mut t);
        t.extend([close, self.doc_id(doc!("rt@enumtmpl"))]);
        self.back_list(&t);
    }

    /// enumerate.sty's `\@enum@` after the template's width: `\leftmargin<d>` set, then
    /// `\list{\csname label\@enumctr\endcsname}{\usecounter{\@enumctr}\def\makelabel##1{\hss\llap{##1}}}`.
    fn enum_template_open(&mut self) {
        let w = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        self.e.eqtb.replace_box(TEMP_BOX, None);
        let d = self.rw(R_ENUMDEPTH) as usize;
        self.set_rw(R_LEFTMARGINI + d - 1, w, false);
        let i = self.doc.as_ref().map_or(0, |doc| doc.enum_templates.len() - 1);
        let list = List { label: Label::Tmpl(d as u8, i as u16), make: Make::Llap, numbered: true, counter: R_ENUMI + d - 1, centercr: false, auto_item: false, endpe_false: false };
        self.set_rw(list.counter, 0, true);
        if !self.list_open(list) {
            return;
        }
        self.trivlist(|m| m.list_tail());
    }

    /// Template `i` with `\theenum<d>` at `n`.
    fn enum_template_tokens(&self, i: usize, n: i32, t: &mut Vec<Token>) {
        let Some((parts, style)) = self.doc.as_ref().and_then(|d| d.enum_templates.get(i)) else { return };
        for p in parts {
            match p {
                Some(tok) => t.push(*tok),
                None => t.extend(enum_form(*style, n).chars().map(|c| Token::char(if c.is_ascii_digit() { Cat::Other } else { Cat::Letter }, c))),
            }
        }
    }

    /// The style of `\theenum<d>`: the template's of the innermost open list at depth `d`,
    /// else the kernel's (`\arabic`, `\alph`, `\roman`, `\Alph`).
    fn enum_style(&self, d: u8) -> u8 {
        if let Some(doc) = self.doc.as_ref() {
            for l in doc.lists.iter().rev() {
                match l.label {
                    Label::Tmpl(e, i) if e == d => return doc.enum_templates[i as usize].1,
                    Label::Enum(e) | Label::Enit(e, _) if e == d => break,
                    _ => {}
                }
            }
        }
        b"1aiA"[d.clamp(1, 4) as usize - 1]
    }

    /// enumitem's `\enit@enumerate@i`/`\enit@itemize@i` (enumitem.sty:1347) after `\list`'s
    /// `\@list<d>`: `\enit@preset` sets the keys `\setlist` kept for `list`,
    /// `list<\@listdepth>`, the environment and the environment at its depth, then the
    /// optional argument's; `\enit@calcleft` (enumitem.sty:874) with `leftmargin=*`
    /// measures the label with each counter at its widest (`\AddEnumerateCounter`: `0`,
    /// `m`, `M`, `viii`, `VIII`) in `enit_left`; then `\@trivlist`. A value length is
    /// `\setlength`'s (`\enit@setlength`): the engine's assignment reads it.
    fn enit_setup(&mut self, enumerate: bool) {
        let d = self.rw(if enumerate { R_ENUMDEPTH } else { R_ITEMDEPTH }) as u8;
        let ld = self.rw(R_LISTDEPTH) as u8;
        let kind = if enumerate { 1 } else { 2 };
        let mut keys: Vec<Vec<Token>> = Vec::new();
        if let Some(doc) = self.doc.as_ref() {
            for (n, l) in [(0, 0), (0, ld), (kind, 0), (kind, d)] {
                if let Some(k) = doc.enit_set.iter().find(|s| s.0 == n && s.1 == l) {
                    keys.push(k.2.clone());
                }
            }
        }
        if self.next_is('[') {
            keys.push(self.bracket_arg());
        }
        let (mut star, mut label) = (false, None);
        let zero = Glue::default();
        for (key, v) in keys.iter().flat_map(|k| enit_pairs(k)) {
            let flag = v.is_empty() || v.iter().map(|t| t.chr()).eq("true".chars());
            match key.as_str() {
                "topsep" | "itemsep" | "parsep" | "partopsep" => {
                    let r = [R_TOPSEP, R_ITEMSEP, R_PARSEP, R_PARTOPSEP][["topsep", "itemsep", "parsep", "partopsep"].iter().position(|k| *k == key).unwrap_or(0)];
                    let Some(g) = self.enit_value(&v, true) else { return };
                    self.set_rg(r, g);
                }
                "nosep" | "noitemsep" | "nolistsep" if flag => {
                    for r in [R_ITEMSEP, R_PARSEP] {
                        self.set_rg(r, zero);
                    }
                    if key != "noitemsep" {
                        self.set_rg(R_PARTOPSEP, zero);
                        let t = if key == "nosep" { zero } else { self.glue_of("0pt plus .1pt") };
                        self.set_rg(R_TOPSEP, t);
                    }
                }
                "leftmargin" if v == [Token::char(Cat::Other, '*')] => star = true,
                "leftmargin" | "labelsep" | "labelwidth" | "itemindent" | "rightmargin" | "listparindent" => {
                    let r = [R_LEFTMARGIN, R_LABELSEP, R_LABELWIDTH, R_ITEMINDENT, R_RIGHTMARGIN, R_LISTPARINDENT][["leftmargin", "labelsep", "labelwidth", "itemindent", "rightmargin", "listparindent"].iter().position(|k| *k == key).unwrap_or(0)];
                    let Some(g) = self.enit_value(&v, false) else { return };
                    self.set_rw(r, g.width, false);
                    // `\enit@calcset`: a value after `*` cancels it.
                    star &= key != "leftmargin";
                }
                "label" => label = Some(v),
                _ => return self.not_native("an enumitem key the native job does not take"),
            }
        }
        if let Some(l) = label {
            let Some(i) = self.enit_template(&l, enumerate) else {
                return self.not_native("enumitem: a label with two counter styles, or a counter in itemize");
            };
            if let Some(list) = self.doc.as_mut().and_then(|doc| doc.lists.last_mut()) {
                list.label = if enumerate { Label::Enit(d, i as u16) } else { Label::Given(i as u16) };
            }
        }
        if !star {
            return self.trivlist(|m| m.list_tail());
        }
        // `\settowidth\labelwidth{\@itemlabel}` (`\setbox\@tempboxa\hbox{{#3}}`) with the
        // counter at its widest.
        let Some(list) = self.doc.as_ref().and_then(|doc| doc.lists.last().copied()) else { return };
        let style = match list.label {
            Label::Enit(_, i) | Label::Given(i) => self.doc.as_ref().map_or(0, |doc| doc.enum_templates[i as usize].1),
            Label::Enum(e) => self.enum_style(e),
            _ => 0,
        };
        let widest = match style {
            b'a' | b'A' => 13,
            b'i' | b'I' => 8,
            _ => 0,
        };
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let mut t = vec![self.named_cs(named!("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Other, '='), self.named_cs(named!("hbox")), open, open]);
        if list.numbered {
            let saved = self.rw(list.counter);
            self.set_rw(list.counter, widest, true);
            self.item_label(list, &mut t);
            self.set_rw(list.counter, saved, true);
        } else {
            self.item_label(list, &mut t);
        }
        t.extend([close, close, self.doc_id(doc!("rt@enitleft"))]);
        self.back_list(&t);
    }

    /// `\enit@calcleft` for `leftmargin=*` (enumitem.sty:874), after the label's width:
    /// `\labelwidth` the width, `\leftmargin` less `\leftmargin+\itemindent-\labelsep
    /// -\labelindent-\labelwidth` (`\labelindent` is 0); then `\@trivlist`.
    fn enit_left(&mut self) {
        let w = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        self.e.eqtb.replace_box(TEMP_BOX, None);
        let lm = self.rw(R_LEFTMARGIN);
        let rest = lm + self.rw(R_ITEMINDENT) - self.rw(R_LABELSEP) - w;
        self.set_rw(R_LABELWIDTH, w, false);
        self.set_rw(R_LEFTMARGIN, lm - rest, false);
        self.trivlist(|m| m.list_tail());
    }

    /// A key's value read by the engine's assignment (`#1 #2\relax`): glue, or a dimension
    /// as the width; `None` (the job declines) when the value is not one. An expression
    /// (`\dimexpr…`) takes the `\relax` as its end; a marker after it shows the value ended.
    fn enit_value(&mut self, v: &[Token], glue: bool) -> Option<Glue> {
        let relax = self.named_cs(named!("relax"));
        let end = self.doc_id(doc!("rt@fi"));
        let mut t = v.to_vec();
        t.extend([relax, end]);
        self.back_list(&t);
        let g = if glue { self.scan_glue(false) } else { Glue { width: self.scan_dimen(false).0, ..Glue::default() } };
        let mut next = self.get_token();
        if next == Some(relax) {
            next = self.get_token();
        }
        if next != Some(end) {
            self.not_native("an enumitem value that is not a length");
            return None;
        }
        Some(g)
    }

    /// enumitem's `label` (`\enit@normlabel`): each `\arabic*`, `\alph*`, `\Alph*`,
    /// `\roman*`, `\Roman*` the list's counter, kept as a template; `None` for two styles,
    /// or a counter outside `enumerate`.
    fn enit_template(&mut self, label: &[Token], enumerate: bool) -> Option<usize> {
        let star = Token::char(Cat::Other, '*');
        let (mut parts, mut style, mut k) = (Vec::with_capacity(label.len()), 0u8, 0);
        while k < label.len() {
            let t = label[k];
            let s = t.cs_id().and_then(|id| match self.e.cs.name(id) {
                "arabic" => Some(b'1'),
                "alph" => Some(b'a'),
                "Alph" => Some(b'A'),
                "roman" => Some(b'i'),
                "Roman" => Some(b'I'),
                _ => None,
            });
            match s {
                Some(s) if label.get(k + 1) == Some(&star) => {
                    if !enumerate || style != 0 && style != s {
                        return None;
                    }
                    style = s;
                    parts.push(None);
                    k += 2;
                }
                _ => {
                    parts.push(Some(t));
                    k += 1;
                }
            }
        }
        let doc = self.doc.as_mut()?;
        doc.enum_templates.push((parts, style));
        Some(doc.enum_templates.len() - 1)
    }

    /// `\@currentlabel` of an item at enumerate depth `k + 1` when an enumitem label is
    /// open: `\p@enum<d>\theenum<d>` as latex.ltx composes them (`\theenumi`,
    /// `\theenumi\theenumii`, `\theenumi(\theenumii)\theenumiii`, then `\theenumiv`), a
    /// level with a `label` key being `{label}` (`\enit@reflabel`, which also empties its
    /// own `\p@enum<d>`).
    fn enit_current(&self, k: usize) -> Vec<Token> {
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let enit = |j: usize| -> Option<u16> {
            for l in doc.lists.iter().rev() {
                match l.label {
                    Label::Enit(e, i) if e as usize == j + 1 => return Some(i),
                    Label::Enum(e) | Label::Tmpl(e, _) if e as usize == j + 1 => return None,
                    _ => {}
                }
            }
            None
        };
        let the = |j: usize, t: &mut Vec<Token>| match enit(j) {
            Some(i) => {
                t.push(Token::char(Cat::BeginGroup, '{'));
                self.enum_template_tokens(i as usize, self.rw(R_ENUMI + j), t);
                t.push(Token::char(Cat::EndGroup, '}'));
            }
            None => t.extend(label_tokens(&enum_form(self.enum_style(j as u8 + 1), self.rw(R_ENUMI + j)))),
        };
        let mut t = Vec::new();
        if k == 0 || enit(k).is_some() {
            the(k, &mut t);
            return t;
        }
        the(0, &mut t);
        if k >= 2 {
            t.push(Token::char(Cat::Other, '('));
        }
        the(1, &mut t);
        if k >= 2 {
            t.push(Token::char(Cat::Other, ')'));
            the(2, &mut t);
        }
        if k >= 3 {
            the(3, &mut t);
        }
        t
    }

    /// enumitem's `\setlist[names]{keys}` (enumitem.sty:1645): the keys kept for each name
    /// (`enumerate`, `itemize`, `description`; none is `list`) at each level (none is 0),
    /// each replacing what was kept for it.
    fn set_list(&mut self) {
        let names = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        let text: String = names.iter().map(|t| t.chr()).collect();
        let (mut kinds, mut levels) = (Vec::new(), Vec::new());
        for p in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match p {
                "enumerate" => kinds.push(1),
                "itemize" => kinds.push(2),
                "description" => kinds.push(3),
                _ => match p.parse::<u8>() {
                    Ok(l) => levels.push(l),
                    Err(_) => return self.not_native("\\setlist of a list the native job does not keep"),
                },
            }
        }
        if kinds.is_empty() {
            kinds.push(0);
        }
        if levels.is_empty() {
            levels.push(0);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        for &n in &kinds {
            for &l in &levels {
                doc.enit_set.retain(|s| s.0 != n || s.1 != l);
                doc.enit_set.push((n, l, keys.clone()));
            }
        }
    }

    /// `\labelitem<d>`: `\labelitemfont` (`\normalfont`) and `\textbullet`, `\bfseries
    /// \textendash`, `\textasteriskcentered`, `\textperiodcentered`.
    fn label_mark(&mut self) {
        let Some(Label::Item(d)) = self.doc.as_ref().and_then(|d| d.lists.last()).map(|l| l.label) else { return };
        let (_, size) = self.font_key();
        let key = self.rm_key("m/n");
        self.select_nfss(&key, size);
        let (enc, shape, c) = match d {
            1 => ("TS1", "m/n", 0x88),
            // `\textendash`: OT1 123, T1 21.
            2 if self.t1() => ("T1", "bx/n", 21),
            2 => ("OT1", "bx/n", 0x7b),
            3 => ("TS1", "m/n", 0x2a),
            _ => ("TS1", "m/n", 0xb7),
        };
        let key = format!("{enc}/{}/{shape}", self.family(0));
        self.select_nfss(&key, size);
        self.append_char(c);
    }

    /// `\@labels`'s new contents: `\unhbox\@labels \hskip\itemindent \hskip-\labelwidth
    /// \hskip-\labelsep`, the label (`\hbox to\labelwidth` when narrower), `\hskip\labelsep`.
    fn item_labels(&mut self) {
        let w = self.rw(R_LABELWIDTH);
        let sep = self.rw(R_LABELSEP);
        let indent = self.rw(R_ITEMINDENT);
        self.push_nest(Mode::Horizontal, true, 1000);
        self.unhbox_labels();
        for g in [indent, -w, -sep] {
            let n = self.e.nodes.glue_node(Glue { width: g, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        if let Some(tmp) = self.e.eqtb.box_reg(TEMP_BOX) {
            self.e.eqtb.replace_box(TEMP_BOX, None);
            if self.e.nodes.boxes[tmp as usize].width > w {
                self.cur.push(Node::new(Kind::Box, tmp as i32));
            } else {
                let r = self.e.nodes.boxes[tmp as usize];
                self.push_nest(Mode::Horizontal, true, 1000);
                self.cur.extend_from_slice(self.e.nodes.get(r.list));
                self.e.nodes.free_block(r.list);
                self.e.nodes.free_box(tmp);
                let start = self.start as usize;
                let b = self.hpack(start, w, true);
                self.pop_nest();
                let b = self.e.nodes.add_box(b);
                self.cur.push(Node::new(Kind::Box, b as i32));
            }
        }
        let n = self.e.nodes.glue_node(Glue { width: sep, ..Glue::default() }, 0);
        self.cur.push(n);
        self.set_labels();
        // microtype's `item` patch: `\ignorespaces\leftprotrusion`, where `\ignorespaces`
        // meets `\leftprotrusion` and expands nothing.
        if self.microtype() {
            return self.left_protrusion();
        }
        self.ignore_spaces();
    }

    /// `\unhbox\@labels` into the current list.
    fn unhbox_labels(&mut self) {
        if let Some(b) = self.e.eqtb.box_reg(LABELS_BOX) {
            self.e.eqtb.replace_box(LABELS_BOX, None);
            let r = self.e.nodes.boxes[b as usize];
            self.cur.extend_from_slice(self.e.nodes.get(r.list));
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(b);
        }
    }

    /// `\global\setbox\@labels\hbox{…}` from the current (inner) list, which it closes.
    fn set_labels(&mut self) {
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        let b = self.e.nodes.add_box(b);
        self.e.eqtb.set_box(LABELS_BOX, Some(b), true);
    }

    /// `\@item`'s `\everypar`.
    fn item_everypar(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.newlist = false;
        doc.floats.minipage = false;
        if doc.inlabel {
            doc.inlabel = false;
            // `{\setbox\z@\lastbox \ifvoid\z@ \kern-\itemindent \fi}`.
            if !self.drop_last_box() {
                let k = -self.rw(R_ITEMINDENT);
                self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
            }
            // `\box\@labels \penalty\z@`.
            if let Some(b) = self.e.eqtb.box_reg(LABELS_BOX) {
                self.e.eqtb.replace_box(LABELS_BOX, None);
                self.cur.push(Node::new(Kind::Box, b as i32));
            }
            self.cur.push(Node::new(Kind::Penalty, 0));
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.nobreak {
            doc.nobreak = false;
            self.set_int_named(param!("clubpenalty"), 10000);
        } else {
            let v = self.rw(R_CLUBPENALTY);
            self.set_int_named(param!("clubpenalty"), v);
            self.set_everypar(if self.microtype() { MT_PROT } else { 0 });
        }
    }

    /// Whether the document loads microtype.
    pub(super) fn microtype(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"microtype"))
    }

    /// microtype's `\leftprotrusion` before the next token, read unexpanded past spaces as
    /// its `\futurelet` reads it (`\MT@prot@get@first@token`): before a character, a kern of
    /// the character's margin kern (`\MT@get@prot`), if that is not zero. A brace, a font
    /// switch or a text font command (`\MT@prot@check@cmds`) is passed and the search goes
    /// on after it (a kern makes the same list there, since they add no node); `\relax`,
    /// `\ignorespaces` and `\@empty` are dropped. Another command gives no kern, except a
    /// text symbol or accent (its character's), which is not native yet.
    pub(super) fn left_protrusion(&mut self) {
        const SWITCHES: [&str; 22] = ["rmfamily", "sffamily", "ttfamily", "mdseries", "bfseries", "upshape", "itshape", "slshape", "scshape", "em", "normalfont", "selectfont", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];
        const TEXT_COMMANDS: [&str; 11] = ["textrm", "textsf", "texttt", "textnormal", "textbf", "textmd", "textit", "textsl", "textsc", "textup", "emph"];
        let t = loop {
            let Some(t) = self.get_token() else { return };
            if t.cat() != Some(Cat::Space) {
                break t;
            }
        };
        let again = self.doc_id(doc!("rt@leftprotrusion"));
        match t.cat() {
            Some(Cat::Letter | Cat::Other) if (t.chr() as u32) < 128 => {
                let w = self.protrusion_of(t.chr() as u8, false);
                let mut list = Vec::new();
                if w != 0 {
                    list.extend([self.doc_id(LEAVE), self.named_cs(named!("kern"))]);
                    Self::sp_tokens(w, &mut list);
                }
                list.push(t);
                self.back_list(&list);
            }
            Some(Cat::Letter | Cat::Other | Cat::Active) if (t.chr() as u32) >= 128 => {
                self.input.back(t);
                self.not_native("microtype: a protrusion before a UTF-8 character");
            }
            Some(Cat::BeginGroup) => self.back_list(&[t, again]),
            None => {
                let name = t.cs_id().map_or("", |id| self.e.cs.name(id));
                if matches!(name, "relax" | "ignorespaces" | "@empty") {
                    return self.left_protrusion();
                }
                if SWITCHES.contains(&name) {
                    return self.back_list(&[t, again]);
                }
                if TEXT_COMMANDS.contains(&name) {
                    let Some(open) = self.get_token() else { return };
                    if open.cat() != Some(Cat::BeginGroup) {
                        self.back_list(&[t, open]);
                        return self.not_native("microtype: a protrusion before a text command without braces");
                    }
                    return self.back_list(&[t, open, again]);
                }
                self.input.back(t);
                // A text symbol or accent (`\@current@cmd`, `\@changed@cmd` first); robust
                // commands such as `\textcolor` start with `\protect` and give no kern.
                let licr = (name.starts_with("text") && !matches!(name, "textcolor" | "textsuperscript" | "textsubscript" | "textwidth" | "textheight" | "textstyle" | "textfont" | "textfraction" | "textfloatsep")) || name.len() == 1 || matches!(name, "ss" | "ae" | "AE" | "oe" | "OE" | "aa" | "AA" | "o" | "O" | "l" | "L" | "i" | "j" | "c" | "v" | "u" | "H" | "r" | "t" | "d" | "b" | "k");
                if licr {
                    self.not_native("microtype: a protrusion before a text symbol or accent");
                }
            }
            _ => self.input.back(t),
        }
    }

    /// `\\`: in verse, center, flushleft and flushright `\@centercr` (`\unskip\par`,
    /// `\@ifstar{\nobreak…}`, `\addvspace{-\parskip}`, `\@ifnextchar[\@icentercr
    /// \ignorespaces`); elsewhere not native yet.
    fn newline(&mut self) {
        if self.in_aligned() {
            return self.aligned_cr();
        }
        if self.in_align() {
            return self.align_cr();
        }
        if self.in_eqnarray() {
            return self.eqnarray_cr();
        }
        if self.in_tabular() {
            return self.tabular_cr();
        }
        if !self.doc.as_ref().is_some_and(|d| d.lists[d.cr_base.min(d.lists.len())..].iter().any(|l| l.centercr)) {
            return self.normal_cr();
        }
        self.center_cr();
    }

    /// `\@centercr`: `\\` after `\centering` and kin.
    fn center_cr(&mut self) {
        if self.in_tabular() {
            return self.tabular_cr();
        }
        if self.mode == Mode::Horizontal {
            self.unskip();
        } else {
            self.error("There's no line here to end");
        }
        let par = self.cur_par();
        self.run_ops(vec![
            par,
            Op::Call(|m| {
                let mut ops = Vec::new();
                if m.next_is('*') {
                    ops.push(Op::Penalty(10000));
                }
                ops.push(Op::Call(|m| {
                    let parskip = m.glue_param(super::engine_pack::PAR_SKIP);
                    m.add_vspace(neg(parskip));
                    // `\@icentercr[#1]`: `\@vspace@calcify{#1}` is `\setlength\sp@ce@skip{#1}
                    // \vskip\sp@ce@skip`, the glue scanned from `#1\relax`.
                    if m.next_is('[') {
                        let mut arg = m.bracket_arg();
                        let relax = m.named_cs(named!("relax"));
                        arg.push(relax);
                        m.back_list(&arg);
                        let g = m.scan_glue(false);
                        if let Some(t) = m.get_token().filter(|&t| t != relax) {
                            m.input.back(t);
                        }
                        return m.run_ops(vec![Op::Vskip(g), Op::Call(|m| m.ignore_spaces())]);
                    }
                    m.ignore_spaces();
                }));
                m.run_ops(ops);
            }),
        ]);
    }

    /// `\\` as `\@normalcr`: `\@ifstar` (`\reserved@e` is `\vadjust`, `\reserved@f`
    /// `\nobreak`), `\@xnewline`'s `[#1]` (`\vadjust`, `\@vspace@calcify{#1}`), then
    /// `\@gnewline`: in vertical mode `\@nolnerr`, else `\unskip \reserved@e{\reserved@f#1}
    /// \nobreak \hfil \break`. Without either, `\reserved@e` is `\relax` and the braces are
    /// an empty group, which adds nothing.
    fn normal_cr(&mut self) {
        let star = self.next_is('*');
        let skip = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        if self.mode == Mode::Vertical {
            return self.error("There's no line here to end");
        }
        // `\@vspace@calcify{#1}`: `\setlength\sp@ce@skip{#1}`, the glue `#1` scans.
        let skip = match skip {
            Some(arg) => match self.glue_arg(&arg) {
                Some(g) => Some(g),
                None => return self.not_native("\\\\[...] of something not a glue"),
            },
            None => None,
        };
        self.unskip();
        // `\vadjust{\nobreak\vskip\sp@ce@skip}`, as much of it as was asked for.
        if star || skip.is_some() {
            self.build_adjust(|m| {
                if star {
                    m.build_penalty(10000);
                }
                if let Some(g) = skip {
                    m.build_glue(g, false);
                }
            });
        }
        // `\nobreak \hfil \break`.
        self.build_penalty(10000);
        self.build_glue(super::engine_box::fil_glue(super::engine_prim::Prim::Hfil), false);
        self.build_penalty(-10000);
    }

    /// `\vspace{#1}` and `\vspace*{#1}` (latex.ltx `\@vspace`, `\@vspacer`). In vertical
    /// mode, `\vskip` the glue `#1\relax` scans (`\@vspace@calcify`), then `\vskip\z@skip`;
    /// `\vspace*` puts `\hrule height0pt \nobreak` first and restores `\prevdepth` after,
    /// through `\dimen@` as LaTeX does. Otherwise the same inside `\vadjust`, between
    /// `\@bsphack` and `\@esphack` ([`Self::esphack_end`]) in horizontal mode.
    fn vspace(&mut self) {
        let star = self.next_is('*');
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        // `\@vspace@calcify`: `\setlength\sp@ce@skip{#1}\vskip\sp@ce@skip`.
        let Some(g) = self.glue_arg(&arg) else { return self.not_native("\\vspace of something not a glue") };
        // `\vskip\sp@ce@skip` then `\vskip\z@skip`; `\vspace*` first `\hrule height\z@`
        // and `\nobreak`.
        let skips = |m: &mut Self| {
            if star {
                m.build_rule([RUNNING, 0, 0]);
                m.build_penalty(10000);
            }
            m.build_glue(g, false);
            m.build_glue(Glue::default(), false);
        };
        if self.mode == Mode::Vertical {
            // `\dimen@\prevdepth` before and `\prevdepth\dimen@` after (`\@vspacer`).
            if star {
                let depth = self.aux;
                self.build_dimen_register(0, depth);
            }
            skips(self);
            if star {
                self.aux = self.dimen_register(0);
            }
            return;
        }
        // `\@bsphack\vadjust{...}\@esphack` in horizontal mode, `\vadjust` alone in math.
        let horizontal = self.mode == Mode::Horizontal;
        if horizontal {
            let saved = (self.last_skip(), self.aux);
            if let Some(doc) = self.doc.as_mut() {
                doc.sphack.push(saved);
            }
        }
        self.build_adjust(skips);
        if horizontal {
            self.esphack_end();
        }
    }

    /// `\@esphack`: `\spacefactor\@savsf`; when `\@savsk` was positive, `\nobreak
    /// \hskip\z@skip` if `\lastskip` is zero, then `\ignorespaces`.
    fn esphack_end(&mut self) {
        let Some((skip, sf)) = self.doc.as_mut().and_then(|d| d.sphack.pop()) else { return };
        if self.mode != Mode::Horizontal {
            return;
        }
        self.aux = sf;
        if skip.width > 0 {
            if self.last_skip().width == 0 {
                self.cur.push(Node::new(Kind::Penalty, 10000));
                let g = self.e.nodes.glue_node(Glue::default(), 0);
                self.cur.push(g);
            }
            self.ignore_spaces();
        }
    }

    /// Whether amsmath is loaded: natives it changes follow it (`PACKAGE_FOLLOWS`).
    /// Whether color.sty is loaded: `\normalcolor` and `\color@setgroup` then push and pop
    /// the colour (`2026-10-26-color-design.md`).
    pub(super) fn colored(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.iter().any(|p| matches!(*p, "color" | "xcolor")))
    }

    pub(super) fn ams(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"amsmath"))
    }

    /// amsmath's `\mathdisplay`: `\global\let\df@label\@empty`, the display's kind.
    fn ams_display(&mut self, kind: u8) {
        if let Some(doc) = self.doc.as_mut() {
            if doc.ams_display != 0 {
                return self.not_native("a display inside a display");
            }
            doc.ams_display = kind;
            doc.df_label = None;
            doc.df_tag = None;
        }
    }

    /// `\end{equation}` and `\end{equation*}` after amsmath: `\endmathdisplay` (with
    /// `\@eqnswtrue` the tag `\veqno\tagform@\theequation`, i.e. `\eqno\hbox{\m@th
    /// \normalfont(\ignorespaces n\unskip\/)}`, then `\ltx@label` of a stored key), `$$`,
    /// `\ignorespacesafterend`.
    fn end_ams_display(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let (kind, label, n) = (std::mem::take(&mut doc.ams_display), doc.df_label.take(), doc.equation);
        let _ = n;
        let tag = doc.df_tag.take();
        let mut t = Vec::new();
        if kind == 1 {
            t.extend([self.named_cs(named!("eqno")), self.doc_id(doc!("rt@amstag"))]);
        }
        // `\veqno\alt@tag\df@tag`: the tag's box, and `\@currentlabel` its text for the label.
        if let Some((text, star)) = tag {
            self.set_current_label(&text);
            if let Some(doc) = self.doc.as_mut() {
                doc.df_tag = Some((text, star));
            }
            t.extend([self.named_cs(named!("eqno")), self.doc_id(doc!("rt@dftag"))]);
        }
        if let Some(key) = label {
            t.extend([self.doc_id(doc!("rt@ltxlabel")), Token::char(Cat::BeginGroup, '{')]);
            t.extend(key);
            t.push(Token::char(Cat::EndGroup, '}'));
        }
        let dollar = Token::char(Cat::MathShift, '$');
        t.extend([dollar, dollar, self.doc_id(doc!("rt@endequation"))]);
        self.back_list(&t);
    }

    /// amsmath's `\nonumber` (`\notag`): in `equation`, the number stepped back
    /// (`\addtocounter{equation}\m@ne`) and no tag (`\global\@eqnswfalse`).
    fn nonumber(&mut self) {
        // In `eqnarray`, latex.ltx's `\global\@eqnswfalse`; amsmath's also lets
        // `\incr@eqnum` be empty in the entry, which no later step of the row reads.
        if let Some(e) = self.doc.as_mut().and_then(|d| d.eqnarray.as_mut()) {
            e.eqnsw = false;
            return;
        }
        // In `align`, the row's `\global\@eqnswfalse`; in a `split`'s `gather`, whose
        // `\incr@eqnum` the `equation` has done, also `\addtocounter{equation}\m@ne`.
        if let Some(doc) = self.doc.as_mut() {
            if let Some(a) = doc.align.as_mut() {
                a.eqnsw = false;
                if a.split {
                    doc.equation -= 1;
                }
                return;
            }
        }
        if let Some(doc) = self.doc.as_mut().filter(|d| d.ams_display == 1) {
            doc.equation -= 1;
            doc.ams_display = 2;
        }
    }

    /// amsmath's `\tag{text}` and `\tag*{text}` in `equation`, `equation*` or `\[ \]`
    /// (`\tag@in@display`): `\nonumber`, then `\make@df@tag@@@` (`\tagform@{text}`) or
    /// `\make@df@tag@@` (`\maketag@@@{text}`), each with `\@currentlabel` the text. The text
    /// is characters only; a second `\tag`, or one in another display, is not native.
    fn tag(&mut self) {
        // hyperref's `\Hy@make@anchor`: an `AMS.N` target from `\Hy@linkcounter`.
        if self.hyper() {
            return self.not_native("hyperref: \\tag");
        }
        let star = self.next_is('*');
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(doc) = self.doc.as_ref() else { return };
        // `\tag@in@align` in an `align` or `gather` row (not a `split`'s), `\tag@in@display`
        // in `equation`, `equation*` and `\[ \]`.
        let in_rows = self.in_tag_rows();
        let in_display = matches!(doc.ams_display, 1 | 2) && doc.align.is_none();
        if self.mode != Mode::Math || !(in_rows || in_display) || doc.eqnarray.is_some() || doc.aligned.is_some() {
            return self.not_native("\\tag outside equation, equation*, \\[ \\], align or gather");
        }
        if doc.df_tag.is_some() {
            return self.not_native("a second \\tag");
        }
        if arg.iter().any(|t| t.cs_id().is_some()) {
            return self.not_native("a \\tag with commands");
        }
        let text: String = arg.iter().map(|t| t.chr()).collect();
        self.nonumber();
        if let Some(doc) = self.doc.as_mut() {
            doc.df_tag = Some((text, star));
        }
    }

    /// `\df@tag`'s box: `\tagform@{text}` is `\hbox{\m@th\normalfont(\ignorespaces text
    /// \unskip\/)}`, `\maketag@@@{text}` `\hbox{\m@th\normalfont text}`.
    fn df_tag(&mut self) {
        let Some((text, star)) = self.doc.as_mut().and_then(|d| d.df_tag.take()) else { return };
        self.build_hbox(|m| {
            m.begin_group(SIMPLE_GROUP);
            m.normal_font();
            if star {
                m.build_text(text.as_bytes());
            } else {
                m.protrusion_kern(b'(', false);
                m.build_text(format!("({}", text.trim_start()).as_bytes());
                m.unskip();
                m.append_italic_correction();
                m.build_text(b")");
                m.protrusion_kern(b')', true);
            }
            m.end_group(SIMPLE_GROUP);
        });
    }

    /// amsmath's `\eqref{key}`: `\textup{\tagform@{\ref{key}}}`.
    fn eqref(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("\\eqref in math");
        }
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("textup")), open, self.doc_id(doc!("rt@eqrefopen")), self.doc_id(doc!("ref")), open];
        t.extend(key);
        t.extend([close, self.doc_id(doc!("rt@eqrefclose")), close]);
        self.back_list(&t);
    }

    /// A log-like operator (`\rt@op` and its flags digit, bit 0 `\nolimits`, bit 1 the
    /// `\kern\z@`): `\mathop{\kern\z@\fam\z@` here, the word and `}` after it in the macro.
    fn math_operator(&mut self) {
        let flags = self.get_token().map_or(0, |t| t.chr() as u8 - b'0');
        if self.mode != Mode::Math {
            return self.not_native("an operator outside math");
        }
        self.build_math_group(OP, if flags & 1 != 0 { 2 } else { 0 });
        if flags & 2 != 0 {
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
        }
        self.set_cur_fam(0);
    }

    /// The delimiter of a matrix's `\left` (`open`) or `\right`; `cases` opens with `\lbrace`.
    fn matrix_fence(&self, name: &str, open: bool) -> Token {
        let other = |c| Token::char(Cat::Other, c);
        match (name, open) {
            ("pmatrix", true) => other('('),
            ("pmatrix", false) => other(')'),
            ("bmatrix", true) => other('['),
            ("bmatrix", false) => other(']'),
            ("vmatrix", true) => self.named_cs(named!("lvert")),
            ("vmatrix", false) => self.named_cs(named!("rvert")),
            ("Vmatrix", true) => self.named_cs(named!("lVert")),
            ("Vmatrix", false) => self.named_cs(named!("rVert")),
            (_, true) => self.named_cs(named!("lbrace")),
            (_, false) => self.named_cs(named!("rbrace")),
        }
    }

    /// `cases` after its `\left\lbrace`: `\def\arraystretch{1.2}\array{@{}l@{\quad}l@{}}`.
    fn cases_open(&mut self) {
        let Some(id) = self.doc.as_ref().map(|d| d.stretch_id) else { return };
        let text = self.e.toks.store(&[Token::END_MATCH, Token::char(Cat::Other, '1'), Token::char(Cat::Other, '.'), Token::char(Cat::Other, '2')]);
        self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: 0 }), false);
        self.array_open_spec(None, &[b'@', b'{', b'}', b'l', b'@', b'{', tabular::QUAD, b'}', b'l', b'@', b'{', b'}']);
    }

    /// A matrix after its `\left` delimiter: `\hskip-\arraycolsep\array{*\c@MaxMatrixCols c}`.
    fn matrix_open(&mut self) {
        self.matrix_skip();
        self.array_open_spec(None, b"*{10}{c}");
    }

    /// `\@arraycr`'s strut: `\vrule height\z@ depth\@tempdima width\z@`.
    fn array_strut(&mut self) {
        let d = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(0, |t| t.strut);
        self.build_rule([0, 0, d]);
    }

    /// `\begin{equation*}` or `\end{equation*}` (`\[`, `\]` with amsmath).
    fn env_tokens(&mut self, cmd: usize) {
        let mut t = vec![self.doc_id(cmd), Token::char(Cat::BeginGroup, '{')];
        t.extend(label_tokens("equation*"));
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// `\eqref`'s `\hbox{\normalfont(` (`\ignorespaces` before `\ref` meets no space).
    fn eqref_open(&mut self) {
        self.open_box(HBOX_GROUP, Ctx::Shift(0), false, 0, false);
        self.normal_font();
        self.build_text(b"(");
    }

    /// `\unskip\/)}`.
    fn eqref_close(&mut self) {
        self.unskip();
        self.append_italic_correction();
        self.build_text(b")");
        self.package();
    }

    /// `\refstepcounter{equation}`: `\c@equation` up one, `\@currentlabel` its `\arabic`.
    pub(super) fn equation_step(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.equation += 1;
        let n = self.equation_text();
        self.set_current_label(&n);
    }

    /// article's `\appendix`: `\par\setcounter{section}{0}\setcounter{subsection}{0}
    /// \gdef\thesection{\@Alph\c@section}`. The counters are set before the `\par`, which
    /// reads none of them.
    fn appendix(&mut self) {
        if self.hyper() {
            return self.not_native("hyperref: \\appendix");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.counters[..2].fill(0);
        doc.appendix = true;
        let par = self.named_cs(named!("par"));
        self.input.back(par);
    }

    /// `\theequation`: `\arabic{equation}`, after the heading numbers once amsmath's
    /// `\numberwithin` has set a level.
    pub(super) fn equation_text(&self) -> String {
        self.doc.as_ref().map_or_else(String::new, |d| d.the_prefix[2].clone() + &within_text(&d.counters, d.appendix, d.eq_within, d.equation))
    }

    /// amsmath's `\numberwithin{equation}{level}` (`\counterwithin`): `\@addtoreset
    /// {equation}{level}` and `\theequation` after the level's number. Another counter, a
    /// level the native job does not number or the optional format is not native.
    fn number_within(&mut self) {
        if self.next_is('[') {
            return self.not_native("\\numberwithin with a format");
        }
        let (mut counter, mut level) = (Vec::new(), Vec::new());
        self.read_arg(&mut counter);
        self.read_arg(&mut level);
        let text = |a: &[Token]| a.iter().map(|t| t.chr()).collect::<String>();
        let (counter, level) = (text(&counter), text(&level));
        let Some(l) = ["section", "subsection", "subsubsection"].iter().position(|s| *s == level.trim()) else {
            return self.not_native("\\numberwithin within that level");
        };
        if counter.trim() != "equation" {
            return self.not_native("\\numberwithin of a counter other than equation");
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.eq_within = Some(l);
        }
    }

    /// amsmath's `\allowdisplaybreaks[#1]` (amsmath.sty:1147): `\interdisplaylinepenalty
    /// \getdsp@pen{#1}`, which is `\@M`, 9999, 6999, 2999 or 0 for 0 to 4 (4 by default).
    /// In the body it is a local assignment; only the preamble's is native.
    fn allow_display_breaks(&mut self) {
        let mut level = 4;
        if self.next_is('[') {
            let arg: String = self.bracket_arg().iter().map(|t| t.chr()).collect();
            match arg.trim().parse::<usize>() {
                Ok(n) if n <= 4 => level = n,
                _ => return self.not_native("\\allowdisplaybreaks with that level"),
            }
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.body {
            return self.not_native("\\allowdisplaybreaks in the body");
        }
        doc.display_pen = [10000, 9999, 6999, 2999, 0][level];
    }

    /// `\interdisplaylinepenalty` under amsmath (`Doc::display_pen`).
    pub(super) fn display_pen(&self) -> i32 {
        self.doc.as_ref().map_or(10000, |d| d.display_pen)
    }

    /// After `\endequation`'s display: `\@ignoretrue`, `\end`'s `\endgroup` (`\@endpe`
    /// is false), then `\if@ignore` `\ignorespaces`.
    pub(super) fn end_equation(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        self.ignore_spaces();
    }

    /// `\normalfont`: `\usefont` of the defaults (`OT1/cmr/m/n`) at the current size.
    fn normal_font(&mut self) {
        self.switch_font(10, |m| {
            let (_, size) = m.font_key();
            let key = m.rm_key("m/n");
            m.select_nfss(&key, size);
        });
    }

    /// `[#1]` after its `[`: the tokens to the first `]` outside braces, as TeX matches a
    /// delimited argument (§392).
    pub(super) fn bracket_arg(&mut self) -> Vec<Token> {
        let mut arg = Vec::new();
        let mut depth = 0;
        while let Some(t) = self.get_token() {
            match t {
                _ if depth == 0 && t == Token::char(Cat::Other, ']') => break,
                _ if t.cat() == Some(Cat::BeginGroup) => depth += 1,
                _ if t.cat() == Some(Cat::EndGroup) => depth -= 1,
                _ => {}
            }
            arg.push(t);
        }
        arg
    }

    /// The `size1x.clo` size commands: `\@setfontsize` (`\baselineskip`, `\selectfont` at
    /// the size), then the display skips and `\@listi` the command sets (`\normalsize`,
    /// `\small`, `\footnotesize`), all from the class table.
    fn size_command(&mut self, i: usize) {
        let (key, _) = self.font_key();
        self.apply_size(i, &key);
    }

    fn apply_size(&mut self, i: usize, key: &str) {
        self.set_size(i);
        let row = self.size_row(i);
        self.select_nfss(key, row.fsize);
        let (mask, skips) = row.display;
        for (k, r) in [param!("abovedisplayskip"), param!("abovedisplayshortskip"), param!("belowdisplayshortskip"), param!("belowdisplayskip")].into_iter().enumerate() {
            if mask & 1 << k != 0 {
                self.set_glue_named(r, glue_words(skips[k]));
            }
        }
        // `\let\@listi\@listI` and kin: the command whose `\@listi` holds, plus one (0 is
        // `\normalsize`'s, as at `\begin{document}`).
        if row.listi.0 != 0 {
            self.e.eqtb.set_word(LISTI, i as i32 + 1, false);
        }
    }

    /// article's `\maketitle` (one column): `\par`, then in a group `\newpage` and
    /// `\@maketitle`: `\null \vskip 2em`, a `center` environment with `{\LARGE\@title\par}`,
    /// `\vskip 1.5em`, `{\large \lineskip .5em \begin{tabular}[t]{c}\@author\end{tabular}
    /// \par}`, `\vskip 1em`, `{\large\@date}`; then `\par \vskip 1.5em`. `\thispagestyle
    /// {plain}` (for `\pagestyle{empty}`, `special`); `\setcounter{footnote}{0}`.
    fn make_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let [title, author, mut date] = std::mem::take(&mut doc.stored);
        doc.thanks = Some(Vec::new());
        // `\thispagestyle{plain}`: `\@specialpagetrue`, for the page this title ends up on.
        doc.special = Some(false);
        let (dated, authored) = (doc.dated, doc.authored);
        if !dated {
            date = vec![self.named_cs(named!("today"))];
        }
        let mut author = author;
        if !authored {
            author = vec![self.named_cs(named!("relax"))];
        }
        let cs = |m: &Self, n: usize| m.named_cs(n);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![cs(self, named!("par")), cs(self, named!("begingroup")), self.doc_id(NEW_PAGE), cs(self, named!("hbox")), open, close];
        let skip = |m: &mut Self, t: &mut Vec<Token>, amount: &str| {
            t.push(cs(m, named!("vskip")));
            t.extend(amount.chars().map(|c| if c.is_ascii_alphabetic() { Token::char(Cat::Letter, c) } else { Token::char(Cat::Other, c) }));
            t.push(Token::char(Cat::Space, ' '));
        };
        let env = |m: &mut Self, t: &mut Vec<Token>, cmd: usize| {
            t.extend([cs(m, cmd), open]);
            m.word_tokens("center", t);
            t.push(close);
        };
        skip(self, &mut t, "2em");
        env(self, &mut t, named!("begin"));
        // `\let\footnote\thanks`: the token microtype's `\leftprotrusion` meets after
        // `center`'s `\item\relax`.
        t.push(self.doc_id(doc!("rt@fi")));
        t.extend([open, cs(self, named!("LARGE"))]);
        t.extend(title);
        t.extend([cs(self, named!("par")), close]);
        skip(self, &mut t, "1.5em");
        t.extend([open, cs(self, named!("large")), cs(self, named!("lineskip")), Token::char(Cat::Other, '.'), Token::char(Cat::Other, '5')]);
        self.word_tokens("em", &mut t);
        t.extend([Token::char(Cat::Space, ' '), cs(self, named!("begin")), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, Token::char(Cat::Other, '['), Token::char(Cat::Letter, 't'), Token::char(Cat::Other, ']'), open, Token::char(Cat::Letter, 'c'), close]);
        t.extend(author);
        t.extend([cs(self, named!("end")), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, cs(self, named!("par")), close]);
        skip(self, &mut t, "1em");
        t.extend([open, cs(self, named!("large"))]);
        t.extend(date);
        t.push(close);
        env(self, &mut t, named!("end"));
        t.push(cs(self, named!("par")));
        skip(self, &mut t, "1.5em");
        t.extend([self.doc_id(TITLE_END), cs(self, named!("endgroup"))]);
        self.back_list(&t);
    }

    /// The kernel's `\thanks{text}` in the title (`latex.ltx:17216`): `\footnotemark` with
    /// article's title `\thefootnote` (`\@fnsymbol`) and `\@makefnmark`
    /// (`\rlap{\@textsuperscript{\normalfont\@thefnmark}}`), the text kept for `\@thanks`.
    fn thanks(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.thanks.is_none()) {
            return self.not_native("\\thanks outside \\maketitle");
        }
        if self.hyper() {
            return self.not_native("\\thanks under hyperref");
        }
        if self.leave_vmode() {
            return;
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let sf = if self.mode == Mode::Horizontal { self.aux } else { -1 };
        if self.mode == Mode::Horizontal {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.footnote += 1;
        let n = doc.footnote;
        doc.footnote_sf.push(sf);
        if let Some(t) = doc.thanks.as_mut() {
            t.push(text);
        }
        let Some(sym) = self.fn_symbol_tokens(n) else { return self.not_native("a tenth \\thanks") };
        // `\rlap`: `\hb@xt@\z@{#1\hss}`.
        let mut list = vec![self.named_cs(named!("hbox"))];
        self.word_tokens("to", &mut list);
        list.extend(ZERO_PT);
        list.push(Token::char(Cat::BeginGroup, '{'));
        self.superscript_mark(&sym, &mut list);
        list.extend([self.named_cs(named!("hss")), Token::char(Cat::EndGroup, '}'), self.doc_id(THANKS_MARK_END)]);
        self.back_list(&list);
    }

    /// `\@footnotemark`'s end in the title: `\ifhmode\spacefactor\@x@sf\fi`.
    fn thanks_mark_end(&mut self) {
        let sf = self.doc.as_mut().and_then(|d| d.footnote_sf.pop()).unwrap_or(-1);
        if self.mode == Mode::Horizontal && sf >= 0 {
            self.aux = sf;
        }
    }

    /// `\@fnsymbol` in text (`\TextOrMath`'s first branch), for marks 1 to 9.
    fn fn_symbol_tokens(&mut self, n: i32) -> Option<Vec<Token>> {
        const SYMBOLS: [&str; 9] = ["\\textasteriskcentered", "\\textdagger", "\\textdaggerdbl", "\\textsection", "\\textparagraph", "\\textbardbl", "{\\textasteriskcentered\\textasteriskcentered}", "{\\textdagger\\textdagger}", "{\\textdaggerdbl\\textdaggerdbl}"];
        let s = SYMBOLS.get(usize::try_from(n).ok()?.checked_sub(1)?)?;
        Some(self.tex_tokens(s))
    }

    /// `\@textsuperscript{\normalfont m}`: `{\m@th\ensuremath{^{\mbox{\fontsize\sf@size
    /// \sf@size\selectfont\normalfont m}}}}`.
    fn superscript_mark(&mut self, mark: &[Token], out: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let math = Token::char(Cat::MathShift, '$');
        out.extend([open, self.named_cs(named!("mathsurround")), Token::char(Cat::Other, '0')]);
        self.word_tokens("pt", out);
        out.extend([math, Token::char(Cat::Superscript, '^'), open, self.named_cs(named!("mbox")), open, self.doc_id(FN_MARK_FONT)]);
        out.extend_from_slice(mark);
        out.extend([close, close, math, close]);
    }

    /// The kernel's `\and` in the author's `tabular`: `\end{tabular}\hskip 1em \@plus.17fil
    /// \begin{tabular}[t]{c}`.
    fn title_and(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.thanks.is_none()) {
            return self.not_native("\\and outside \\maketitle");
        }
        let t = self.tex_tokens("\\end{tabular}\\hskip 1em plus.17fil\\begin{tabular}[t]{c}");
        self.back_list(&t);
    }

    /// The end of article's `\maketitle` group: `\@thanks` (each `\footnotetext[n]{text}`
    /// with the title's `\@makefntext`), then after the group `\setcounter{footnote}{0}`.
    fn title_end(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let thanks = doc.thanks.take().unwrap_or_default();
        let first = doc.footnote - thanks.len() as i32;
        let mut list = Vec::new();
        for (k, text) in thanks.into_iter().enumerate() {
            list.extend([self.doc_id(doc!("rt@thanksnote")), Token::char(Cat::BeginGroup, '{')]);
            list.extend(((first + 1 + k as i32).to_string()).chars().map(|c| Token::char(Cat::Other, c)));
            list.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::BeginGroup, '{')]);
            list.extend(text);
            list.push(Token::char(Cat::EndGroup, '}'));
        }
        list.push(self.doc_id(doc!("rt@fnreset")));
        self.back_list(&list);
    }

    /// `\footnotetext[n]{text}` of `\@thanks`: `\@thefnmark` the symbol of `n`, then
    /// `\@footnotetext{text}` (`\insert\footins{…}`).
    fn thanks_note(&mut self) {
        let mut digits = Vec::new();
        self.read_arg(&mut digits);
        let n = digits.iter().map(|t| t.chr()).collect::<String>().parse().unwrap_or(0);
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.footnote_text = text;
        doc.fn_symbol = true;
        let saved = std::mem::replace(&mut doc.footnote, n);
        doc.footnote_sf.push(saved);
        let mut list = vec![self.named_cs(named!("insert"))];
        list.extend(FOOTINS.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        list.extend([Token::char(Cat::BeginGroup, '{'), self.doc_id(FN_SETUP)]);
        self.back_list(&list);
    }

    /// `\setcounter{footnote}{0}` after `\maketitle`'s group.
    fn footnote_reset(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.footnote = 0;
        }
    }

    /// `\newpage`: `\if@nobreak\@nobreakfalse\everypar{}\fi \par \vfil \penalty-\@M`.
    fn new_page(&mut self) {
        // `\if@noskipsec \leavevmode \global\@noskipsecfalse \fi` (in the document
        // body), `\if@inlabel \leavevmode \global\@inlabelfalse \fi`.
        if self.doc.as_ref().is_some_and(|d| d.noskipsec || d.inlabel) && self.leave_vmode() {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            (doc.noskipsec, doc.inlabel) = (false, false);
        }
        if let Some(doc) = self.doc.as_mut() {
            if doc.nobreak {
                doc.nobreak = false;
                self.set_everypar(0);
            }
        }
        let par = self.cur_par();
        self.run_ops(vec![par, Op::Vskip(Glue { stretch: 65536, stretch_order: 1, ..Glue::default() }), Op::Penalty(-10000)]);
    }

    /// `\today`: `\monthname \number\day, \number\year`.
    fn today(&mut self) {
        let get = |m: &Self, n: &str| super::engine_names::param(n).map_or(0, |r| m.e.eqtb.word(r));
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        let month = MONTHS.get((get(self, "month") - 1).max(0) as usize).copied().unwrap_or("");
        let text = format!("{month} {}, {}", get(self, "day"), get(self, "year"));
        let t: Vec<Token> = text.chars().map(|c| if c == ' ' { Token::char(Cat::Space, ' ') } else if c.is_ascii_alphabetic() { Token::char(Cat::Letter, c) } else { Token::char(Cat::Other, c) }).collect();
        self.back_list(&t);
    }

    /// `\(` and `\[`: `$`, `$$`. `\[` in vertical mode first makes an empty line
    /// (`\nointerlineskip\makebox[.6\linewidth]{}`), not native yet.
    fn math_open(&mut self, display: bool) {
        if self.mode == Mode::Math {
            return self.error("Bad math environment delimiter");
        }
        // amsmath: `\[` is `\begin{equation*}`.
        if display && self.ams() {
            return self.env_tokens(doc!("begin"));
        }
        if display && self.mode == Mode::Vertical {
            return self.not_native("\\[ in vertical mode");
        }
        let d = Token::char(Cat::MathShift, '$');
        let list = [d, d];
        self.back_list(&list[..1 + display as usize]);
    }

    /// `\)` and `\]` (`$$\ignorespaces`).
    fn math_close(&mut self, display: bool) {
        if display && self.ams() {
            return self.env_tokens(doc!("end"));
        }
        if self.mode != Mode::Math || self.inner == display {
            return self.error("Bad math environment delimiter");
        }
        let d = Token::char(Cat::MathShift, '$');
        if display {
            let list = [d, d, self.named_cs(named!("ignorespaces"))];
            self.back_list(&list);
        } else {
            self.input.back(d);
        }
    }

    /// latex.ltx's frozen `\everymath`/`\everydisplay`: `\check@mathfonts`, and after a
    /// display `\ignorespaces` (`\aftergroup`).
    fn every_math(&mut self, display: bool) {
        if display {
            let level = self.e.eqtb.level();
            let t = self.named_cs(named!("ignorespaces"));
            self.after_group.push((level, t));
        }
        self.check_math_fonts();
    }

    /// Table `k` (the kernel's, or package `pkg`'s, `data_kernel`) into the job:
    /// - `\chardef`s and `\mathchardef`s (`\&`, `\sum`, `\prime`);
    /// - the log-like operators as protected macros: `\operator@font` is `\mathgroup
    ///   \symoperators`, family 0, and `\,` in math `\mskip\thinmuskip`;
    /// - the harvested macros (`\frac`, `\quad`, `\neq`), as the format has them; a name the
    ///   native job writes itself keeps its native;
    /// - delimiters: `\lbrace` is `\delimiter"4266308 `;
    /// - the math version: symbol fonts appended to `fams`, alphabets to `alphabets`, each
    ///   alphabet a `Meaning::Doc` past `DOC`;
    /// - the package's commands the native job runs as Rust natives ([`PACKAGE_NATIVES`]).
    fn install(&mut self, k: &'static crate::data::data_kernel::KernelTable<'static>, pkg: Option<&str>, fams: &mut Vec<[&'static str; 2]>, alphabets: &mut Vec<(&'static str, bool, Alphabet)>, relocate: Relocate) {
        use crate::data::data_kernel::{Def, MacroToken, MathFont};
        for (name, d) in k.chars() {
            let cs = self.e.cs.intern(name);
            let m = match d {
                Def::Char(v) if name.starts_with("sym") => Meaning::CharDef(relocate.fam(v)),
                Def::Char(v) => Meaning::CharDef(v),
                Def::MathChar(v) => Meaning::MathCharDef(relocate.at(v, 8)),
            };
            self.e.eqtb.set_meaning(cs, m, true);
        }
        // An operator is `\rt@op`, its flags, its word (`\,` the native thin space) and the
        // `}` of the group `\rt@op` opens ([`Self::math_operator`]).
        let mut t = Vec::new();
        let (op_native, thin) = (Token::cs(self.e.cs.intern("rt@op")), Token::cs(self.e.cs.intern("rt@thinmskip")));
        for op in k.ops() {
            t.clear();
            t.extend([Token::END_MATCH, op_native, Token::char(Cat::Other, char::from(b'0' + op.nolimits as u8 + 2 * op.kern as u8))]);
            let mut w = op.word;
            while !w.is_empty() {
                if let Some(rest) = w.strip_prefix("\\,") {
                    t.push(thin);
                    w = rest;
                } else {
                    t.push(Token::char(Cat::Letter, w.as_bytes()[0] as char));
                    w = &w[1..];
                }
            }
            t.push(Token::char(Cat::EndGroup, '}'));
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(op.name);
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: super::engine_prim::PROTECTED }), true);
        }
        for (name, flags, toks) in k.macros() {
            if native_name(name) {
                continue;
            }
            t.clear();
            t.extend(toks.iter().map(|tok| match *tok {
                MacroToken::Cs(n) => Token::cs(self.e.cs.intern(n)),
                MacroToken::Raw(v) => Token(v),
            }));
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags }), true);
        }
        // amsmath's accents: `\name` is `\rt@amsaccent{code}`.
        for (name, v) in k.accents() {
            t.clear();
            t.push(Token::END_MATCH);
            t.push(Token::cs(self.e.cs.intern("rt@amsaccent")));
            t.push(Token::char(Cat::BeginGroup, '{'));
            t.extend(relocate.at(v, 8).to_string().chars().map(|c| Token::char(Cat::Other, c)));
            t.push(Token::char(Cat::EndGroup, '}'));
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: super::engine_prim::PROTECTED }), true);
        }
        for (name, p) in k.lets() {
            let m = self.e.eqtb.meaning(self.e.cs.intern(p));
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, m, true);
        }
        // `\protect` is `\relax` while typesetting (`\@typeset@protect`); package macros keep
        // it before a robust command (amsmath's `\protect\relbar`).
        if pkg.is_none() {
            let relax = self.e.eqtb.meaning(self.e.cs.intern("relax"));
            let id = self.e.cs.intern("protect");
            self.e.eqtb.set_meaning(id, relax, true);
        }
        for (name, v) in k.delims() {
            t.clear();
            // `\delimiter v `: the core's own grammar after `\left` and in a formula.
            t.extend([Token::END_MATCH, Token::cs(self.e.cs.intern("delimiter"))]);
            let v = relocate.at(relocate.at(v, 8), 20);
            t.extend(v.to_string().chars().map(|c| Token::char(Cat::Other, c)));
            t.push(Token::char(Cat::Space, ' '));
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: 0 }), true);
        }
        for f in k.math_fonts() {
            let (name, same, a) = match f {
                // `\SetSymbolFont` of a family declared before the table's own (lmodern's
                // operators, letters, symbols, largesymbols) replaces its shapes.
                MathFont::Symbol { fam, normal, bold } if (fam as u32) < relocate.from && (fam as usize) < fams.len() => {
                    fams[fam as usize] = [normal, bold];
                    continue;
                }
                MathFont::Symbol { normal, bold, .. } => {
                    fams.push([normal, bold]);
                    continue;
                }
                MathFont::SymbolAlphabet { name, fam, same } => (name, same, Alphabet::Fam(relocate.fam(fam as u32) as u8)),
                MathFont::Alphabet { name, same, normal, .. } => (name, same, Alphabet::Shape(normal, None)),
            };
            let id = self.e.cs.intern(name);
            match alphabets.iter().position(|a| a.0 == name) {
                Some(i) => alphabets[i] = (name, same, a),
                None => {
                    self.e.eqtb.set_meaning(id, Meaning::Doc((DOC.len() + alphabets.len()) as u16), true);
                    alphabets.push((name, same, a));
                }
            }
        }
        for &(_, name, native) in PACKAGE_NATIVES.iter().filter(|p| Some(p.0) == pkg) {
            if let Some(i) = DOC.iter().position(|d| d.0 == native) {
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Doc(i as u16), true);
            }
        }
    }

    /// `\usepackage{a,b}` in the preamble, for packages with a native table: each loads once,
    /// after the packages it loads (`\RequirePackage`). Options, a package without a table,
    /// and `\usepackage` in the body (LaTeX's error) are not native.
    fn use_package(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\usepackage after \\begin{document}");
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.font_switch.clear();
        }
        let mut t = Vec::new();
        let mut opts = String::new();
        if let Some(open) = self.next_unblank() {
            if open == Token::char(Cat::Other, '[') {
                while let Some(x) = self.get_token() {
                    if x == Token::char(Cat::Other, ']') {
                        break;
                    }
                    if x.cs_id().is_some() {
                        return self.not_native("package options");
                    }
                    opts.push(x.chr());
                }
            } else {
                self.back_list(&[open]);
            }
        }
        self.read_arg(&mut t);
        let names: String = t.iter().filter(|x| x.cs_id().is_none()).map(|x| x.chr()).collect();
        for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if name == "geometry" {
                self.geometry_load(&opts);
                continue;
            }
            if name == "hyperref" {
                self.load_package(name);
                if let Err(what) = self.hyper_keys(&opts) {
                    return self.not_native(what);
                }
                continue;
            }
            if name == "fontenc" {
                self.font_encoding(&opts);
                continue;
            }
            if !same_options(name, &opts) {
                return self.not_native("package options");
            }
            self.load_package(name);
        }
    }

    /// `\usepackage[T1]{fontenc}`. `t1enc.def` is in the format (LaTeX 2020), so the
    /// package only sets `\encodingdefault` (`fontenc.sty:79`) and does `\usefont` of the
    /// defaults (`:116`). The T1 meanings of the text symbols are `\T1`-prefixed; with one
    /// encoding for the whole document they become the plain ones here. Any other option,
    /// several encodings, or a second load are not native.
    fn font_encoding(&mut self, opts: &str) {
        if opts.split(',').map(str::trim).filter(|o| !o.is_empty()).ne(["T1"]) {
            return self.not_native("fontenc options other than T1");
        }
        if self.t1() {
            return;
        }
        if let Some(d) = self.doc.as_mut() {
            d.t1 = true;
            d.font_switch.clear();
        }
        for (name, code) in T1_TEXT_SYMBOLS {
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::CharDef(code), true);
        }
        let (_, size) = self.font_key();
        let key = self.rm_key("m/n");
        self.select_nfss(&key, size);
    }

    /// `\usepackage[opts]{geometry}`: `\ProcessOptionsKV[c]` (the class's paper is the
    /// default, `\Gm@setdefaultpaper`), the package's keys, then `\Gm@process`. A landscape
    /// class and a second load are not native.
    fn geometry_load(&mut self, opts: &str) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.geometry.is_some() {
            if !opts.trim().is_empty() {
                self.not_native("geometry loaded twice with options");
            }
            return;
        }
        if doc.options & 8 != 0 {
            return self.not_native("geometry with a landscape class");
        }
        let mut g = super::engine_geometry::Geometry::new(PAPER_SIZES[(doc.options & 7) as usize]);
        self.load_package("geometry");
        self.geometry_keys(&mut g, opts);
    }

    /// The length `text` as `\setlength` reads it (the engine's own scanner), or `None` when
    /// something follows it.
    fn length_of(&mut self, text: &str) -> Option<i32> {
        let relax = self.named_cs(named!("relax"));
        let mut t = self.tex_tokens(text);
        t.push(relax);
        self.back_list(&t);
        let (n, _) = self.scan_dimen(false);
        let next = self.get_token();
        if next != Some(relax) {
            while self.get_token().is_some_and(|t| t != relax) {}
            return None;
        }
        Some(n)
    }

    /// `\current@color`'s literal.
    pub(super) fn current_color(&self) -> String {
        let i = self.e.eqtb.word(CURRENT_COLOR);
        match self.doc.as_ref() {
            Some(d) if i > 0 => d.colors[i as usize - 1].clone(),
            _ => DEFAULT_COLOR.to_string(),
        }
    }

    /// The literal of a colour: `name` (a `\definecolor` one or predefined), or with a model,
    /// pdftex.def's `\color@<model>` of the values. xcolor's `\definecolor` (`define`)
    /// keeps its values as printed lengths (`1.0` becomes `1`); its own RGB, HTML and mixes
    /// are not native yet. Values with spaces are refused: color.sty keeps them, xcolor
    /// drops them.
    fn color_literal(&mut self, model: Option<&str>, spec: &str, define: bool) -> Option<String> {
        let xcolor = self.doc.as_ref()?.packages.contains(&"xcolor");
        let Some(model) = model else {
            let doc = self.doc.as_ref()?;
            let named = |n: &str| doc.color_names.iter().rev().find(|(k, _)| k == n).map(|(_, l)| l.clone()).or_else(|| crate::data::data_color::named(n, xcolor).map(str::to_string));
            // xcolor's `c!p` and `c1!p!c2!..` (`\@xcolor@@`): each step mixes with the next
            // colour, white when none is named.
            if xcolor && spec.contains('!') {
                let parts: Vec<&str> = spec.split('!').collect();
                let mut cur = xcolor::Spec::from_literal(&named(parts[0])?)?;
                for pair in parts[1..].chunks(2) {
                    let other = xcolor::Spec::from_literal(&named(pair.get(1).copied().unwrap_or("white"))?)?;
                    if pair[0].is_empty() || !pair[0].bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                        return None;
                    }
                    cur = xcolor::mix(&cur, pair[0], &other)?;
                }
                return Some(cur.literal());
            }
            return named(spec);
        };
        // xcolor's HTML, and RGB in `\definecolor`, kept as rgb from `\rdivide` by 255;
        // `\color[RGB]` goes to pdftex.def's `\c@lor@RGB` (`0.59999` for 153).
        if xcolor && (model == "HTML" || model == "RGB" && define) {
            return xcolor::from_255(model, spec).map(|c| c.literal());
        }
        let parts: Vec<&str> = spec.split(',').collect();
        if parts.iter().any(|p| p.is_empty() || p.contains(char::is_whitespace)) {
            return None;
        }
        let norm = define && xcolor;
        let mut vals = Vec::with_capacity(parts.len());
        for p in &parts {
            vals.push(match model {
                // `\c@lor@RGB@rgb`: `\dimen@ #1\p@ \divide\dimen@ \@cclv \strip@pt`.
                "RGB" if !norm => strip_pt(self.length_of(&format!("{p}pt"))? / 255),
                "rgb" | "gray" | "cmyk" if norm => strip_pt(self.length_of(&format!("{p}pt"))?),
                "rgb" | "gray" | "cmyk" => p.to_string(),
                _ => return None,
            });
        }
        let v = vals.join(" ");
        Some(match (model, vals.len()) {
            ("rgb" | "RGB", 3) => format!("{v} rg {v} RG"),
            ("gray", 1) => format!("{v} g {v} G"),
            ("cmyk", 4) => format!("{v} k {v} K"),
            _ => return None,
        })
    }

    /// The text of tokens that are all characters.
    fn plain_text(t: &[Token]) -> Option<String> {
        t.iter().map(|x| x.cs_id().is_none().then(|| x.chr())).collect()
    }

    /// `\color[model]{spec}` or `\color{name}` in the body: `\current@color` set and
    /// `\set@color` (a push, the pop after the group), then `\ignorespaces`.
    fn color_command(&mut self) {
        if self.doc.as_ref().is_none_or(|d| !d.body) {
            return self.not_native("\\color in the preamble");
        }
        let model = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let (Some(model), Some(spec)) = (model.map_or(Some(None), |m| Self::plain_text(&m).map(Some)), Self::plain_text(&t)) else {
            return self.not_native("a colour with commands");
        };
        let Some(lit) = self.color_literal(model.as_deref(), spec.trim(), false) else {
            return self.not_native("a colour the native job does not know");
        };
        let Some(doc) = self.doc.as_mut() else { return };
        let i = match doc.colors.iter().position(|c| *c == lit) {
            Some(i) => i,
            None => {
                doc.colors.push(lit.clone());
                doc.colors.len() - 1
            }
        };
        self.e.eqtb.set_word(CURRENT_COLOR, i as i32 + 1, false);
        self.color_stack(Some(&lit));
        let after = [self.named_cs(named!("aftergroup")), self.doc_id(doc!("rt@resetcolor")), self.named_cs(named!("ignorespaces"))];
        self.back_list(&after);
    }

    /// `\textcolor[model]{spec}{text}`: `\leavevmode{\color[model]{spec}text}`.
    fn text_color(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![open, self.doc_id(doc!("rt@color"))];
        if self.next_is('[') {
            t.push(Token::char(Cat::Other, '['));
            t.extend(self.bracket_arg());
            t.push(Token::char(Cat::Other, ']'));
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        t.push(open);
        t.extend_from_slice(&arg);
        t.push(close);
        self.read_arg(&mut arg);
        t.extend_from_slice(&arg);
        t.push(close);
        self.back_list(&t);
    }

    /// `\definecolor{name}{model}{spec}` in the preamble.
    fn define_color(&mut self) {
        let mut a = [Vec::new(), Vec::new(), Vec::new()];
        for x in &mut a {
            self.read_arg(x);
        }
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\definecolor in the body");
        }
        let [Some(name), Some(model), Some(spec)] = a.map(|x| Self::plain_text(&x)) else {
            return self.not_native("a colour with commands");
        };
        let Some(lit) = self.color_literal(Some(model.trim()), spec.trim(), true) else {
            return self.not_native("a colour the native job does not know");
        };
        if let Some(doc) = self.doc.as_mut() {
            doc.color_names.push((name.trim().to_string(), lit));
        }
    }

    /// `\geometry{keys}` (preamble only): `\Gm@clean`, the keys, `\Gm@process`.
    fn geometry_command(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let Some(mut g) = self.doc.as_ref().filter(|d| !d.body).and_then(|d| d.geometry) else {
            return self.not_native("\\geometry after \\begin{document}");
        };
        let mut list = String::new();
        for x in &t {
            match x.cs_id() {
                Some(id) => {
                    list.push('\\');
                    list.push_str(self.e.cs.name(id));
                    list.push(' ');
                }
                None => list.push(x.chr()),
            }
        }
        g.clean();
        self.geometry_keys(&mut g, &list);
    }

    /// The keys of `list` into `g`, each length read by the engine's `\setlength`, then
    /// `\Gm@@process` into the geometry registers (`\begin{document}` reads them back).
    fn geometry_keys(&mut self, g: &mut super::engine_geometry::Geometry, list: &str) {
        if let Err(what) = g.keys(list, &mut |v| self.length_of(v)) {
            return self.not_native(what);
        }
        let dimen = |m: &Self, i: usize| m.e.eqtb.word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 });
        let twoside = self.doc.as_ref().is_some_and(|d| d.options & 16 != 0);
        let (dims, head) = g.process([dimen(self, 5), dimen(self, 6), dimen(self, 7)], twoside);
        for (i, v) in dims.into_iter().chain(head).enumerate() {
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 }, v, false);
        }
        if let Some(w) = g.footins {
            self.e.eqtb.set_glue(Reg::register(Region::Glue, FOOTINS), Glue { width: w, ..Glue::default() }, false);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.geometry = Some(*g);
        }
    }

    /// Package `name` from its native table, after the packages it loads; a second load does
    /// nothing. Its families must follow the ones in use when it was evaluated (`fam0`): the
    /// table's codes and macros name them as digits.
    fn load_package(&mut self, name: &str) {
        if NO_EFFECT_PACKAGES.contains(&name) {
            return;
        }
        if let Some(&p) = DRIVER_PACKAGES.iter().find(|&&p| p == name) {
            // tabularx's `\RequirePackage{array}`, then `\TX@delta\hfuzz`.
            if p == "tabularx" {
                self.load_package("array");
                let fuzz = self.dimen_param(super::engine_pack::HFUZZ);
                if let Some(doc) = self.doc.as_mut() {
                    doc.tx_delta = fuzz;
                }
            }
            let Some(doc) = self.doc.as_mut() else { return };
            if !doc.packages.contains(&p) {
                doc.packages.push(p);
                for &(_, cmd, native) in PACKAGE_NATIVES.iter().filter(|q| q.0 == p) {
                    if let Some(i) = DOC.iter().position(|d| d.0 == native) {
                        let id = self.e.cs.intern(cmd);
                        self.e.eqtb.set_meaning(id, Meaning::Doc(i as u16), true);
                    }
                }
                if p == "booktabs" {
                    self.booktabs_load();
                }
                if p == "fancyhdr" {
                    self.fancy_load();
                }
                // hyperref's `\RequirePackage{url}`.
                if p == "hyperref" {
                    self.load_package("url");
                }
                // siunitx's `\RequirePackage{amstext}` and `{array}` (siunitx.sty:4694, 9852);
                // its color comes at `\begin{document}`. None of its commands is native.
                if p == "siunitx" {
                    self.load_package("amstext");
                    self.load_package("array");
                }
                if p == "parskip" {
                    self.parskip_load();
                }
                if p == "url" {
                    let pens = [param!("relpenalty"), param!("binoppenalty")].map(|r| self.e.eqtb.word(r));
                    if let Some(doc) = self.doc.as_mut() {
                        doc.url_penalties = pens;
                    }
                }
            }
            return;
        }
        let Some(k) = crate::data::data_kernel::package(name) else {
            return self.not_native(&format!("package {name}"));
        };
        if self.doc.as_ref().is_none_or(|d| d.packages.contains(&name)) {
            return;
        }
        let Some((fam0, deps)) = k.package() else { return };
        for d in deps {
            self.load_package(d);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        // A table's families are numbered from `fam0`; one that adds none and names only
        // families below it (amsmath) loads after any other.
        use crate::data::data_kernel::{Def, MathFont};
        let own = k.math_fonts().any(|f| match f {
            MathFont::Symbol { fam, .. } => fam >= fam0,
            MathFont::SymbolAlphabet { fam, .. } => fam >= fam0,
            MathFont::Alphabet { .. } => false,
        })
            || k.chars().any(|(_, d)| matches!(d, Def::MathChar(v) if (v >> 8 & 15) >= fam0 as u32));
        // LaTeX numbers a package's families from the next free one: a table made alone
        // (from `fam0`) is moved up when other math fonts came first, if every family it
        // names is a math character, a `\sym…` number, a delimiter or an accent.
        let mut relocate = Relocate { from: fam0 as u32, by: 0 };
        if own && doc.fams.len() != fam0 as usize {
            let fams_after = doc.fams.len() + k.math_fonts().filter(|f| matches!(f, MathFont::Symbol { fam, .. } if *fam >= fam0)).count();
            if doc.fams.len() < fam0 as usize || fams_after > 16 || k.macro_names().next().is_some() || k.ops().next().is_some() {
                return self.not_native("a math package after other math fonts");
            }
            relocate.by = (doc.fams.len() - fam0 as usize) as u32;
        }
        let pkg = crate::data::data_kernel::packages().find(|p| p.0 == name).map_or("", |p| p.0);
        doc.packages.push(pkg);
        let (mut fams, mut alphabets) = (std::mem::take(&mut doc.fams), std::mem::take(&mut doc.alphabets));
        self.install(k, Some(pkg), &mut fams, &mut alphabets, relocate);
        // amsfonts' `\symAMSb`, the family of its wide accents.
        let msb = fams.iter().position(|&[f, _]| f == "U/msb/m/n").unwrap_or(0) as u32;
        if let Some(doc) = self.doc.as_mut() {
            (doc.fams, doc.alphabets) = (fams, alphabets);
            doc.wide_fam = msb;
            doc.math_size = "";
        }
    }

    /// `\strut` (`\relax\ifmmode\copy\strutbox\else\unhcopy\strutbox\fi`): in a paragraph
    /// or a box, the strut box's rule, `\vrule\@height.7\baselineskip\@depth.3\baselineskip
    /// \@width\z@` as `strut_update` last measured it.
    fn strut(&mut self) {
        if self.mode != Mode::Horizontal {
            return self.not_native("\\strut outside a paragraph or a box");
        }
        let (h, d) = (self.e.eqtb.word(STRUT_HT), self.e.eqtb.word(STRUT_DP));
        self.build_rule([0, h, d]);
    }

    /// multirow's `\multirow[pos]{n}[struts]{*}[move]{text}` (`\@xmultirow`, the `*` width):
    /// `\multirow@dima` is `n` times the array strut's height plus `n` times its depth
    /// (negated for `n` < 0), box 0 `\vtop to\multirow@dima{\vfill\multirowsetup\hbox{\strut
    /// text\strut}\vfill}` (no `\vfill` above for `t`, below for `b`), then the shift
    /// `multirow_end` reads.
    fn multirow(&mut self) {
        // `\leavevmode` before the box: in a cell or a paragraph it does nothing.
        if self.mode != Mode::Horizontal {
            return self.not_native("\\multirow outside a paragraph or a cell");
        }
        let pos = if self.next_is('[') { self.bracket_arg() } else { vec![Token::char(Cat::Letter, 'c')] };
        let pos = match pos[..] {
            [t] if t.cs_id().is_none() && matches!(t.chr(), 'c' | 't' | 'b') => t.chr(),
            _ => return self.not_native("a \\multirow position other than c, t or b"),
        };
        let mut n = Vec::new();
        self.read_arg(&mut n);
        let digits: String = n.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
        let digits = digits.trim();
        let body = digits.strip_prefix('-').unwrap_or(digits);
        if body.is_empty() || !body.bytes().all(|c| c.is_ascii_digit() || c == b'.') || body.bytes().filter(|&c| c == b'.').count() > 1 {
            return self.not_native("a \\multirow row count that is not a number");
        }
        if self.next_is('[') {
            let struts = self.bracket_arg();
            if !struts.iter().all(|t| t.cs_id().is_none() && t.chr() == '0') {
                return self.not_native("\\multirow big struts");
            }
        }
        let mut width = Vec::new();
        self.read_arg(&mut width);
        if !matches!(width[..], [t] if t.cs_id().is_none() && t.chr() == '*') {
            return self.not_native("a \\multirow width other than *");
        }
        let shift = if self.next_is('[') {
            let arg = self.bracket_arg();
            self.dimen_arg(arg)
        } else {
            0
        };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        // `#2\ht\@arstrutbox` and `#2\dp\@arstrutbox`, each scanned as TeX does.
        let o = |c: char| Token::char(Cat::Other, c);
        let box_dimen = |m: &mut Self, which: usize| {
            let mut t: Vec<Token> = digits.chars().map(o).collect();
            t.push(m.named_cs(which));
            t.extend(ARSTRUT_BOX.to_string().chars().map(o));
            t.push(Token::char(Cat::Space, ' '));
            m.dimen_arg(t)
        };
        let mut dima = box_dimen(self, named!("ht")) + box_dimen(self, named!("dp"));
        let negative = digits.starts_with('-') && body.bytes().any(|c| matches!(c, b'1'..=b'9'));
        if negative {
            dima = -dima;
        }
        let (ht, dp) = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or((0, 0), |b| {
            let b = &self.e.nodes.boxes[b as usize];
            (b.height, b.depth)
        });
        let positive = !negative && body.bytes().any(|c| matches!(c, b'1'..=b'9'));
        // `\multirow@dima` after the box, `t` adding `\ht0` at the end.
        let after = if positive {
            match pos {
                't' => 0,
                'b' => ht + dp,
                _ => ht,
            }
        } else {
            match pos {
                'b' => dima,
                't' => dima - dp - ht,
                _ => dima - dp,
            }
        } + shift;
        let cs = |m: &Self, i: usize| m.named_cs(i);
        let sp = |v: i32| v.to_string().chars().map(o).chain("sp".chars().map(|c| Token::char(Cat::Letter, c))).collect::<Vec<_>>();
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![cs(self, named!("setbox")), o('0'), cs(self, named!("vtop")), Token::char(Cat::Letter, 't'), Token::char(Cat::Letter, 'o')];
        t.extend(sp(dima));
        t.push(open);
        if pos != 't' {
            t.push(cs(self, named!("vfill")));
        }
        t.extend([cs(self, named!("multirowsetup")), cs(self, named!("hbox")), open, self.doc_id(doc!("strut"))]);
        t.extend(text);
        t.extend([self.doc_id(doc!("strut")), close]);
        if pos != 'b' {
            t.push(cs(self, named!("vfill")));
        }
        t.extend([close, self.doc_id(if pos == 't' { doc!("rt@multirowendt") } else { doc!("rt@multirowend") })]);
        t.extend(sp(after));
        t.push(Token::char(Cat::Space, ' '));
        self.back_list(&t);
    }

    /// The end of `\@xmultirow`: `\multirow@dima` (read here; `t` adds `\ht0`), `\leavevmode
    /// \setbox0\vtop{\vskip-\multirow@dima\box0\vss}\dp0=\z@\box0`.
    fn multirow_end(&mut self, top: bool) {
        let (mut dima, _) = self.scan_dimen(false);
        if top {
            dima += self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].height);
        }
        let o = |c: char| Token::char(Cat::Other, c);
        let sp = |v: i32| v.to_string().chars().map(o).chain("sp".chars().map(|c| Token::char(Cat::Letter, c))).collect::<Vec<_>>();
        let cs = |m: &Self, i: usize| m.named_cs(i);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![cs(self, named!("setbox")), o('0'), cs(self, named!("vtop")), open, cs(self, named!("vskip"))];
        t.extend(sp(-dima));
        t.extend([cs(self, named!("box")), o('0'), cs(self, named!("vss")), close, cs(self, named!("dp")), o('0'), o('=')]);
        t.extend(sp(0));
        t.extend([cs(self, named!("box")), o('0')]);
        self.back_list(&t);
    }

    /// A native the main loop's lookahead (§1038) expands, as `get_x_token` expands the macro
    /// it stands for, so a word's ligatures and kerns go on through it: a UTF-8 character's
    /// lead byte, and in T1 a text accent whose composite is a character
    /// (`\@text@composite`). `true` when its tokens are back in the input; a T1 accent that
    /// is not a composite gets its argument back, to be obeyed after the word.
    pub(in crate::engine) fn lookahead_doc(&mut self, x: super::engine_run::X) -> bool {
        let Meaning::Doc(i) = x.m else { return false };
        if i as usize == UTF8_LEAD {
            self.utf8_char(x.t);
            return true;
        }
        if !self.t1() || self.mode == Mode::Math {
            return false;
        }
        let Some(k) = DOC.get(i as usize).and_then(|d| T1_ACCENT_CMD.iter().position(|&c| d.0.len() == 1 && d.0.as_bytes()[0] == c)) else { return false };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let base = match arg[..] {
            [t] if t.cat() == Some(Cat::Letter) && t.chr() == 'i' && T1_ACCENT_CMD[k] == b'=' => None,
            [t] if t.cat() == Some(Cat::Letter) && t.chr().is_ascii_alphabetic() => Some(t.chr() as u16),
            _ => None,
        };
        if let Some(Ok(j)) = base.map(|b| T1_COMPOSITE_KEYS.binary_search(&((T1_ACCENT_CMD[k] as u16) << 8 | b))) {
            let c = char::from(T1_COMPOSITE_SLOTS[j]);
            self.back_list(&[Token::char(Cat::Letter, c)]);
            return true;
        }
        let mut t = Vec::with_capacity(arg.len() + 2);
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(arg);
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
        false
    }

    /// A UTF-8 character, from its active lead byte `lead` (utf8.def's `\UTFviii@two@octets`,
    /// `three`, `four`): the next bytes, then the text of its `\u8:` macro inside `\IeC`
    /// (`data_utf8`), read again.
    pub(super) fn utf8_char(&mut self, lead: Token) {
        let b = lead.chr() as u32;
        let more = if b < 0xE0 { 1 } else if b < 0xF0 { 2 } else { 3 };
        let mut c = b & (0x7F >> (more + 1));
        for _ in 0..more {
            let next = self.get_token().filter(|t| t.cs_id().is_none() && (0x80..0xC0).contains(&(t.chr() as u32)));
            let Some(t) = next else { return self.not_native("a UTF-8 character cut short") };
            c = c << 6 | (t.chr() as u32 & 0x3F);
        }
        let Some(text) = crate::data::data_utf8::text(c) else { return self.not_native("a UTF-8 character the kernel does not declare") };
        let toks = self.tex_tokens(text);
        self.back_list(&toks);
    }

    /// The tokens of `text` as TeX reads it under the document's catcodes: control words
    /// (skipping the spaces after them), control symbols, braces, letters, spaces and other
    /// characters. For `data_utf8` entries and the kernel's short definitions made of
    /// primitives (`\\r A`, `\\l`).
    pub(super) fn tex_tokens(&mut self, text: &str) -> Vec<Token> {
        let s = text.as_bytes();
        let mut out = Vec::with_capacity(s.len());
        let mut i = 0;
        while i < s.len() {
            let c = s[i];
            i += 1;
            out.push(match c {
                b'\\' => {
                    let n = s[i..].iter().take_while(|c| c.is_ascii_alphabetic()).count().max(1);
                    let name = &text[i..i + n];
                    i += n;
                    if name.as_bytes()[0].is_ascii_alphabetic() {
                        i += s[i..].iter().take_while(|&&c| c == b' ').count();
                    }
                    Token::cs(self.e.cs.intern(name))
                }
                b'{' => Token::char(Cat::BeginGroup, '{'),
                b'}' => Token::char(Cat::EndGroup, '}'),
                b' ' => Token::char(Cat::Space, ' '),
                b'#' => Token::char(Cat::Param, '#'),
                b'&' => Token::char(Cat::AlignTab, '&'),
                b'$' => Token::char(Cat::MathShift, '$'),
                c if c.is_ascii_alphabetic() => Token::char(Cat::Letter, char::from(c)),
                c => Token::char(Cat::Other, char::from(c)),
            });
        }
        out
    }

    /// `booktabs.sty`'s registers and their values (`\heavyrulewidth=.08em`, ... in the
    /// font of the `\usepackage`, `\cmidrulesep=\doublerulesep`). The registers are hidden
    /// ones ([`BT_BASE`] on): the native job keeps no `\count10`/`\count11` to allocate from,
    /// and a register's number changes nothing on the page.
    fn booktabs_load(&mut self) {
        let mut regs = [Reg::register(Region::Int, 0); 17];
        for (k, &(name, region)) in BT_REGS.iter().enumerate() {
            regs[k] = Reg::register(region, BT_BASE + k as u32);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Reg(regs[k]), true);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.booktabs = Some(regs);
        }
        let ch = |c: char| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c);
        let mut t = Vec::new();
        for (name, value) in [("heavyrulewidth", ".08em"), ("lightrulewidth", ".05em"), ("cmidrulewidth", ".03em"), ("belowrulesep", ".65ex"), ("belowbottomsep", "0pt"), ("aboverulesep", ".4ex"), ("abovetopsep", "0pt"), ("cmidrulesep", ""), ("cmidrulekern", ".5em"), ("defaultaddspace", ".5em"), ("@lastruleclass", "0")] {
            t.push(Token::cs(self.e.cs.intern(name)));
            t.push(ch('='));
            if value.is_empty() {
                t.push(Token::cs(self.e.cs.intern("doublerulesep")));
            } else {
                t.extend(value.chars().map(ch));
            }
        }
        t.push(self.named_cs(named!("relax")));
        self.back_list(&t);
    }

    /// amsfonts' `\widehat` and `\widetilde` (`amsfonts.sty`): the argument measured in text
    /// style (`\@mathmeasure\z@\textstyle{#1}`, `\setbox0\hbox{$\textstyle#1$}` with
    /// `\everymath` empty and `\m@th`), then [`Self::wide_accent_end`]: wider than `2em`, the
    /// AMSb accent `big` (`\mathaccent"0\hexnumber@\symAMSb 5B`), else the kernel's `small`.
    fn wide_accent(&mut self, big: u32, small: u32) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(doc) = self.doc.as_mut() else { return };
        let fam = doc.wide_fam;
        doc.wide.push((arg.clone(), (fam << 8 | big) << 16 | small));
        self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
        self.set_every_math(&[]);
        self.math_surround_zero();
        self.enter_text_math();
        self.build_style(2);
        self.build_math_group(ORD, 0);
        let mut t = arg;
        t.extend([Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@wideaccent"))]);
        self.back_list(&t);
    }

    /// `\ifdim\wd\z@>\tw@ em` (the current font's quad), then `\mathaccent` of the chosen
    /// accent on the argument, braced.
    fn wide_accent_end(&mut self) {
        self.after_math();
        self.package();
        let Some((arg, codes)) = self.doc.as_mut().and_then(|d| d.wide.pop()) else { return };
        let wd = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let quad = self.e.fonts.params(self.cur_font()).get(5).copied().unwrap_or(0);
        let code = if wd > 2 * quad { codes >> 16 } else { codes & 0xffff };
        self.build_math_accent(code);
        let mut t = arg;
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// `\check@mathfonts`: when the size differs from the one the families were set for,
    /// `\glb@settings`.
    fn check_math_fonts(&mut self) {
        let (_, size) = self.font_key();
        if self.doc.as_ref().is_some_and(|d| d.math_size == size) {
            return;
        }
        self.glb_settings();
    }

    /// `\glb@settings`: the math version's families set (globally) at the size and its
    /// script sizes, the symbol fonts operators, letters, symbols and largesymbols as
    /// families 0 to 3; then `\every@math@size`.
    fn glb_settings(&mut self) {
        let (_, size) = self.font_key();
        // `\calculate@math@sizes` for a size of `\fontsize` is not ported.
        if (0..data_class::SIZE_COMMANDS.len()).all(|i| self.size_row(i).fsize != size) {
            return self.not_native("math at a font size no size command sets");
        }
        let bold = self.bold_version() as usize;
        let fams = self.doc.as_ref().map(|d| d.fams.clone()).unwrap_or_default();
        let sizes = self.math_sizes(size);
        for (fam, keys) in fams.into_iter().enumerate() {
            if !keys[bold].is_empty() {
                self.set_fam(fam, keys[bold], sizes);
            }
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.math_size = size;
        }
        // amsmath's `\every@math@size`: `\Mathstrutbox@` is `(` of its text font, and
        // `\big@size` is 1.2 times its height plus 1.2 times its depth.
        if self.ams() {
            let code = self.e.eqtb.code(super::engine_eqtb::Code::Math, b'(' as u32);
            let font = match self.e.eqtb.meaning(super::FAM_BASE_ID + ((code >> 8) & 15) as u32) {
                Meaning::Font(f) => f,
                _ => 0,
            };
            let [_, h, d, _] = self.e.fonts.dims(font, (code & 255) as u8).unwrap_or([0; 4]);
            let big = super::engine_graphics::times("1.2", h).unwrap_or(0) + super::engine_graphics::times("1.2", d).unwrap_or(0);
            if let Some(doc) = self.doc.as_mut() {
                doc.big_size = big;
            }
        }
    }

    /// Whether the math version (`\math@version`) is `bold`: `\boldmath`, `\mathversion{bold}`
    /// or `\boldsymbol` set it until the group ends, kept as the meaning of `rt@mathversion`,
    /// a `\chardef` of 1.
    fn bold_version(&mut self) -> bool {
        let id = self.e.cs.intern("rt@mathversion");
        self.e.eqtb.meaning(id) == Meaning::CharDef(1)
    }

    /// `\mathversion{v}`, `bold` or `normal`, once its checks pass: `\edef\math@version{v}`,
    /// `\gdef\glb@currsize{}` and `\aftergroup\glb@settings`.
    fn set_math_version(&mut self, bold: bool) {
        let id = self.e.cs.intern("rt@mathversion");
        self.e.eqtb.set_meaning(id, Meaning::CharDef(bold as u32), false);
        if let Some(doc) = self.doc.as_mut() {
            doc.math_size = "";
        }
        let t = [self.named_cs(named!("aftergroup")), self.doc_id(doc!("rt@glbsettings"))];
        self.back_list(&t);
    }

    /// `\unboldmath` (0), `\boldmath` (1) and `\mathversion{v}` (2) for `normal` or `bold`.
    /// In math LaTeX's `\@nomath` warns, which is not native.
    fn math_version(&mut self, v: u8) {
        let bold = if v == 2 {
            let mut arg = Vec::new();
            self.read_arg(&mut arg);
            match arg.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect::<String>().as_str() {
                "bold" => true,
                "normal" => false,
                _ => return self.not_native("a math version other than normal and bold"),
            }
        } else {
            v == 1
        };
        if self.mode == Mode::Math {
            return self.not_native("\\boldmath or \\mathversion in math");
        }
        self.set_math_version(bold);
    }

    /// amsbsy's `\boldsymbol{a}`: `\begingroup\mathversion{bold}\binrel@{a}\binrel@@{\mathchoice
    /// {\hbox{$\m@th\displaystyle a$}}…}\endgroup`, one `\hbox` per style. See
    /// [`Self::binrel_tokens`].
    fn boldsymbol(&mut self) {
        if self.mode != Mode::Math {
            return self.not_native("\\boldsymbol outside math");
        }
        // bm.sty:468: `\let\boldsymbol\bm`, unless amsbsy loads after bm and defines it again.
        if let Some(d) = self.doc.as_ref() {
            let at = |p: &str| d.packages.iter().position(|&q| q == p);
            if at("bm").is_some_and(|b| at("amsbsy").is_none_or(|a| a < b)) {
                return self.bm();
            }
        }
        let mut a = Vec::new();
        self.read_arg(&mut a);
        let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
        let mut t = vec![self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@boldversion"))];
        t.extend(self.binrel_tokens(&a));
        t.extend([open, self.named_cs(named!("mathchoice"))]);
        for style in [named!("displaystyle"), named!("textstyle"), named!("scriptstyle"), named!("scriptscriptstyle")] {
            t.extend([open, self.named_cs(named!("hbox")), open, shift, self.named_cs(named!("mathsurround"))]);
            t.extend(ZERO_PT);
            t.push(self.named_cs(style));
            t.extend(a.iter().copied());
            t.extend([shift, close, close]);
        }
        t.extend([close, self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// bm.sty's `\bm{a}` for math characters: `\bm@general` expands `a` token by token and
    /// gives each math character its family moved to the bold one (`\bm@mathchar`,
    /// `\bm@changefam`: operators, letters and symbols to `\symboldoperators`,
    /// `\symboldletters`, `\symboldsymbols`), as a plain `\mathchar` after the group; `^`
    /// and `_` pass through (`\bm@add`). A group, a macro, an accent, a family without a bold
    /// one (`\pmb`, the boxed `\bm@gr@up`) and the prime are not native yet.
    fn bm(&mut self) {
        if self.mode != Mode::Math {
            return self.not_native("\\bm outside math");
        }
        let mut a = Vec::new();
        self.read_arg(&mut a);
        let bold = ["symboldoperators", "symboldletters", "symboldsymbols"].map(|n| {
            let cs = self.e.cs.intern(n);
            match self.e.eqtb.meaning(cs) {
                Meaning::CharDef(f) if f < 16 => Some(f),
                _ => None,
            }
        });
        let mut t = Vec::new();
        for x in a {
            let v = match (x.cat(), x.cs_id()) {
                (Some(Cat::Space), _) => continue,
                (Some(Cat::Superscript | Cat::Subscript), _) => {
                    t.push(x);
                    continue;
                }
                (Some(Cat::Letter | Cat::Other), _) => self.e.eqtb.code(super::engine_eqtb::Code::Math, x.chr() as u32) as u32,
                (_, Some(c)) => match self.e.eqtb.meaning(c) {
                    Meaning::MathCharDef(v) => v,
                    _ => return self.not_native("\\bm of a command"),
                },
                _ => return self.not_native("\\bm of a group"),
            };
            let Some(Some(f)) = bold.get((v >> 8 & 15) as usize).copied().filter(|_| v < 0x8000) else {
                return self.not_native("\\bm of a character with no bold family");
            };
            let m = self.tex_tokens(&format!("\\mathchar{} ", v & !0xF00 | f << 8));
            t.extend(m);
        }
        self.back_list(&t);
    }

    /// `\getanddefine@fonts` with `\globaldefs` 1: family `fam`'s text, script and
    /// scriptscript fonts, shape `key` at `size`'s math sizes.
    /// A shape already selected at a size is known by the control sequence of its name:
    /// while that still means the font, the selection is the font (`select_nfss` would
    /// find it the same way), and only the families are set.
    fn set_fam(&mut self, fam: usize, key: &'static str, sizes: (&'static str, &'static str, &'static str)) {
        let saved = self.cur_font();
        for (k, sz) in [sizes.0, sizes.1, sizes.2].into_iter().enumerate() {
            let known = self.doc.as_ref().and_then(|d| d.fam_fonts.get(&(key, sz)).copied());
            let f = match known {
                Some((cs, f)) if self.e.eqtb.meaning(cs) == Meaning::Font(f) => f,
                _ => {
                    let cs = self.select_nfss_cs(key, sz);
                    let f = self.cur_font();
                    if let (Some(cs), Some(d)) = (cs, self.doc.as_mut()) {
                        d.fam_fonts.insert((key, sz), (cs, f));
                    }
                    f
                }
            };
            self.e.eqtb.set_meaning(super::FAM_BASE_ID + 16 * k as u32 + fam as u32, Meaning::Font(f), true);
        }
        self.select_font(saved, false);
    }

    /// A math alphabet `k` (`\mathbf{…}`, `\mathcal{…}`): `\use@mathgroup`, so
    /// `{\mathgroup N\relax …}` (`\math@egroup` takes the argument without its braces). A
    /// `\DeclareMathAlphabet` alphabet's first use (`\document@select@group`) gives it the next
    /// family, globally: its fonts at the current math sizes and in every later
    /// `\check@mathfonts`, and that use inside one more group. Outside math it is LaTeX's
    /// error `\non@alpherr`.
    fn math_alphabet(&mut self, k: usize) {
        if self.mode != Mode::Math {
            return self.not_native("a math alphabet outside math");
        }
        let Some(&(_, same, a)) = self.doc.as_ref().and_then(|d| d.alphabets.get(k)) else { return };
        if !same {
            return self.not_native("a math alphabet of another encoding");
        }
        // `\mv@bold` counts its own alphabet families (`\c@mv@bold`).
        if matches!(a, Alphabet::Shape(..)) && self.bold_version() {
            return self.not_native("a math alphabet in the bold math version");
        }
        let (fam, first) = match a {
            Alphabet::Fam(f) | Alphabet::Shape(_, Some(f)) => (f, false),
            Alphabet::Shape(key, None) => {
                let Some(doc) = self.doc.as_mut() else { return };
                let fam = doc.fams.len();
                // `\e@mathgroup@top` 16 less `\c@localmathalphabets` 2: past that LaTeX
                // freezes the version and allocates locally.
                if fam >= 14 {
                    return self.not_native("more than 14 math families");
                }
                doc.fams.push([key, ""]);
                doc.alphabets[k].2 = Alphabet::Shape(key, Some(fam as u8));
                let size = doc.math_size;
                let sizes = self.math_sizes(size);
                self.set_fam(fam, key, sizes);
                (fam as u8, true)
            }
        };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let close = Token::char(Cat::EndGroup, '}');
        if first {
            self.build_math_group(ORD, 0);
            arg.push(close);
        }
        self.build_math_group(ORD, 0);
        self.set_cur_fam(fam as i32);
        arg.push(close);
        self.back_list(&arg);
    }

    /// `'` in math (mathcode `"8000`): `^{\prime…}`, one `\prime` per `'`, then a following
    /// superscript's argument inside the same group (`\prim@s`, `\pr@@@t`).
    fn math_prime(&mut self) {
        let prime = self.named_cs(named!("prime"));
        let mut list = vec![Token::char(Cat::Superscript, '^'), Token::char(Cat::BeginGroup, '{'), prime];
        while let Some(t) = self.get_token() {
            if t == Token::char(Cat::Other, '\'') {
                list.push(prime);
            } else if t.cat() == Some(Cat::Superscript) {
                let mut arg = Vec::new();
                self.read_arg(&mut arg);
                list.extend_from_slice(&arg);
                break;
            } else {
                self.input.back(t);
                break;
            }
        }
        list.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&list);
    }

    fn set_glue_named(&mut self, r: Reg, g: Glue) {
        self.e.eqtb.set_glue(r, g, false);
    }

    fn set_dimen_named(&mut self, r: Reg, v: i32) {
        self.e.eqtb.set_word(r, v, false);
    }

    /// `v` as the tokens `<v>sp`, for a primitive's dimension.
    fn sp_tokens(v: i32, out: &mut Vec<Token>) {
        out.extend(v.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        out.extend("sp".chars().map(|c| Token::char(Cat::Letter, c)));
    }

    fn word_tokens(&mut self, w: &str, out: &mut Vec<Token>) {
        out.extend(w.chars().map(|c| Token::char(Cat::Letter, c)));
    }

    /// `\@makefnmark`: `\hbox{\@textsuperscript{\normalfont\@thefnmark}}`, with
    /// `\@textsuperscript#1` = `{\m@th\ensuremath{^{\mbox{\fontsize\sf@size\sf@size#1}}}}`:
    /// the number in the engine's math, in the script size.
    fn footnote_mark_tokens(&mut self, n: i32, out: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let math = Token::char(Cat::MathShift, '$');
        out.extend([self.named_cs(named!("hbox")), open, open, self.named_cs(named!("mathsurround")), Token::char(Cat::Other, '0')]);
        self.word_tokens("pt", out);
        out.extend([math, Token::char(Cat::Superscript, '^'), open, self.named_cs(named!("mbox")), open, self.doc_id(FN_MARK_FONT)]);
        out.extend(n.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        out.extend([close, close, math, close, close]);
    }

    /// `\footnote{text}`: `\stepcounter{footnote}`, `\@footnotemark` (`\leavevmode`, the
    /// space factor kept, `\nobreak`, `\@makefnmark`), then `\@footnotetext{text}`.
    fn footnote(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.thanks.is_some()) {
            return self.thanks();
        }
        if self.in_parbox() {
            return self.not_native("a footnote in a \\parbox or minipage");
        }
        if self.leave_vmode() {
            return;
        }
        if self.next_is('[') {
            return self.not_native("\\footnote[n]");
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let sf = if self.mode == Mode::Horizontal { self.aux } else { -1 };
        if self.mode == Mode::Horizontal {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.footnote += 1;
        let n = doc.footnote;
        doc.footnote_text = text;
        doc.footnote_sf.push(sf);
        let mut list = Vec::new();
        // hyperref's `\@footnotemark`: `\hyper@linkstart{link}{Hfootnote.N}\@makefnmark
        // \hyper@linkend`.
        let hyper = doc.packages.contains(&"hyperref");
        if hyper {
            doc.hfootnote += 1;
            doc.link = Some((hyperref::LINK, format!("Hfootnote.{}", doc.hfootnote)));
            list.push(self.doc_id(START_LINK_NATIVE));
        }
        self.footnote_mark_tokens(n, &mut list);
        if hyper {
            list.push(self.doc_id(END_LINK_NATIVE));
        }
        list.push(self.doc_id(FN_MARK_END));
        self.back_list(&list);
    }

    /// `\fontsize\sf@size\sf@size\selectfont` then `\normalfont`, inside the mark's `\mbox`.
    fn footnote_mark_font(&mut self) {
        let (_, size) = self.font_key();
        let script = self.math_sizes(size).1;
        let w = data_nfss::decimal_sp(script).unwrap_or(0);
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, Glue { width: w, ..Glue::default() }, false);
        self.strut_update();
        let key = self.rm_key("m/n");
        self.select_nfss(&key, script);
    }

    /// `\textsuperscript{text}` and `\textsubscript{text}`: `\@textsuperscript{\selectfont
    /// text}`, `{\m@th\ensuremath{^{\mbox{\fontsize\sf@size\sf@size\selectfont text}}}}`
    /// (`_` for the subscript).
    fn text_script(&mut self, c: char) {
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let math = Token::char(Cat::MathShift, '$');
        let cat = if c == '^' { Cat::Superscript } else { Cat::Subscript };
        let mut t = vec![open, self.named_cs(named!("mathsurround")), Token::char(Cat::Other, '0')];
        self.word_tokens("pt", &mut t);
        t.extend([math, Token::char(cat, c), open, self.named_cs(named!("mbox")), open, self.doc_id(doc!("rt@scriptfont"))]);
        t.extend(text);
        t.extend([close, close, math, close]);
        self.back_list(&t);
    }

    /// `\fontsize\sf@size\sf@size\selectfont`: the current shape at the script size of the
    /// size around the formula, `\baselineskip` that size.
    fn script_font(&mut self) {
        let (key, size) = self.font_key();
        let script = self.math_sizes(size).1;
        let w = data_nfss::decimal_sp(script).unwrap_or(0);
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, Glue { width: w, ..Glue::default() }, false);
        self.strut_update();
        self.select_nfss(&key, script);
    }

    /// `\ifhmode\spacefactor\@x@sf\fi`, then `\@footnotetext`: `\insert\footins{…}`.
    fn footnote_mark_end(&mut self) {
        let sf = self.doc.as_mut().and_then(|d| d.footnote_sf.pop()).unwrap_or(-1);
        if self.mode == Mode::Horizontal && sf >= 0 {
            self.aux = sf;
        }
        let mut list = vec![self.named_cs(named!("insert"))];
        list.extend(FOOTINS.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        list.extend([Token::char(Cat::BeginGroup, '{'), self.doc_id(FN_SETUP)]);
        self.back_list(&list);
    }

    /// The start of `\@footnotetext`'s insertion: `\reset@font\footnotesize`, the split and
    /// float parameters, `\hsize\columnwidth`, `\@parboxrestore` (with `\sloppy`), then
    /// article's `\@makefntext`: `\parindent 1em\noindent\hb@xt@1.8em{\hss\@makefnmark}`,
    /// `\rule\z@\footnotesep\ignorespaces` and the text.
    fn footnote_setup(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.footnote_flags.push(doc.nobreak);
        doc.nobreak = false;
        let n = doc.footnote;
        let text = std::mem::take(&mut doc.footnote_text);
        let width = doc.class.dimen("textwidth");
        // `\protected@edef\@currentlabel{\csname p@footnote\endcsname\@thefnmark}`, local to
        // the insertion's group; with hyperref `\@currentHref` is the mark's anchor.
        let anchor = self.doc.as_ref().filter(|d| d.packages.contains(&"hyperref")).map(|d| format!("Hfootnote.{}", d.hfootnote));
        self.store_current_label(&n.to_string());
        // hyperref's `\@footnotemark` restores the outer `\@currentHref` (`\Hy@saved@currentHref`):
        // the note's own is local to the insertion.
        if let Some(name) = &anchor {
            self.store_current_href(name, false);
        }
        // `\reset@font\footnotesize`.
        let key = self.rm_key("m/n");
        self.apply_size(2, &key);
        // `\interlinepenalty\interfootnotelinepenalty` (100), `\splittopskip\footnotesep`,
        // `\splitmaxdepth\dp\strutbox`, `\floatingpenalty\@MM`, `\hsize\columnwidth`.
        self.set_int_named(param!("interlinepenalty"), 100);
        let sep = self.rw(R_FOOTNOTESEP);
        self.set_glue_named(param!("splittopskip"), Glue { width: sep, ..Glue::default() });
        let strut_depth = self.strut_depth();
        self.set_dimen_named(param!("splitmaxdepth"), strut_depth);
        self.set_int_named(param!("floatingpenalty"), 20000);
        self.set_dimen_named(param!("hsize"), width);
        // `\@arrayparboxrestore`: `\let\par\@@par`, `\everypar{}`, `\linewidth\hsize`,
        // `\@totalleftmargin\z@`.
        if let Some((id, m)) = self.doc.as_ref().map(|d| (d.par_id, d.par_prim)) {
            self.e.eqtb.set_meaning(id, m, false);
        }
        self.set_everypar(0);
        self.set_rw(R_LINEWIDTH, width, false);
        self.set_rw(R_TOTALLEFTMARGIN, 0, false);
        self.set_dimen_named(param!("parindent"), 0);
        for r in [param!("parskip"), param!("leftskip"), param!("rightskip")] {
            self.set_glue_named(r, Glue::default());
        }
        self.set_rg(R_ATRIGHTSKIP, Glue::default());
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        let one = self.dimen_of("1pt");
        self.set_glue_named(param!("lineskip"), Glue { width: one, ..Glue::default() });
        self.set_dimen_named(param!("lineskiplimit"), 0);
        // `\sloppy`.
        self.set_int_named(param!("tolerance"), 9999);
        let e = self.dimen_of("3em");
        self.set_dimen_named(param!("emergencystretch"), e);
        let half = self.dimen_of(".5pt");
        self.set_dimen_named(param!("hfuzz"), half);
        self.set_dimen_named(param!("vfuzz"), half);
        // `\@makefntext`.
        let em = self.dimen_of("1em");
        self.set_dimen_named(param!("parindent"), em);
        let mut list = vec![self.named_cs(named!("noindent")), self.named_cs(named!("hbox"))];
        self.word_tokens("to", &mut list);
        Self::sp_tokens(self.dimen_of("1.8em"), &mut list);
        list.extend([Token::char(Cat::BeginGroup, '{'), self.named_cs(named!("hss"))]);
        // The title's `\@makefntext`: `\@textsuperscript{\normalfont\@thefnmark}`, no `\hbox`.
        if std::mem::take(&mut self.doc.as_mut().expect("a native document").fn_symbol) {
            let sym = self.fn_symbol_tokens(n).unwrap_or_default();
            self.superscript_mark(&sym, &mut list);
            let saved = self.doc.as_mut().and_then(|d| d.footnote_sf.pop()).unwrap_or(0);
            if let Some(d) = self.doc.as_mut() {
                d.footnote = saved;
            }
        } else {
            self.footnote_mark_tokens(n, &mut list);
        }
        list.push(Token::char(Cat::EndGroup, '}'));
        // `\rule\z@\footnotesep`: `\hbox{\vrule\@width\z@\@height\footnotesep\@depth-\z@}`.
        list.extend([self.named_cs(named!("hbox")), Token::char(Cat::BeginGroup, '{'), self.named_cs(named!("vrule"))]);
        for (k, v) in [("width", 0), ("height", sep), ("depth", 0)] {
            self.word_tokens(k, &mut list);
            Self::sp_tokens(v, &mut list);
        }
        list.extend([Token::char(Cat::EndGroup, '}'), self.named_cs(named!("ignorespaces"))]);
        // hyperref's `\@footnotetext`: `\Hy@raisedlink{\hyper@@anchor{Hfootnote.N}{\relax}}
        // \ignorespaces` before the text.
        if let Some(name) = anchor {
            let t = self.text_raised_link(name);
            list.extend(t);
            list.push(self.named_cs(named!("ignorespaces")));
        }
        // microtype's `footnote` patch: `\ignorespaces\leftprotrusion`.
        if self.microtype() {
            list.push(self.doc_id(doc!("rt@leftprotrusion")));
        }
        list.extend_from_slice(&text);
        list.push(self.doc_id(FN_END));
        self.back_list(&list);
    }

    /// `\size@update`'s `\setbox\strutbox\hbox{\vrule\@height.7\baselineskip
    /// \@depth.3\baselineskip\@width\z@}`, kept as its height and depth in hidden dimension
    /// registers (local, as the box is). `.7` and `.3` are §452's fractions 45875 and 19661
    /// of the unit, applied as §455's `xn_over_d`.
    fn strut_update(&mut self) {
        let g = self.glue_param(BASELINE_SKIP);
        self.e.eqtb.set_glue(NORMAL_BASELINE_SKIP, g, false);
        let b = g.width as i64;
        self.e.eqtb.set_word(STRUT_HT, (b * 45875 / 65536) as i32, false);
        self.e.eqtb.set_word(STRUT_DP, (b * 19661 / 65536) as i32, false);
    }

    /// `\ht\strutbox` and `\dp\strutbox`.
    fn strut_height(&self) -> i32 {
        self.e.eqtb.word(STRUT_HT)
    }

    fn strut_depth(&self) -> i32 {
        self.e.eqtb.word(STRUT_DP)
    }

    /// `\@finalstrut\strutbox` (`\unskip`, `\nobreak`, a strut's depth), `\par`, and the
    /// insertion's `}`.
    fn footnote_end(&mut self) {
        if let Some(n) = self.last_item(Kind::Glue) {
            self.cur.pop();
            self.e.nodes.flush_node(n);
        }
        let d = self.strut_depth();
        if self.mode == Mode::Horizontal {
            self.cur.push(Node::new(Kind::Penalty, 10000));
            let r = self.e.nodes.rule_node([0, 0, d]);
            self.cur.push(r);
            self.aux = 1000;
        }
        self.par();
        if let Some(doc) = self.doc.as_mut() {
            if let Some(nobreak) = doc.footnote_flags.pop() {
                doc.nobreak = nobreak;
            }
        }
        self.input.back(Token::char(Cat::EndGroup, '}'));
    }

    /// `\color@hbox … \color@endbox` without `color`: a natural `\hbox` around box `b`.
    fn wrap_hbox(&mut self, b: BoxRec) -> u32 {
        let inner = self.e.nodes.add_box(b);
        let start = self.cur.len();
        let colored = self.colored();
        if colored {
            self.color_stack(Some(DEFAULT_COLOR));
        }
        self.cur.push(Node::new(Kind::Box, inner as i32));
        if colored {
            self.color_stack(None);
        }
        let outer = self.hpack(start, 0, false);
        self.e.nodes.add_box(outer)
    }
}

/// Whether `name` is one of the kernel's `\chardef`s or `\mathchardef`s.
/// The tables that name a control sequence, as bits: [`KERNEL_BIT`] for the kernel's chars,
/// operators, delimiters, macros and alphabets and the primitives harvested macros use, and
/// [`package_bit`] for each package's. A perfect hash generated from those tables
/// (`assets/tables/names.rtdb`, `rushtex-gen names`): the pre-scan and `predict` ask it once
/// per command, and loading it builds nothing.
fn table_names(name: &str) -> u32 {
    names_entry(name).0
}

const IN_WORDS: u8 = 1;
const IN_PREAMBLE: u8 = 2;
const PACKAGE_NATIVE: u8 = 4;
const DRIVER_NATIVE: u8 = 8;

/// Which of the pre-scan's fixed lists hold `name`: [`WORDS`] (`IN_WORDS`), [`PREAMBLE`]
/// (`IN_PREAMBLE`), [`PACKAGE_NATIVES`] (`PACKAGE_NATIVE`, and `DRIVER_NATIVE` when the
/// package is one of [`DRIVER_PACKAGES`]). A column of the generated names table, so one
/// lookup per name, not a walk of each list.
fn scan_list(name: &str) -> u8 {
    names_row(name).2
}

/// [`scan_list`]'s bits of `name`, from the lists themselves: what `rushtex-gen names` stores.
#[cfg(any(test, feature = "gen"))]
fn list_bits(name: &str) -> u8 {
    let mut v = 0;
    if WORDS.contains(&name) {
        v |= IN_WORDS;
    }
    if PREAMBLE.contains(&name) {
        v |= IN_PREAMBLE;
    }
    for (package, _, _) in PACKAGE_NATIVES.iter().filter(|p| p.1 == name) {
        v |= PACKAGE_NATIVE | if DRIVER_PACKAGES.contains(package) { DRIVER_NATIVE } else { 0 };
    }
    v
}

/// The bits of the packages that change `name` without the native job following them
/// (`KernelTable::blocked`): after one of them, a document naming it goes to the engine.
fn blocked_names(name: &str) -> u32 {
    names_entry(name).1
}

/// `name`'s row of the generated names table: its [`table_names`] bits and its
/// [`blocked_names`] bits.
fn names_entry(name: &str) -> (u32, u32) {
    let (bits, blocked, _) = names_row(name);
    (bits, blocked)
}

/// [`names_entry`] and the [`scan_list`] bits of `name`, from one lookup.
fn names_row(name: &str) -> (u32, u32, u8) {
    use crate::data::{data_blob::Blob, data_phf::Phf};
    type Names = (Phf<'static>, crate::data::data_blob::U32s<'static>, crate::data::data_blob::U32s<'static>, &'static [u8]);
    static NAMES: &[u8] = include_bytes!("../../assets/tables/names.rtdb");
    static TABLE: std::sync::OnceLock<Option<Names>> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let b = Blob::new(NAMES)?;
        let (phf, bits, blocked, lists) = (Phf::from_blob(&b, "nmf")?, b.u32s("nm.b")?, b.u32s("nm.x")?, b.section("nm.l")?);
        (bits.len() == phf.len() && blocked.len() == phf.len() && lists.len() == phf.len()).then_some((phf, bits, blocked, lists))
    });
    t.as_ref().and_then(|(phf, bits, blocked, lists)| phf.get(name.as_bytes()).map(|i| (bits.get(i as usize), blocked.get(i as usize), lists[i as usize]))).unwrap_or((0, 0, 0))
}

/// [`table_names`]' contents, from the tables themselves: what `rushtex-gen names` writes.
#[cfg(any(test, feature = "gen"))]
pub fn names_table() -> Vec<u8> {
    use crate::data::data_kernel::{KernelTable, MathFont};
    let mut m = std::collections::BTreeMap::<&'static str, u32>::new();
    let mut add = |k: &'static KernelTable<'static>, bit: u32| {
        let alphabets = k.math_fonts().filter_map(|f| match f {
            MathFont::SymbolAlphabet { name, .. } | MathFont::Alphabet { name, .. } => Some(name),
            MathFont::Symbol { .. } => None,
        });
        for n in k.chars().map(|c| c.0).chain(k.ops().map(|o| o.name)).chain(k.delims().map(|d| d.0)).chain(k.macro_names()).chain(k.lets().map(|l| l.0)).chain(k.accents().map(|a| a.0)).chain(alphabets) {
            *m.entry(n).or_default() |= bit;
        }
    };
    if let Some(k) = crate::data::data_kernel::kernel() {
        add(k, KERNEL_BIT);
    }
    for (p, k) in crate::data::data_kernel::packages() {
        add(k, package_bit(p));
    }
    for n in crate::data::data_kernel::MACRO_PRIMITIVES {
        *m.entry(n).or_default() |= KERNEL_BIT;
    }
    for (p, n, _) in PACKAGE_NATIVES.iter().filter(|q| package_bit(q.0) != 0) {
        *m.entry(n).or_default() |= package_bit(p);
    }
    // The pre-scan's fixed lists (`scan_list`) have rows too.
    for n in WORDS.iter().chain(PREAMBLE.iter()).chain(PACKAGE_NATIVES.iter().map(|p| &p.1)) {
        m.entry(n).or_default();
    }
    // The names each package blocks (`KernelTable::blocked`), by its bit.
    let mut x = std::collections::BTreeMap::<&'static str, u32>::new();
    for (p, k) in crate::data::data_kernel::packages() {
        for n in k.blocked() {
            m.entry(n).or_default();
            *x.entry(n).or_default() |= package_bit(p);
        }
    }
    let mut w = crate::data::data_blob::BlobWriter::default();
    let names: Vec<&[u8]> = m.keys().map(|n| n.as_bytes()).collect();
    crate::data::data_phf::write(&mut w, "nmf", &names).expect("a perfect hash");
    let probe = w.finish();
    // The hash numbers the names its own way: the bits follow its IDs.
    let b = crate::data::data_blob::Blob::new(&probe).expect("a blob");
    let phf = crate::data::data_phf::Phf::from_blob(&b, "nmf").expect("the hash");
    let (mut bits, mut blocked, mut lists) = (vec![0u32; m.len()], vec![0u32; m.len()], vec![0u8; m.len()]);
    for (n, v) in &m {
        let i = phf.get(n.as_bytes()).expect("a name") as usize;
        bits[i] = *v;
        blocked[i] = x.get(n).copied().unwrap_or(0);
        lists[i] = list_bits(n);
    }
    let mut w = crate::data::data_blob::BlobWriter::default();
    crate::data::data_phf::write(&mut w, "nmf", &names).expect("a perfect hash");
    w.u32s("nm.b", &bits);
    w.u32s("nm.x", &blocked);
    w.bytes("nm.l", lists);
    w.finish()
}

/// [`table_names`]' bit for the kernel.
const KERNEL_BIT: u32 = 1;

/// [`table_names`]' bit for a package with a native table: 2 for the first.
fn package_bit(name: &str) -> u32 {
    crate::data::data_kernel::packages().position(|p| p.0 == name).map_or(0, |k| 2 << k)
}

/// A value calc reads as an expression: an operator after its first character.
fn calc_expression(value: &str) -> bool {
    value.trim().get(1..).is_some_and(|r| r.contains(['+', '-', '*', '/']))
}

/// Package `name` and the packages it loads, into `loaded` (each once, after its own loads);
/// `false` if one of them has no native table.
fn require(name: &str, loaded: &mut Vec<&'static str>) -> bool {
    if NO_EFFECT_PACKAGES.contains(&name) {
        return true;
    }
    if let Some(&p) = DRIVER_PACKAGES.iter().find(|&&p| p == name) {
        // tabularx's `\RequirePackage{array}`; siunitx's amstext, array and color.
        if p == "tabularx" {
            require("array", loaded);
        }
        if p == "siunitx" {
            require("amstext", loaded);
            require("array", loaded);
            require("color", loaded);
        }
        if !loaded.contains(&p) {
            loaded.push(p);
        }
        return true;
    }
    let Some((p, k)) = crate::data::data_kernel::packages().find(|p| p.0 == name) else { return false };
    if loaded.contains(&p) {
        return true;
    }
    if !k.package().is_some_and(|(_, mut deps)| deps.all(|d| require(d, loaded))) {
        return false;
    }
    loaded.push(p);
    true
}




/// A size's text as the class tables' own string (a size they do not know is `10`).
fn static_size(size: &str) -> &'static str {
    let rows = (0..data_class::CLASS_SIZES.len()).filter_map(data_class::article_size).flat_map(|t| (0..data_class::SIZE_COMMANDS.len()).map(move |i| t.size(i)));
    rows.flat_map(|r| [r.fsize, r.sf, r.ssf]).find(|&s| s == size).or_else(|| EXTRA_SIZES.lock().ok()?.iter().find(|&&s| s == size).copied()).unwrap_or("10")
}

/// Font sizes no class size command sets, as `\fontsize` names them (`\f@size`), kept for
/// the life of the program: a document names a few.
static EXTRA_SIZES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

/// `size` as a `&'static str`: a class size, else one of [`EXTRA_SIZES`], added once.
fn size_name(size: &str) -> &'static str {
    let known = static_size(size);
    if known == size {
        return known;
    }
    let Ok(mut extra) = EXTRA_SIZES.lock() else { return "10" };
    let name: &'static str = Box::leak(size.to_string().into_boxed_str());
    extra.push(name);
    name
}

/// Whether what follows `\parbox` or `\begin{minipage}` is a position the native box
/// takes: none, or one `[c]`, `[t]`, `[b]` with no height after it.
fn box_position_ok(after: &str) -> bool {
    let a = after.trim_start();
    match a.strip_prefix('[') {
        None => true,
        Some(r) => matches!(r.as_bytes(), [b'c' | b't' | b'b', b']', ..]) && !r[2..].trim_start().starts_with('['),
    }
}

/// A size argument of `\fontsize` as `\@defaultunits` reads it: a decimal, `pt` if no
/// unit; `None` for anything else (a macro, another unit).
fn size_arg(text: &str) -> Option<i32> {
    let t = text.trim();
    let t = t.strip_suffix("pt").unwrap_or(t).trim_end();
    (!t.is_empty() && t.bytes().all(|c| c.is_ascii_digit() || c == b'.')).then(|| data_nfss::decimal_sp(t)).flatten()
}

/// A table's glue words as glue.
fn glue_words(w: data_class::GlueWords) -> Glue {
    Glue { width: w[0], stretch: w[1], shrink: w[2], stretch_order: w[3] as u8, shrink_order: w[4] as u8 }
}

/// `-g`: every component negated.
/// `\@alph`/`\@Alph`: 1 is a.
fn alph(n: i32, base: u8) -> impl Iterator<Item = char> {
    (1..=26).contains(&n).then(|| (base + n as u8 - 1) as char).into_iter()
}

/// `\theenum<d>` at `n` in a style of `enum_style`.
fn enum_form(style: u8, n: i32) -> String {
    match style {
        b'a' | b'A' => alph(n, style).collect(),
        b'i' => roman(n),
        b'I' => roman(n).to_uppercase(),
        _ => n.to_string(),
    }
}

/// enumitem's keys as `\enitkv@setkeys` splits them: `key` or `key=value` at brace depth 0,
/// at commas; the key without spaces, the value without the spaces around it and one pair
/// of braces around all of it.
fn enit_pairs(keys: &[Token]) -> Vec<(String, Vec<Token>)> {
    let level = |depth: &mut i32, t: &Token| match t.cat() {
        Some(Cat::BeginGroup) => *depth += 1,
        Some(Cat::EndGroup) => *depth -= 1,
        _ => {}
    };
    let mut depth = 0;
    let mut out = Vec::new();
    for part in keys.split(|t| {
        level(&mut depth, t);
        depth == 0 && *t == Token::char(Cat::Other, ',')
    }) {
        let mut d = 0;
        let eq = part.iter().position(|t| {
            level(&mut d, t);
            d == 0 && *t == Token::char(Cat::Other, '=')
        });
        let (k, mut v) = match eq {
            Some(e) => (&part[..e], &part[e + 1..]),
            None => (part, &[][..]),
        };
        let key: String = k.iter().filter(|t| t.cat() != Some(Cat::Space)).map(|t| t.chr()).collect();
        if key.is_empty() {
            continue;
        }
        while v.first().is_some_and(|t| t.cat() == Some(Cat::Space)) {
            v = &v[1..];
        }
        while v.last().is_some_and(|t| t.cat() == Some(Cat::Space)) {
            v = &v[..v.len() - 1];
        }
        if v.first().is_some_and(|t| t.cat() == Some(Cat::BeginGroup)) {
            let mut d = 0;
            if v.iter().position(|t| {
                level(&mut d, t);
                d == 0
            }) == Some(v.len() - 1)
            {
                v = &v[1..v.len() - 1];
            }
        }
        out.push((key, v.to_vec()));
    }
    out
}

/// The enumitem keys the native job takes (`enit_setup`), as the pre-scan reads them:
/// vertical and horizontal lengths with a value, `leftmargin=*`, the `nosep` family, and
/// `label`.
fn enit_keys_ok(keys: &str) -> bool {
    let mut depth = 0;
    let parts = keys.split(|c| {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        depth == 0 && c == ','
    });
    parts.map(str::trim).filter(|p| !p.is_empty()).all(|p| {
        let (k, v) = p.split_once('=').map_or((p, ""), |(k, v)| (k.trim(), v.trim()));
        match k {
            "topsep" | "itemsep" | "parsep" | "partopsep" | "labelsep" | "labelwidth" | "itemindent" | "rightmargin" | "listparindent" => !v.is_empty() && !v.contains(['*', '!']),
            "leftmargin" => v == "*" || !v.is_empty() && !v.contains(['*', '!']),
            "nosep" | "noitemsep" | "nolistsep" => v.is_empty() || v == "true",
            "label" => true,
            _ => false,
        }
    })
}

/// `\romannumeral` (§69).
fn roman(mut n: i32) -> String {
    let mut s = String::new();
    for (v, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

fn neg(g: Glue) -> Glue {
    Glue { width: -g.width, stretch: -g.stretch, shrink: -g.shrink, ..g }
}

/// §1239 `\advance` of glue: stretch and shrink add at equal orders, else the higher
/// nonzero one wins; a zero component is of order `normal`.
fn glue_add(a: Glue, b: Glue) -> Glue {
    let part = |x: i32, xo: u8, y: i32, yo: u8| -> (i32, u8) {
        let (v, o) = if xo == yo {
            (x + y, xo)
        } else if xo < yo && y != 0 {
            (y, yo)
        } else {
            (x, xo)
        };
        (v, if v == 0 { 0 } else { o })
    };
    let (stretch, stretch_order) = part(a.stretch, a.stretch_order, b.stretch, b.stretch_order);
    let (shrink, shrink_order) = part(a.shrink, a.shrink_order, b.shrink, b.shrink_order);
    Glue { width: a.width + b.width, stretch, shrink, stretch_order, shrink_order }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_end_matches_the_table_walk() {
        // Every byte value at every offset of a word, between plain filler of each class.
        for class in [SCAN_PLAIN, PREDICT_PLAIN] {
            for c in 0..=255u8 {
                for at in 0..20 {
                    let mut b = vec![b'a'; 24];
                    b[at] = c;
                    for start in [0, 1, 7, 9] {
                        let slow = start + b[start..].iter().take_while(|&&x| BYTE_CLASS[x as usize] & class != 0).count();
                        assert_eq!(plain_end(&b, start, class), slow, "class {class} byte {c} at {at} from {start}");
                    }
                }
            }
        }
    }

    #[test]
    fn names_table_is_generated_from_the_tables() {
        let committed: &[u8] = include_bytes!("../../assets/tables/names.rtdb");
        assert!(committed == names_table().as_slice(), "assets/tables/names.rtdb is stale: run rushtex-gen names");
    }

    #[test]
    fn footins_is_the_class_tables() {
        let t = data_class::article().unwrap();
        let i = data_class::CLASS_REGS.iter().position(|n| *n == "count footins").unwrap();
        assert_eq!(t.reg(i).1, super::super::engine_eqtb::INT_PARAMS + FOOTINS);
        for (i, name) in [(R_LEFTMARGINI, "leftmargini"), (R_LABELSEP, "labelsep"), (R_LABELWIDTH, "labelwidth"), (R_LEFTMARGIN, "leftmargin"), (R_RIGHTMARGIN, "rightmargin"), (R_LISTPARINDENT, "listparindent"), (R_ITEMINDENT, "itemindent"), (R_TOTALLEFTMARGIN, "@totalleftmargin"), (R_LINEWIDTH, "linewidth"), (R_BEGINPARPENALTY, "@beginparpenalty"), (R_ENDPARPENALTY, "@endparpenalty"), (R_ITEMPENALTY, "@itempenalty"), (R_CLUBPENALTY, "@clubpenalty"), (R_LISTDEPTH, "@listdepth"), (R_ITEMDEPTH, "@itemdepth"), (R_ENUMDEPTH, "@enumdepth"), (R_ENUMI, "c@enumi"), (R_PARTOPSEP, "partopsep"), (R_TOPSEP, "topsep"), (R_PARSEP, "parsep"), (R_ITEMSEP, "itemsep"), (R_ATTOPSEP, "@topsep"), (R_TOPSEPADD, "@topsepadd"), (R_OUTERPARSKIP, "@outerparskip"), (R_TABCOLSEP, "tabcolsep"), (R_ARRAYCOLSEP, "arraycolsep")] {
            assert_eq!(data_class::CLASS_REGS[i], name);
        }
    }

    #[test]
    fn predict_numbers_labels_as_latex_does() {
        let src = "\\documentclass{article}\\begin{document}\\label{none}\\section{A}\\label{a}\\begin{enumerate}\\item\\label{i}\\begin{enumerate}\\item x\\item\\label{ii}\\end{enumerate}\\end{enumerate}\\label{after}\\footnote{\\label{fn}}\\section*{S}\\subsection{B}{\\small\\label{b}}\\begin{itemize}\\item\\label{it}\\end{itemize}";
        let p = predict(src);
        let get = |k: &str| p.labels.get(k).map(String::as_str);
        assert_eq!((get("none"), get("a"), get("i"), get("ii"), get("after")), (Some(""), Some("1"), Some("1"), Some("1b"), Some("1")));
        assert_eq!((get("fn"), get("b"), get("it")), (Some("1"), Some("1.1"), Some("1.1")));
        // A macro the document defines might step a counter: the prediction stops there.
        let p = predict("\\begin{document}\\footnote{x}\\maketitle\\footnote{\\label{f}}");
        assert_eq!(p.labels.get("f").map(String::as_str), Some("1"));
        let p = predict("\\begin{document}\\label{a}\\mine\\label{b}");
        assert!(p.labels.contains_key("a") && !p.labels.contains_key("b") && !p.complete);
    }

    #[test]
    fn predict_passes_preamble_macros_that_number_nothing() {
        // Text, font commands and an earlier quiet macro number nothing; a heading, an `@`
        // name or an unknown command might.
        let pre = "\\newcommand{\\gdl}{Guarded Direct Lowering}\\newcommand*\\rt[1][x]{\\textbf{#1}\\gdl}\\renewcommand{\\sec}{\\section{S}}\\providecommand{\\at}{\\foo@bar}\\newcommand{\\un}{\\mine}";
        assert_eq!(quiet_macros(pre, &[]).0, ["gdl", "rt"]);
        let stars = "\\newcommand{\\q}[2]{\\subsection*{#1\\hfill[#2 points]}}\\newcommand{\\s}{\\section{x}}";
        assert_eq!(quiet_macros(stars, &[]).2, [("q", 1)]);
        let p = predict(&format!("{pre}\\begin{{document}}\\gdl\\rt{{a}}\\section{{A}}\\label{{a}}"));
        assert!(p.complete && p.labels.get("a").map(String::as_str) == Some("1"));
        assert!(!predict(&format!("{pre}\\begin{{document}}\\sec\\label{{a}}")).complete);
    }

    #[test]
    fn predict_numbers_theorems() {
        // `[section]` resets with the section; `[theorem]` shares its counter.
        let p = predict("\\newtheorem{thm}{Theorem}[section]\\newtheorem{lem}[thm]{Lemma}\\newtheorem{conj}{Conjecture}\\begin{document}\\section{A}\\begin{thm}\\label{a}\\end{thm}\\begin{lem}[Note]\\label{b}\\end{lem}\\begin{conj}\\label{c}\\end{conj}\\section{B}\\begin{lem}\\label{d}\\end{lem}\\begin{conj}\\label{e}\\end{conj}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"], &p.labels["d"], &p.labels["e"]], ["1.1", "1.2", "1", "2.1", "2"]);
        assert_eq!(newtheorem_args("{lem}[thm]{Lemma}x").map(|a| (a.0, a.1, a.2, a.3, a.4, a.5)), Some(("lem", Some("thm"), "Lemma", None, 17, None)));
        assert_eq!(newtheorem_args("{thm}{\\bf T}").map(|a| a.2), Some("\\bf T"));
        assert_eq!(newtheorem_args("{thm}{T}[chapter]").map(|a| a.5), Some(Some("chapter")));
    }

    #[test]
    fn predict_skips_starred_theorems_and_proofs() {
        let p = predict("\\newtheorem{thm}{Theorem}\\newtheorem*{rem}{Remark}\\begin{document}\\section{A}\\label{s}\\begin{rem}\\label{r}\\end{rem}\\begin{thm}\\label{t}\\end{thm}\\begin{proof}\\label{p}\\end{proof}");
        assert!(p.complete);
        assert_eq!([&p.labels["s"], &p.labels["r"], &p.labels["t"], &p.labels["p"]], ["1", "1", "1", "1"]);
    }

    #[test]
    fn predict_numbers_eqnarray_rows() {
        // A label takes the counter as its row has it; a trailing `\\` adds a numbered row.
        let p = predict("\\begin{document}\\begin{eqnarray}a&=&b\\label{a}\\\\ c&=&d\\nonumber\\label{b}\\\\ e&=&f\\label{c}\\\\\\end{eqnarray}\\begin{equation}\\label{d}\\end{equation}\\begin{eqnarray*}x\\label{e}\\end{eqnarray*}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"], &p.labels["d"], &p.labels["e"]], ["1", "2", "2", "4", "5"]);
    }

    #[test]
    fn predict_numbers_gather_rows_and_multline_once() {
        // A `gather` numbers each row as `align` does; a `multline` is one number, its rows
        // whatever they are.
        let p = predict("\\begin{document}\\begin{gather}a\\label{a}\\\\ b\\nonumber\\\\ c\\label{b}\\end{gather}\\begin{multline}x\\\\ y\\label{c}\\\\ z\\end{multline}\\begin{equation}\\label{d}\\end{equation}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"], &p.labels["d"]], ["1", "2", "3", "4"]);
    }

    #[test]
    fn predict_takes_a_tag_as_the_label() {
        // `\tag{text}`: no number stepped; the display's label is the text, before or after it.
        let p = predict("\\begin{document}\\begin{equation}\\label{a}\\end{equation}\\begin{equation}\\label{b}x\\tag{A}\\end{equation}\\begin{equation*}y\\tag*{3'}\\label{c}\\end{equation*}\\begin{equation}\\label{d}\\end{equation}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"], &p.labels["d"]], ["1", "A", "3'", "2"]);
    }

    #[test]
    fn predict_takes_an_align_row_tag_as_the_label() {
        // A row's `\tag{text}` replaces its number; the other rows count on.
        let p = predict("\\begin{document}\\begin{align}a\\label{a}\\\\b\\tag{T}\\label{t}\\\\c\\label{c}\\end{align}\\begin{gather*}d\\tag*{S}\\label{s}\\end{gather*}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["t"], &p.labels["c"], &p.labels["s"]], ["1", "T", "2", "S"]);
    }

    #[test]
    fn utf8_characters_read_as_the_kernel_declares_them() {
        // One `char` per byte, as the job reads a file.
        let bytes = |s: &str| s.bytes().map(char::from).collect::<String>();
        assert_eq!(utf8_text(&bytes("é!")), Some((4, "\\'e")));
        assert_eq!(utf8_text(&bytes("’")), Some((6, "\\textquoteright")));
        assert_eq!(utf8_text(&bytes("鹏")), None);
        assert_eq!(text_names("\\c \\ ").collect::<Vec<_>>(), ["c", " "]);
        let doc = |body: &str| bytes(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}"));
        assert!(scan(&doc("Café, Gödel – it’s")).is_ok());
        assert!(scan(&doc("Å")).is_ok() && scan(&doc("鹏")).is_err());
    }

    #[test]
    fn scan_sends_a_redefined_internal_to_the_engine() {
        // `\def` defines as `\newcommand` does; a macro the natives read, or an `@` name, stops.
        let doc = |pre: &str| format!("\\documentclass{{article}}{pre}\\begin{{document}}x\\end{{document}}");
        let internal = Err("a definition of an internal macro");
        assert!(scan(&doc("\\def\\R#1{R#1}")).is_ok());
        assert_eq!(scan(&doc("\\renewcommand{\\thesection}{S}")), internal);
        // A plain prefix of `\thefigure`, `\thetable`, `\theequation`; nothing else.
        assert!(scan(&doc("\\renewcommand{\\thefigure}{S\\arabic{figure}}\\renewcommand\\theequation{A.\\arabic{equation}}")).is_ok());
        assert_eq!(scan(&doc("\\renewcommand{\\thefigure}{\\Roman{figure}}")), internal);
        assert_eq!(the_prefixes("\\renewcommand{\\thetable}{S-\\arabic{table}}\\begin{document}")[1], "S-");
        assert_eq!(scan(&doc("\\def\\labelitemi{--}")), internal);
        // The arrow fills call `\mathsm@sh` in place of amsmath's `\relbar`.
        assert_eq!(scan(&doc("\\usepackage{amsmath}\\renewcommand{\\relbar}{-}")), internal);
        assert_eq!(scan(&doc("\\makeatletter\\def\\@seccntformat#1{}\\makeatother")), internal);
        assert!(scan(&doc("\\providecommand{\\section}{S}")).is_ok());
        assert!(scan(&doc("\\renewcommand{\\baselinestretch}{1.5}\\renewcommand{\\textfraction}{.1}")).is_ok());
    }

    #[test]
    fn predict_reads_past_a_def_macro() {
        let p = predict("\\def\\half#1{#1/2}\\begin{document}\\section{S}\\label{s}$\\half{a}$\\def\\x{y}\\x\\section{T}\\label{t}");
        assert!(p.complete);
        assert_eq!([&p.labels["s"], &p.labels["t"]], ["1", "2"]);
    }

    #[test]
    fn predict_numbers_report_chapters() {
        // report: sections within chapters; equations, figures and footnotes reset per
        // chapter, with `\thechapter.` from the first chapter on.
        let p = predict("\\documentclass{report}\\begin{document}\\begin{equation}\\label{e0}\\end{equation}\\chapter{A}\\label{c1}\\section{S}\\label{s}\\subsection{T}\\label{t}\\begin{figure}\\caption{F}\\label{f}\\end{figure}\\chapter*{U}\\chapter{B}\\begin{equation}\\label{e}\\end{equation}\\section{S}\\label{s2}");
        assert!(p.complete);
        assert_eq!([&p.labels["e0"], &p.labels["c1"], &p.labels["s"], &p.labels["t"], &p.labels["f"], &p.labels["e"], &p.labels["s2"]], ["1", "1", "1.1", "1.1.1", "1.1", "2.1", "2.1"]);
    }

    #[test]
    fn predict_letters_sections_after_appendix() {
        // `\appendix`: sections from `A`, subsections `A.1`, equations within them `A.1`.
        let p = predict("\\numberwithin{equation}{section}\\begin{document}\\section{S}\\label{s}\\appendix\\section{A}\\label{a}\\subsection{B}\\label{b}\\begin{equation}\\label{e}\\end{equation}\\section{C}\\label{c}");
        assert!(p.complete);
        assert_eq!([&p.labels["s"], &p.labels["a"], &p.labels["b"], &p.labels["e"], &p.labels["c"]], ["1", "A", "A.1", "A.1", "B"]);
    }

    #[test]
    fn predict_numbers_equations_within_sections() {
        // `\numberwithin{equation}{section}`: `\thesection.n`, reset by a numbered section
        // only.
        let p = predict("\\numberwithin{equation}{section}\\begin{document}\\section{A}\\begin{equation}\\label{a}\\end{equation}\\subsection{B}\\begin{equation}\\label{b}\\end{equation}\\section*{C}\\begin{equation}\\label{c}\\end{equation}\\section{D}\\begin{equation}\\label{d}\\end{equation}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"], &p.labels["d"]], ["1.1", "1.2", "1.3", "2.1"]);
    }

    #[test]
    fn predict_reads_arrays_in_align_rows() {
        // A `cases` row's own `\\` does not end the align's row: two numbers, not four.
        let p = predict("\\begin{document}\\begin{align}f &= \\begin{cases} 0 & x\\\\ 1 & y \\end{cases} \\label{a}\\\\ g &= \\begin{pmatrix} 1\\\\ 2 \\end{pmatrix} \\label{b}\\end{align}\\section{S}\\label{c}");
        assert!(p.complete);
        assert_eq!([&p.labels["a"], &p.labels["b"], &p.labels["c"]], ["1", "2", "1"]);
        assert_eq!(split_rows("a \\begin{cases} x\\\\y \\end{cases} \\\\ b").len(), 2);
    }

    #[test]
    fn forward_refs_are_native_and_checked() {
        let doc = |body: &str| format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}");
        let now = 1792454400;
        assert!(run(&doc("See~\\ref{s}.\\section{S}\\label{s}"), now, None).is_ok());
        // Used, then labelled twice: pdfLaTeX's final run gives every `\ref` the last value.
        assert!(run(&doc("\\section{A}\\label{s}\\ref{s}\\section{B}\\label{s}"), now, None).is_err());
        // Never labelled: pdfLaTeX prints `??`.
        assert!(run(&doc("\\ref{nowhere}"), now, None).is_err());
        // A macro hides a heading from the pre-scan: no prediction, so not native.
        assert!(run("\\documentclass{article}\\newcommand{\\sec}{\\section{X}}\\begin{document}\\ref{x}\\sec\\label{x}\\end{document}", now, None).is_err());
    }

    #[test]
    fn contents_titles_from_the_source() {
        assert_eq!(heading_title(" [Short]{Long}"), Some("Short"));
        assert_eq!(heading_title("{A {b} c}"), Some("A {b} c"));
        assert!(toc_safe("The \\LaTeX\\ Way~and \\emph{x}") && !toc_safe("A\\label{x}") && !toc_safe("\\mine"));
        let doc = |pre: &str, body: &str| format!("\\documentclass{{article}}{pre}\\begin{{document}}\\tableofcontents{body}\\end{{document}}");
        let now = 1792454400;
        assert!(run(&doc("", "\\section{A}\\subsection{B}"), now, None).is_ok());
        // A macro in a title is expanded by the `.toc` file's `\write`: not native.
        assert!(run(&doc("\\newcommand{\\x}{y}", "\\section{\\x}"), now, None).is_err());
        // No entries: `\@starttoc`'s group would leave `\if@nobreak` as `\section*` set it.
        assert!(run(&doc("", "Text."), now, None).is_err());
    }

    #[test]
    fn class_options_pick_a_table_and_a_combination() {
        use data_class::option_key;
        assert_eq!(class_options("{article}"), Ok((0, 0, false)));
        assert_eq!(class_options("[12pt, a4paper,twoside]{article}"), Ok((2, option_key(3, false, true, false), false)));
        assert_eq!(class_options("[11pt]{report}"), Ok((1, 0, true)));
        assert_eq!(class_options("[11pt,landscape,draft,final]{article}"), Err("final"));
        assert_eq!(class_options("[twocolumn]{article}"), Err("twocolumn"));
        assert_eq!(class_options("{book}"), Err("documentclass"));
    }

    #[test]
    fn accepts_enumitem_keys_it_takes() {
        let doc = |pre: &str, body: &str| accepts(&format!("\\documentclass{{article}}\\usepackage{{enumitem}}{pre}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("", "\\begin{enumerate}[label=(\\alph*), leftmargin=*, itemsep=0pt]\\item a\\end{enumerate}"));
        assert!(doc("\\setlist{nosep}\\setlist[itemize,2]{label={--}}", "\\begin{itemize}\\item a\\end{itemize}"));
        assert!(!doc("", "\\begin{description}[leftmargin=*]\\item a\\end{description}"));
        assert!(!doc("", "\\begin{itemize}[align=left]\\item a\\end{itemize}"));
        assert!(!doc("", "\\begin{itemize}[labelwidth=*]\\item a\\end{itemize}"));
        assert!(!doc("", "\\setlist{nosep}\\arabic*"));
        assert!(!doc("", "\\setlist{nosep}"));
    }

    #[test]
    fn accepts_text_scripts() {
        let doc = |body: &str| accepts(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("1\\textsuperscript{st} H\\textsubscript{2}O"));
    }

    #[test]
    fn enumitem_label_templates() {
        assert_eq!(enit_template("label=(\\roman*), leftmargin=*").as_deref(), Some("(\\roman*)"));
        assert_eq!(enit_template("topsep=2pt, label=Case \\Alph*:").as_deref(), Some("Case \\Alph*:"));
        assert_eq!(enit_template("label=\\textbf{\\arabic*}"), None);
        assert_eq!(enit_template("label=[\\arabic*]"), None);
        let doc = |body: &str| accepts(&format!("\\documentclass{{article}}\\usepackage{{enumitem}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("\\begin{description}\\item[A] b\\end{description}"));
    }

    #[test]
    fn accepts_fancyhdr_slots() {
        let doc = |pre: &str, body: &str| accepts(&format!("\\documentclass{{article}}{pre}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("\\usepackage{fancyhdr}\\pagestyle{fancy}\\fancyhf{}\\lhead{Lab}\\rhead{\\thepage}\\fancyfoot[C]{x}", "\\section{A}y"));
        assert!(!doc("", "\\fancyhf{}"));
        assert!(!doc("\\usepackage{fancyhdr}", "\\thepage"));
    }

    #[test]
    fn accepts_colorbox_under_xcolor_only() {
        let doc = |pre: &str, body: &str| accepts(&format!("\\documentclass{{article}}{pre}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("\\usepackage{xcolor}", "\\colorbox{yellow}{x}\\fcolorbox{red}{white}{y}\\fbox{z}"));
        assert!(!doc("\\usepackage{color}", "\\colorbox{yellow}{x}"));
        assert!(doc("", "\\fbox{z}"));
    }

    #[test]
    fn accepts_parbox_and_minipage_without_height() {
        let doc = |body: &str| accepts(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("\\parbox{3cm}{x} \\parbox[t]{3cm}{x} \\parbox [b]{3cm}{x}"));
        assert!(doc("\\begin{minipage}{3cm}x\\end{minipage}\\begin{minipage}[t]{.4\\textwidth}x\\end{minipage}"));
        assert!(!doc("\\parbox[t][2cm]{3cm}{x}"));
        assert!(!doc("\\parbox[s]{3cm}{x}"));
        assert!(!doc("\\begin{minipage}[c][2cm][c]{3cm}x\\end{minipage}"));
    }

    #[test]
    fn accepts_fontsize_with_selectfont() {
        let doc = |body: &str| accepts(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(doc("{\\fontsize{12}{14.5pt}\\selectfont x}"));
        assert!(doc("\\fontsize{42}{46} \\selectfont\\bfseries x"));
        assert!(!doc("\\fontsize{12}{14}x"));
        assert!(!doc("\\fontsize{\\fsize}{14}\\selectfont x"));
        assert!(!doc("\\fontsize{12}{14}\\selectfonts x"));
        assert!(!doc("\\selectfont x"));
    }

    #[test]
    fn accepts_plain_article_text_only() {
        let doc = "\\documentclass{article}\n\\begin{document}\nHello world.\n\\end{document}\n";
        assert_eq!(accepts(doc), Some("\\documentclass{article}\n\\begin{document}\nHello world.\n"));
        assert_eq!(accepts("\\documentclass{book}\\begin{document}x\\end{document}"), None);
        assert_eq!(accepts("\\documentclass{article}\\usepackage{x}\\begin{document}x\\end{document}"), None);
        assert_eq!(accepts("\\documentclass{article}\\begin{document}\\section{x}\\end{document}"), Some("\\documentclass{article}\\begin{document}\\section{x}"));
        assert_eq!(accepts("\\documentclass{article}\\begin{document}\\begin{tabular}x\\end{document}"), None);
        // `tabular`: a preamble `mkpream_kernel` takes, and `&` and `\\hline` only inside one.
        let tab = |body: &str| accepts(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(tab("\\begin{tabular}[t]{|l|*{2}{c}|}\\hline a&b&c\\\\\n\n\\hline\\end{tabular}"));
        assert!(tab("\\begin{tabular}{p{1cm}}a\\end{tabular}"));
        assert!(!tab("\\begin{tabular}{m{1cm}}a\\end{tabular}"));
        assert!(tab("\\begin{tabular}{@{:}c}a\\end{tabular}"));
        assert!(!tab("\\begin{tabular}{@{\\extracolsep{\\fill}}c}a\\end{tabular}"));
        assert!(!tab("a&b"));
        assert!(!tab("\\begin{tabular}{c}a\\end{tabular} b & c"));
        assert!(!tab("\\hline"));
        // A user macro is accepted where it is defined; an unknown command is not.
        let ip = "\\documentclass{article}\\newcommand{\\ip}[2]{(#1, #2)}\\begin{document}\\ip{a}{b}";
        assert_eq!(accepts(&format!("{ip}\\end{{document}}")), Some(ip));
        assert_eq!(accepts("\\documentclass{article}\\begin{document}\\ip{a}{b}\\end{document}"), None);
        // small2e: `\end{document}` in a comment does not end the text.
        let c = "\\documentclass{article}\n\\begin{document} % the \\end{document} command\n\\section{x}\n";
        assert_eq!(accepts(&format!("{c}\\end{{document}}")), Some(c));
        // Preamble code, `\let` to `\document`, `\endinput`, and `\@` before a letter.
        let lp = "\\makeatletter\\ifx\\@preamblecmds\\@notprerr\\else\\documentclass{article}\\let\\X\\document\\let\\endX\\enddocument\\fi\\makeatother\\begin{X}A\\@ B.\\end{X}";
        assert_eq!(accepts(&format!("{lp}\\endinput")), Some(lp));
        assert_eq!(accepts("\\makeatletter\\begin{document}x\\end{document}"), None);
    }
}
