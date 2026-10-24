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

use super::engine_catcode::Cat;
use super::engine_eqtb::{Code, Glue, Reg, Region};
use super::engine_node::{BoxRec, Kind, Node, IGNORE_DEPTH, RUNNING};
use super::engine_pack::{BASELINE_SKIP, BOX_MAX_DEPTH, HANG_INDENT};
use super::engine_prim::Meaning;
use super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::engine_scan::{scan_dimen_ref, scan_glue};
use super::engine_token::Token;
use crate::data::data_class::{self, ClassTable};
use crate::data::data_nfss;

#[path = "engine_tabular.rs"]
mod tabular;
#[path = "engine_float.rs"]
mod float;

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
pub const PACKAGE_NATIVES: [(&str, &str, &str); 2] = [("amsfonts", "widehat", "rt@widehat"), ("amsfonts", "widetilde", "rt@widetilde")];

/// Whether the native job writes `name` itself, so a table's macro of that name is not
/// installed (`install`): a `DOC` native or an OT1 text symbol.
pub fn native_name(name: &str) -> bool {
    DOC.iter().any(|d| d.0 == name) || TEXT_SYMBOLS.iter().any(|s| s.0 == name)
}

/// A math alphabet's family: a symbol font's, or one given on first use.
#[derive(Clone, Copy)]
enum Alphabet {
    Fam(u8),
    Shape(&'static str, Option<u8>),
}

/// The native commands, by `Meaning::Doc` index. Names starting `rt@` are the steps of a
/// command that typesets material in between (read back from the input).
static DOC: [(&str, Native); 172] = [
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
    ("$", |m| m.text_symbol("TS1", 36)),
    // `\protected\def\{{\ifmmode\lbrace\else\textbraceleft\fi}`.
    ("{", |m| m.brace_symbol("lbrace", 102)),
    ("}", |m| m.brace_symbol("rbrace", 103)),
    ("_", |m| m.text_underscore()),
    ("textunderscore", |m| m.text_underscore()),
    ("LaTeX", |m| m.logo(true)),
    ("TeX", |m| m.logo(false)),
    ("ldots", |m| m.ellipsis()),
    ("dots", |m| m.ellipsis()),
    ("textellipsis", |m| m.ellipsis()),
    (",", |m| m.thin_space()),
    ("thinspace", |m| m.thin_space()),
    ("@", |m| m.space_factor_1000()),
    ("nobreakspace", |m| m.nobreak_space()),
    ("\0~", |m| m.nobreak_space()),
    ("mbox", |m| m.mbox()),
    ("newcommand", |m| m.new_command()),
    ("renewcommand", |m| m.new_command()),
    ("title", |m| m.store(0)),
    ("author", |m| m.store(1)),
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
    ("setlength", |m| m.set_length(false)),
    ("addtolength", |m| m.set_length(true)),
    ("setcounter", |m| m.set_counter()),
    ("pagestyle", |m| m.page_style()),
    ("usepackage", |m| m.use_package()),
    ("rt@widehat", |m| m.wide_accent(0x5B, 0x0362)),
    ("rt@widetilde", |m| m.wide_accent(0x5D, 0x0365)),
    ("rt@wideaccent", |m| m.wide_accent_end()),
    ("vspace", |m| m.vspace()),
    ("rt@esphack", |m| m.esphack_end()),
    ("rt@eqstep", |m| m.equation_step()),
    ("rt@endequation", |m| m.end_equation()),
    ("rt@normalfont", |m| m.normal_font()),
    ("rt@quotation", |m| m.list_env("quotation")),
    ("cite", |m| m.cite()),
    ("bibitem", |m| m.bibitem()),
    ("rt@bibopen", |m| m.bib_open()),
    ("sqrt", |m| m.sqrt()),
    ("rt@tabu", |m| m.tabular_template(false)),
    ("rt@tabv", |m| m.tabular_template(true)),
    // Read by `align_peek` (`align_peek_native`); anywhere else not native.
    ("rt@tabend", |m| m.not_native("\\endtabular outside its alignment")),
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
];
const OUTPUT: usize = 0;
const OUTPUT_FOOT: usize = 1;
const SEC_BOX: usize = 5;
const SEC_TITLE: usize = 6;
const SEC_END: usize = 7;
const TEXT_END: usize = 8;
const TEXT_END_N: usize = 9;
const MAYBE_IC: usize = 10;
const EVERY_MATH: usize = 48;
const EVERY_DISPLAY: usize = 49;
const FN_MARK_FONT: usize = 52;
const FN_MARK_END: usize = 53;
const FN_SETUP: usize = 54;
const FN_END: usize = 55;
const STEP: usize = 56;
const PARA_HOOK: usize = 57;
const LIST_PAR: usize = 60;
const ENDPE_PAR: usize = 61;
const PROPAGATE: usize = 62;
const ITEM_LABELS: usize = 63;
const LABEL_MARK: usize = 64;
const NEW_PAGE: usize = 76;
const TABULAR: usize = 77;
const NOTPRERR: usize = 85;
const STOP: usize = 86;
const RUN_IN_END: usize = 90;
const DESC_FONT: usize = 91;
const VERBATIM: usize = 93;
const LEAVE: usize = 94;
const PRIM_PAR: usize = 95;
const END: usize = 43;
const TAB_U: usize = 133;
const TAB_V: usize = 134;
const TAB_END: usize = 135;
const HLINE: usize = 136;
const TAB_SKIP: usize = 137;
const MULTICOLUMN: usize = 139;
const MC_BODY: usize = 140;
const MC_TAIL: usize = 141;
const CLINE_CMD: usize = 142;
const CLINE: usize = 143;
const NEWLINE: usize = 59;
const CENTER_CR: usize = 165;
const TAB_PAR: usize = 167;
const CAPTION_MID: usize = 169;
const CAPTION_END: usize = 170;
const FLOAT_FLUSH: usize = 171;
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
/// `\l@nohyphenation` (language.dat's third language).
const NOHYPHENATION: i32 = 2;
/// `\@labels`, `\@tempboxa` and `\@arstrutbox`, hidden box registers.
const LABELS_BOX: u32 = 32700;
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
/// `\ht\strutbox` and `\dp\strutbox` (`strut_update`), hidden dimension registers.
const STRUT_HT: Reg = Reg::register(Region::Dimen, 32700);
const STRUT_DP: Reg = Reg::register(Region::Dimen, 32701);

/// [`data_class::CLASS_REGS`] indexes.
/// The first of the registers holding [`data_class::CLASS_DIMENS`] (dimensions, in order).
const GEOMETRY: u32 = 32680;
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
    /// `thebibliography`'s `\@biblabel{\@arabic\c@enumiv}`.
    Bib,
}
/// `\footins` (`\newinsert`): its box, `\count`, `\dimen` and `\skip` register.
const FOOTINS: u32 = 253;

