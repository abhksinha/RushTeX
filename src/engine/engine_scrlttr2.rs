//! KOMA-Script's letter class scrlttr2 on the GDL path (`2026-11-01-w9-scrlttr2.md`): the
//! class table (`rushtex-gen class --class scrlttr2`, DIN.lco loaded at the end of the
//! class), the letter variables and fonts (`\setkomavar`, `\setkomafont`, `\usekomavar`,
//! `\usekomafont`), the `letter` environment, `\opening` with its first-page fields,
//! `\closing`, `\ps`, `\encl` and `\cc`.
//!
//! The first page is what scrlttr2.cls builds in `\opening`: each field a `\vb@t@z` box of
//! height zero placed by `\move@topt` (back to the paper's top edge) and a `\vskip` of the
//! field's pseudo-length, then `\move@frompt` back. Its macros are written here as tokens of
//! primitives and native commands with DIN.lco's pseudo-lengths (`\setplength`) in sp, the
//! document's variables spliced in unchanged, so TeX's own arithmetic builds the boxes.
//!
//! Native: the a4 page (DIN.lco's `\LetterOptionNeedsPapersize`), `fromalign=left`, `center`
//! and `right` with the generated `firsthead`, a user `firsthead`, `firstfoot`, the fold
//! marks, the address field with the underlined back address, the date line (place and
//! date), the subject before or after the opening (untitled), one page per letter. Each of
//! the rest declines: `location` and the reference fields (`yourref`, `myref`, `customer`,
//! `invoice`, `yourmail`), `title`, `specialmail`, `enlargefirstpage`, a letter that runs to
//! a second page (scrlttr2's `\ps@plain` with `nexthead` and `nextfoot`), `\LoadLetterOption`.

use super::super::engine_catcode::Cat;
use super::super::engine_run::{Machine, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{KOMA_PAPER_SIZES, PRIM_PAR};

/// The class options scrlttr2 takes natively, read by `class_options` and the class hook:
/// the size, `parskip=half` (the table's bit 32), and the layout switches of the first page.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) struct Options {
    pub(super) size: Option<usize>,
    pub(super) parskip_half: bool,
    /// `fromalign`: `\@alignfrom` 0 (left, the default), 1 (center), 2 (right), 10000 (no).
    align_from: u16,
    /// `fromrule`: `\@rulefrom` 0 (false), 1 (aftername), 2 (below).
    rule_from: u8,
    phone: bool,
    email: bool,
    /// `foldmarks=false` (`\@foldfalse`).
    no_fold: bool,
    /// `subject=afteropening` (`\@subjaftertrue`).
    subj_after: bool,
    /// `enlargefirstpage=true` (`\@enlargefptrue`): not native.
    enlarge_fp: bool,
}

/// `true`, `on`, `yes` or `false`, `off`, `no` (`\KOMA@set@ifkey`); a bare key is true.
fn bool_value(v: Option<&str>) -> Option<bool> {
    match v {
        None | Some("true" | "on" | "yes") => Some(true),
        Some("false" | "off" | "no") => Some(false),
        _ => None,
    }
}

/// scrlttr2's class options (`[options]` of `\documentclass`), each declared by
/// scrlttr2.cls with `\KOMA@key`: the ones the native job follows, or the first other one.
pub(super) fn options(opts: &str) -> Result<Options, &str> {
    let mut o = Options::default();
    for opt in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        let (key, value) = match opt.split_once('=') {
            Some((k, v)) => (k.trim(), Some(v.trim())),
            None => (opt, None),
        };
        let size = |s: &str| super::data_class::CLASS_SIZES.iter().position(|c| *c == s);
        match (key, value) {
            ("fontsize", Some(v)) => o.size = Some(size(v).ok_or(opt)?),
            (k, None) if size(k).is_some() => o.size = size(k),
            // DIN.lco needs a4 (`\LetterOptionNeedsPapersize{DIN}{a4}`), the table's paper.
            ("paper", Some("a4" | "A4")) | ("a4paper", None) => {}
            ("parskip", Some("half")) => o.parskip_half = true,
            ("parskip", Some("false" | "off" | "no")) => o.parskip_half = false,
            ("fromalign", Some(v)) => {
                o.align_from = match v {
                    "left" => 0,
                    "center" | "centered" | "middle" => 1,
                    "right" => 2,
                    "no" | "false" | "off" => 10000,
                    _ => return Err(opt),
                }
            }
            ("fromrule", v) => {
                o.rule_from = match v.unwrap_or("below") {
                    "false" | "no" | "off" => 0,
                    "aftername" => 1,
                    "below" | "afteraddress" | "true" | "yes" | "on" => 2,
                    _ => return Err(opt),
                }
            }
            ("fromphone", v) => o.phone = bool_value(v).ok_or(opt)?,
            ("fromemail", v) => o.email = bool_value(v).ok_or(opt)?,
            ("foldmarks", v) => o.no_fold = !bool_value(v).ok_or(opt)?,
            ("subject", Some("beforeopening")) => o.subj_after = false,
            ("subject", Some("afteropening")) => o.subj_after = true,
            ("subject", Some("untitled")) => {}
            ("enlargefirstpage", v) => o.enlarge_fp = bool_value(v).ok_or(opt)?,
            _ => return Err(opt),
        }
    }
    Ok(o)
}

/// The letter variables and fonts, kept as token lists by name.
type Named = Vec<(Box<str>, Vec<Token>)>;

/// scrlttr2's state (`Packages::lttr`).
#[derive(Clone, PartialEq, Default)]
pub(super) struct Lttr {
    opts: Options,
    /// `\setkomavar{name}{content}`, `\setkomavar*{name}{description}`, `\setkomafont`.
    vars: Named,
    names: Named,
    fonts: Named,
    /// The three at `\begin{letter}`: `\setkomavar` and `\setkomafont` are local, the
    /// environment's group restores them.
    saved: Option<Box<(Named, Named, Named)>>,
    /// `\c@letter`.
    letters: u32,
    /// The pseudo-lengths that follow the size: `\baselineskip` at the class's end
    /// (`refaftervskip`, half `sigbeforevskip` and `subjectaftervskip`), `firstfootvpos`.
    base: i32,
    foot_vpos: i32,
}

