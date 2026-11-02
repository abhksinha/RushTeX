//! The preamble programming real documents write, natively (latex.ltx's ltdefns.dtx):
//! `\newcommand` and `\newenvironment` with an optional argument's default (`\@xargdef`).
//! The document's own macros stay tokens for the engine's `\def` (GDL R1's exception); the
//! natives here only build what the kernel's definers build around them.

use super::super::engine_catcode::Cat;
use super::super::engine_prim::{Macro, Meaning};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;

/// The lists' names a document may redefine (`own_head_name`).
pub(super) const OWN_NAMES: &[&str] = &["contentsname", "listfigurename", "listtablename", "bibname", "chaptername", "appendixname"];

/// The natives of this file in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@testopt", |m| m.test_opt()), ("newif", |m| m.new_if()), ("AtBeginDocument", |m| m.at_begin_document()), ("rt@ifthenelse", |m| m.if_then_else()), ("rt@newboolean", |m| m.new_boolean(false)), ("rt@provideboolean", |m| m.new_boolean(true)), ("rt@setboolean", |m| m.set_boolean()), ("rt@ifthenword", |m| m.not_native("an ifthen test outside \\ifthenelse")), ("rt@etbunseen", |m| m.etb_unseen())];

/// The kernel macros an etoolbox `\preto`/`\appto` may patch where the document never runs
/// them, and what in the source would run them: `\@verbatim`, which only the `verbatim`
/// environments (latex.ltx, verbatim.sty, moreverb) and `\verbatiminput` start.
const UNSEEN_TARGETS: &[(&str, &[&str])] = &[("@verbatim", &["\\begin{verbatim", "verbatiminput", "\\begin{verbatimtab", "\\begin{listinginput", "\\begin{boxedverbatim"])];

/// The pre-scan's check of etoolbox's `\preto`, `\appto`, `\gpreto`, `\gappto` at the start of
/// `rest` (after the name), `src` the whole source: `{\target}{code}` of an
/// [`UNSEEN_TARGETS`] macro nothing in `src` runs, so the patch changes nothing a page shows.
/// The bytes taken (the code unread), `None` otherwise.
pub(super) fn etb_unseen_patch(src: &str, rest: &str) -> Option<usize> {
    let t = rest.trim_start();
    let (target, r) = match t.strip_prefix('{') {
        Some(b) => b.trim_start().strip_prefix('\\')?.split_once('}')?,
        None => {
            let b = t.strip_prefix('\\')?;
            let n = b.bytes().take_while(|c| c.is_ascii_alphabetic() || *c == b'@').count();
            (&b[..n], &b[n..])
        }
    };
    let (_, users) = UNSEEN_TARGETS.iter().find(|(name, _)| *name == target.trim_end())?;
    if users.iter().any(|u| src.contains(u)) {
        return None;
    }
    let body = r.trim_start().strip_prefix('{')?;
    let mut depth = 1usize;
    let end = body.bytes().position(|c| {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        depth == 0
    })?;
    Some(rest.len() - body[end + 1..].len())
}

/// One step of an `\ifthenelse` test as ifthen.sty evaluates it: an atom's value, `\NOT`
/// before the next atom, `\AND` or `\OR` before it.
#[derive(Clone, Copy, PartialEq)]
enum TestStep {
    Atom(bool),
    Not,
    And,
    Or,
}

impl Machine<'_, '_> {
    /// ifthen's `\newboolean{name}` (`\@ifdefinable\csname ifname\endcsname{\newif..}`) and
    /// `\provideboolean{name}` (nothing when `\ifname` is defined): `\newif\ifname` pushed,
    /// [`Self::new_if`]'s. A name the kernel or a package has, or one defined before, goes
    /// to the engine for `\newboolean` (`\@ifdefinable`'s error).
    /// etoolbox's `\preto{\cs}{code}` and kin (`\appto`, `\gpreto`, `\gappto`) of an
    /// [`UNSEEN_TARGETS`] macro, which the pre-scan took only where nothing runs it
    /// (`etb_unseen_patch`): read, nothing changed. Another target goes to the engine.
    fn etb_unseen(&mut self) {
        let (mut target, mut code) = (Vec::new(), Vec::new());
        self.read_arg(&mut target);
        self.read_arg(&mut code);
        let ok = matches!(target[..], [t] if t.cs_id().is_some_and(|id| UNSEEN_TARGETS.iter().any(|(n, _)| *n == self.e.cs.name(id))));
        if !ok {
            self.not_native("etoolbox: a patch of a macro the document may run");
        }
    }

