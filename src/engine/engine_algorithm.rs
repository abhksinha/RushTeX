//! algorithm.sty (2009/08/24 v0.1), algorithmicx.sty (2005/04/27 v1.2) and algpseudocode.sty
//! natively.
//!
//! algorithm.sty is float.sty's `\newfloat{algorithm}{htbp}{loa}[within]` in the style its
//! options name (`ruled` by default), run here as the preamble commands it is
//! (`engine_float`). algorithmicx's `algorithmic` is a `list` whose label, `\ALG@step`,
//! counts the lines and prints every `n`th number (`\alglinenumber`); its commands are
//! entities (`\ALG@makeentity`) of a block table: an entity may close the open block, end
//! it into a new one, or open one, a block lasting until an entity closes it or, with
//! finite credits (`\State`'s 0), until an entity it cannot take forces it shut
//! (`\ALG@closebyforce`). Each entity is an `\item` (`\item[]\nointerlineskip` when its text
//! is off, `\algtext*`), `\noindent\hskip\ALG@tlm`, then its text; a block opened after
//! that adds `\algorithmicindent` to `\ALG@tlm`. The table is algpseudocode's, kept here
//! with the text of each entity (its `\algnewcommand` keywords stay macros, so
//! `\algrenewcommand` and `\renewcommand` of them work); `\algdef`, `\algblock` and the
//! other table commands, `\algstore`, `\algrestore` and languages are not native, nor a
//! `\label` on a line (its `\@currentlabel` is the line number, which `predict` does not
//! count).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_prim::Meaning;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_scan::scan_dimen_ref;
use super::super::engine_token::Token;
use super::{Label, List, Make};
use super::{R_ITEMINDENT, R_ITEMSEP, R_LABELSEP, R_LABELWIDTH, R_LEFTMARGIN, R_LISTPARINDENT, R_PARSEP, R_PARTOPSEP, R_RIGHTMARGIN, R_TOPSEP};

/// The packages' natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@algentity", |m| m.alg_entity()),
    ("rt@algbreak", |m| m.alg_break()),
    ("rt@algnoint", |m| m.alg_nointerlineskip()),
    ("rt@algcall", |m| m.alg_call()),
    ("rt@algtext", |m| m.alg_text()),
    ("rt@algnotext", |m| m.alg_notext()),
    ("rt@algmacro", |m| m.not_native("algorithmicx: a command defined at the package's load")),
];

/// The blocks (`\ALG@createblock`): 0 is none (the BIG block, `\ALG@bl@`).
const STATE: u8 = 1;
const WHILE: u8 = 2;
const FOR: u8 = 3;
const LOOP: u8 = 4;
const REPEAT: u8 = 5;
const IF: u8 = 6;
const ELSE: u8 = 7;
const PROCEDURE: u8 = 8;
const FUNCTION: u8 = 9;

/// An entity: its command, its arguments and text (`\ALG@t@<L>@<entity>`, `#1`, `#2` its
/// arguments), and what it does from each block (`\ALG@b@<L>@<entity>@<block>`, the new
/// block or 0; `\ALG@c@`, its credits, `None` for infinite).
struct Entity {
    name: &'static str,
    args: u8,
    text: &'static str,
    from: &'static [(u8, u8, Option<u32>)],
}