/// A variable's default content (scrlttr2.cls's `\setkomavar` at load), when the document has
/// not set it: `signature` is `\usekomavar{fromname}`, `date` `\@date` (`\today`), the
/// separators `,\nobreakspace`. `backaddress` and `firsthead` are built at use.
fn default_var(name: &str) -> &'static str {
    match name {
        "signature" => "\\usekomavar{fromname}",
        "date" => "\\today",
        "placeseparator" | "backaddressseparator" => ",\\nobreakspace ",
        "enclseparator" | "ccseparator" | "subjectseparator" => ": ",
        _ => "",
    }
}

/// A variable's default description (`\usekomavar*`): `fromphone`'s is `\usekomavar*
/// {phoneseparator}\usekomavar{phoneseparator}`, `\phonename` and `:\nobreakspace`.
fn default_name(name: &str, german: bool) -> &'static str {
    // scrlttr2's `\providecaptionname` at `\begin{document}` (english, and german with
    // ngerman), after babel's own `\enclname` and `\ccname` (babel-german's `Anlage(n)`,
    // `Verteiler`).
    match (name, german) {
        ("fromphone", false) => "Phone:\\nobreakspace ",
        ("fromemail", false) => "Email:\\nobreakspace ",
        ("enclseparator", false) => "encl",
        ("enclseparator", true) => "Anlage(n)",
        ("ccseparator", false) => "cc",
        ("ccseparator", true) => "Verteiler",
        ("yourref", false) => "Your ref.",
        ("yourref", true) => "Ihr Zeichen",
        ("yourmail", false) => "Your letter of",
        ("yourmail", true) => "Ihr Schreiben vom",
        ("myref", false) => "Our ref.",
        ("myref", true) => "Unser Zeichen",
        ("customer", false) => "Customer no.",
        ("customer", true) => "Kundennummer",
        ("invoice", false) => "Invoice no.",
        ("invoice", true) => "Rechnungsnummer",
        ("date", false) => "Date",
        ("date", true) => "Datum",
        _ => "",
    }
}

/// scrlttr2's font defaults (`\newkomafont`): `backaddress` `\maybesffamily` (`\sffamily`),
/// `lettersubject` `\normalfont\bfseries`, `refname` `\sffamily\scriptsize`; the others empty.
fn default_font(name: &str) -> &'static str {
    match name {
        "backaddress" => "\\sffamily ",
        "lettersubject" => "\\normalfont\\bfseries ",
        "refname" => "\\sffamily\\scriptsize ",
        _ => "",
    }
}

/// `\usekomafont`'s aliases (`\scr@fnt@instead@subject` is `lettersubject`).
fn font_name(name: &str) -> &str {
    if name == "subject" { "lettersubject" } else { name }
}

/// The variables the native first page does not place yet: set to anything, they decline.
const LATER_VARS: &[&str] = &["title", "specialmail", "fromlogo"];

/// `\defaultreffields`: the reference line's fields in order.
const REF_FIELDS: [&str; 5] = ["yourref", "yourmail", "myref", "customer", "invoice"];

/// babel's languages whose names scrlttr2's natives know (`engine_babel::LANGS`): english,
/// british, ngerman.
const GERMAN: usize = 5;

/// `\\` in the back address (`\def\\{\usekomavar{backaddressseparator}\@ogobble}`): the
/// separator, and `\@ogobble` (`\@protected@testopt`) takes the spaces after it and an
/// optional argument.
fn split_back_address(list: &[Token], cr: Token, sep: &[Token]) -> Vec<Token> {
    let mut out = Vec::with_capacity(list.len());
    let mut k = 0;
    while k < list.len() {
        if list[k] != cr {
            out.push(list[k]);
            k += 1;
            continue;
        }
        out.extend_from_slice(sep);
        k += 1;
        while list.get(k).is_some_and(|t| t.cat() == Some(Cat::Space)) {
            k += 1;
        }
        if list.get(k) == Some(&Token::char(Cat::Other, '[')) {
            while k < list.len() && list[k] != Token::char(Cat::Other, ']') {
                k += 1;
            }
            k += 1;
        }
    }
    out
}

/// §455's `xn_over_d(x, f, 65536)` with the sign outside: a decimal coefficient `f/65536`
/// times a dimension, as TeX scans `.5\paperwidth`.
fn times(x: i32, f: i64) -> i32 {
    let v = (x.unsigned_abs() as i64 * f / 65536) as i32;
    if x < 0 { -v } else { v }
}

/// One inch (§458: `7227/100`pt, 4736286sp).
const IN: i32 = 4736286;

impl Machine<'_, '_> {
    fn lttr(&self) -> Option<&Lttr> {
        self.doc.as_ref().and_then(|d| d.pkg.lttr.as_deref())
    }

    pub(super) fn lttr_on(&self) -> bool {
        self.lttr().is_some()
    }

    fn lttr_mut(&mut self) -> Option<&mut Lttr> {
        self.doc.as_mut().and_then(|d| d.pkg.lttr.as_deref_mut())
    }