/// The control words a native document may use (with the macros it defines), and its
/// environments.
const WORDS: [&str; 103] = [
    "section", "subsection", "subsubsection", "emph", "textbf", "textmd", "textit", "textsl", "textsc", "textup", "textrm", "textsf", "texttt", "LaTeX", "TeX", "ldots", "dots", "textellipsis", "thinspace", "nobreakspace", "mbox", "newcommand", "renewcommand", "title", "author", "date", "begin", "end", "documentclass", "textunderscore", "footnote", "item",
    "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "maketitle", "today", "providecommand", "label", "ref", "paragraph", "pageref", "tableofcontents", "H", "u", "v", "r", "i", "j", "ss", "ae", "oe", "o", "AE", "OE", "O", "setlength", "addtolength", "setcounter", "pagestyle", "vspace", "cite", "bibitem", "sqrt", "hline", "tabularnewline", "multicolumn", "cline", "bfseries", "mdseries", "itshape", "slshape", "scshape", "upshape", "rmfamily", "sffamily", "ttfamily", "normalfont", "bf", "it", "sl", "sc", "rm", "sf", "tt", "newpage", "centering", "raggedright", "raggedleft", "hspace", "noindent", "textwidth", "textheight", "linewidth", "par", "caption",
];
/// ot1enc.def's `\DeclareTextSymbol`s: in OT1 each is its `\OT1\name`, a `\chardef`, so
/// the main loop's ligatures and kerns see it as a character.
const TEXT_SYMBOLS: [(&str, u32); 9] = [("i", 16), ("j", 17), ("ss", 25), ("ae", 26), ("oe", 27), ("o", 28), ("AE", 29), ("OE", 30), ("O", 31)];
/// Commands of preamble code (`\makeatletter`, `\ifx`, `\let`, …); `\let` and
/// `\providecommand` define their first name, as `\newcommand` does.
const PREAMBLE: [&str; 13] = ["makeatletter", "makeatother", "ifx", "else", "fi", "bgroup", "egroup", "relax", "document", "enddocument", "@preamblecmds", "@notprerr", "endinput"];
const ENVS: [&str; 20] = ["figure", "table", "figure*", "table*", "tabular", "document", "em", "quote", "quotation", "verse", "itemize", "enumerate", "center", "description", "verbatim", "flushleft", "flushright", "abstract", "equation", "thebibliography"];

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
    /// The math version `normal` (`\mv@normal`): each family's NFSS shape, the symbol fonts
    /// then the math alphabets in their order of first use.
    fams: Vec<&'static str>,
    /// The math alphabets, by `Meaning::Doc` index past `DOC`: name, whether `\use@mathgroup`
    /// skips its encoding switch, and its family or its shape and family once given one.
    alphabets: Vec<(&'static str, bool, Alphabet)>,
    /// The packages loaded from native tables (`data_kernel::package`).
    packages: Vec<&'static str>,
    /// amsfonts' `\symAMSb`, and each pending wide accent: its argument and its accent.
    wide_fam: u32,
    wide: Vec<(Vec<Token>, u32)>,
    /// `\c@equation`; `\@savsk` and `\@savsf` of each `\@bsphack` awaiting its `\@esphack`.
    equation: i32,
    sphack: Vec<(Glue, i32)>,
    /// `\c@footnote`; the footnote text between the mark and the insertion; the space
    /// factor before each mark (`\@x@sf`, -1 outside horizontal mode); `\if@nobreak` and
    /// the heading's `\everypar` outside each footnote (`\@parboxrestore` clears them).
    footnote: i32,
    footnote_text: Vec<Token>,
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
    /// What follows `\@trivlist` (`\list`'s rest, `\trivlist`'s).
    trivlist_then: Native,
    /// Whether `\date` was given (else `\today`).
    dated: bool,
    /// The first construct met that the native job does not implement.
    unsupported: Option<String>,
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
    toc_seen: usize,
    toc_pages: Vec<Option<i32>>,
    /// `\tableofcontents`'s page-number boxes, each with its entry: typeset with a digit
    /// standing in, filled in before the pages ship (`finish_contents`).
    toc_boxes: Vec<(u32, usize)>,
    /// With a contents list, pages are kept (`deferred`) and shipped in order at the end;
    /// `replay` while they ship, so the page records are not taken again.
    defer: bool,
    deferred: Vec<u32>,
    replay: bool,
    /// `\@afterindenttrue` inside `\@dottedtocline`'s group, restored after it.
    toc_afterindent: Vec<bool>,
    /// After `\begin{document}`: the page geometry is fixed from then on.
    body: bool,
    /// `\pagestyle`: `\ps@plain` (false) or `\ps@empty` (true); `\thispagestyle{plain}`
    /// (`\@specialpage`) for the next page; whether the page being shipped has an empty foot.
    empty_style: bool,
    special_plain: bool,
    empty_foot: bool,
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
    let mut class = false;
    // The packages `\usepackage` loads from native tables, with the packages they load.
    let mut loaded: Vec<&'static str> = Vec::new();
    // Open `tabular`s: `&`, `\hline` and `\tabularnewline` are taken only inside one.
    let mut tabs = 0usize;
    let mut floats = 0usize;
    let mut i = 0;
    while i < b.len() {
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
                let have = loaded.iter().fold(KERNEL_BIT, |m, &p| m | package_bit(p));
                let known = |name: &str| table_names(name) & have != 0 || WORDS.contains(&name) || PREAMBLE.contains(&name) || defined.contains(&name);
                if rest[..n].contains('@') && !known(&rest[..n]) {
                    n = rest[..n].find('@').unwrap_or(n);
                }
                if n == 0 {
                    let symbol = rest.get(..rest.chars().next().map_or(0, char::len_utf8)).unwrap_or("");
                    if !rest.bytes().next().is_some_and(|c| b" $&#%_{},@\r\n\t()[]\\\"'.=^`~".contains(&c)) && !known(symbol) {
                        stop(rest.get(..rest.chars().next().map_or(0, char::len_utf8)).unwrap_or(""), &mut blocked)?;
                        i += 1 + rest.chars().next().map_or(0, char::len_utf8);
                        continue;
                    }
                    i += 2;
                    continue;
                }
                let name = &rest[..n];
                if tabs == 0 && matches!(name, "hline" | "tabularnewline" | "multicolumn" | "cline") || floats == 0 && name == "caption" {
                    stop(name, &mut blocked)?;
                }
                if name == "multicolumn" {
                    // `{n}{spec}`: one column `tabular::mkpream` takes.
                    let after = rest[n..].trim_start();
                    let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| r[k + 1..].trim_start())).and_then(|r| r.strip_prefix('{')).and_then(|r| balanced(r).map(|k| &r[..k]));
                    if !spec.and_then(|s| tabular::mkpream(s.as_bytes())).is_some_and(|p| p.cols.len() == 1) {
                        stop("multicolumn preamble", &mut blocked)?;
                    }
                }
                if define_next {
                    defined.push(name);
                    define_next = false;
                } else if matches!(name, "newcommand" | "renewcommand" | "providecommand" | "let") {
                    define_next = true;
                } else if name == "usepackage" {
                    // `\usepackage[options]{a,b}`: each a package with a native table, no options.
                    let after = rest[n..].trim_start();
                    let (opts, after) = match after.strip_prefix('[') {
                        Some(r) => r.split_once(']').map_or(("x", ""), |(o, a)| (o, a.trim_start())),
                        None => ("", after),
                    };
                    if !opts.trim().is_empty() {
                        stop("package options", &mut blocked)?;
                    }
                    let Some(list) = after.strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0) else { return Err(name) };
                    for p in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                        if !require(p, &mut loaded) {
                            stop(p, &mut blocked)?;
                        }
                    }
                    i = src.len() - after.len() + 1 + list.len() + 1;
                    continue;
                } else if name == "documentclass" {
                    if class {
                        stop("documentclass", &mut blocked)?;
                    }
                    if let Err(what) = class_options(&rest[n..]) {
                        stop(what, &mut blocked)?;
                    }
                    class = true;
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
                    i += 1 + n + (rest.len() - n - value.len()) + len + 2;
                    continue;
                }
                if name == "begin" || name == "end" {
                    let env = rest[n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0).ok_or(name)?;
                    if !ENVS.contains(&env) && !defined.contains(&env) {
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
                    if env == "tabular" && name == "end" {
                        tabs = tabs.saturating_sub(1);
                    } else if env == "tabular" && !defined.contains(&env) {
                        // `[pos]{spec}`: a preamble `tabular::mkpream` takes.
                        tabs += 1;
                        let after = rest[n + env.len() + 2..].trim_start();
                        let after = match after.strip_prefix('[') {
                            Some(r) => r.split_once(']').map_or("", |(_, a)| a.trim_start()),
                            None => after,
                        };
                        let spec = after.strip_prefix('{').and_then(|r| balanced(r).map(|k| &r[..k]));
                        if spec.and_then(|s| tabular::mkpream(s.as_bytes())).is_none() {
                            stop("tabular preamble", &mut blocked)?;
                        }
                    }
                }
                i += 1 + n;
            }
            c if c.is_ascii_alphanumeric() || b" \t\r\n.,;:!?'`\"()[]*/-{}~#$^_=+<>|@".contains(&c) => i += 1,
            _ => {
                let n = src[i..].chars().next().map_or(1, char::len_utf8);
                stop(src.get(i..i + n).filter(|c| c.is_ascii()).unwrap_or("a non-ASCII character"), &mut blocked)?;
                i += n;
            }
        }
    }
    Err("no \\end{document}")
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

/// `\documentclass[options]{article}` after its name: the size option's index in
/// `data_class::CLASS_SIZES` and the combination of the parameter-only options
/// (`data_class::option_key`), else the first thing the native job does not take:
/// another class, an option with behaviour of its own (`twocolumn`, `titlepage`, `leqno`,
/// `fleqn`, `openbib`), an unknown one, or two of one group (`\ProcessOptions` would apply
/// them in the class's order, not the document's).
fn class_options(rest: &str) -> Result<(usize, u32), &str> {
    let (opts, after) = match rest.strip_prefix('[') {
        Some(r) => r.split_once(']').ok_or("documentclass")?,
        None => ("", rest),
    };
    if !after.trim_start().starts_with("{article}") {
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
                _ => return Err(o),
            }
        };
        if slot.0.replace(slot.1).is_some() {
            return Err(o);
        }
    }
    let key = data_class::option_key(paper.unwrap_or(0), orient == Some(1), side == Some(1), draft == Some(1));
    Ok((size.unwrap_or(0), key))
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
    struct Group {
        label: String,
        depth: usize,
        enumerate: bool,
    }
    let mut groups = vec![Group { label: String::new(), depth: 0, enumerate: false }];
    let (mut sec, mut enums, mut footnote, mut equation) = ([0u32; SECNUMDEPTH], [0u32; 4], 0u32, 0u32);
    let mut pending: Option<String> = None;
    let (mut floats, mut figtab) = (Vec::new(), [0u32; 2]);
    let b = src.as_bytes();
    let next = |i: usize| src[i..].trim_start_matches([' ', '\t', '\r', '\n']).as_bytes().first().copied();
    let push = |groups: &mut Vec<Group>, pending: &mut Option<String>| {
        let top = groups.last().expect("a group");
        let g = Group { label: pending.take().unwrap_or_else(|| top.label.clone()), depth: top.depth, enumerate: top.enumerate };
        groups.push(g);
    };
    let mut i = start + "\\begin{document}".len();
    while i < b.len() {
        match b[i] {
            b'%' => i = src[i..].find('\n').map_or(b.len(), |n| i + n + 1),
            b'{' => {
                push(&mut groups, &mut pending);
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
                    i += 2;
                    continue;
                }
                let name = &rest[..n];
                i += 1 + n;
                if rest[n..].starts_with('@') {
                    return out;
                }
                let arg = || rest[n..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0);
                match name {
                    "begin" | "end" => {
                        let Some(env) = arg() else { return out };
                        if name == "end" {
                            if groups.len() > 1 {
                                groups.pop();
                            }
                            if matches!(env, "figure" | "table" | "figure*" | "table*") {
                                floats.pop();
                            }
                        } else if env == "verbatim" {
                            let Some(e) = src[i..].find("\\end{verbatim}") else { return out };
                            i += e + "\\end{verbatim}".len();
                            continue;
                        } else {
                            push(&mut groups, &mut pending);
                            let g = groups.last_mut().expect("a group");
                            match env {
                                "enumerate" => {
                                    if g.depth == 4 {
                                        return out;
                                    }
                                    g.depth += 1;
                                    g.enumerate = true;
                                    enums[g.depth - 1] = 0;
                                }
                                "itemize" | "description" | "quote" | "quotation" | "verse" | "center" | "flushleft" | "flushright" | "abstract" | "thebibliography" => g.enumerate = false,
                                "equation" => {
                                    equation += 1;
                                    g.label = equation.to_string();
                                }
                                // An entry is a group; `\label` there takes the label around.
                                "em" | "document" | "tabular" => {}
                                "figure" | "figure*" | "table" | "table*" => {
                                    g.enumerate = false;
                                    floats.push(env.starts_with("table") as usize);
                                }
                                _ => return out,
                            }
                        }
                        i += 2 + env.len();
                    }
                    "section" | "subsection" | "subsubsection" | "paragraph" => {
                        if next(i) == Some(b'*') {
                            continue;
                        }
                        let level = ["section", "subsection", "subsubsection", "paragraph"].iter().position(|s| *s == name).expect("a heading");
                        let mut number = String::new();
                        if level < SECNUMDEPTH {
                            sec[level] += 1;
                            sec[level + 1..].fill(0);
                            number = sec[..=level].iter().map(u32::to_string).collect::<Vec<_>>().join(".");
                            groups.last_mut().expect("a group").label.clone_from(&number);
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
                        if !g.enumerate {
                            continue;
                        }
                        if next(i) == Some(b'[') {
                            return out;
                        }
                        let d = g.depth;
                        enums[d - 1] += 1;
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
                        g.label = text;
                    }
                    // `\caption`'s `\refstepcounter\@captype`, in the float's group.
                    "caption" => {
                        let Some(t) = floats.last().copied() else { return out };
                        figtab[t] += 1;
                        groups.last_mut().expect("a group").label = figtab[t].to_string();
                    }
                    "footnote" => {
                        if next(i) == Some(b'[') {
                            return out;
                        }
                        footnote += 1;
                        pending = Some(footnote.to_string());
                    }
                    // `\maketitle`'s `\setcounter{footnote}{0}`.
                    "maketitle" => footnote = 0,
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
                        out.labels.insert(key.to_string(), groups.last().expect("a group").label.clone());
                        i += 2 + key.len();
                    }
                    "ref" => {
                        let Some(key) = arg() else { return out };
                        i += 2 + key.len();
                    }
                    _ if WORDS.contains(&name) && !matches!(name, "newcommand" | "renewcommand" | "providecommand") => {}
                    // Kernel values, operators and math macros number nothing.
                    _ if table_names(name) != 0 => {}
                    _ => return out,
                }
            }
            _ => i += 1,
        }
    }
    out.complete = true;
    out
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