/// algpseudocode.sty's entities after algorithmicx's `\State` (`\algnewlanguage`'s
/// `\algdef{SL}[STATE]{State}{0}{}`). `Procedure` and `Function` add `(#2)` unless it is
/// empty (`\ifthenelse{\equal{#2}{}}{}{(#2)}`).
const ENTITIES: &[Entity] = &[
    Entity { name: "State", args: 0, text: "", from: &[(0, STATE, Some(0))] },
    Entity { name: "While", args: 1, text: "\\algorithmicwhile\\ #1\\ \\algorithmicdo", from: &[(0, WHILE, None)] },
    Entity { name: "EndWhile", args: 0, text: "\\algorithmicend\\ \\algorithmicwhile", from: &[(WHILE, 0, None)] },
    Entity { name: "For", args: 1, text: "\\algorithmicfor\\ #1\\ \\algorithmicdo", from: &[(0, FOR, None)] },
    Entity { name: "ForAll", args: 1, text: "\\algorithmicforall\\ #1\\ \\algorithmicdo", from: &[(0, FOR, None)] },
    Entity { name: "EndFor", args: 0, text: "\\algorithmicend\\ \\algorithmicfor", from: &[(FOR, 0, None)] },
    Entity { name: "Loop", args: 0, text: "\\algorithmicloop", from: &[(0, LOOP, None)] },
    Entity { name: "EndLoop", args: 0, text: "\\algorithmicend\\ \\algorithmicloop", from: &[(LOOP, 0, None)] },
    Entity { name: "Repeat", args: 0, text: "\\algorithmicrepeat", from: &[(0, REPEAT, None)] },
    Entity { name: "Until", args: 1, text: "\\algorithmicuntil\\ #1", from: &[(REPEAT, 0, None)] },
    Entity { name: "If", args: 1, text: "\\algorithmicif\\ #1\\ \\algorithmicthen", from: &[(0, IF, None)] },
    Entity { name: "ElsIf", args: 1, text: "\\algorithmicelse\\ \\algorithmicif\\ #1\\ \\algorithmicthen", from: &[(IF, IF, None)] },
    Entity { name: "Else", args: 0, text: "\\algorithmicelse", from: &[(IF, ELSE, None)] },
    Entity { name: "EndIf", args: 0, text: "\\algorithmicend\\ \\algorithmicif", from: &[(IF, 0, None), (ELSE, 0, None)] },
    Entity { name: "Procedure", args: 2, text: "\\algorithmicprocedure\\ \\textproc{#1}", from: &[(0, PROCEDURE, None)] },
    Entity { name: "EndProcedure", args: 0, text: "\\algorithmicend\\ \\algorithmicprocedure", from: &[(PROCEDURE, 0, None)] },
    Entity { name: "Function", args: 2, text: "\\algorithmicfunction\\ \\textproc{#1}", from: &[(0, FUNCTION, None)] },
    Entity { name: "EndFunction", args: 0, text: "\\algorithmicend\\ \\algorithmicfunction", from: &[(FUNCTION, 0, None)] },
];

/// The entities `\algtext*` turns off with algpseudocode's `noend` option.
const NOEND: [&str; 6] = ["EndWhile", "EndFor", "EndLoop", "EndIf", "EndProcedure", "EndFunction"];

/// algorithmicx's state: the open `algorithmic` (none nests here), its blocks (`\theALG@nested`
/// levels: the block, its credits, its indent; the first the BIG block), `\ALG@tlm`, the
/// counters `ALG@line` and `ALG@rem` and `\ALG@numberfreq` (0: no numbers); the entities whose
/// text is off (by bit); the entities algpseudocode defined (else algorithmicx's `State`
/// alone, its default language).
#[derive(Default, Clone, PartialEq)]
pub(super) struct Alg {
    open: bool,
    levels: Vec<(u8, Option<u32>, i32)>,
    tlm: i32,
    line: i32,
    rem: i32,
    freq: i32,
    notext: u32,
    pseudocode: bool,
}

/// algorithm.sty's options: the float style and the counter it numbers within; the
/// `\DeclareOption*` name (`\ALG@name`).
pub(super) fn algorithm_options(opts: &str) -> Result<(&'static str, Option<&'static str>, String), &'static str> {
    let (mut style, mut within, mut name) = ("ruled", None, "Algorithm".to_string());
    for o in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        match o {
            "plain" => style = "plain",
            "ruled" => style = "ruled",
            "boxed" => style = "boxed",
            "chapter" => within = Some("chapter"),
            "section" => within = Some("section"),
            "subsection" => within = Some("subsection"),
            "subsubsection" => within = Some("subsubsection"),
            "nothing" => within = None,
            "part" => return Err("algorithm: the option part"),
            o if o.bytes().all(|c| c.is_ascii_alphabetic()) => name = o.to_string(),
            _ => return Err("algorithm: an option of this form"),
        }
    }
    Ok((style, within, name))
}