    /// The tokens of `s`, as [`Self::tex_tokens`] makes them but with `@` a letter in names
    /// (scrlttr2's own natives), and `\endgraf` the primitive `\par` (`rt@@par`).
    fn lt(&mut self, out: &mut Vec<Token>, s: &str) {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            i += 1;
            if c != b'\\' {
                out.extend(self.tex_tokens(&s[i - 1..i]));
                continue;
            }
            let n = b[i..].iter().take_while(|c| c.is_ascii_alphabetic() || **c == b'@').count().max(1);
            let name = &s[i..i + n];
            i += n;
            if name.as_bytes()[0].is_ascii_alphabetic() {
                i += b[i..].iter().take_while(|&&c| c == b' ').count();
            }
            out.push(if name == "endgraf" { self.doc_id(PRIM_PAR) } else { Token::cs(self.e.cs.intern(name)) });
        }
    }

    /// A variable's content: the document's, else the class's default.
    fn kvar(&mut self, name: &str) -> Vec<Token> {
        if let Some(v) = self.lttr().and_then(|l| l.vars.iter().find(|v| &*v.0 == name)) {
            return v.1.clone();
        }
        let mut t = Vec::new();
        if name == "backaddress" {
            // `\Ifkomavarempty{fromname}{}{\strut\ignorespaces\usekomavar{fromname}
            // \Ifkomavarempty{fromaddress}{}{\\}}\Ifkomavarempty{fromaddress}{}{\strut
            // \ignorespaces\usekomavar{fromaddress}}`.
            let (name, address) = (self.kvar("fromname"), self.kvar("fromaddress"));
            if !name.is_empty() {
                self.lt(&mut t, "\\strut\\ignorespaces ");
                t.extend(name);
                if !address.is_empty() {
                    self.lt(&mut t, "\\\\");
                }
            }
            if !address.is_empty() {
                self.lt(&mut t, "\\strut\\ignorespaces ");
                t.extend(address);
            }
            return t;
        }
        self.lt(&mut t, default_var(name));
        t
    }

    /// A variable's description (`\usekomavar*`).
    fn kname(&mut self, name: &str) -> Vec<Token> {
        if let Some(v) = self.lttr().and_then(|l| l.names.iter().find(|v| &*v.0 == name)) {
            return v.1.clone();
        }
        let german = self.doc.as_ref().and_then(|d| d.babel.as_ref()).is_some_and(|b| b.main == GERMAN);
        let mut t = Vec::new();
        self.lt(&mut t, default_name(name, german));
        t
    }

    /// A font element's commands (`\usekomafont`).
    fn kfont(&mut self, name: &str) -> Vec<Token> {
        let name = font_name(name);
        if let Some(v) = self.lttr().and_then(|l| l.fonts.iter().find(|v| &*v.0 == name)) {
            return v.1.clone();
        }
        let mut t = Vec::new();
        self.lt(&mut t, default_font(name));
        t
    }

    /// The class hook: the options, and the pseudo-lengths that follow the size (DIN.lco's
    /// `refaftervskip` is `\baselineskip`; `firstfootvpos` is `1in+\topmargin+\headheight+
    /// \headsep+\textheight+\footskip`, DIN.lco's branch for versions after 2.9t).
    pub(super) fn lttr_setup(&mut self, opts: &str) {
        let o = match options(opts) {
            Ok(o) => o,
            Err(_) => return self.not_native("a scrlttr2 option"),
        };
        let base = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width;
        let Some(doc) = self.doc.as_mut() else { return };
        let d = |n: &str| doc.class.dimen(n);
        let foot_vpos = IN + d("topmargin") + d("headheight") + d("headsep") + d("textheight") + d("footskip");
        doc.pkg.lttr = Some(Box::new(Lttr { opts: o, base, foot_vpos, ..Lttr::default() }));
    }

    /// `\setkomavar{name}[description]{content}`, `\setkomavar*{name}{description}`.
    pub(super) fn set_koma_var(&mut self) {
        let star = self.next_is('*');
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: Box<str> = name.iter().map(|t| t.chr()).collect();
        let desc = if star { None } else { self.next_is('[').then(|| self.bracket_arg()) };
        let mut value = Vec::new();
        self.read_arg(&mut value);
        let Some(l) = self.lttr_mut() else { return self.not_native("\\setkomavar outside scrlttr2") };
        let set = |list: &mut Named, v: Vec<Token>| match list.iter_mut().find(|x| x.0 == name) {
            Some(x) => x.1 = v,
            None => list.push((name.clone(), v)),
        };
        if star {
            set(&mut l.names, value);
            return;
        }
        if let Some(d) = desc {
            set(&mut l.names, d);
        }
        // `\scr@firsthead@postsetvar`: a document's `firsthead` sets `fromalign=no`.
        if &*name == "firsthead" {
            l.opts.align_from = 10000;
        }
        set(&mut l.vars, value);
    }

    /// `\setkomafont{element}{commands}`, `\addtokomafont` (appended).
    pub(super) fn set_koma_font(&mut self, add: bool) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: String = name.iter().map(|t| t.chr()).collect();
        let mut value = Vec::new();
        self.read_arg(&mut value);
        let mut v = if add { self.kfont(&name) } else { Vec::new() };
        v.extend(value);
        let name: Box<str> = font_name(&name).into();
        let Some(l) = self.lttr_mut() else { return self.not_native("\\setkomafont outside scrlttr2") };
        match l.fonts.iter_mut().find(|x| x.0 == name) {
            Some(x) => x.1 = v,
            None => l.fonts.push((name, v)),
        }
    }

    /// `\usekomavar[command]{name}`, `\usekomavar*`: the content (or description), given to
    /// the command as its argument.
    pub(super) fn use_koma_var(&mut self) {
        let star = self.next_is('*');
        let cmd = self.next_is('[').then(|| self.bracket_arg());
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: String = name.iter().map(|t| t.chr()).collect();
        if self.lttr().is_none() {
            return self.not_native("\\usekomavar outside scrlttr2");
        }
        let v = if star { self.kname(&name) } else { self.kvar(&name) };
        let t = match cmd {
            Some(mut c) => {
                c.push(Token::char(Cat::BeginGroup, '{'));
                c.extend(v);
                c.push(Token::char(Cat::EndGroup, '}'));
                c
            }
            None => v,
        };
        self.back_list(&t);
    }

    /// `\usekomafont{element}`.
    pub(super) fn use_koma_font(&mut self) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let name: String = name.iter().map(|t| t.chr()).collect();
        if self.lttr().is_none() {
            return self.not_native("\\usekomafont outside scrlttr2");
        }
        let t = self.kfont(&name);
        self.back_list(&t);
    }

    /// `\begin{letter}{#2}`: `\cleardoubleoddemptypage` (`\clearpage{\pagestyle{empty}
    /// \cleardoubleoddstandardpage}`, one-sided: two `\clearpage`s), `\raggedbottom` (the
    /// table's), `\refstepcounter{letter}`, `\setcounter{page}{1}`, `\setcounter{footnote}{0}`,
    /// `\interlinepenalty=200`, `\@processto{#2}` (`toname` up to the first `\\`,
    /// `toaddress` after it), `\KOMAoptions{#1}` (an optional argument declines).
    pub(super) fn lttr_begin(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.next_is('[') {
            return self.not_native("letter options");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let cr = self.named_cs(named!("\\"));
        let (name, address) = match super::letter::after_cr(&arg, cr) {
            Some(rest) => (arg[..arg.len() - rest.len() - 1].to_vec(), super::letter::strip_group(rest).to_vec()),
            None => (arg.clone(), Vec::new()),
        };
        let Some(l) = self.lttr_mut() else { return self.not_native("the letter environment outside scrlttr2") };
        l.saved = Some(Box::new((l.vars.clone(), l.names.clone(), l.fonts.clone())));
        l.letters += 1;
        for (k, v) in [("toname", name), ("toaddress", address)] {
            match l.vars.iter_mut().find(|x| &*x.0 == k) {
                Some(x) => x.1 = v,
                None => l.vars.push((k.into(), v)),
            }
        }
        let mut t = vec![self.doc_id(doc!("clearpage")), self.doc_id(doc!("clearpage"))];
        self.lt(&mut t, "\\setcounter{page}{1}\\setcounter{footnote}{0}\\interlinepenalty200 \\rt@lttranchor ");
        self.back_list(&t);
    }

    /// `\end{letter}`: `\stopletter` (empty), `\label{\thisletter.lastpage}`,
    /// `\@@par\pagebreak\@@par` (`\pagebreak` in vertical mode is `\penalty-\@M`); then
    /// `\end`'s `\endgroup`, which restores the variables, fonts and `\@afterindent`.
    pub(super) fn lttr_end(&mut self) {
        let Some(l) = self.lttr_mut() else { return };
        let n = l.letters;
        if let Some(s) = l.saved.take() {
            (l.vars, l.names, l.fonts) = *s;
        }
        if let Some(d) = self.doc.as_mut() {
            d.afterindent = true;
        }
        let mut t = Vec::new();
        self.lt(&mut t, &format!("\\label{{{n}.lastpage}}\\endgraf\\penalty-10000 \\endgraf\\endgroup "));
        self.back_list(&t);
    }

    /// `\move@topt`: `\null`, then back to the paper's top edge (`\vskip-1in`, `-\topmargin`,
    /// `-\headheight`, `-\headsep`, `-\topskip`).
    fn move_to_top(&mut self, t: &mut Vec<Token>) {
        let Some(c) = self.doc.as_ref().map(|d| d.class) else { return };
        self.lt(t, &format!("\\hbox{{}}\\vskip-1in\\vskip{}sp\\vskip{}sp\\vskip{}sp\\vskip-\\topskip ", -c.dimen("topmargin"), -c.dimen("headheight"), -c.dimen("headsep")));
    }

    /// `\move@frompt`: `\vskip 1in`, `\topmargin`, `\headheight`, `\headsep`, `\topskip`,
    /// `-\baselineskip`.
    fn move_from_top(&mut self, t: &mut Vec<Token>) {
        let Some(c) = self.doc.as_ref().map(|d| d.class) else { return };
        self.lt(t, &format!("\\vskip1in\\vskip{}sp\\vskip{}sp\\vskip{}sp\\vskip\\topskip\\vskip-\\baselineskip ", c.dimen("topmargin"), c.dimen("headheight"), c.dimen("headsep")));
    }

    /// `\rlap{\move@topl\hskip x` and the field, then `\hss}`: `\move@topl` is `\null\hskip
    /// -1in\hskip-\oddsidemargin` on an odd page.
    fn left_edge(&mut self, t: &mut Vec<Token>, x: i32) {
        let osm = self.doc.as_ref().map_or(0, |d| d.class.dimen("oddsidemargin"));
        self.lt(t, &format!("\\hbox to0pt{{\\hbox{{}}\\hskip-1in\\hskip{}sp\\hskip{x}sp", -osm));
    }

    /// `\vb@t@z{#1}`: `{\setbox\z@\vbox to\z@{\color@begingroup #1\color@endgroup\vss}`, the
    /// paragraph sizes, `\baselineskip\z@`, `\box\z@\vskip-(\ht0+\dp0)}` (zero). With a
    /// colour package `\color@begingroup` is `\begingroup`, `\color@endgroup` `\endgraf
    /// \endgroup`.
    fn vbtz(&mut self, t: &mut Vec<Token>, body: Vec<Token>) {
        let colored = self.colored();
        self.lt(t, if colored { "{\\setbox0\\vbox to0pt{\\begingroup " } else { "{\\setbox0\\vbox to0pt{" });
        t.extend(body);
        self.lt(t, if colored { "\\endgraf\\endgroup\\vss}\\baselineskip0pt\\box0\\vskip0pt}" } else { "\\vss}\\baselineskip0pt\\box0\\vskip0pt}" });
    }

    /// `\@foldmark{#1}{#2}`: `\vb@t@z{\vskip(#1-.5 thickness)\rlap{\move@topl\hskip
    /// foldmarkhpos\usekomafont{foldmark}{\rule{#2}{thickness}}}}`.
    fn fold_mark(&mut self, t: &mut Vec<Token>, pos: i32, len: i32) {
        let (thick, hpos) = (13107, self.dimen_of("3.5mm"));
        let mut b = Vec::new();
        self.lt(&mut b, &format!("\\vskip{}sp", pos + times(thick, -32768)));
        self.left_edge(&mut b, hpos);
        b.extend(self.kfont("foldmark"));
        self.lt(&mut b, &format!("{{\\rule{{{len}sp}}{{{thick}sp}}}}\\hss}}"));
        self.vbtz(t, b);
    }

    /// The generated `firsthead` (`\@gen@firsthead` by `\@alignfrom`): `\parbox[b]{\textwidth}
    /// {\raggedright` (`\centering`, `\raggedleft`) the name, the rule after it, `\\`, the
    /// address, phone and email lines, the rule below, `\\}`.
    fn gen_first_head(&mut self, t: &mut Vec<Token>, width: i32) {
        let o = self.lttr().map(|l| l.opts).unwrap_or_default();
        let align = ["\\raggedright ", "\\centering ", "\\raggedleft "][o.align_from as usize];
        // `\setlength{\@tempdima}{\useplength{fromrulewidth}}` (0), so `\textwidth`; then
        // `\expandafter\rule\expandafter{\the\@tempdima}{\useplength{fromrulethickness}}`.
        let rule = |m: &mut Self, t: &mut Vec<Token>| {
            m.lt(t, "\\\\[-.5\\baselineskip]");
            t.extend(m.kfont("fromrule"));
            m.lt(t, &format!("{{\\rule{{{width}sp}}{{26214sp}}}}"));
        };
        self.lt(t, &format!("\\parbox[b]{{{width}sp}}{{{align}{{"));
        t.extend(self.kfont("fromname"));
        self.lt(t, "\\strut\\ignorespaces ");
        t.extend(self.kvar("fromname"));
        self.lt(t, "{");
        if o.rule_from == 1 {
            rule(self, t);
        }
        self.lt(t, "}\\\\}{");
        t.extend(self.kfont("fromaddress"));
        self.lt(t, "\\strut\\ignorespaces ");
        t.extend(self.kvar("fromaddress"));
        for (on, var) in [(o.phone, "fromphone"), (o.email, "fromemail")] {
            if on {
                self.lt(t, "\\\\");
                t.extend(self.kname(var));
                t.extend(self.kvar(var));
            }
        }
        if o.rule_from == 2 {
            rule(self, t);
        }
        self.lt(t, "\\\\}}");
    }

    /// `\opening{#1}`: `\@gen@firsthead`, `\thispagestyle{empty}`, `\noindent`, the fold
    /// marks, the head and foot of the first page, the address field, the location field,
    /// the date line, the title, the subject before the opening, `#1` with no paragraph
    /// skips, `\vskip(\baselineskip-\parskip)` when positive, the subject after it,
    /// `\@afterindentfalse\@afterheading`.
    pub(super) fn lttr_opening(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(l) = self.lttr() else { return self.not_native("\\opening outside scrlttr2") };
        let (o, base, foot_vpos) = (l.opts, l.base, l.foot_vpos);
        if LATER_VARS.iter().any(|v| l.vars.iter().any(|x| &*x.0 == *v && !x.1.is_empty())) {
            return self.not_native("scrlttr2's title, special mail or logo");
        }
        let Some(doc) = self.doc.as_ref() else { return };
        // The names of the reference line, `\encl` and `\cc` follow babel's language.
        if let Some(b) = doc.babel.as_ref() {
            if b.main > 1 && b.main != GERMAN || b.main == GERMAN && (o.phone || o.email) {
                return self.not_native("scrlttr2's names in this language");
            }
        }
        if doc.page % 2 == 0 {
            return self.not_native("\\opening on an even page");
        }
        let [pw, ph] = KOMA_PAPER_SIZES[3];
        let tw = doc.class.dimen("textwidth");
        let osm = doc.class.dimen("oddsidemargin");
        // DIN.lco's pseudo-lengths (each `\setplength` keeps `\the` of the value, which reads
        // back to the same sp).
        let mm = |m: &Self, x: &str| m.dimen_of(x);
        let (head_vpos, addr_vpos, addr_hpos, addr_width, addr_height, back_height, ref_vpos) = (mm(self, "8mm"), mm(self, "45mm"), mm(self, "20mm"), mm(self, "85mm"), mm(self, "45mm"), mm(self, "5mm"), mm(self, "98.5mm"));
        // `firstheadwidth` and `firstfootwidth`: `\paperwidth` less twice `toaddrhpos`.
        let head_width = pw - 2 * addr_hpos;
        let mut t = vec![self.doc_id(doc!("thispagestyle"))];
        self.lt(&mut t, "{empty}\\noindent ");
        // `\@foldmarks`: P at `.5\paperheight` (4mm), T and B (DIN.lco's 105mm and 210mm,
        // 2mm); M and L are zero.
        if !o.no_fold {
            self.move_to_top(&mut t);
            let (four, two) = (self.dimen_of("4mm"), self.dimen_of("2mm"));
            self.fold_mark(&mut t, times(ph, 32768), four);
            let (tpos, bpos) = (self.dimen_of("105mm"), self.dimen_of("210mm"));
            self.fold_mark(&mut t, tpos, two);
            self.fold_mark(&mut t, bpos, two);
            self.move_from_top(&mut t);
        }
        // `\@firstheadfootfield`: `firstheadhpos` is `\maxdimen`, so the head is centred
        // (`.5\paperwidth-.5 firstheadwidth`); its box is `toaddrvpos-firstheadvpos-
        // \baselineskip` high (`\@savevbox`), the head followed by `\vskip\z@\@plus.00001fil`.
        let centre = times(pw, 32768) + times(head_width, -32768);
        self.move_to_top(&mut t);
        self.lt(&mut t, &format!("\\vskip{head_vpos}sp"));
        let mut b = Vec::new();
        self.left_edge(&mut b, centre);
        self.lt(&mut b, &format!("\\vbox{{\\hsize{head_width}sp\\textwidth{head_width}sp\\parfillskip0pt plus1fil\\begingroup\\vbox to\\dimexpr{}sp-\\baselineskip\\relax{{\\noindent ", addr_vpos - head_vpos));
        if o.align_from == 10000 {
            b.extend(self.kvar("firsthead"));
        } else {
            self.gen_first_head(&mut b, head_width);
        }
        self.lt(&mut b, "\\vskip0pt plus.00001fil}\\endgroup}\\hss}");
        self.vbtz(&mut t, b);
        self.lt(&mut t, &format!("\\vskip{}sp", -head_vpos));
        self.move_from_top(&mut t);
        // The foot (`firstfoothpos` `\maxdimen`: centred, at `firstfootvpos`).
        self.move_to_top(&mut t);
        let mut b = Vec::new();
        self.left_edge(&mut b, centre);
        self.lt(&mut b, &format!("\\vbox{{\\hsize{head_width}sp\\vskip{foot_vpos}sp\\textwidth{head_width}sp\\parfillskip0pt plus1fil\\vbox{{\\hsize{head_width}sp\\noindent "));
        b.extend(self.kvar("firstfoot"));
        self.lt(&mut b, "}}\\hss}");
        self.vbtz(&mut t, b);
        self.move_from_top(&mut t);
        // `firstfootvpos` against the page's bottom less `\ht\strutbox` (versions from 3.48):
        // within `\maxdepth` of zero nothing; above it `shrinkfirstpage`, below it
        // `enlargefirstpage`: `\enlargethispage{-\@tempdima}`, `\insert\@kludgeins{\vskip
        // -\@tempskipa}` (`\@kludgeins` is insert 200, `\count` 1000, `\dimen` `\maxdimen`).
        {
            let c = self.doc.as_ref().map(|d| d.class);
            let bottom = c.map_or(0, |c| IN + c.dimen("topmargin") + c.dimen("headheight") + c.dimen("headsep") + c.dimen("textheight") + c.dimen("footskip"));
            let d = bottom - foot_vpos - self.e.eqtb.word(super::STRUT_HT);
            let md = self.dimen_param(super::super::engine_page::MAX_DEPTH);
            let enlarge = if d > -md && d < md { false } else if d > 0 { true } else { o.enlarge_fp };
            if enlarge {
                self.lt(&mut t, &format!("\\global\\count200=1000 \\global\\dimen200=1073741823sp \\global\\skip200=0pt\\insert200{{\\vskip{d}sp}}"));
            }
        }
        // `\@addrfield` (`addrfield=true`): at `toaddrvpos`, `toaddrhpos` from the left edge,
        // a `toaddrheight` box of `toaddrwidth`: the back address in a `backaddrheight` box
        // (`\vss` first), `\vfil`, the addressee, `\vfil\null\vskip-\baselineskip`.
        self.move_to_top(&mut t);
        self.lt(&mut t, &format!("\\vskip{addr_vpos}sp"));
        let mut b = Vec::new();
        self.lt(&mut b, "{\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil ");
        self.left_edge(&mut b, addr_hpos);
        self.lt(&mut b, &format!("\\begingroup\\vbox to{addr_height}sp{{\\hsize{addr_width}sp\\begingroup\\vbox to{back_height}sp{{\\vss\\hsize{addr_width}sp"));
        let back = self.kvar("backaddress");
        if !back.is_empty() {
            // `{\noindent\backaddr@format{\scriptsize{\def\\{..}\usekomafont{backaddress}
            // {\usekomavar{backaddress}}}}}`, `\backaddr@format` LaTeX's `\underline`:
            // `$\@@underline{\hbox{#1}}\m@th$\relax`.
            // `\usekomavar`'s `\@ifstar` (`\futurelet`) ends the characters before it: no kern
            // or ligature reaches the separator (`\relax` here).
            let mut sep = vec![self.named_cs(named!("relax"))];
            sep.extend(self.kvar("backaddressseparator"));
            let cr = self.named_cs(named!("\\"));
            let back = split_back_address(&back, cr, &sep);
            self.lt(&mut b, "{\\noindent\\relax$\\underline{\\hbox{\\scriptsize{");
            b.extend(self.kfont("backaddress"));
            self.lt(&mut b, "{");
            b.extend(back);
            self.lt(&mut b, "}}}}\\mathsurround0pt$\\relax}");
        }
        self.lt(&mut b, &format!("}}\\endgroup\\vfil\\noindent\\hskip0pt\\vbox{{\\hsize{addr_width}sp\\raggedright "));
        b.extend(self.kfont("addressee"));
        self.lt(&mut b, "{{");
        b.extend(self.kfont("toname"));
        self.lt(&mut b, "{");
        b.extend(self.kvar("toname"));
        self.lt(&mut b, "\\\\}}{");
        b.extend(self.kfont("toaddress"));
        self.lt(&mut b, "{");
        b.extend(self.kvar("toaddress"));
        self.lt(&mut b, "\\endgraf}}}}\\vfil\\hbox{}\\vskip-\\baselineskip}\\endgroup\\hss}}");
        self.vbtz(&mut t, b);
        self.lt(&mut t, &format!("\\vskip{}sp", -addr_vpos));
        self.move_from_top(&mut t);
        // `\@locfield` (`fromalign` not a location one): with a location, `locwidth` half of
        // `\paperwidth-2toaddrhpos-toaddrwidth`, at `toaddrvpos`, right-aligned at `\paperwidth
        // -toaddrhpos`, a `toaddrheight` box of the location and `\vskip\z@\@plus.00001fil`.
        let location = self.kvar("location");
        if !location.is_empty() {
            let width = times(pw - 2 * addr_hpos - addr_width, 32768);
            self.move_to_top(&mut t);
            self.lt(&mut t, &format!("\\vskip{addr_vpos}sp"));
            let mut b = Vec::new();
            self.lt(&mut b, &format!("\\noindent\\makebox[0pt][l]{{\\hbox{{}}\\hskip-1in\\hskip{}sp\\hskip{}sp\\makebox[0pt][r]{{\\begingroup\\vbox to{addr_height}sp{{\\noindent\\hsize{width}sp", -osm, pw - addr_hpos));
            b.extend(location);
            self.lt(&mut b, "\\vskip0pt plus.00001fil}\\endgroup}}");
            self.vbtz(&mut t, b);
            self.lt(&mut t, &format!("\\vskip{}sp", -addr_vpos));
            self.move_from_top(&mut t);
        }
        // `\@datefield` (`refline=narrow`, `refwidth` 0): `refwidth` is `\textwidth`,
        // `refhpos` `\oddsidemargin+1in`; at `refvpos`, the reference fields and the date as
        // one more (`\@datepos` 1), or with no field `\enskip\hfill` and the place, its
        // separator and the date; then `refaftervskip`.
        self.lt(&mut t, "{\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil ");
        self.move_to_top(&mut t);
        self.lt(&mut t, &format!("\\vskip{ref_vpos}sp"));
        let date = self.kvar("date");
        let mut fields: Vec<&str> = REF_FIELDS.into_iter().filter(|f| self.lttr().is_some_and(|l| l.vars.iter().any(|x| &*x.0 == *f && !x.1.is_empty()))).collect();
        if !fields.is_empty() && !date.is_empty() {
            fields.push("date");
        }
        if !date.is_empty() || !fields.is_empty() {
            // `\rlap{\noindent\move@topl..}`: `\noindent` in the box does nothing.
            self.left_edge(&mut t, osm + IN);
            self.lt(&mut t, &format!("\\vbox{{\\hsize{tw}sp\\noindent "));
            if fields.is_empty() {
                self.lt(&mut t, "\\hskip.5em\\relax\\hfill{");
                t.extend(self.kfont("placeanddate"));
                self.lt(&mut t, "{");
                let place = self.kvar("place");
                if !place.is_empty() {
                    t.extend(place);
                    t.extend(self.kvar("placeseparator"));
                }
                t.extend(date);
                self.lt(&mut t, "}}");
            }
            for (k, f) in fields.into_iter().enumerate() {
                self.ref_field(&mut t, f, k > 0);
            }
            self.lt(&mut t, "}\\hss}");
        }
        self.lt(&mut t, &format!("\\vskip{base}sp}}"));
        // `\@makelettertitle`: no title (declined above).
        if !o.subj_after {
            self.subject_field(&mut t, base, false);
        }
        // `{\setparsizes{\z@}{\z@}{\z@ plus 1fil}\par@updaterelative#1\par}`, then
        // `\vskip(\baselineskip-\parskip)` (widths, as `\setlength` of a dimension takes).
        self.lt(&mut t, "{\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil ");
        t.extend(arg);
        self.lt(&mut t, "\\par}");
        let skip = self.glue_param(super::super::engine_pack::BASELINE_SKIP).width - self.glue_param(super::super::engine_pack::PAR_SKIP).width;
        if skip > 0 {
            self.lt(&mut t, &format!("\\vskip{skip}sp"));
        }
        if o.subj_after {
            self.subject_field(&mut t, base, true);
        }
        if let Some(d) = self.doc.as_mut() {
            d.afterindent = false;
        }
        t.push(self.doc_id(doc!("rt@afterheading")));
        self.back_list(&t);
    }

    /// `\@set@reffield{#1}`: `\settowidth{\@tempdima}{\strut\usekomafont{refvalue}` the value
    /// `}`, `\sbox\z@{\strut\usekomafont{refname}` the description `}`, the wider of the two,
    /// `\enskip\hfill` after the first field, `\parbox[t]{\@tempdima}{\noindent\raggedright
    /// \strut\box\z@\\\usekomafont{refvalue}\strut` the value `}`.
    fn ref_field(&mut self, t: &mut Vec<Token>, name: &str, after: bool) {
        let (value, desc) = (self.kvar(name), self.kname(name));
        let colored = self.colored();
        self.lt(t, "\\setbox32701\\hbox{{\\strut ");
        t.extend(self.kfont("refvalue"));
        t.extend(value.iter().copied());
        // `\sbox`'s `\color@setgroup` (`\begingroup\set@color`) with a colour package.
        self.lt(t, if colored { "}}\\dimen0\\wd32701 \\setbox0\\hbox{\\begingroup\\rt@setcolor\\strut " } else { "}}\\dimen0\\wd32701 \\setbox0\\hbox{\\strut " });
        t.extend(self.kfont("refname"));
        t.extend(desc);
        self.lt(t, if colored { "\\endgroup}" } else { "}" });
        self.lt(t, "\\ifdim\\dimen0<\\wd0 \\dimen0\\wd0 \\fi ");
        if after {
            self.lt(t, "\\hskip.5em\\relax\\hfill ");
        }
        self.lt(t, "\\parbox[t]{\\dimen0}{\\noindent\\raggedright\\strut\\box0\\\\");
        t.extend(self.kfont("refvalue"));
        self.lt(t, "\\strut ");
        t.extend(value);
        self.lt(t, "}");
    }

    /// `\@subjfield` with `subjectvpos` 0, untitled: `\vskip` `subjectbeforevskip` (0pt),
    /// `\begingroup\usekomafont{lettersubject}`, `\raggedsubject` (`\noindent\@hangfrom`, after the opening `\centering`; its
    /// argument the empty title) `{\strut\ignorespaces` the subject `\par\endgroup`,
    /// `\vskip` `subjectaftervskip` (`2\baselineskip` at the class's load).
    fn subject_field(&mut self, t: &mut Vec<Token>, base: i32, after: bool) {
        let subject = self.kvar("subject");
        if subject.is_empty() {
            return;
        }
        self.lt(t, "{\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil\\vskip0pt\\begingroup ");
        t.extend(self.kfont("lettersubject"));
        // After the opening `\raggedsubject` is `\centering`: `{}` starts the paragraph.
        self.lt(t, if after { "\\centering{}\\strut\\ignorespaces " } else { "\\noindent\\setbox32701\\hbox{{}}\\hangindent\\wd32701 \\noindent\\box32701 \\strut\\ignorespaces " });
        t.extend(subject);
        self.lt(t, &format!("\\par\\endgroup\\vskip{}sp}}", 2 * base));
    }

    /// `\closing{#1}` with `\raggedsignature` `\centering` (`\sigalign` c) and `sigindent` 0:
    /// in a group, the paragraph sizes zero, `\par\nobreak\vskip\baselineskip`, the width
    /// `\linewidth` narrowed to the wider of `#1` and the signature's tabular when both are
    /// narrower, `\hskip` `sigindent`, `\parbox[t]{width}{\centering\ignorespaces#1\par
    /// \rule[-\dp\strutbox]{0pt}{\baselineskip+sigbeforevskip}\begin{tabular}[t]{@{}c@{}}`
    /// the signature `\end{tabular}}`; after the group `\par\nobreak\vskip\baselineskip`.
    pub(super) fn lttr_closing(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(base) = self.lttr().map(|l| l.base) else { return self.not_native("\\closing outside scrlttr2") };
        let sig = self.kvar("signature");
        let dp = self.e.eqtb.word(super::STRUT_DP);
        let mut t = Vec::new();
        self.lt(&mut t, "{\\parindent0pt\\parskip0pt\\parfillskip0pt plus1fil\\par\\nobreak\\vskip\\baselineskip\\setbox2\\hbox{");
        t.extend(arg.iter().copied());
        self.lt(&mut t, "}\\setbox4\\hbox{\\begin{tabular}[t]{@{}c@{}}");
        t.extend(sig.iter().copied());
        self.lt(&mut t, "\\ifhmode\\unskip\\ifhmode\\nobreak\\strut\\fi\\fi\\end{tabular}}\\dimen0=\\linewidth\\ifdim\\dimen0>\\wd2 \\ifdim\\dimen0>\\wd4 \\ifdim\\wd2>\\wd4 \\dimen0=\\wd2 \\else\\dimen0=\\wd4 \\fi\\fi\\fi\\hskip0pt\\parbox[t]{\\dimen0}{\\centering\\ignorespaces ");
        t.extend(arg);
        self.lt(&mut t, &format!("\\par\\rule[{}sp]{{0pt}}{{\\dimexpr\\baselineskip+{}sp\\relax}}\\begin{{tabular}}[t]{{@{{}}c@{{}}}}", -dp, 2 * base));
        t.extend(sig);
        self.lt(&mut t, "\\end{tabular}}}\\par\\nobreak\\vskip\\baselineskip ");
        self.back_list(&t);
    }

    /// `\ps`: `\par`, `\vskip\baselineskip` when `\parskip` is zero, `\startbreaks`
    /// (`\interlinepenalty 200`, `\def\par{\@@par\penalty 200\relax}`, local to the letter),
    /// `\noindent\ignorespaces`.
    pub(super) fn lttr_ps(&mut self) {
        if self.lttr().is_none() {
            return self.not_native("\\ps outside scrlttr2");
        }
        let zero = self.glue_param(super::super::engine_pack::PAR_SKIP).width == 0;
        let mut t = Vec::new();
        self.lt(&mut t, if zero { "\\par\\vskip\\baselineskip" } else { "\\par" });
        self.lt(&mut t, "\\interlinepenalty200 \\def\\par{\\endgraf\\penalty200\\relax}\\noindent\\ignorespaces ");
        self.back_list(&t);
    }

    /// `\encl{#1}` and `\cc{#1}`: `\par`, `\vskip\baselineskip` when `\parskip` is zero,
    /// `\noindent\begingroup\parbox[t]{\textwidth}{\@hangfrom{\strut` the description and
    /// separator `}\ignorespaces #1\strut}\setlength{\parfillskip}{\z@\@plus 1fil}\par
    /// \endgroup`.
    pub(super) fn lttr_encl(&mut self, var: &str) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if self.lttr().is_none() {
            return self.not_native("\\encl or \\cc outside scrlttr2");
        }
        let tw = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let zero = self.glue_param(super::super::engine_pack::PAR_SKIP).width == 0;
        let (name, sep) = (self.kname(var), self.kvar(var));
        let mut t = Vec::new();
        self.lt(&mut t, if zero { "\\par\\vskip\\baselineskip" } else { "\\par" });
        self.lt(&mut t, &format!("\\noindent\\begingroup\\parbox[t]{{{tw}sp}}{{"));
        if !name.is_empty() {
            self.lt(&mut t, "\\setbox32701\\hbox{{\\strut ");
            t.extend(name);
            t.extend(sep);
            self.lt(&mut t, "}}\\hangindent\\wd32701 \\noindent\\box32701 ");
        }
        self.lt(&mut t, "\\ignorespaces ");
        t.extend(arg);
        self.lt(&mut t, "\\strut}\\parfillskip0pt plus1fil\\par\\endgroup ");
        self.back_list(&t);
    }
}