/// What [`predict`] reads off a body: the labels' values, the contents entries, and whether
/// it read the whole body (else the entries are not all there).
#[derive(Default)]
struct Predicted {
    labels: std::collections::HashMap<String, String>,
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
    let body = scan(src).map_err(|what| format!("the pre-scan: {what}"))?;
    let src_class = src.find("\\documentclass").map(|i| &src[i + "\\documentclass".len()..]).ok_or("no \\documentclass")?;
    let (size, key) = class_options(src_class).map_err(|what| format!("the class options: {what}"))?;
    let class = data_class::article_size(size).ok_or("no article class table in this build")?;
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
    let mut predicted = if body.contains("\\ref") || body.contains("\\tableofcontents") { predict(body) } else { Predicted::default() };
    if body.contains("\\cite") {
        predict_bib(body, &mut predicted.labels);
    }
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

/// A file from the binary (`data_embed`): `pdftex.map` is `rushtex.map`, the map lines of
/// the TFM table's fonts; a table font's `.vf` is embedded when the pin has one.
fn carried(name: &str) -> Carried {
    let Some(emb) = crate::data::data_embed::embedded() else { return Carried::Unknown };
    let file = if name == "pdftex.map" { "rushtex.map" } else { name };
    if let Some(b) = emb.read(file) {
        return Carried::File(b);
    }
    match name.strip_suffix(".vf") {
        Some(stem) if crate::data::data_tfm::fonts().is_some_and(|t| t.lookup(stem).is_some()) && emb.contains("rushtex.map") => Carried::Absent,
        _ => Carried::Unknown,
    }
}

fn job(text: &str, class: &'static ClassTable<'static>, key: u32, now: i64, read: &Read<'_>, predicted: &Predicted) -> Result<Output, String> {
    // `RT_TIME=1`: the job's phases, in milliseconds, on standard error.
    let clock = std::env::var_os("RT_TIME").map(|_| std::time::Instant::now());
    let lap = |what: &str| {
        if let Some(c) = clock {
            eprintln!("{:8.3} ms  {what}", c.elapsed().as_secs_f64() * 1e3);
        }
    };
    let mut e = super::Engine::default();
    let mut m = Machine::new(&mut e, text);
    m.read_file = Some(Box::new(read));
    m.typeset = true;
    lap("engine");
    m.load_class_with(class, key)?;
    m.fix_date_and_time(now);
    if let Some(doc) = m.doc.as_mut() {
        doc.predicted.clone_from(&predicted.labels);
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
        // latex.ltx: `\def\^^M{\ }`, `\let\^^I\^^M`: control space.
        if let Some(space) = self.e.cs.get(" ").map(|id| self.e.eqtb.meaning(id)) {
            for name in ["\r", "\t"] {
                let cs = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(cs, space, true);
            }
        }
        let output = self.e.toks.store(&[Token::char(Cat::BeginGroup, '{'), Token::cs(ids[OUTPUT]), Token::char(Cat::EndGroup, '}')]);
        if let Some(r) = super::engine_names::param("output") {
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
            self.install(k, None, &mut fams, &mut alphabets);
        }
        let par_id = self.e.cs.intern("par");
        let (cr_id, crcr_id, stretch_id) = (self.e.cs.intern("cr"), self.e.cs.intern("crcr"), self.e.cs.intern("arraystretch"));
        let (omit_id, span_id, newline_id) = (self.e.cs.intern("omit"), self.e.cs.intern("span"), self.e.cs.intern("\\"));
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
        for (name, r) in data_class::CLASS_REGS.iter().zip(regs) {
            if !name.contains(' ') {
                let id = self.e.cs.intern(name);
                self.e.eqtb.set_meaning(id, Meaning::Reg(r), true);
            }
        }
        self.doc = Some(Box::new(Doc { class: Class { table: t, vals }, page: 1, normal_font, column: None, ids, nobreak: false, afterindent: true, inlabel: false, newlist: false, noitemarg: false, endpe: false, lists: Vec::new(), tabs: Vec::new(), tab_group: 0, floats: Default::default(), cr_base: 0, font_switch: Vec::new(), families: DEFAULT_FAMILIES, font_fixed: Vec::new(), cr_id, crcr_id, stretch_id, newline_id, omit_id, span_id, par_prim, par_id, counters: [0; 3], title: Vec::new(), number_box: None, after: Glue::default(), numbered: true, toc: true, noskipsec: false, stored: Default::default(), math_size: "", fams, alphabets, packages: Vec::new(), wide_fam: 0, wide: Vec::new(), equation: 0, sphack: Vec::new(), footnote: 0, footnote_text: Vec::new(), footnote_sf: Vec::new(), footnote_flags: Vec::new(), steps: Vec::new(), level: 0, after_before: Glue::default(), indent: None, current: 0, item_label: None, trivlist_then: |_| {}, dated: false, unsupported: None, verbatim: Vec::new(), labels: Default::default(), predicted: Default::default(), forward: Default::default(), used: Default::default(), label_keys: Vec::new(), label_pages: Default::default(), contents: None, toc_seen: 0, toc_pages: Vec::new(), toc_boxes: Vec::new(), defer: false, deferred: Vec::new(), replay: false, toc_afterindent: Vec::new(), body: false, empty_style: false, special_plain: false, empty_foot: false, regs }));
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
        let p = super::engine_names::param("outputpenalty").map_or(0, |r| self.e.eqtb.word(r));
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
        let (page, font) = (doc.page, doc.normal_font);
        // `\@outputpage`: `\if@specialpage \global\@specialpagefalse \@nameuse{ps@\@specialstyle}
        // \fi`; `\ps@empty`'s `\@oddfoot` is `\@empty`.
        let empty = self.doc.as_mut().is_some_and(|d| {
            d.empty_foot = d.empty_style && !d.special_plain;
            d.special_plain = false;
            d.empty_foot
        });
        self.push_nest(Mode::Horizontal, true, 1000);
        if empty {
            let t = Token::cs(self.e.cs.intern(DOC[OUTPUT_FOOT].0));
            return self.input.back(t);
        }
        // `\@thefoot` of `\ps@plain`: `\reset@font\hfil\thepage\hfil`, in the output group.
        self.select_font(font, false);
        let n = self.e.nodes.glue_node(Glue { stretch: 65536, stretch_order: 1, ..Glue::default() }, 0);
        self.cur.push(n);
        let mut list: Vec<Token> = page.to_string().chars().map(|c| Token::char(Cat::Other, c)).collect();
        list.push(Token::cs(self.e.cs.intern(DOC[OUTPUT_FOOT].0)));
        self.back_list(&list);
    }

    /// LaTeX's `\@outputpage` for `\ps@plain`, one column, one side: the page box
    ///
    /// `\vbox{\vskip\topmargin \moveright\oddsidemargin\vbox{head \vskip\headsep column foot}}`
    ///
    /// with the head a `\vbox to\headheight` of depth 0, the column after `\lineskip` glue
    /// (`\baselineskip`, `\lineskip` and `\lineskiplimit` are 0) and the foot at
    /// `\baselineskip\footskip`. Then `\stepcounter{page}`.
    fn output_page(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(column) = doc.column.take() else { return };
        doc.page += 1;
        let class = doc.class;
        let empty = doc.empty_foot;
        let width = class.dimen("textwidth");
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        // The foot: `\hb@xt@\textwidth{\hfil 1\hfil}` in `\color@hbox` (empty for `\ps@empty`).
        if !empty {
            let n = self.e.nodes.glue_node(fil, 0);
            self.cur.push(n);
        }
        let start = self.start as usize;
        let inner = self.hpack(start, width, true);
        self.pop_nest();
        let foot = self.wrap_hbox(inner);
        // The head: `\vbox to\headheight{\vfil \color@hbox{\hb@xt@\textwidth{}}}`.
        let start = self.cur.len();
        let empty = self.hpack(start, width, true);
        let head_line = self.wrap_hbox(empty);
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
        let shipped = self.vpack(start, 0, false, d);
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

    fn set_int_named(&mut self, name: &str, v: i32) {
        if let Some(r) = super::engine_names::param(name) {
            self.e.eqtb.set_word(r, v, false);
        }
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
                    self.set_int_named("clubpenalty", 10000);
                    if drop {
                        self.drop_last_box();
                    }
                } else {
                    let v = self.rw(R_CLUBPENALTY);
                    self.set_int_named("clubpenalty", v);
                    self.set_everypar(0);
                }
            }
            ITEM => self.item_everypar(),
            // `\@setminipage`'s `\everypar{\@minipagefalse\everypar{}}`.
            MINIPAGE => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.floats.minipage = false;
                }
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
    fn read_arg(&mut self, out: &mut Vec<Token>) {
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
    fn next_is(&mut self, c: char) -> bool {
        let Some(t) = self.next_unblank() else { return false };
        if t == Token::char(Cat::Other, c) {
            return true;
        }
        self.input.back(t);
        false
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
        doc.title = title;
        doc.numbered = !star && level < SECNUMDEPTH;
        doc.toc = !star;
        doc.level = level;
        let noskipsec = doc.noskipsec;
        if doc.numbered {
            // `\refstepcounter`'s `\@currentlabel`: the number as `\@sect` steps it.
            let mut c = doc.counters;
            c[level] += 1;
            c[level + 1..].fill(0);
            let text = c[..=level].iter().map(|v| v.to_string()).collect::<Vec<_>>().join(".");
            self.set_current_label(&text);
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
        let sec = &SECS[level];
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
            self.set_int_named("clubpenalty", v);
            return self.set_everypar(0);
        }
        doc.noskipsec = false;
        let mut list = std::mem::take(&mut doc.title);
        let size = SECS[doc.level].size;
        self.drop_last_box();
        self.set_int_named("clubpenalty", 10000);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_size(size);
        let key = self.rm_key("bx/n");
        self.select_nfss(&key, self.size_row(size).fsize);
        let g = self.e.nodes.glue_node(Glue::default(), 0);
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
        let Some(level) = self.doc.as_ref().map(|d| d.level) else { return };
        let g = neg(self.glue_of(SECS[level].after));
        let g = self.e.nodes.glue_node(g, 0);
        self.cur.push(g);
    }

    /// `\@sect`: `\refstepcounter`, whose `\stepcounter` resets the levels below, then the
    /// heading's group and number box.
    fn section_start(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let level = doc.level;
        let sec = &SECS[level];
        let mut number = String::new();
        if doc.numbered {
            doc.counters[level] += 1;
            doc.counters[level + 1..].fill(0);
            for (k, c) in doc.counters[..=level].iter().enumerate() {
                if k > 0 {
                    number.push('.');
                }
                number.push_str(&c.to_string());
            }
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.set_size(sec.size);
        let key = self.rm_key("bx/n");
        self.select_nfss(&key, self.size_row(sec.size).fsize);
        // `\setbox\@tempboxa\hbox{{\hskip\z@\relax\@svsec}}`.
        self.push_nest(Mode::Horizontal, true, 1000);
        let g = self.e.nodes.glue_node(Glue::default(), 0);
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
        if let Some(b) = b {
            self.cur.push(Node::new(Kind::Box, b as i32));
            self.aux = 1000;
        }
        self.set_int_named("interlinepenalty", 10000);
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
        // `\addcontentsline{toc}`: an unstarred heading's `\write` to the `.aux` file.
        if self.doc.as_ref().is_some_and(|d| d.toc) {
            self.contents_write();
        }
        let Some(after) = self.doc.as_ref().map(|d| d.after) else { return };
        let mut ops = Vec::new();
        if after.width > 0 {
            ops.extend([self.cur_par(), Op::Penalty(10000), Op::Vskip(after)]);
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

    /// `\@setfontsize`'s `\size@update`: `\baselineskip` from the size table.
    fn set_size(&mut self, size: usize) {
        let g = glue_words(self.size_row(size).baselineskip);
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, g, false);
        self.strut_update();
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
        let name = format!("{key}/{size}");
        let cs = self.e.cs.intern(&name);
        if let Meaning::Font(f) = self.e.eqtb.meaning(cs) {
            return self.select_font(f, false);
        }
        // A shape a loaded package declares, else the `.fd` files' (`data_nfss`).
        let packages = self.doc.as_ref().map_or(&[][..], |d| &d.packages[..]);
        let Some(t) = packages.iter().rev().filter_map(|p| crate::data::data_kernel::package_shapes(p)).find(|t| t.shape(key).is_some()).or_else(data_nfss::nfss) else { return };
        let Some(ext) = t.shape(key).and_then(|s| t.external(s, size)) else {
            return self.error(format!("Font shape `{key}' undefined"));
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
            Meaning::Font(f) => f,
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

    /// `OT1/\rmdefault/<series/shape>`: the key `\normalfont` and the headings start from.
    fn rm_key(&self, series_shape: &str) -> String {
        format!("OT1/{}/{series_shape}", self.family(0))
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
        Some(body.iter().filter_map(|t| match t.cs_id() {
            Some(c) if self.e.cs.name(c) == "@empty" => None,
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
        let relax = self.doc_id_named("relax");
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
    fn brace_symbol(&mut self, delim: &str, c: u8) {
        if self.mode == Mode::Math {
            let t = Token::cs(self.e.cs.intern(delim));
            return self.input.back(t);
        }
        self.text_symbol("OMS", c);
    }

    fn text_symbol(&mut self, enc: &str, c: u8) {
        self.begin_group(SIMPLE_GROUP);
        let (key, size) = self.font_key();
        let rest = key.split_once('/').map_or("cmr/m/n", |k| k.1);
        self.select_nfss(&format!("{enc}/{rest}"), size);
        let mut list = vec![Token::cs(self.e.cs.intern("char"))];
        list.extend(c.to_string().chars().map(|d| Token::char(Cat::Other, d)));
        list.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&list);
    }

    /// OT1's `\textunderscore`: `\leavevmode\kern.06em\vbox{\hrule\@width.3em}`.
    fn text_underscore(&mut self) {
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
    fn leave_vmode(&mut self) -> bool {
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

    /// `\,` outside math: `\leavevmode@ifvmode\kern+.16667em`.
    fn thin_space(&mut self) {
        if self.mode == Mode::Math {
            let list = [Token::cs(self.e.cs.intern("mskip")), Token::cs(self.e.cs.intern("thinmuskip"))];
            return self.back_list(&list);
        }
        if self.leave_vmode() {
            return;
        }
        let k = self.dimen_of(".16667em");
        self.hkern(k);
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
        list.push(Token::cs(self.e.cs.intern("hbox")));
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
            list.push(Token::cs(self.e.cs.intern("long")));
        }
        list.push(Token::cs(self.e.cs.intern("def")));
        list.push(cs);
        for k in 1..=n {
            list.push(Token::char(Cat::Param, '#'));
            list.push(Token::char(Cat::Other, (b'0' + k) as char));
        }
        self.back_list(&list);
    }

    /// `\providecommand`: `\newcommand` when the name is undefined, else the definition is
    /// read and dropped (`\@reserveda`).
    fn provide_command(&mut self) {
        let star = self.next_is('*');
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let Some(&cs) = name.first() else { return };
        let target = if self.meaning(cs) == Meaning::Undefined { cs } else { Token::cs(self.e.cs.intern("rt@reserveda")) };
        let mut list = Vec::with_capacity(4);
        list.push(self.doc_id_named("newcommand"));
        if star {
            list.push(Token::char(Cat::Other, '*'));
        }
        list.push(target);
        self.back_list(&list);
    }

    fn set_current_label(&mut self, text: &str) {
        let span = self.e.toks.store(&label_tokens(text));
        self.e.eqtb.set_toks(CURRENT_LABEL, span, false);
    }

    /// `\label{key}`: `\@bsphack`, the key's value (`\@currentlabel`) kept in memory, the
    /// `.aux` write's whatsit, `\@esphack`.
    fn label(&mut self) {
        let saved = (self.mode == Mode::Horizontal).then(|| (self.last_skip(), self.aux));
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let key: String = key.iter().map(|t| t.chr()).collect();
        let value = self.e.toks.get(self.e.eqtb.toks(CURRENT_LABEL)).to_vec();
        if !self.defined_once(&key, &value) {
            return;
        }
        let Some(doc) = self.doc.as_mut() else { return };
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
        doc.used.insert(key);
        v.extend([Token::cs(self.e.cs.intern("hbox")), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
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
        for key in keys.split(',') {
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
            self.build_hbox(|m| m.build_text(label));
        }
        // `]` stays a token: it forms a word with the document's characters after it.
        let close = Token::char(Cat::Other, ']');
        match opt {
            Some(note) => {
                self.build_text(b",");
                self.append_space();
                self.begin_group(SIMPLE_GROUP);
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
        let label = opt.map(|o| {
            let mut l = vec![Token::char(Cat::Other, '[')];
            l.extend(o);
            l.extend([Token::char(Cat::Other, ']'), Token::cs(self.e.cs.intern("hfill"))]);
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
        let list = List { label: Label::Bib, make: Make::Mklab, numbered: true, counter: R_ENUMI + 3, centercr: false, auto_item: false };
        if !self.list_open(list) {
            return;
        }
        self.set_rw(R_LABELWIDTH, w, false);
        let sep = self.rw(R_LABELSEP);
        self.set_rw(R_LEFTMARGIN, w + sep, false);
        self.set_rw(list.counter, 0, true);
        self.set_rw(R_CLUBPENALTY, 4000, false);
        // Read after `\@trivlist`'s steps, which go on the input above them.
        let mut t = Vec::new();
        self.tex("\\tolerance9999 \\emergencystretch3em\\hfuzz.5pt\\vfuzz\\hfuzz\\clubpenalty4000 \\widowpenalty4000 \\sfcode46=1000 ", &mut t);
        self.back_list(&t);
        self.trivlist(|m| m.list_tail());
    }

    /// `\sqrt` (`\@ifnextchar[\@sqrt\sqrtsign`): without `[`, `\sqrtsign`, the kernel's
    /// `\radical`. `\@sqrt[#1]` is `\root #1\of`, which needs `\rootbox` and `\dimen@`:
    /// not native yet.
    fn sqrt(&mut self) {
        if self.next_is('[') {
            return self.not_native("\\sqrt[n]");
        }
        let t = Token::cs(self.e.cs.intern("sqrtsign"));
        self.input.back(t);
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
        doc.used.insert(key);
        let mut v = label_tokens(&page.to_string());
        v.extend([Token::cs(self.e.cs.intern("hbox")), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
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
        let Some(doc) = self.doc.as_mut() else { return };
        let entries = match doc.contents.take() {
            Some(t) if !doc.defer && t.iter().any(|e| e.level < 3) => t,
            _ => return self.not_native("a \\tableofcontents the pre-scan cannot read whole"),
        };
        doc.defer = true;
        let d = self.glue_of("1.5em").width;
        let mut t = Vec::new();
        self.tex("\\section*{Contents}\\begingroup", &mut t);
        for e in entries.iter().filter(|e| e.level < 3) {
            let mut title = Vec::new();
            self.title_tokens(&e.title, &mut title);
            if e.level == 0 {
                self.tex(&format!("\\rt@tocsec\\begingroup\\parindent0pt\\rightskip1.55em\\parfillskip-1.55em\\rt@leavevmode\\rt@bfseries\\advance\\leftskip{d}sp\\hskip-\\leftskip\\hbox to{d}sp{{{}\\hfil}}", e.number), &mut t);
                t.extend(title);
                self.tex("\\penalty10000\\hfil\\penalty10000\\hbox to1.55em{\\hss0\\kern-1pt\\kern1pt}\\rt@tocpage\\par\\endgroup", &mut t);
            } else {
                let (indent, width) = if e.level == 1 { ("1.5em", "2.3em") } else { ("3.8em", "3.2em") };
                self.tex(&format!("\\vskip0pt plus.2pt{{\\leftskip{indent}\\rightskip2.55em\\parfillskip-\\rightskip\\parindent{indent}\\rt@afterindent\\interlinepenalty10000\\rt@leavevmode\\advance\\leftskip{width}\\hbox{{}}\\penalty10000\\hskip-\\leftskip{{\\hbox to{width}{{{}\\hfil}}", e.number), &mut t);
                t.extend(title);
                self.tex("}\\penalty10000\\leaders\\hbox{$\\mathsurround0pt\\mkern4.5mu\\hbox{.}\\mkern4.5mu$}\\hfill\\penalty10000\\hbox to1.55em{\\hfil0\\kern-1pt\\kern1pt}\\rt@tocpage\\par}\\rt@afterindentrestore", &mut t);
            }
        }
        self.tex("\\rt@nobreakfalse\\endgroup", &mut t);
        if let Some(doc) = self.doc.as_mut() {
            doc.contents = Some(entries);
        }
        self.back_list(&t);
    }

    /// `\l@section`'s `\addpenalty\@secpenalty \addvspace{1.0em \@plus\p@}`.
    fn toc_section_skip(&mut self) {
        let mut ops = Vec::new();
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
                self.refit_contents_line(b)?;
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
    /// line takes up the difference, as §679 would have computed it from the final line.
    /// A line whose neighbours are not `\baselineskip` glue, or whose new glue would fall
    /// below `\lineskiplimit` (so §679 would have chosen `\lineskip`), is not native.
    fn refit_contents_line(&mut self, b: u32) -> Result<(), String> {
        const NOT: &str = "not native yet: a contents line whose height changes";
        let roots = self.doc.as_ref().map(|d| d.deferred.clone()).unwrap_or_default();
        let (g, i, line) = roots.iter().find_map(|&r| self.line_holding(r, b)).ok_or(NOT)?;
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
        let list = self.e.nodes.boxes[g as usize].list;
        let nodes = self.e.nodes.get(list);
        // The glue before the line takes the height's change, the glue after it the depth's.
        let above = (rec.height - old.height, i.checked_sub(1));
        let below = (rec.depth - old.depth, nodes[i + 1..].iter().position(|n| n.kind == Kind::Box).map(|k| i + k));
        for (delta, at) in [above, below] {
            if delta == 0 {
                continue;
            }
            let n = at.map(|k| self.e.nodes.get(list)[k]).ok_or(NOT)?;
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

    /// The box in `root`'s tree whose list holds box `b`: its parent list's box, its index
    /// there, and the box.
    fn line_holding(&self, root: u32, b: u32) -> Option<(u32, usize, u32)> {
        let nodes = self.e.nodes.get(self.e.nodes.boxes[root as usize].list);
        for (i, n) in nodes.iter().enumerate().filter(|(_, n)| n.kind == Kind::Box) {
            let c = n.a as u32;
            if self.e.nodes.get(self.e.nodes.boxes[c as usize].list).iter().any(|m| m.kind == Kind::Box && m.a as u32 == b) {
                return Some((root, i, c));
            }
            if let Some(hit) = self.line_holding(c, b) {
                return Some(hit);
            }
        }
        None
    }

    /// Tokens for a fixed string of primitives and `rt@` natives: control words (letters and
    /// `@`, the spaces after them skipped), braces, `$`, letters, other characters.
    fn tex(&mut self, text: &str, out: &mut Vec<Token>) {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            i += 1;
            match c {
                b'\\' => {
                    let n = b[i..].iter().take_while(|c| c.is_ascii_alphabetic() || **c == b'@').count();
                    // A control symbol (`\ `) is its one character, spaces after it kept.
                    if n == 0 {
                        out.push(Token::cs(self.e.cs.intern(&text[i..i + 1])));
                        i += 1;
                        continue;
                    }
                    out.push(Token::cs(self.e.cs.intern(&text[i..i + n])));
                    i += n;
                    while b.get(i) == Some(&b' ') {
                        i += 1;
                    }
                }
                b'{' => out.push(Token::char(Cat::BeginGroup, '{')),
                b'}' => out.push(Token::char(Cat::EndGroup, '}')),
                b'$' => out.push(Token::char(Cat::MathShift, '$')),
                b' ' => out.push(Token::char(Cat::Space, ' ')),
                c if c.is_ascii_alphabetic() => out.push(Token::char(Cat::Letter, c as char)),
                c => out.push(Token::char(Cat::Other, c as char)),
            }
        }
    }

    /// A contents title from the source, lexed as the document's text is: inside braces, so
    /// it starts mid-line as in the `.toc` file's `\numberline {1}Title`, with `%` ending the
    /// last line.
    fn title_tokens(&mut self, title: &str, out: &mut Vec<Token>) {
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
        match name.as_str() {
            "document" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                self.document();
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
                let mut t = Vec::new();
                self.tex("\\small\\begin{center}{\\rt@bfseries\\abstractname\\vspace{-.5em}\\vspace{0pt}}\\end{center}\\rt@quotation ", &mut t);
                self.back_list(&t);
            }
            // article.cls: `\section*{\refname}`, then the list (`bib_open`).
            "thebibliography" => {
                let mut arg = Vec::new();
                self.read_arg(&mut arg);
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                let mut t = Vec::new();
                self.tex(&format!("\\section*{{\\refname}}\\setbox{TEMP_BOX}=\\hbox{{["), &mut t);
                t.extend(arg);
                self.tex("]}\\rt@bibopen ", &mut t);
                self.back_list(&t);
            }
            "figure" | "table" | "figure*" | "table*" => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if let Some(doc) = self.doc.as_mut() {
                    doc.endpe = false;
                }
                self.float_open(if name.starts_with("figure") { 1 } else { 2 });
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
                let mut t = Vec::new();
                self.tex("$$\\rt@eqstep ", &mut t);
                self.back_list(&t);
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
        self.begin_document_values();
        let n = self.e.cs.intern("@preamblecmds");
        self.e.eqtb.set_meaning(n, Meaning::Doc(NOTPRERR as u16), true);
        self.aux_write(0);
        self.ignore_spaces();
    }

    /// What `\document` computes from the preamble's values (the class table holds them as
    /// they are after `\document` with the class's own geometry): the geometry read back from
    /// its registers, `\vsize\textheight`, `\columnwidth\textwidth`, `\hsize\columnwidth`,
    /// `\linewidth\hsize`, `\normalsize` (its `\baselineskip`, display skips and `\@listi`)
    /// and `\global\@maxdepth\maxdepth`.
    fn begin_document_values(&mut self) {
        let geometry: Vec<i32> = (0..data_class::CLASS_DIMENS.len()).map(|i| self.e.eqtb.word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 })).collect();
        let max_depth = self.dimen_param(super::engine_page::MAX_DEPTH);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.body = true;
        doc.class.vals[..geometry.len()].copy_from_slice(&geometry);
        let i = data_class::CLASS_DIMENS.iter().position(|n| *n == "@maxdepth").expect("@maxdepth");
        doc.class.vals[i] = max_depth;
        let (width, height) = (doc.class.dimen("textwidth"), doc.class.dimen("textheight"));
        self.font_defaults();
        self.set_dimen_named("vsize", height);
        self.set_dimen_named("hsize", width);
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
        let mut list = Vec::new();
        if add {
            self.tex("\\advance", &mut list);
        }
        list.push(t);
        list.push(Token::char(Cat::Space, ' '));
        list.extend(value);
        self.tex("\\relax", &mut list);
        self.back_list(&list);
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
        let relax = Token::cs(self.e.cs.intern("relax"));
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
        match name.as_str() {
            "page" => doc.page = n,
            "footnote" => doc.footnote = n,
            "section" => doc.counters[0] = n,
            "subsection" => doc.counters[1] = n,
            _ => doc.counters[2] = n,
        }
    }

    /// `\pagestyle{plain}` or `{empty}`, in the preamble (in the body it is local to the
    /// group, which the native job does not keep).
    fn page_style(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let style: String = arg.iter().map(|t| t.chr()).collect();
        let Some(doc) = self.doc.as_mut() else { return };
        match style.as_str() {
            "plain" | "empty" if !doc.body => doc.empty_style = style == "empty",
            _ => self.not_native("\\pagestyle other than plain or empty, or in the body"),
        }
    }

    /// `\enddocument`: `\clearpage` (its `\newpage`), and the job's end.
    fn end_document(&mut self) {
        let list = [self.doc_id(NEW_PAGE), self.doc_id(FLOAT_FLUSH), self.doc_id(STOP)];
        self.back_list(&list);
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
        match name.as_str() {
            "document" => self.end_document(),
            "em" => self.end_group(SEMI_SIMPLE_GROUP),
            "quote" | "quotation" | "verse" | "itemize" | "enumerate" | "description" | "abstract" | "thebibliography" => self.end_list(),
            // `\endequation`: `\eqno \hbox{\@eqnnum}\dollardollar@end\@ignoretrue`, with
            // `\@eqnnum` `{\normalfont \normalcolor (\theequation)}`.
            "equation" => {
                let n = self.doc.as_ref().map_or(0, |d| d.equation);
                let mut t = Vec::new();
                self.tex(&format!("\\eqno\\hbox{{{{\\rt@normalfont({n})}}}}$$\\rt@endequation "), &mut t);
                self.back_list(&t);
            }
            // `\endverbatim`: `\if@newlist \leavevmode\fi \endtrivlist`; its last line ended
            // in vertical mode, so `\leavevmode` is not needed.
            "center" | "flushleft" | "flushright" | "verbatim" => self.end_trivlist(),
            "tabular" => self.tabular_end(),
            "figure" | "table" | "figure*" | "table*" => self.float_close(),
            // `\csname endname\endcsname \endgroup`.
            _ => {
                let list = [Token::cs(self.e.cs.intern(&format!("end{name}"))), Token::cs(self.e.cs.intern("endgroup"))];
                self.back_list(&list);
            }
        }
    }

    /// article's list environments: `\list{label}{setup}`, then `\item\relax` for the
    /// displayed-text ones.
    fn list_env(&mut self, name: &str) {
        let (em, _) = self.em_ex();
        let em = em as i32;
        let mut list = List { label: Label::Empty, make: Make::Mklab, numbered: false, counter: 0, centercr: false, auto_item: true };
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
            // `\labelwidth\z@ \itemindent-\leftmargin`.
            "description" => {
                self.set_rw(R_LABELWIDTH, 0, false);
                let l = self.rw(R_LEFTMARGIN);
                self.set_rw(R_ITEMINDENT, -l, false);
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
        let id = |m: &mut Self, name: &str| Token::cs(m.e.cs.intern(name));
        let (nbsp, kern, leave, hbox, par, penalty, ilp) = (id(self, "nobreakspace"), id(self, "kern"), self.doc_id(LEAVE), id(self, "hbox"), self.doc_id(PRIM_PAR), id(self, "penalty"), id(self, "interlinepenalty"));
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
        list.push(id(self, "end"));
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.extend("verbatim".chars().map(|c| Token::char(Cat::Letter, c)));
        list.push(Token::char(Cat::EndGroup, '}'));
        if let Some(doc) = self.doc.as_mut() {
            doc.verbatim = list;
        }
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: false, auto_item: true });
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
        self.set_glue_named("leftskip", Glue { width: left, ..Glue::default() });
        self.set_glue_named("rightskip", Glue::default());
        self.set_dimen_named("parindent", 0);
        self.set_glue_named("parfillskip", Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        self.set_glue_named("parskip", Glue::default());
        self.run_ops(vec![Op::Par, Op::Call(|m| {
            m.set_int_named("language", NOHYPHENATION);
            let (_, size) = m.font_key();
            m.select_nfss("OT1/cmtt/m/n", size);
            if m.everypar() == ITEM {
                m.set_everypar(VERB_ITEM);
            }
            let list = m.doc.as_mut().map(|d| std::mem::take(&mut d.verbatim)).unwrap_or_default();
            m.back_list(&list);
        })]);
    }

    /// A text accent of OT1 (`\DeclareTextAccent`, code `code`) on its argument:
    /// `\add@accent` (latex.ltx), `\hmode@bgroup`, the base's space factor from
    /// `\hbox{#2}`, `\accent#1 #2\egroup`, `\spacefactor` restored. The base is one letter
    /// or `\i`/`\j`; ot1enc.def's composites put `\`, `\'`, `\^`, `\"` and `\=` over
    /// `\i` for `i`. `\.i` (a composite to `\i` itself), `\r A` (`\ooalign`) and math
    /// mode (`\ddot` and kin) are not native yet.
    fn text_accent(&mut self, code: u32) {
        if self.mode == Mode::Math {
            return self.not_native("a text accent in math");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let base = match arg[..] {
            [t] if t.cat() == Some(Cat::Letter) => match t.chr() {
                'i' if code == 95 => return self.not_native("\\.i"),
                'i' if matches!(code, 18 | 19 | 94 | 127 | 22) => 16,
                'A' if code == 23 => return self.not_native("\\r A"),
                c => c as u32,
            },
            [t] => match t.cs_id().map(|id| self.e.cs.name(id)) {
                Some("i") if code != 95 => 16,
                Some("j") => 17,
                _ => return self.not_native("an accent on more than a letter"),
            },
            _ => return self.not_native("an accent on more than a letter"),
        };
        // `\hbox{#2}` starts at space factor 1000 (§1034).
        let sf = match self.e.eqtb.code(Code::Sf, base) {
            0 => 1000,
            k => k,
        };
        let mut t = Vec::new();
        self.tex(&format!("\\rt@leavevmode{{\\accent{code} \\char{base} }}"), &mut t);
        if sf != 1000 {
            self.tex(&format!("\\spacefactor{sf} "), &mut t);
        }
        self.back_list(&t);
    }

    /// `\begin{center}`, `\begin{flushleft}` and `\begin{flushright}` (`kind` 0, 1, 2):
    /// `\trivlist`, then `\centering`, `\raggedright` or `\raggedleft`, then `\item\relax`.
    fn center(&mut self, kind: usize) {
        let parskip = self.glue_param(super::engine_pack::PAR_SKIP);
        self.set_rg(R_PARSEP, parskip);
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(List { label: Label::Empty, make: Make::Plain, numbered: false, counter: 0, centercr: true, auto_item: true });
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
                self.set_glue_named("rightskip", fil);
                self.set_glue_named("leftskip", fil);
                self.set_glue_named("parfillskip", Glue::default());
            }
            // `\raggedright`: `\@rightskip\@flushglue \rightskip\@rightskip
            // \finalhyphendemerits\z@ \leftskip\z@skip \parindent\z@`.
            1 => {
                self.set_rg(R_ATRIGHTSKIP, fil);
                self.set_glue_named("rightskip", fil);
                self.set_glue_named("leftskip", Glue::default());
            }
            // `\raggedleft`: `\rightskip\z@skip \leftskip\@flushglue
            // \finalhyphendemerits\z@ \parindent\z@ \parfillskip\z@skip`.
            _ => {
                self.set_glue_named("rightskip", Glue::default());
                self.set_glue_named("leftskip", fil);
                self.set_glue_named("parfillskip", Glue::default());
            }
        }
        self.set_int_named("finalhyphendemerits", 0);
        self.set_dimen_named("parindent", 0);
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
        if let Some(doc) = self.doc.as_mut() {
            doc.lists.push(list);
        }
        true
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
        self.set_glue_named("leftskip", Glue::default());
        let right = self.rg(R_ATRIGHTSKIP);
        self.set_glue_named("rightskip", right);
        self.set_glue_named("parfillskip", Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
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
        self.set_glue_named("parskip", parsep);
        let i = self.rw(R_LISTPARINDENT);
        self.set_dimen_named("parindent", i);
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
        self.set_int_named("clubpenalty", v);
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
                doc.lists.pop();
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
                let n = |m: &Self, j: usize| m.rw(R_ENUMI + j);
                let mut text = if bib { v.to_string() } else { n(self, 0).to_string() };
                if k >= 1 && !bib {
                    if k >= 2 {
                        text.push('(');
                    }
                    text.extend(alph(n(self, 1), b'a'));
                    if k >= 2 {
                        text.push(')');
                        text.push_str(&roman(n(self, 2)));
                    }
                    if k >= 3 {
                        text.extend(alph(n(self, 3), b'A'));
                    }
                }
                self.set_current_label(&text);
            }
        }
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        let mut t = vec![Token::cs(self.e.cs.intern("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Other, '='), Token::cs(self.e.cs.intern("hbox")), open]);
        // `\makelabel`: `\hss\llap{#1}` (itemize, enumerate) or `\@mklab` (`\hfil #1`).
        match list.make {
            Make::Llap => {
                t.extend([Token::cs(self.e.cs.intern("hss")), Token::cs(self.e.cs.intern("hbox"))]);
                self.word_tokens("to", &mut t);
                t.push(Token::char(Cat::Other, '0'));
                self.word_tokens("pt", &mut t);
                t.extend([open, Token::cs(self.e.cs.intern("hss"))]);
            }
            Make::Mklab => t.push(Token::cs(self.e.cs.intern("hfil"))),
            Make::Plain => {}
            Make::Description => {
                t.push(Token::cs(self.e.cs.intern("hskip")));
                Self::sp_tokens(self.rw(R_LABELSEP), &mut t);
                t.push(self.doc_id(DESC_FONT));
            }
        }
        match given {
            Some(l) => t.extend(l),
            None => self.item_label(list, &mut t),
        }
        if list.make == Make::Llap {
            t.push(close);
        }
        t.extend([close, self.doc_id(ITEM_LABELS)]);
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
            Label::Bib => {
                t.push(o('['));
                t.extend(n.to_string().chars().map(o));
                t.push(o(']'));
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
            self.set_int_named("clubpenalty", 10000);
        } else {
            let v = self.rw(R_CLUBPENALTY);
            self.set_int_named("clubpenalty", v);
            self.set_everypar(0);
        }
    }

    /// `\\`: in verse, center, flushleft and flushright `\@centercr` (`\unskip\par`,
    /// `\@ifstar{\nobreak…}`, `\addvspace{-\parskip}`, `\@ifnextchar[\@icentercr
    /// \ignorespaces`); elsewhere not native yet.
    fn newline(&mut self) {
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
                        let relax = Token::cs(m.e.cs.intern("relax"));
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

    /// `\refstepcounter{equation}`: `\c@equation` up one, `\@currentlabel` its `\arabic`.
    fn equation_step(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.equation += 1;
        let n = doc.equation.to_string();
        self.set_current_label(&n);
    }

    /// After `\endequation`'s display: `\@ignoretrue`, `\end`'s `\endgroup` (`\@endpe`
    /// is false), then `\if@ignore` `\ignorespaces`.
    fn end_equation(&mut self) {
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
    fn bracket_arg(&mut self) -> Vec<Token> {
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
        for (k, name) in ["abovedisplayskip", "abovedisplayshortskip", "belowdisplayshortskip", "belowdisplayskip"].into_iter().enumerate() {
            if mask & 1 << k != 0 {
                self.set_glue_named(name, glue_words(skips[k]));
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
    /// {plain}` (for `\pagestyle{empty}`, `special_plain`); `\setcounter{footnote}{0}`.
    fn make_title(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let [title, author, mut date] = std::mem::take(&mut doc.stored);
        doc.footnote = 0;
        // `\thispagestyle{plain}`: `\@specialpagetrue`, for the page this title ends up on.
        doc.special_plain = true;
        if !doc.dated {
            date = vec![self.doc_id_named("today")];
        }
        let cs = |m: &mut Self, n: &str| Token::cs(m.e.cs.intern(n));
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![cs(self, "par"), cs(self, "begingroup"), self.doc_id(NEW_PAGE), cs(self, "hbox"), open, close];
        let skip = |m: &mut Self, t: &mut Vec<Token>, amount: &str| {
            t.push(cs(m, "vskip"));
            t.extend(amount.chars().map(|c| if c.is_ascii_alphabetic() { Token::char(Cat::Letter, c) } else { Token::char(Cat::Other, c) }));
            t.push(Token::char(Cat::Space, ' '));
        };
        let env = |m: &mut Self, t: &mut Vec<Token>, cmd: &str| {
            t.extend([cs(m, cmd), open]);
            m.word_tokens("center", t);
            t.push(close);
        };
        skip(self, &mut t, "2em");
        env(self, &mut t, "begin");
        t.extend([open, cs(self, "LARGE")]);
        t.extend(title);
        t.extend([cs(self, "par"), close]);
        skip(self, &mut t, "1.5em");
        t.extend([open, cs(self, "large"), cs(self, "lineskip"), Token::char(Cat::Other, '.'), Token::char(Cat::Other, '5')]);
        self.word_tokens("em", &mut t);
        t.extend([Token::char(Cat::Space, ' '), cs(self, "begin"), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, Token::char(Cat::Other, '['), Token::char(Cat::Letter, 't'), Token::char(Cat::Other, ']'), open, Token::char(Cat::Letter, 'c'), close]);
        t.extend(author);
        t.extend([cs(self, "end"), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, cs(self, "par"), close]);
        skip(self, &mut t, "1em");
        t.extend([open, cs(self, "large")]);
        t.extend(date);
        t.push(close);
        env(self, &mut t, "end");
        t.push(cs(self, "par"));
        skip(self, &mut t, "1.5em");
        t.push(cs(self, "endgroup"));
        self.back_list(&t);
    }

    fn doc_id_named(&mut self, name: &str) -> Token {
        Token::cs(self.e.cs.intern(name))
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
        if display && self.mode == Mode::Vertical {
            return self.not_native("\\[ in vertical mode");
        }
        let d = Token::char(Cat::MathShift, '$');
        let list = [d, d];
        self.back_list(&list[..1 + display as usize]);
    }

    /// `\)` and `\]` (`$$\ignorespaces`).
    fn math_close(&mut self, display: bool) {
        if self.mode != Mode::Math || self.inner == display {
            return self.error("Bad math environment delimiter");
        }
        let d = Token::char(Cat::MathShift, '$');
        if display {
            let list = [d, d, Token::cs(self.e.cs.intern("ignorespaces"))];
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
            let t = Token::cs(self.e.cs.intern("ignorespaces"));
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
    fn install(&mut self, k: &'static crate::data::data_kernel::KernelTable<'static>, pkg: Option<&str>, fams: &mut Vec<&'static str>, alphabets: &mut Vec<(&'static str, bool, Alphabet)>) {
        use crate::data::data_kernel::{Def, MacroToken, MathFont};
        for (name, d) in k.chars() {
            let cs = self.e.cs.intern(name);
            let m = match d {
                Def::Char(v) => Meaning::CharDef(v),
                Def::MathChar(v) => Meaning::MathCharDef(v),
            };
            self.e.eqtb.set_meaning(cs, m, true);
        }
        let mut t = Vec::new();
        for op in k.ops() {
            t.clear();
            t.push(Token::END_MATCH);
            let word = op.word.replace("\\,", "\\mskip\\thinmuskip ");
            self.tex(&format!("\\mathop{{\\fam0 {word}}}{}", if op.nolimits { "\\nolimits" } else { "" }), &mut t);
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
        for (name, v) in k.delims() {
            t.clear();
            t.push(Token::END_MATCH);
            self.tex(&format!("\\delimiter{v} "), &mut t);
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(name);
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::engine_prim::Macro { text, flags: 0 }), true);
        }
        for f in k.math_fonts() {
            let (name, same, a) = match f {
                MathFont::Symbol { normal, .. } => {
                    fams.push(normal);
                    continue;
                }
                MathFont::SymbolAlphabet { name, fam, same } => (name, same, Alphabet::Fam(fam)),
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
        if let Some(open) = self.next_unblank() {
            if open == Token::char(Cat::Other, '[') {
                while let Some(x) = self.get_token() {
                    if x == Token::char(Cat::Other, ']') {
                        break;
                    }
                    if x.cat() != Some(Cat::Space) {
                        return self.not_native("package options");
                    }
                }
            } else {
                self.back_list(&[open]);
            }
        }
        self.read_arg(&mut t);
        let names: String = t.iter().filter(|x| x.cs_id().is_none()).map(|x| x.chr()).collect();
        for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            self.load_package(name);
        }
    }

    /// Package `name` from its native table, after the packages it loads; a second load does
    /// nothing. Its families must follow the ones in use when it was evaluated (`fam0`): the
    /// table's codes and macros name them as digits.
    fn load_package(&mut self, name: &str) {
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
        if doc.fams.len() != fam0 as usize {
            return self.not_native("a math package after other math fonts");
        }
        let pkg = crate::data::data_kernel::packages().find(|p| p.0 == name).map_or("", |p| p.0);
        doc.packages.push(pkg);
        let (mut fams, mut alphabets) = (std::mem::take(&mut doc.fams), std::mem::take(&mut doc.alphabets));
        self.install(k, Some(pkg), &mut fams, &mut alphabets);
        // amsfonts' `\symAMSb`, the family of its wide accents.
        let msb = fams.iter().position(|&f| f == "U/msb/m/n").unwrap_or(0) as u32;
        if let Some(doc) = self.doc.as_mut() {
            (doc.fams, doc.alphabets) = (fams, alphabets);
            doc.wide_fam = msb;
            doc.math_size = "";
        }
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
        let mut t = Vec::new();
        self.tex("\\setbox0\\hbox{\\everymath{}\\mathsurround0pt$\\textstyle{", &mut t);
        t.extend_from_slice(&arg);
        self.tex("}$}\\rt@wideaccent ", &mut t);
        self.back_list(&t);
    }

    /// `\ifdim\wd\z@>\tw@ em` (the current font's quad), then `\mathaccent` of the chosen
    /// accent on the argument, braced.
    fn wide_accent_end(&mut self) {
        let Some((arg, codes)) = self.doc.as_mut().and_then(|d| d.wide.pop()) else { return };
        let wd = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let quad = self.e.fonts.params(self.cur_font()).get(5).copied().unwrap_or(0);
        let code = if wd > 2 * quad { codes >> 16 } else { codes & 0xffff };
        let mut t = Vec::new();
        self.tex(&format!("\\mathaccent{code}{{"), &mut t);
        t.extend_from_slice(&arg);
        self.tex("}", &mut t);
        self.back_list(&t);
    }

    /// `\check@mathfonts`: when the size differs from the one the families were set for,
    /// `\glb@settings` sets them (globally): the symbol fonts operators, letters, symbols
    /// and largesymbols as families 0 to 3, at the size and its script sizes.
    fn check_math_fonts(&mut self) {
        let (_, size) = self.font_key();
        if self.doc.as_ref().is_some_and(|d| d.math_size == size) {
            return;
        }
        let fams = self.doc.as_ref().map(|d| d.fams.clone()).unwrap_or_default();
        for (fam, key) in fams.into_iter().enumerate() {
            self.set_fam(fam, key, size);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.math_size = size;
        }
    }

    /// `\getanddefine@fonts` with `\globaldefs` 1: family `fam`'s text, script and
    /// scriptscript fonts, shape `key` at `size`'s math sizes.
    fn set_fam(&mut self, fam: usize, key: &'static str, size: &'static str) {
        let sizes = self.math_sizes(size);
        let saved = self.cur_font();
        for (k, sz) in [sizes.0, sizes.1, sizes.2].into_iter().enumerate() {
            self.select_nfss(key, sz);
            let f = self.cur_font();
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
                doc.fams.push(key);
                doc.alphabets[k].2 = Alphabet::Shape(key, Some(fam as u8));
                let size = doc.math_size;
                self.set_fam(fam, key, size);
                (fam as u8, true)
            }
        };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut t = Vec::new();
        self.tex(&format!("{}{{\\fam{fam}\\relax ", if first { "{" } else { "" }), &mut t);
        t.extend_from_slice(&arg);
        self.tex(if first { "}}" } else { "}" }, &mut t);
        self.back_list(&t);
    }

    /// `'` in math (mathcode `"8000`): `^{\prime…}`, one `\prime` per `'`, then a following
    /// superscript's argument inside the same group (`\prim@s`, `\pr@@@t`).
    fn math_prime(&mut self) {
        let prime = Token::cs(self.e.cs.intern("prime"));
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

    fn set_glue_named(&mut self, name: &str, g: Glue) {
        if let Some(r) = super::engine_names::param(name) {
            self.e.eqtb.set_glue(r, g, false);
        }
    }

    fn set_dimen_named(&mut self, name: &str, v: i32) {
        if let Some(r) = super::engine_names::param(name) {
            self.e.eqtb.set_word(r, v, false);
        }
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
        out.extend([Token::cs(self.e.cs.intern("hbox")), open, open, Token::cs(self.e.cs.intern("mathsurround")), Token::char(Cat::Other, '0')]);
        self.word_tokens("pt", out);
        out.extend([math, Token::char(Cat::Superscript, '^'), open, Token::cs(self.e.cs.intern("mbox")), open, self.doc_id(FN_MARK_FONT)]);
        out.extend(n.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        out.extend([close, close, math, close, close]);
    }

    /// `\footnote{text}`: `\stepcounter{footnote}`, `\@footnotemark` (`\leavevmode`, the
    /// space factor kept, `\nobreak`, `\@makefnmark`), then `\@footnotetext{text}`.
    fn footnote(&mut self) {
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
        self.footnote_mark_tokens(n, &mut list);
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

    /// `\ifhmode\spacefactor\@x@sf\fi`, then `\@footnotetext`: `\insert\footins{…}`.
    fn footnote_mark_end(&mut self) {
        let sf = self.doc.as_mut().and_then(|d| d.footnote_sf.pop()).unwrap_or(-1);
        if self.mode == Mode::Horizontal && sf >= 0 {
            self.aux = sf;
        }
        let mut list = vec![Token::cs(self.e.cs.intern("insert"))];
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
        // the insertion's group.
        self.set_current_label(&n.to_string());
        // `\reset@font\footnotesize`.
        let key = self.rm_key("m/n");
        self.apply_size(2, &key);
        // `\interlinepenalty\interfootnotelinepenalty` (100), `\splittopskip\footnotesep`,
        // `\splitmaxdepth\dp\strutbox`, `\floatingpenalty\@MM`, `\hsize\columnwidth`.
        self.set_int_named("interlinepenalty", 100);
        let sep = self.rw(R_FOOTNOTESEP);
        self.set_glue_named("splittopskip", Glue { width: sep, ..Glue::default() });
        let strut_depth = self.strut_depth();
        self.set_dimen_named("splitmaxdepth", strut_depth);
        self.set_int_named("floatingpenalty", 20000);
        self.set_dimen_named("hsize", width);
        // `\@arrayparboxrestore`: `\let\par\@@par`, `\everypar{}`, `\linewidth\hsize`,
        // `\@totalleftmargin\z@`.
        if let Some((id, m)) = self.doc.as_ref().map(|d| (d.par_id, d.par_prim)) {
            self.e.eqtb.set_meaning(id, m, false);
        }
        self.set_everypar(0);
        self.set_rw(R_LINEWIDTH, width, false);
        self.set_rw(R_TOTALLEFTMARGIN, 0, false);
        self.set_dimen_named("parindent", 0);
        for name in ["parskip", "leftskip", "rightskip"] {
            self.set_glue_named(name, Glue::default());
        }
        self.set_rg(R_ATRIGHTSKIP, Glue::default());
        self.set_glue_named("parfillskip", Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        let one = self.dimen_of("1pt");
        self.set_glue_named("lineskip", Glue { width: one, ..Glue::default() });
        self.set_dimen_named("lineskiplimit", 0);
        // `\sloppy`.
        self.set_int_named("tolerance", 9999);
        let e = self.dimen_of("3em");
        self.set_dimen_named("emergencystretch", e);
        let half = self.dimen_of(".5pt");
        self.set_dimen_named("hfuzz", half);
        self.set_dimen_named("vfuzz", half);
        // `\@makefntext`.
        let em = self.dimen_of("1em");
        self.set_dimen_named("parindent", em);
        let mut list = vec![Token::cs(self.e.cs.intern("noindent")), Token::cs(self.e.cs.intern("hbox"))];
        self.word_tokens("to", &mut list);
        Self::sp_tokens(self.dimen_of("1.8em"), &mut list);
        list.extend([Token::char(Cat::BeginGroup, '{'), Token::cs(self.e.cs.intern("hss"))]);
        self.footnote_mark_tokens(n, &mut list);
        list.push(Token::char(Cat::EndGroup, '}'));
        // `\rule\z@\footnotesep`: `\hbox{\vrule\@width\z@\@height\footnotesep\@depth-\z@}`.
        list.extend([Token::cs(self.e.cs.intern("hbox")), Token::char(Cat::BeginGroup, '{'), Token::cs(self.e.cs.intern("vrule"))]);
        for (k, v) in [("width", 0), ("height", sep), ("depth", 0)] {
            self.word_tokens(k, &mut list);
            Self::sp_tokens(v, &mut list);
        }
        list.extend([Token::char(Cat::EndGroup, '}'), Token::cs(self.e.cs.intern("ignorespaces"))]);
        list.extend_from_slice(&text);
        list.push(self.doc_id(FN_END));
        self.back_list(&list);
    }

    /// `\size@update`'s `\setbox\strutbox\hbox{\vrule\@height.7\baselineskip
    /// \@depth.3\baselineskip\@width\z@}`, kept as its height and depth in hidden dimension
    /// registers (local, as the box is). `.7` and `.3` are §452's fractions 45875 and 19661
    /// of the unit, applied as §455's `xn_over_d`.
    fn strut_update(&mut self) {
        let b = self.glue_param(BASELINE_SKIP).width as i64;
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
        self.cur.push(Node::new(Kind::Box, inner as i32));
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
    use crate::data::{data_blob::Blob, data_phf::Phf};
    static NAMES: &[u8] = include_bytes!("../../assets/tables/names.rtdb");
    static TABLE: std::sync::OnceLock<Option<(Phf<'static>, crate::data::data_blob::U32s<'static>)>> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let b = Blob::new(NAMES)?;
        let (phf, bits) = (Phf::from_blob(&b, "nmf")?, b.u32s("nm.b")?);
        (bits.len() == phf.len()).then_some((phf, bits))
    });
    t.as_ref().and_then(|(phf, bits)| phf.get(name.as_bytes()).map(|i| bits.get(i as usize))).unwrap_or(0)
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
        for n in k.chars().map(|c| c.0).chain(k.ops().map(|o| o.name)).chain(k.delims().map(|d| d.0)).chain(k.macro_names()).chain(alphabets) {
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
    for (p, n, _) in PACKAGE_NATIVES {
        *m.entry(n).or_default() |= package_bit(p);
    }
    let mut w = crate::data::data_blob::BlobWriter::default();
    let names: Vec<&[u8]> = m.keys().map(|n| n.as_bytes()).collect();
    crate::data::data_phf::write(&mut w, "nmf", &names).expect("a perfect hash");
    let probe = w.finish();
    // The hash numbers the names its own way: the bits follow its IDs.
    let b = crate::data::data_blob::Blob::new(&probe).expect("a blob");
    let phf = crate::data::data_phf::Phf::from_blob(&b, "nmf").expect("the hash");
    let mut bits = vec![0u32; m.len()];
    for (n, v) in &m {
        bits[phf.get(n.as_bytes()).expect("a name") as usize] = *v;
    }
    let mut w = crate::data::data_blob::BlobWriter::default();
    crate::data::data_phf::write(&mut w, "nmf", &names).expect("a perfect hash");
    w.u32s("nm.b", &bits);
    w.finish()
}

/// [`table_names`]' bit for the kernel.
const KERNEL_BIT: u32 = 1;

/// [`table_names`]' bit for a package with a native table: 2 for the first.
fn package_bit(name: &str) -> u32 {
    crate::data::data_kernel::packages().position(|p| p.0 == name).map_or(0, |k| 2 << k)
}

/// Package `name` and the packages it loads, into `loaded` (each once, after its own loads);
/// `false` if one of them has no native table.
fn require(name: &str, loaded: &mut Vec<&'static str>) -> bool {
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
    rows.flat_map(|r| [r.fsize, r.sf, r.ssf]).find(|&s| s == size).unwrap_or("10")
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
    fn names_table_is_generated_from_the_tables() {
        let committed: &[u8] = include_bytes!("../../assets/tables/names.rtdb");
        assert!(committed == names_table().as_slice(), "assets/tables/names.rtdb is stale: run rushtex-gen names");
    }

    #[test]
    fn footins_is_the_class_tables() {
        let t = data_class::article().unwrap();
        let i = data_class::CLASS_REGS.iter().position(|n| *n == "count footins").unwrap();
        assert_eq!(t.reg(i).1, super::super::engine_eqtb::INT_PARAMS + FOOTINS);
        for (i, name) in [(R_LEFTMARGINI, "leftmargini"), (R_LABELSEP, "labelsep"), (R_LABELWIDTH, "labelwidth"), (R_LEFTMARGIN, "leftmargin"), (R_RIGHTMARGIN, "rightmargin"), (R_LISTPARINDENT, "listparindent"), (R_ITEMINDENT, "itemindent"), (R_TOTALLEFTMARGIN, "@totalleftmargin"), (R_LINEWIDTH, "linewidth"), (R_BEGINPARPENALTY, "@beginparpenalty"), (R_ENDPARPENALTY, "@endparpenalty"), (R_ITEMPENALTY, "@itempenalty"), (R_CLUBPENALTY, "@clubpenalty"), (R_LISTDEPTH, "@listdepth"), (R_ITEMDEPTH, "@itemdepth"), (R_ENUMDEPTH, "@enumdepth"), (R_ENUMI, "c@enumi"), (R_PARTOPSEP, "partopsep"), (R_TOPSEP, "topsep"), (R_PARSEP, "parsep"), (R_ITEMSEP, "itemsep"), (R_ATTOPSEP, "@topsep"), (R_TOPSEPADD, "@topsepadd"), (R_OUTERPARSKIP, "@outerparskip"), (R_TABCOLSEP, "tabcolsep")] {
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
        assert_eq!(class_options("{article}"), Ok((0, 0)));
        assert_eq!(class_options("[12pt, a4paper,twoside]{article}"), Ok((2, option_key(3, false, true, false))));
        assert_eq!(class_options("[11pt,landscape,draft,final]{article}"), Err("final"));
        assert_eq!(class_options("[twocolumn]{article}"), Err("twocolumn"));
        assert_eq!(class_options("{report}"), Err("documentclass"));
    }

    #[test]
    fn step_indexes_name_their_natives() {
        use super::*;
        for (i, name) in [(OUTPUT, "rt@output"), (OUTPUT_FOOT, "rt@outputfoot"), (SEC_BOX, "rt@secbox"), (SEC_TITLE, "rt@sectitle"), (SEC_END, "rt@secend"), (TEXT_END, "rt@textend"), (TEXT_END_N, "rt@textendn"), (MAYBE_IC, "rt@maybeic"), (EVERY_MATH, "rt@everymath"), (EVERY_DISPLAY, "rt@everydisplay"), (FN_MARK_FONT, "rt@fnmarkfont"), (FN_MARK_END, "rt@fnmarkend"), (FN_SETUP, "rt@fnsetup"), (FN_END, "rt@fnend"), (STEP, "rt@step"), (PARA_HOOK, "rt@parahook"), (LIST_PAR, "rt@listpar"), (ENDPE_PAR, "rt@endpepar"), (PROPAGATE, "rt@propagate"), (ITEM_LABELS, "rt@itemlabels"), (LABEL_MARK, "rt@labelmark"), (NEW_PAGE, "rt@newpage"), (TABULAR, "rt@tabular"), (NOTPRERR, "@notprerr"), (STOP, "rt@stop"), (RUN_IN_END, "rt@runinend"), (DESC_FONT, "rt@descfont"), (VERBATIM, "rt@verbatim"), (LEAVE, "rt@leavevmode"), (PRIM_PAR, "rt@@par"), (END, "end"), (TAB_U, "rt@tabu"), (TAB_V, "rt@tabv"), (TAB_END, "rt@tabend"), (HLINE, "hline"), (TAB_SKIP, "rt@tabskip"), (MULTICOLUMN, "multicolumn"), (MC_BODY, "rt@mcbody"), (MC_TAIL, "rt@mctail"), (CLINE_CMD, "cline"), (CLINE, "rt@cline"), (NEWLINE, "\\"), (CENTER_CR, "rt@centercr"), (TAB_PAR, "rt@tabpar"), (CAPTION_MID, "rt@captionmid"), (CAPTION_END, "rt@captionend"), (FLOAT_FLUSH, "rt@floatflush")] {
            assert_eq!(DOC[i].0, name);
        }
    }

    #[test]
    fn accepts_plain_article_text_only() {
        let doc = "\\documentclass{article}\n\\begin{document}\nHello world.\n\\end{document}\n";
        assert_eq!(accepts(doc), Some("\\documentclass{article}\n\\begin{document}\nHello world.\n"));
        assert_eq!(accepts("\\documentclass{book}\\begin{document}x\\end{document}"), None);
        assert_eq!(accepts("\\documentclass{article}\\usepackage{x}\\begin{document}x\\end{document}"), None);
        assert_eq!(accepts("\\documentclass{article}\\begin{document}\\section{x}\\end{document}"), Some("\\documentclass{article}\\begin{document}\\section{x}"));
        assert_eq!(accepts("\\documentclass{article}\\begin{document}\\begin{tabular}x\\end{document}"), None);
        // `tabular`: a preamble `mkpream` takes, and `&` and `\\hline` only inside one.
        let tab = |body: &str| accepts(&format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}")).is_some();
        assert!(tab("\\begin{tabular}[t]{|l|*{2}{c}|}\\hline a&b&c\\\\\n\n\\hline\\end{tabular}"));
        assert!(!tab("\\begin{tabular}{p{1cm}}a\\end{tabular}"));
        assert!(!tab("\\begin{tabular}{@{:}c}a\\end{tabular}"));
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