    fn new_boolean(&mut self, provide: bool) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if arg.is_empty() || arg.iter().any(|t| t.cat() != Some(Cat::Letter)) {
            return self.not_native("ifthen's boolean of a name not all letters");
        }
        let name: String = arg.iter().map(|t| t.chr()).collect();
        let cs = format!("if{name}");
        let id = self.e.cs.intern(&cs);
        let fresh = super::table_names(&cs) == 0 && super::scan_list(&cs) == 0 && matches!(self.e.eqtb.meaning(id), Meaning::Unset | Meaning::Undefined);
        if !fresh {
            if provide && super::table_names(&cs) == 0 && super::scan_list(&cs) == 0 {
                return;
            }
            return self.not_native("ifthen's boolean of a name defined before");
        }
        let newif = self.e.cs.intern("newif");
        self.back_list(&[Token::cs(newif), Token::cs(id)]);
    }

    /// ifthen's `\setboolean{name}{value}`: `\lowercase` of the value `true` or `false`, then
    /// `\csname name<value>\endcsname`, the `\newif`'s switch. Another value or an undefined
    /// boolean is ifthen's error: the engine's.
    fn set_boolean(&mut self) {
        let (mut name, mut value) = (Vec::new(), Vec::new());
        self.read_arg(&mut name);
        self.read_arg(&mut value);
        if name.iter().chain(&value).any(|t| t.cs_id().is_some()) {
            return self.not_native("ifthen's \\setboolean of a command");
        }
        let value: String = value.iter().map(|t| t.chr().to_ascii_lowercase()).collect();
        let name: String = name.iter().map(|t| t.chr()).collect();
        let Some(id) = self.e.cs.get(&format!("{name}{value}")).filter(|_| matches!(value.as_str(), "true" | "false")) else { return self.not_native("ifthen's \\setboolean of an undefined boolean or value") };
        if !matches!(self.e.eqtb.meaning(id), Meaning::Macro(_)) {
            return self.not_native("ifthen's \\setboolean of an undefined boolean");
        }
        self.back_list(&[Token::cs(id)]);
    }

    /// ifthen's `\ifthenelse{test}{then}{else}`: the test evaluated as ifthen.sty's `\xdef`
    /// of `\TE@eval` chains it, left to right (`\AND` keeps a true value to the next atom,
    /// `\OR` a false one, `\NOT` negates the next atom), then `\@firstoftwo` or
    /// `\@secondoftwo` of the branches. Atoms: `\boolean{name}` (the `\newif`'s state),
    /// `\equal{a}{b}` (both read as the document's macros without parameters expand to
    /// characters, compared with their categories as `\ifx` compares the `\def`s), `\isodd
    /// {n}` and `n<m`, `n=m`, `n>m` of integers and `\value{counter}` (`counter_value`).
    /// `\(`, `\)`, `\lengthtest`, `\isundefined` and anything else go to the engine.
    fn if_then_else(&mut self) {
        let (mut test, mut yes, mut no) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut test);
        self.read_arg(&mut yes);
        self.read_arg(&mut no);
        let Some(steps) = self.ifthen_steps(&test) else { return self.not_native("an \\ifthenelse test the native job does not read") };
        let (mut val, mut negate, mut op) = (false, false, None);
        for s in steps {
            match s {
                TestStep::Not => negate = !negate,
                TestStep::And | TestStep::Or => op = Some(s),
                TestStep::Atom(a) => {
                    let a = a != negate;
                    negate = false;
                    val = match op.take() {
                        None => a,
                        Some(TestStep::And) => val && a,
                        _ => val || a,
                    };
                }
            }
        }
        self.back_list(if val { &yes } else { &no });
    }

    /// [`Self::if_then_else`]'s test as steps; `None` for a form it does not read.
    fn ifthen_steps(&mut self, test: &[Token]) -> Option<Vec<TestStep>> {
        let names = ["boolean", "equal", "isodd", "value", "NOT", "not", "AND", "and", "OR", "or"].map(|n| self.e.cs.intern(n));
        let (iftrue, iffalse) = (self.e.cs.intern("iftrue"), self.e.cs.intern("iffalse"));
        let mut steps = Vec::new();
        let mut i = 0;
        // A braced group at `i` (spaces before it skipped, as an argument is read).
        let group = |i: &mut usize| -> Option<Vec<Token>> {
            while test.get(*i).is_some_and(|t| t.cat() == Some(Cat::Space)) {
                *i += 1;
            }
            if test.get(*i)?.cat() != Some(Cat::BeginGroup) {
                return None;
            }
            let mut depth = 0;
            let start = *i + 1;
            while let Some(t) = test.get(*i) {
                match t.cat() {
                    Some(Cat::BeginGroup) => depth += 1,
                    Some(Cat::EndGroup) => {
                        depth -= 1;
                        if depth == 0 {
                            *i += 1;
                            return Some(test[start..*i - 1].to_vec());
                        }
                    }
                    _ => {}
                }
                *i += 1;
            }
            None
        };
        // An integer in `list` at `i`: digits after an optional sign, or `\value{counter}`.
        fn number(m: &Machine<'_, '_>, list: &[Token], value: u32, i: &mut usize) -> Option<i64> {
            while list.get(*i).is_some_and(|t| t.cat() == Some(Cat::Space)) {
                *i += 1;
            }
            if list.get(*i)?.cs_id() == Some(value) {
                *i += 1;
                while list.get(*i).is_some_and(|t| t.cat() == Some(Cat::Space)) {
                    *i += 1;
                }
                if list.get(*i)?.cat() != Some(Cat::BeginGroup) {
                    return None;
                }
                let end = *i + list[*i..].iter().position(|t| t.cat() == Some(Cat::EndGroup))?;
                let name = &list[*i + 1..end];
                *i = end + 1;
                return m.counter_value(name).map(i64::from);
            }
            let neg = list[*i] == Token::char(Cat::Other, '-');
            if neg {
                *i += 1;
            }
            let start = *i;
            while list.get(*i).is_some_and(|t| t.cat() == Some(Cat::Other) && t.chr().is_ascii_digit()) {
                *i += 1;
            }
            let digits: String = list[start..*i].iter().map(|t| t.chr()).collect();
            let n: i64 = digits.parse().ok().filter(|n| *n <= i64::from(i32::MAX))?;
            Some(if neg { -n } else { n })
        }
        while i < test.len() {
            let t = test[i];
            if t.cat() == Some(Cat::Space) {
                i += 1;
                continue;
            }
            let at = |k: usize| t.cs_id() == Some(names[k]);
            if at(4) || at(5) {
                i += 1;
                steps.push(TestStep::Not);
            } else if at(6) || at(7) || at(8) || at(9) {
                i += 1;
                steps.push(if at(6) || at(7) { TestStep::And } else { TestStep::Or });
            } else if at(0) {
                i += 1;
                let name = group(&mut i)?;
                if name.iter().any(|t| t.cs_id().is_some()) {
                    return None;
                }
                let s: String = name.iter().map(|t| t.chr()).collect();
                let id = self.e.cs.get(&format!("if{s}"))?;
                let m = self.e.eqtb.meaning(id);
                let value = if m == self.e.eqtb.meaning(iftrue) {
                    true
                } else if m == self.e.eqtb.meaning(iffalse) {
                    false
                } else {
                    return None;
                };
                steps.push(TestStep::Atom(value));
            } else if at(1) {
                i += 1;
                let (a, b) = (group(&mut i)?, group(&mut i)?);
                let (a, b) = (self.plain_expansion(&a, 0)?, self.plain_expansion(&b, 0)?);
                steps.push(TestStep::Atom(a == b));
            } else if at(2) {
                i += 1;
                // `\TE@odd`: `\count@` set to the argument, then `\ifodd`.
                let arg = group(&mut i)?;
                let mut k = 0;
                let n = number(self, &arg, names[3], &mut k)?;
                if arg[k..].iter().any(|t| t.cat() != Some(Cat::Space)) {
                    return None;
                }
                steps.push(TestStep::Atom(n % 2 != 0));
            } else {
                let a = number(self, test, names[3], &mut i)?;
                while test.get(i).is_some_and(|t| t.cat() == Some(Cat::Space)) {
                    i += 1;
                }
                let rel = test.get(i).filter(|t| t.cat() == Some(Cat::Other)).map(|t| t.chr())?;
                i += 1;
                let b = number(self, test, names[3], &mut i)?;
                steps.push(TestStep::Atom(match rel {
                    '<' => a < b,
                    '=' => a == b,
                    '>' => a > b,
                    _ => return None,
                }));
            }
        }
        // ifthen's chain reads atoms joined by `\AND`/`\OR`, each after its `\NOT`s.
        let mut want_atom = true;
        for s in &steps {
            match s {
                TestStep::Not if want_atom => {}
                TestStep::Atom(_) if want_atom => want_atom = false,
                TestStep::And | TestStep::Or if !want_atom => want_atom = true,
                _ => return None,
            }
        }
        (!want_atom).then_some(steps)
    }

    /// The characters `\protected@edef` makes of `list` when it holds only characters and the
    /// document's macros without parameters that expand to characters (with their
    /// categories, as `\ifx` compares two such `\def`s); `None` for anything else.
    fn plain_expansion(&self, list: &[Token], depth: u8) -> Option<Vec<Token>> {
        let mut out = Vec::new();
        for &t in list {
            match t.cs_id() {
                Some(id) if depth < 16 => match self.e.eqtb.meaning(id) {
                    Meaning::Macro(m) => match self.e.toks.get(m.text) {
                        [Token::END_MATCH, rest @ ..] => {
                            let rest = rest.to_vec();
                            out.extend(self.plain_expansion(&rest, depth + 1)?);
                        }
                        _ => return None,
                    },
                    _ => return None,
                },
                Some(_) => return None,
                None if matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) => out.push(t),
                None => return None,
            }
        }
        Some(out)
    }
    /// `\AtBeginDocument{code}` (`\AddToHook{begindocument}`): in the preamble the code joins
    /// the document's (label `top-level`, which lthooks runs after every package's), run by
    /// `\document` after the hooks the native job models ([`Self::run_begin_hook`]); in the
    /// body the one-time hook is spent and the code runs at once. `\document` sets
    /// `\@maxdepth` from `\maxdepth` and checks `\topskip` after the hook, where the native
    /// job has read them already: code naming either goes to the engine.
    fn at_begin_document(&mut self) {
        let mut code = Vec::new();
        self.read_arg(&mut code);
        let (maxdepth, topskip) = (self.e.cs.intern("maxdepth"), self.e.cs.intern("topskip"));
        if code.iter().any(|t| t.cs_id().is_some_and(|id| id == maxdepth || id == topskip)) {
            return self.not_native("\\AtBeginDocument with \\maxdepth or \\topskip");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.body {
            return self.back_list(&code);
        }
        doc.pkg.begin_hook.extend(code);
    }

    /// `\document`'s `\UseOneTimeHook{begindocument}` for the document's code, then
    /// `\ignorespaces`; `false` when there is none.
    pub(super) fn run_begin_hook(&mut self) -> bool {
        let Some(mut code) = self.doc.as_mut().map(|d| std::mem::take(&mut d.pkg.begin_hook)).filter(|c| !c.is_empty()) else { return false };
        code.push(self.named_cs(named!("ignorespaces")));
        self.back_list(&code);
        true
    }

    /// `\newif\ifname` (latex.ltx): `\let\ifname\iffalse`, `\def\nametrue{\let\ifname\iftrue}`,
    /// `\def\namefalse{\let\ifname\iffalse}`, all local (`\@if`; the `\escapechar` change
    /// only builds the names). A name not starting `if` is `\@gobbletwo`'s and goes to the
    /// engine.
    fn new_if(&mut self) {
        let Some(id) = self.get_token().and_then(|t| t.cs_id()) else { return self.not_native("\\newif's name") };
        let Some(base) = self.e.cs.name(id).strip_prefix("if").filter(|b| !b.is_empty()).map(str::to_owned) else { return self.not_native("\\newif's name") };
        let (let_, iftrue, iffalse) = (self.e.cs.intern("let"), self.e.cs.intern("iftrue"), self.e.cs.intern("iffalse"));
        let m = self.e.eqtb.meaning(iffalse);
        self.e.eqtb.set_meaning(id, m, false);
        for (suffix, value) in [("true", iftrue), ("false", iffalse)] {
            let t = [Token::END_MATCH, Token::cs(let_), Token::cs(id), Token::cs(value)];
            let text = self.e.toks.store(&t);
            let name = self.e.cs.intern(&format!("{base}{suffix}"));
            self.e.eqtb.set_meaning(name, Meaning::Macro(Macro { text, flags: 0 }), false);
        }
    }

    /// The default of `\newcommand\name[n][default]`, read as `\@xargdef`'s `[#3]`: to the
    /// first `]` outside braces, one pair of braces around the whole stripped (§392).
    pub(super) fn read_default(&mut self) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0u32;
        while let Some(t) = self.get_token() {
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth = depth.saturating_sub(1),
                _ if depth == 0 && t == Token::char(Cat::Other, ']') => break,
                _ => {}
            }
            out.push(t);
        }
        if out.len() >= 2 && out[0].cat() == Some(Cat::BeginGroup) && out[out.len() - 1].cat() == Some(Cat::EndGroup) {
            // Stripped only when the first `{` closes at the end.
            let mut d = 0i32;
            let whole = out.iter().enumerate().all(|(k, t)| {
                match t.cat() {
                    Some(Cat::BeginGroup) => d += 1,
                    Some(Cat::EndGroup) => d -= 1,
                    _ => {}
                }
                d > 0 || k == out.len() - 1
            });
            if whole {
                out.pop();
                out.remove(0);
            }
        }
        out
    }

    /// `\@xargdef`'s outer macro: `\name` is `\def`ined (not `\long`, locally) to
    /// `\@protected@testopt\name\\name{default}`, and the control sequence `\\name` (`\csname
    /// \string\name\endcsname`) is returned for the caller's `\long\def\\name[#1]#2..`
    /// (`\@yargdef` with `\tw@`). A native document's `\protect` is `\relax`, which is
    /// `\@typeset@protect`, so `\@protected@testopt` is always `\@testopt` ([`Self::test_opt`]).
    pub(super) fn default_arg_macro(&mut self, cs: Token, default: &[Token]) -> Option<Token> {
        let id = cs.cs_id()?;
        let inner = Token::cs(self.e.cs.intern(&format!("\\{}", self.e.cs.name(id))));
        let mut t = Vec::with_capacity(default.len() + 5);
        t.extend([Token::END_MATCH, self.doc_id(doc!("rt@testopt")), inner, Token::char(Cat::BeginGroup, '{')]);
        t.extend_from_slice(default);
        t.push(Token::char(Cat::EndGroup, '}'));
        let text = self.e.toks.store(&t);
        self.e.eqtb.set_meaning(id, Meaning::Macro(Macro { text, flags: 0 }), false);
        Some(inner)
    }

    /// Sets `\@title`, `\@author` or `\@date` (`i`) to a macro of `text` (global, as
    /// `\title`'s `\gdef\@title{#1}`), or leaves it undefined, so that a document reading it
    /// goes to the engine.
    fn set_title_macro(&mut self, i: usize, text: Option<&[Token]>) {
        let id = self.e.cs.intern(["@title", "@author", "@date"][i]);
        let m = match text {
            Some(text) => {
                let mut t = Vec::with_capacity(text.len() + 1);
                t.push(Token::END_MATCH);
                t.extend_from_slice(text);
                Meaning::Macro(Macro { text: self.e.toks.store(&t), flags: 0 })
            }
            None => Meaning::Undefined,
        };
        self.e.eqtb.set_meaning(id, m, true);
    }

    /// Whether the title macros are the kernel's (article, report, book): the classes whose
    /// `\title`, `\author`, `\date` are `\gdef\@title{#1}` and kin, kept in `Doc::stored`.
    fn kernel_titles(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.std_heads && d.authblk.is_none())
    }

    /// The class's start values (latex.ltx): `\@author` is `\@latex@warning@no@line{No
    /// \author given}`, which typesets nothing; `\@date` is `\today`. `\@title` is an error
    /// (`No \title given`), left undefined.
    pub(super) fn title_macros_init(&mut self) {
        if !self.kernel_titles() {
            return;
        }
        self.set_title_macro(1, Some(&[]));
        let today = self.named_cs(named!("today"));
        self.set_title_macro(2, Some(&[today]));
    }

    /// After `\title`, `\author` or `\date` (`i`): the macro holds the stored argument. One
    /// holding `\thanks` or `\and` (which mean something only inside `\maketitle`) stays
    /// undefined.
    pub(super) fn title_macro(&mut self, i: usize) {
        if !self.kernel_titles() {
            return;
        }
        let text = self.doc.as_ref().map(|d| d.stored[i].clone()).unwrap_or_default();
        let (thanks, and) = (Meaning::Doc(doc!("thanks") as u16), Meaning::Doc(doc!("and") as u16));
        let plain = !text.iter().any(|&t| matches!(self.meaning(t), m if m == thanks || m == and));
        self.set_title_macro(i, plain.then_some(&text[..]));
    }

    /// article's, report's and book's `\maketitle` ends with `\global\let\@title\@empty`,
    /// and the same for `\@author` and `\@date`.
    pub(super) fn title_macros_spent(&mut self) {
        if !self.kernel_titles() {
            return;
        }
        for i in 0..3 {
            self.set_title_macro(i, Some(&[]));
        }
    }

    /// `\newcommand` and kin of one of [`OWN_NAMES`]: the document's from now on.
    pub(super) fn mark_own_name(&mut self, cs: Token) {
        let Some(id) = cs.cs_id() else { return };
        if let Some(k) = OWN_NAMES.iter().position(|n| *n == self.e.cs.name(id)) {
            if let Some(doc) = self.doc.as_mut() {
                doc.pkg.own_names |= 1 << k;
            }
        }
    }

    /// A list's head name (`\contentsname`, `\listfigurename`, `\listtablename`) the document
    /// redefined with `\renewcommand` and kin ([`Self::mark_own_name`]; the pre-scan stops a
    /// `\def` of one). A class's or babel's own macro is not the document's.
    /// `Some(Some(cs))` puts `cs` in the heading, as article's `\section*{\contentsname}`
    /// does; `Some(None)` is the class's name. A redefinition under babel (whose captions
    /// set the names again at `\begin{document}`), under a class with its own lists, or with
    /// a running head (which upper-cases the name as text) goes to the engine (`None`).
    pub(super) fn own_head_name(&mut self, name: &str) -> Option<Option<Token>> {
        let id = self.e.cs.intern(name);
        let k = OWN_NAMES.iter().position(|n| *n == name).unwrap_or(0);
        if !self.doc.as_ref().is_some_and(|d| d.pkg.own_names & 1 << k != 0) {
            return Some(None);
        }
        let heads = self.fancy_on_now() || self.doc.as_ref().is_some_and(|d| d.headings || d.my_headings || d.babel.is_some() || d.koma || d.memoir || d.ams.is_some() || d.ieee.is_some() || !d.std_heads);
        if heads {
            self.not_native("a list's name of the document's own under babel, this class or a running head");
            return None;
        }
        Some(Some(Token::cs(id)))
    }

    /// report's and book's `\@chapapp` (`\chaptername`, `\appendixname` after `\appendix`) as
    /// [`Self::own_head_name`] reads it. fancyhdr's `\chaptermark` puts `\chaptername` in a
    /// mark a later page may show, so with fancyhdr's definitions an own name declines.
    pub(super) fn own_chapapp(&mut self) -> Option<Option<Token>> {
        let appendix = self.doc.as_ref().is_some_and(|d| d.appendix);
        let own = self.own_head_name(if appendix { "appendixname" } else { "chaptername" })?;
        // book's `\chaptermark` (from its `headings`, which `\ps@plain` keeps) marks every
        // chapter, and a later `\pagestyle{headings}` shows the mark.
        if own.is_some() && (self.fancy_defs() || self.doc.as_ref().is_some_and(|d| d.book)) {
            self.not_native("a chapter name of the document's own with fancyhdr or under book");
            return None;
        }
        Some(own)
    }

    /// `\@testopt\\name{default}`: `\kernel@ifnextchar[` (spaces passed) `{\\name}`, else
    /// `{\\name[{default}]}`.
    fn test_opt(&mut self) {
        let Some(inner) = self.get_token() else { return };
        let mut default = Vec::new();
        self.read_arg(&mut default);
        let open = Token::char(Cat::Other, '[');
        if self.next_is('[') {
            return self.back_list(&[inner, open]);
        }
        let mut t = Vec::with_capacity(default.len() + 5);
        t.extend([inner, open, Token::char(Cat::BeginGroup, '{')]);
        t.extend(default);
        t.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::Other, ']')]);
        self.back_list(&t);
    }
}