/// scrlttr2's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("setkomavar", |m| m.set_koma_var()),
    ("setkomafont", |m| m.set_koma_font(false)),
    ("addtokomafont", |m| m.set_koma_font(true)),
    ("usekomavar", |m| m.use_koma_var()),
    ("usekomafont", |m| m.use_koma_font()),
    ("ps", |m| m.lttr_ps()),
    ("encl", |m| m.lttr_encl("enclseparator")),
    ("cc", |m| m.lttr_encl("ccseparator")),
    // hyperref's `\refstepcounter{letter}`: `\Hy@raisedlink{\hyper@anchorstart{letter.N}}`, in
    // vertical mode the anchor where it is.
    ("rt@lttranchor", |m| {
        let Some(n) = m.lttr().map(|l| l.letters) else { return };
        if m.doc.as_ref().is_some_and(|d| d.packages.contains(&"hyperref")) {
            m.vertical_anchor(format!("letter.{n}"));
        }
    }),
];

/// scrlttr2's commands in place of the kernel's (`engine_doc::class_hooks`).
pub(super) const HOOKS: super::ClassHooks = super::ClassHooks { setup: |m, opts| m.lttr_setup(opts), ..super::KERNEL_HOOKS };

/// The commands a scrlttr2 document may use natively: scrlttr2's own, and the kernel's text
/// commands it does not change (`2026-11-01-w9-scrlttr2.md`). Anything else stops.
pub(super) const LTTR_NATIVE: &[&str] = &[
    "documentclass", "usepackage", "begin", "end", "par", "setkomavar", "setkomafont", "addtokomafont", "usekomavar", "usekomafont", "opening", "closing", "ps", "encl", "cc",
    "newcommand", "renewcommand", "definecolor", "color", "textcolor", "today", "\\", "#", "&", "%", "$", "_", ",", "textbf", "textit", "textsl", "textsc", "textsf", "texttt", "textrm", "emph",
    "bfseries", "itshape", "slshape", "scshape", "sffamily", "rmfamily", "ttfamily", "mdseries", "upshape", "normalfont", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge",
    "ldots", "dots", "noindent", "LaTeX", "TeX", "quad", "qquad", "hfill", "nobreakspace", "enspace", "enskip", "centering", "raggedright", "raggedleft",
    "smallskip", "medskip", "bigskip", "vspace", "hspace", "underline", "mbox", "href", "url",
];
/// The environments a scrlttr2 document may use natively.
pub(super) const LTTR_ENVS: &[&str] = &["document", "letter", "flushright", "flushleft", "center"];
/// The packages taken under scrlttr2: they change nothing the native letter uses.
pub(super) const LTTR_PACKAGES: &[&str] = &["fontenc", "inputenc", "lmodern", "xcolor", "color", "babel", "microtype", "hyperref", "ragged2e"];
/// scrlttr2's commands, outside it the document's or undefined.
pub(super) const LTTR_ONLY: &[&str] = &["setkomavar", "setkomafont", "addtokomafont", "usekomavar", "usekomafont", "ps", "encl", "cc"];