/// The entity named `name` (its bit in `Alg::notext`).
pub(super) fn entity_index(name: &str) -> Option<usize> {
    ENTITIES.iter().position(|e| e.name == name)
}

/// The entities algpseudocode's `noend` turns off.
pub(super) fn noend_bits() -> u32 {
    ENTITIES.iter().enumerate().filter(|(_, e)| NOEND.contains(&e.name)).fold(0, |b, (k, _)| b | 1 << k)
}

/// `\algtext*{entity}` or `\algnotext{entity}` in source text `s` (after the name): the
/// entity; `None` for another form (`\algtext{entity}{text}`, a block).
pub(super) fn notext_arg(s: &str, algtext: bool) -> Option<usize> {
    let s = s.trim_start();
    let s = match s.strip_prefix('*') {
        Some(r) => r.trim_start(),
        None if algtext => return None,
        None => s,
    };
    let (name, _) = s.strip_prefix('{')?.split_once('}')?;
    entity_index(name.trim())
}

/// algpseudocode's options: `noend` (true), `end`; `compatible` loads algcompatible.
pub(super) fn pseudocode_options(opts: &str) -> Result<bool, &'static str> {
    let mut noend = false;
    for o in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
        match o {
            "noend" => noend = true,
            "end" => noend = false,
            "noncompatible" => {}
            _ => return Err("algpseudocode: this option"),
        }
    }
    Ok(noend)
}

impl Machine<'_, '_> {
    /// `\usepackage[opts]{algorithm}`: float.sty and ifthen, then `\floatstyle{style}
    /// \newfloat{algorithm}{htbp}{loa}[within]`, `\floatname{algorithm}{\ALG@name}` and
    /// `\listalgorithmname` (`List of \ALG@name s`).
    pub(super) fn algorithm_load(&mut self, opts: &str) {
        let (style, within, name) = match algorithm_options(opts) {
            Ok(o) => o,
            Err(what) => return self.not_native(what),
        };
        self.load_package("float");
        self.load_package("ifthen");
        self.load_package("algorithm");
        let within = within.map_or(String::new(), |w| format!("[{w}]"));
        let t = self.tex_tokens(&format!("\\floatstyle{{{style}}}\\newfloat{{algorithm}}{{htbp}}{{loa}}{within}\\floatname{{algorithm}}{{{name}}}\\long\\def\\listalgorithmname{{List of {name}s}}"));
        self.back_list(&t);
    }

    /// `\usepackage{algorithmicx}` (and algpseudocode's, `pseudocode`): ifthen, then the
    /// default language's macros (`\algnewlanguage`: `\algorithmiccomment`,
    /// `\algorithmicindent`, `\alglinenumber`) and the user commands (`\Statex`,
    /// `\Comment`, `\algrenewcommand` as `\renewcommand`: the keywords are plain macros
    /// here); algpseudocode's keywords and commands after it.
    pub(super) fn algorithmicx_load(&mut self, pseudocode: Option<&str>) {
        let noend = match pseudocode.map(pseudocode_options) {
            Some(Err(what)) => return self.not_native(what),
            Some(Ok(n)) => Some(n),
            None => None,
        };
        let first = !self.doc.as_ref().is_some_and(|d| d.packages.contains(&"algorithmicx"));
        self.load_package("ifthen");
        self.load_package("algorithmicx");
        let entity = self.e.cs.intern("rtalgentity");
        if let Some(i) = super::DOC.iter().position(|d| d.0 == "rt@algentity") {
            self.e.eqtb.set_meaning(entity, Meaning::Doc(i as u16), true);
        }
        let mut text = String::new();
        if first {
            text.push_str("\\long\\def\\algorithmiccomment#1{\\hfill\\(\\triangleright\\) #1}\\def\\algorithmicindent{1.5em}\\long\\def\\alglinenumber#1{\\footnotesize #1:}\\def\\Statex{\\item[]}\\def\\Comment{\\algorithmiccomment}\\def\\algrenewcomment{\\algrenewcommand\\algorithmiccomment}\\let\\algrenewcommand\\renewcommand\\let\\algnewcommand\\newcommand\\def\\State{\\rtalgentity{0}}");
        }
        if let Some(noend) = noend {
            if self.doc.as_ref().is_some_and(|d| d.pkg.alg.pseudocode) {
                return;
            }
            self.load_package("algpseudocode");
            for (k, e) in ENTITIES.iter().enumerate().skip(1) {
                text.push_str(&format!("\\def\\{}{{\\rtalgentity{{{k}}}}}", e.name));
            }
            for (k, w) in ["end", "do", "while", "for", "for all", "loop", "repeat", "until", "procedure", "function", "if", "then", "else", "Require:", "Ensure:", "return"].iter().enumerate() {
                let n = ["end", "do", "while", "for", "forall", "loop", "repeat", "until", "procedure", "function", "if", "then", "else", "require", "ensure", "return"][k];
                text.push_str(&format!("\\def\\algorithmic{n}{{\\textbf{{{w}}}}}"));
            }
            text.push_str("\\def\\textproc{\\textsc}\\def\\Require{\\item[\\algorithmicrequire]}\\def\\Ensure{\\item[\\algorithmicensure]}\\def\\Return{\\algorithmicreturn{} }");
            if let Some(d) = self.doc.as_mut() {
                d.pkg.alg.pseudocode = true;
                if noend {
                    for (k, e) in ENTITIES.iter().enumerate() {
                        if NOEND.contains(&e.name) {
                            d.pkg.alg.notext |= 1 << k;
                        }
                    }
                }
            }
        }
        let t = self.tex_tokens(&text);
        self.back_list(&t);
    }

    /// `\begin{algorithmic}[n]` after `\begin`'s group: `\ALG@numberfreq`, the counters
    /// zeroed, `\let\\\algbreak`, then `\begin{list}{\ALG@step}{..}` with algorithmicx's
    /// lengths (the list's group is the environment's here: nothing between the two is
    /// local).
    pub(super) fn alg_begin(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.pkg.alg.open) {
            return self.not_native("algorithmicx: an algorithmic in an algorithmic");
        }
        let freq = if self.next_is('[') {
            let arg = self.bracket_arg();
            let s: String = arg.iter().map(|t| t.chr()).collect();
            match s.trim().parse::<i32>() {
                Ok(n) if n >= 0 && arg.iter().all(|t| t.cs_id().is_none()) => n,
                _ => return self.not_native("algorithmicx: a line number step that is not a number"),
            }
        } else {
            0
        };
        if let Some(d) = self.doc.as_mut() {
            let a = &mut d.pkg.alg;
            (a.open, a.levels, a.tlm, a.line, a.rem, a.freq) = (true, vec![(0, None, 0)], 0, 0, 0, freq);
        }
        let bs = self.e.cs.intern("\\");
        if let Some(i) = super::DOC.iter().position(|d| d.0 == "rt@algbreak") {
            self.e.eqtb.set_meaning(bs, Meaning::Doc(i as u16), false);
        }
        let list = List { label: Label::Alg, make: Make::Mklab, numbered: false, counter: 0, centercr: false, auto_item: false, endpe_false: false };
        if !self.list_open(list) {
            return;
        }
        // The lengths in the font's `em` as TeX scans them (`1.2em` is not `em + em/5`).
        let (em, ex) = self.em_ex();
        let len = |t: &str| scan_dimen_ref(t, em, ex, &|_| None).unwrap_or(0);
        for r in [R_RIGHTMARGIN, R_ITEMINDENT] {
            self.set_rw(r, 0, false);
        }
        for r in [R_ITEMSEP, R_PARTOPSEP, R_PARSEP] {
            self.set_rg(r, Glue::default());
        }
        self.set_rw(R_LISTPARINDENT, len("2em"), false);
        self.set_glue_named(param!("parskip"), Glue::default());
        let (sep, width) = (len("0.5em"), len(if freq == 0 { "0.5em" } else { "1.2em" }));
        self.set_rw(R_LABELSEP, sep, false);
        self.set_rg(R_TOPSEP, Glue { width: len("0.2em"), ..Glue::default() });
        self.set_rw(R_LABELWIDTH, width, false);
        self.set_rw(R_LEFTMARGIN, width + sep, false);
        self.trivlist(|m| m.list_tail());
    }

    /// `\end{algorithmic}`: `\ALG@closeloops`, an open block is an error, then `\end{list}`.
    pub(super) fn alg_end(&mut self) {
        let Some(d) = self.doc.as_mut() else { return };
        let a = &mut d.pkg.alg;
        while a.levels.len() > 1 && a.levels.last().is_some_and(|l| l.1.is_some()) {
            a.levels.pop();
        }
        let open = a.levels.len() > 1;
        a.open = false;
        if open {
            return self.not_native("algorithmicx: a block not closed");
        }
        self.end_list();
    }

    /// `\ALG@step`, the label of an `\item` without one: `ALG@line` and `ALG@rem` stepped,
    /// and `\alglinenumber{\arabic{ALG@line}}` when `ALG@rem` reaches `\ALG@numberfreq`.
    pub(super) fn alg_step(&mut self, t: &mut Vec<Token>) {
        let Some(d) = self.doc.as_mut() else { return };
        let a = &mut d.pkg.alg;
        a.line += 1;
        a.rem += 1;
        if a.rem != a.freq {
            return;
        }
        a.rem = 0;
        let n = a.line;
        t.push(Token::cs(self.e.cs.intern("alglinenumber")));
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(n.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.push(Token::char(Cat::EndGroup, '}'));
    }

    /// An open `algorithmic`.
    pub(super) fn alg_open(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pkg.alg.open)
    }

    /// `\theALG@line`.
    pub(super) fn alg_line(&self) -> i32 {
        self.doc.as_ref().map_or(0, |d| d.pkg.alg.line)
    }

    /// `\algbreak` (`\\` in an `algorithmic`): `\item`.
    fn alg_break(&mut self) {
        self.item();
    }

    /// `\nointerlineskip` after an entity's `\item[]`: `\prevdepth-\@m\p@` in vertical mode.
    fn alg_nointerlineskip(&mut self) {
        if self.mode == Mode::Vertical {
            self.aux = -1000 * 65536;
        }
    }

    /// `\algorithmicindent` as `\ALG@beginblock` reads it (`\setlength`): its text a length.
    fn alg_indent(&mut self) -> Option<i32> {
        let id = self.e.cs.intern("algorithmicindent");
        let s = self.macro_body(id)?;
        let (em, ex) = self.em_ex();
        scan_dimen_ref(s.trim(), em, ex, &|_| None)
    }

    /// `\ALG@makeentity{name}` (`\rtalgentity{k}`): the blocks it closes by force, then
    /// `\ALG@doentity`.
    fn alg_entity(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let k: usize = arg.iter().map(|t| t.chr()).collect::<String>().parse().unwrap_or(0);
        if !self.alg_open() || self.mode == Mode::Math {
            return self.not_native("algorithmicx: an entity outside an algorithmic");
        }
        let e = &ENTITIES[k];
        let trans = |b: u8| e.from.iter().find(|f| f.0 == b).copied();
        let begins = trans(0).is_some();
        // `\ALG@makebegin` and `\ALG@makenobegin`.
        loop {
            let Some(d) = self.doc.as_mut() else { return };
            let a = &mut d.pkg.alg;
            let Some(top) = a.levels.last_mut() else { return };
            let go = match top.1 {
                None => true,
                Some(n) if begins && n > 0 => {
                    if n <= 65534 {
                        top.1 = Some(n - 1);
                    }
                    true
                }
                _ => trans(top.0).is_some(),
            };
            if go {
                break;
            }
            // `\ALG@closebyforce`.
            let (_, _, ind) = a.levels.pop().expect("a block");
            a.tlm -= ind;
        }
        // `\ALG@doentity`.
        let mut args = Vec::new();
        for _ in 0..e.args {
            let mut a = Vec::new();
            self.read_arg(&mut a);
            args.push(a);
        }
        let Some(d) = self.doc.as_mut() else { return };
        let a = &mut d.pkg.alg;
        let cur = a.levels.last().map_or(0, |l| l.0);
        let this = if trans(cur).is_some() { cur } else { 0 };
        let Some((_, to, credits)) = trans(this) else { return self.not_native("algorithmicx: an entity with no block to close") };
        if this != 0 {
            let (_, _, ind) = a.levels.pop().expect("a block");
            a.tlm -= ind;
        }
        let notext = a.notext & 1 << k != 0;
        let tlm = a.tlm;
        if to != 0 {
            let Some(ind) = self.alg_indent() else { return self.not_native("algorithmicx: an \\algorithmicindent that is not a length") };
            let Some(d) = self.doc.as_mut() else { return };
            let a = &mut d.pkg.alg;
            a.tlm += ind;
            a.levels.push((to, credits, ind));
        }
        let mut t = Vec::new();
        if notext {
            t.push(self.doc_id(doc!("rt@algnoint")));
        }
        t.push(self.named_cs(named!("noindent")));
        t.extend(self.tex_tokens(&format!("\\hskip{tlm}sp")));
        // `\hskip` reads on for `plus`: `\relax` ends the glue as the macro's next token does.
        t.push(self.named_cs(named!("relax")));
        if !notext {
            t.extend(self.alg_text_tokens(e, &args));
        }
        self.back_list(&t);
        if let Some(d) = self.doc.as_mut() {
            d.noitemarg = !notext;
        }
        self.item_start(if notext { Some(Vec::new()) } else { None });
    }

    /// An entity's text with its arguments.
    fn alg_text_tokens(&mut self, e: &Entity, args: &[Vec<Token>]) -> Vec<Token> {
        let mut out = Vec::new();
        for t in self.tex_tokens(e.text) {
            out.push(t);
            if out.len() >= 2 && out[out.len() - 2].cat() == Some(Cat::Param) {
                let n = out.pop().map_or(0, |t| t.chr() as usize - '0' as usize);
                out.pop();
                out.extend_from_slice(&args[n - 1]);
            }
        }
        if e.args == 2 && !args[1].is_empty() {
            out.push(Token::char(Cat::Other, '('));
            out.extend_from_slice(&args[1]);
            out.push(Token::char(Cat::Other, ')'));
        }
        out
    }

    /// algpseudocode's `\Call{name}{args}`: `\textproc{#1}`, and `(#2)` unless it is empty.
    fn alg_call(&mut self) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut b);
        let mut t = vec![Token::cs(self.e.cs.intern("textproc")), Token::char(Cat::BeginGroup, '{')];
        t.extend(a);
        t.push(Token::char(Cat::EndGroup, '}'));
        if !b.is_empty() {
            t.push(Token::char(Cat::Other, '('));
            t.extend(b);
            t.push(Token::char(Cat::Other, ')'));
        }
        self.back_list(&t);
    }

    /// `\algtext*{entity}` (`\algnotext`); `\algtext{entity}{text}` (`\algrenewtext`) is not
    /// native.
    fn alg_text(&mut self) {
        if !self.next_is('*') {
            return self.not_native("algorithmicx: \\algtext of a new text");
        }
        self.alg_notext();
    }

    /// `\algnotext{entity}`: its text off (`\ALG@x@notext`), in every block.
    fn alg_notext(&mut self) {
        if self.next_is('[') {
            return self.not_native("algorithmicx: \\algnotext of a block");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let Some(k) = ENTITIES.iter().position(|e| e.name == name.trim()) else { return self.not_native("algorithmicx: \\algnotext of this entity") };
        if k > 0 && !self.doc.as_ref().is_some_and(|d| d.pkg.alg.pseudocode) {
            return self.not_native("algorithmicx: \\algnotext of this entity");
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.alg.notext |= 1 << k;
        }
    }
}
