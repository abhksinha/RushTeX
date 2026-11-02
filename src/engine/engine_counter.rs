//! The document's own counters natively (latex.ltx's ltcounts.dtx): `\newcounter{name}` and
//! `\newcounter{name}[within]` (within another of the document's counters), `\setcounter`,
//! `\addtocounter`, `\stepcounter`, `\refstepcounter`, `\value` as the value those read,
//! `\arabic`, `\roman`, `\Roman`, `\alph`, `\Alph` and `\the<name>`.
//!
//! latex.ltx keeps a counter in the count register `\c@name` (`\@definecounter`'s `\newcount`)
//! and its resets in `\cl@name`; the native job keeps both here ([`Counter`]), since a native
//! document cannot name either (`@` names stop the pre-scan) and the register's number is read
//! only through them (GDL R9). `\the<name>` is the macro `\@definecounter` makes,
//! `\@arabic\c@name`, here `\arabic{name}`, which expands to the same digits; a document may
//! redefine it. Counters of the kernel and the class (`section`, `page`, `enumi`, a theorem's)
//! stay with the commands that keep them, and these commands decline them.

use super::super::engine_catcode::Cat;
use super::super::engine_prim::{Macro, Meaning};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::{Span, Token};

/// A counter `\newcounter` made: `\c@name`, the counter it is reset by (`\@addtoreset`, an
/// index into `Doc::own_counters`, or `level`, a sectional counter's index into `Doc::counters`),
/// and the text of the `\the<name>` it defined (whether the
/// document has redefined it is whether the macro's text is still this one).
#[derive(Clone, PartialEq)]
pub(super) struct Counter {
    pub(super) name: Box<str>,
    pub(super) value: i32,
    pub(super) within: Option<u16>,
    pub(super) level: Option<u8>,
    pub(super) the: Span,
}

/// The counter names the kernel and the classes define: `\newcounter` of one is LaTeX's
/// "Command \c@name already defined", and the commands here leave them to the natives that
/// keep them.
pub(super) const KERNEL_COUNTERS: &[&str] = &["part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph", "page", "equation", "figure", "table", "footnote", "mpfootnote", "enumi", "enumii", "enumiii", "enumiv", "secnumdepth", "tocdepth", "topnumber", "bottomnumber", "totalnumber", "dbltopnumber"];

/// The counter's natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("newcounter", |m| m.new_counter()),
    ("counterwithin", |m| m.counter_within()),
    ("counterwithout", |m| m.counter_without()),
    ("stepcounter", |m| m.step_counter(false)),
    ("refstepcounter", |m| m.step_counter(true)),
    ("addtocounter", |m| m.add_to_counter()),
    ("arabic", |m| m.counter_form(Form::Arabic)),
    ("roman", |m| m.counter_form(Form::Roman)),
    ("Roman", |m| m.counter_form(Form::CapRoman)),
    ("alph", |m| m.counter_form(Form::Alph)),
    ("Alph", |m| m.counter_form(Form::CapAlph)),
    ("value", |m| m.not_native("\\value outside \\setcounter and \\addtocounter")),
    ("thechapter", |m| m.the_heading(0)),
    ("thesection", |m| m.the_heading(1)),
    ("thesubsection", |m| m.the_heading(2)),
    ("thesubsubsection", |m| m.the_heading(3)),
    ("theenumi", |m| m.the_enum(1)),
    ("theenumii", |m| m.the_enum(2)),
    ("theenumiii", |m| m.the_enum(3)),
    ("theenumiv", |m| m.the_enum(4)),
];

/// The kernel's enumerate counters, `enumi` first (`\c@enumi` is `R_ENUMI`).
const ENUM_COUNTERS: [&str; 4] = ["enumi", "enumii", "enumiii", "enumiv"];

/// The heading counters a counter's text may read, `chapter` first.
const HEADINGS: [&str; 4] = ["chapter", "section", "subsection", "subsubsection"];

/// What a counter's `\the` text reads: the document's counters (name, value, and the text
/// of a redefined `\the<name>`, as [`the_text`] writes it), the heading counters
/// (`Doc::counters`), whether the class has chapters, `\appendix`.
pub(super) struct CounterEnv<'a> {
    pub(super) own: &'a [(&'a str, i32, Option<String>)],
    pub(super) sec: &'a [i64],
    pub(super) report: bool,
    pub(super) appendix: bool,
}

impl CounterEnv<'_> {
    /// `\the<heading>` or `\arabic{<heading>}`: the index into `Doc::counters`.
    fn heading(&self, name: &str) -> Option<usize> {
        let k = HEADINGS.iter().position(|h| *h == name)?.checked_sub(usize::from(!self.report))?;
        (k < self.sec.len()).then_some(k)
    }

    /// The text `\protected@edef\@currentlabel{\the<name>}` makes of a counter's `\the` text:
    /// characters, `\the<counter>` of the document's counters (`\@arabic\c@name` unless
    /// redefined) and of the headings (`heading_number`), and `\arabic`, `\roman`, `\Roman`,
    /// `\alph`, `\Alph` of either. `None` for anything else.
    pub(super) fn label(&self, text: &str, depth: u8) -> Option<String> {
        let mut out = String::new();
        let mut r = text;
        while let Some(c) = r.chars().next() {
            if c == '\\' {
                let word = r[1..].bytes().take_while(u8::is_ascii_alphabetic).count();
                let name = &r[1..1 + word];
                r = r[1 + word..].trim_start_matches(' ');
                let form = match name {
                    "arabic" => Some(Form::Arabic),
                    "roman" => Some(Form::Roman),
                    "Roman" => Some(Form::CapRoman),
                    "alph" => Some(Form::Alph),
                    "Alph" => Some(Form::CapAlph),
                    _ => None,
                };
                if let Some(form) = form {
                    let (arg, rest) = r.strip_prefix('{')?.split_once('}')?;
                    r = rest;
                    let n = match self.own.iter().find(|o| o.0 == arg) {
                        Some(o) => o.1,
                        None => i32::try_from(self.sec[self.heading(arg)?]).ok()?,
                    };
                    out.push_str(&form_text(form, n)?);
                    continue;
                }
                let counter = name.strip_prefix("the")?;
                match self.own.iter().find(|o| o.0 == counter) {
                    Some((_, _, Some(t))) if depth < 8 => out.push_str(&self.label(t, depth + 1)?),
                    Some((_, v, None)) => out.push_str(&v.to_string()),
                    Some(_) => return None,
                    None => out.push_str(&super::heading_number(self.sec, self.heading(counter)?, self.appendix)),
                }
            } else if c.is_ascii_alphanumeric() || " .-:/()".contains(c) {
                out.push(c);
                r = &r[c.len_utf8()..];
            } else {
                return None;
            }
        }
        Some(out)
    }
}

/// A macro's text as [`CounterEnv::label`] reads it: a control sequence `\name ` (a space
/// after, which the reader skips as TeX's tokenizer would have), a character itself.
/// The pre-scan's check of `\counterwithin[*]{c}{p}` or `\counterwithout{c}{p}` at the start
/// of `rest` (after the name), `src` the whole source: the bytes taken when `c` is a heading
/// counter below `section` whose counter and those under it (whose `\the` holds it) appear
/// nowhere else, so no number shows the change (R9). `None` otherwise.
pub(super) fn counter_change_unseen(src: &str, rest: &str) -> Option<usize> {
    // `\counterwithin[*]{figure}{section}` and kin (`Machine::float_within`), before
    // `\begin{document}`.
    let float = |rest: &str| {
        let t = rest.strip_prefix('*').unwrap_or(rest);
        let (c, r) = t.strip_prefix('{')?.split_once('}')?;
        let (p, r) = r.strip_prefix('{')?.split_once('}')?;
        (matches!(c, "figure" | "table") && matches!(p, "chapter" | "section" | "subsection" | "subsubsection") && super::doc_start(src).is_none_or(|d| src.len() - rest.len() < d)).then(|| rest.len() - r.len())
    };
    if let Some(len) = float(rest) {
        return Some(len);
    }
    const BELOW: [&str; 4] = ["subsection", "subsubsection", "paragraph", "subparagraph"];
    let t = rest.strip_prefix('*').unwrap_or(rest);
    let (c, r) = t.strip_prefix('{')?.split_once('}')?;
    let (p, r) = r.strip_prefix('{')?.split_once('}')?;
    let from = BELOW.iter().position(|b| *b == c)?;
    if !p.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    // The source without its `\counterwithin`/`\counterwithout` calls.
    let mut other = String::with_capacity(src.len());
    let mut s = src;
    while let Some(k) = s.find("\\counterwith") {
        other.push_str(&s[..k]);
        let a = &s[k..];
        s = a.find('}').and_then(|e| a[e + 1..].find('}').map(|f| &a[e + f + 2..])).unwrap_or("");
    }
    other.push_str(s);
    // `\paragraph` and `\subparagraph` past the classes' `secnumdepth` print no number; their
    // `\the..` (holding `\thesubsection`) shows only through a `\label`, hyperref's anchors or
    // a changed depth.
    let label = other.match_indices("\\label").any(|(k, _)| !other.as_bytes().get(k + 6).is_some_and(u8::is_ascii_alphabetic));
    let unnumbered = |b: &str| matches!(b, "paragraph" | "subparagraph") && !label && !["secnumdepth", "hyperref", "theparagraph", "thesubparagraph"].iter().any(|w| other.contains(w));
    if BELOW[from..].iter().any(|b| other.contains(b) && !unnumbered(b)) {
        return None;
    }
    Some(rest.len() - r.len())
}

pub(super) fn the_text(m: &Machine<'_, '_>, list: &[Token]) -> Option<String> {
    let mut s = String::new();
    for t in list {
        match t.cs_id() {
            Some(id) => {
                let name = m.e.cs.name(id);
                s.push('\\');
                s.push_str(name);
                s.push(' ');
            }
            None if matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space | Cat::BeginGroup | Cat::EndGroup)) => s.push(t.chr()),
            None => return None,
        }
    }
    Some(s)
}

/// The forms of a counter's value: `\@arabic` (`\number`), `\@roman` (`\romannumeral`),
/// `\@Roman`, `\@alph`, `\@Alph`.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Form {
    Arabic,
    Roman,
    CapRoman,
    Alph,
    CapAlph,
}

/// `n` in `form` as latex.ltx writes it, or `None` for `\@ctrerr` ("Counter too large":
/// `\@alph` of a value outside 0-26). `\romannumeral` of zero or less is empty (§69).
pub(super) fn form_text(form: Form, n: i32) -> Option<String> {
    Some(match form {
        Form::Arabic => n.to_string(),
        Form::Roman | Form::CapRoman => {
            let mut s = String::new();
            let mut v = n.max(0);
            for (k, r) in [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
                while v >= k {
                    s.push_str(r);
                    v -= k;
                }
            }
            if form == Form::CapRoman { s.to_ascii_uppercase() } else { s }
        }
        Form::Alph | Form::CapAlph => match n {
            0 => String::new(),
            1..=26 => {
                let c = char::from(b'a' + (n - 1) as u8);
                (if form == Form::CapAlph { c.to_ascii_uppercase() } else { c }).to_string()
            }
            _ => return None,
        },
    })
}

impl Machine<'_, '_> {
    /// The kernel's `\counterwithin{c}{p}` and `\counterwithout{c}{p}` (chngcntr's, latex.ltx
    /// since 2018): `\@addtoreset`/`\@removefromreset` and `\the<c>` redefined. The pre-scan
    /// takes them only for a heading counter below `section` that nothing in the document
    /// steps or prints (`counter_change_unseen`), so there is nothing to change.
    pub(super) fn counter_within(&mut self) {
        let star = self.next_is('*');
        let (mut c, mut p) = (Vec::new(), Vec::new());
        self.read_arg(&mut c);
        self.read_arg(&mut p);
        let text: String = c.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
        if let Some(t) = ["figure", "table"].iter().position(|f| *f == text) {
            self.float_within(t, &p, !star);
        }
    }

    /// `\counterwithout{c}{p}`: only of a heading counter nothing shows (the pre-scan's
    /// `counter_change_unseen`), so nothing to change.
    pub(super) fn counter_without(&mut self) {
        self.next_is('*');
        let (mut c, mut p) = (Vec::new(), Vec::new());
        self.read_arg(&mut c);
        self.read_arg(&mut p);
    }

    /// `\counterwithin{figure}{p}` (latex.ltx:10182) and amsmath's `\numberwithin{figure}{p}`,
    /// also of `table`: `\@addtoreset{figure}{p}` (the heading's step resets it,
    /// `Doc::fig_within`) and, unless starred, `\def\thefigure{\the<p>.\arabic{figure}}`,
    /// which captions, the lists and `\ref` read as any `\thefigure` of the document's
    /// (`own_float_the`). Under hyperref (`\theHfigure`), over an own `\thefigure`, in the
    /// body or within a counter not a heading's, the engine's.
    pub(super) fn float_within(&mut self, t: usize, parent: &[Token], redefine: bool) {
        // Under hyperref the captions' anchors read the heading's number (`float_href`).
        if self.doc.as_ref().is_none_or(|d| d.body || d.appendix) {
            return self.not_native("\\counterwithin of figure or table in the body");
        }
        if !matches!(self.own_float_the(t, 0), Some(None)) {
            return self.not_native("\\counterwithin after the document's \\thefigure or \\thetable");
        }
        let Some(level) = self.sectional_level(parent) else { return self.not_native("\\counterwithin of figure or table within a counter not a heading's") };
        let p: String = parent.iter().map(|t| t.chr()).collect();
        if let Some(doc) = self.doc.as_mut() {
            doc.fig_within[t] = Some(level);
        }
        if redefine {
            let name = ["figure", "table"][t];
            let mut body = vec![Token::END_MATCH, Token::cs(self.e.cs.intern(&format!("the{p}"))), Token::char(Cat::Other, '.'), self.doc_id(doc!("arabic")), Token::char(Cat::BeginGroup, '{')];
            body.extend(name.chars().map(|c| Token::char(Cat::Letter, c)));
            body.push(Token::char(Cat::EndGroup, '}'));
            let text = self.e.toks.store(&body);
            let id = self.e.cs.intern(&format!("the{name}"));
            self.e.eqtb.set_meaning(id, Meaning::Macro(Macro { text, flags: 0 }), true);
        }
    }

    /// The document's counter of this name, if it made one.
    fn own_counter(&self, name: &[Token]) -> Option<usize> {
        let doc = self.doc.as_ref()?;
        doc.own_counters.iter().position(|c| c.name.len() == name.len() && c.name.chars().zip(name).all(|(a, t)| t.cs_id().is_none() && t.chr() == a))
    }

    /// The document's counter named `name`, if it made one.
    pub(super) fn own_counter_named(&self, name: &str) -> Option<usize> {
        self.doc.as_ref()?.own_counters.iter().position(|c| *c.name == *name)
    }

    /// A counter's name argument, and the counter.
    fn counter_arg(&mut self) -> Option<usize> {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        self.own_counter(&name)
    }

    /// `\newcounter{name}[within]`: `\@definecounter` (`\c@name` zero, `\cl@name` empty,
    /// `\p@name` empty, `\gdef\the<name>{\@arabic\c@name}`), then `\@addtoreset{name}{within}`.
    fn new_counter(&mut self) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let text: String = name.iter().map(|t| t.chr()).collect();
        if name.is_empty() || name.iter().any(|t| t.cs_id().is_some() || !t.chr().is_ascii_alphabetic()) || KERNEL_COUNTERS.contains(&text.as_str()) || self.own_counter(&name).is_some() {
            return self.not_native("\\newcounter of that name");
        }
        let (mut within, mut level) = (None, None);
        if self.next_is('[') {
            let arg = self.bracket_arg();
            // Within a sectional counter of the class (`\cl@section`), which the heading's
            // `\refstepcounter` resets (`reset_own_within`).
            match self.within_counter(&arg) {
                Some(Ok(k)) => within = Some(k),
                Some(Err(l)) => level = Some(l),
                None => return self.not_native("\\newcounter within a counter not the document's or a heading's"),
            }
        }
        self.make_own_counter(&name, within, level);
    }

    /// `\@definecounter{name}` of a checked name (letters, not a counter yet) with its
    /// `\@addtoreset`: the document's counter, `within` one of its own or a heading `level`,
    /// `\the<name>` `\@arabic\c@name`. Its index in `Doc::own_counters`.
    pub(super) fn make_own_counter(&mut self, name: &[Token], within: Option<u16>, level: Option<u8>) -> usize {
        let text: String = name.iter().map(|t| t.chr()).collect();
        let the_name = format!("the{text}");
        let id = self.e.cs.intern(&the_name);
        let mut t = vec![Token::END_MATCH, self.doc_id(doc!("arabic")), Token::char(Cat::BeginGroup, '{')];
        t.extend(name.iter().copied());
        t.push(Token::char(Cat::EndGroup, '}'));
        let the = self.e.toks.store(&t);
        self.e.eqtb.set_meaning(id, Meaning::Macro(Macro { text: the, flags: 0 }), true);
        let doc = self.doc.as_mut().expect("a native document");
        doc.own_counters.push(Counter { name: text.into(), value: 0, within, level, the });
        doc.own_counters.len() - 1
    }

    /// The counter a `[within]` names, as `\newcounter` takes it: one of the document's own
    /// (`Ok(Some(k))`, its index) or a heading's level (`Err(level)`); `None` for any other.
    pub(super) fn within_counter(&self, arg: &[Token]) -> Option<Result<u16, u8>> {
        match self.own_counter(arg) {
            Some(k) => Some(Ok(k as u16)),
            None => self.sectional_level(arg).map(Err),
        }
    }

    /// The value a `\setcounter` or `\addtocounter` reads (`#2\relax`): a number of digits and
    /// signs, or `\value{name}` of the document's counter. Anything else declines.
    fn counter_amount(&mut self) -> Option<i32> {
        let mut value = Vec::new();
        self.read_arg(&mut value);
        if value.first() == Some(&self.doc_id(doc!("value"))) {
            let rest = value.get(1..)?;
            let name = match rest.first().and_then(|t| t.cat()) {
                Some(Cat::BeginGroup) if rest.last().and_then(|t| t.cat()) == Some(Cat::EndGroup) => &rest[1..rest.len() - 1],
                _ => rest,
            };
            let k = self.own_counter(name)?;
            return self.doc.as_ref().map(|d| d.own_counters[k].value);
        }
        if value.is_empty() || !value.iter().all(|t| t.cs_id().is_none() && (t.chr().is_ascii_digit() || matches!(t.chr(), '-' | '+' | ' '))) {
            return None;
        }
        let relax = self.named_cs(named!("relax"));
        value.push(relax);
        self.back_list(&value);
        let n = self.scan_int();
        if let Some(t) = self.get_token().filter(|&t| t != relax) {
            self.input.back(t);
        }
        Some(n)
    }

    /// `\setcounter{name}{value}` of the document's counter: `\global\c@name #2\relax`.
    /// `None` when the counter is not the document's (the caller's).
    pub(super) fn set_own_counter(&mut self, name: &[Token]) -> Option<()> {
        let k = self.own_counter(name)?;
        match self.counter_amount() {
            Some(n) => {
                if let Some(d) = self.doc.as_mut() {
                    d.own_counters[k].value = n;
                }
            }
            None => self.not_native("\\setcounter of a value that is not a number"),
        }
        Some(())
    }

    /// `\addtocounter{name}{value}`: `\global\advance\c@name #2\relax`.
    fn add_to_counter(&mut self) {
        let Some(k) = self.counter_arg() else { return self.not_native("\\addtocounter of a counter not the document's") };
        match self.counter_amount() {
            Some(n) => {
                if let Some(d) = self.doc.as_mut() {
                    d.own_counters[k].value = d.own_counters[k].value.wrapping_add(n);
                }
            }
            None => self.not_native("\\addtocounter of a value that is not a number"),
        }
    }

    /// The document's counters as [`CounterEnv`] reads them: a `\the<name>` still
    /// `\newcounter`'s has no text, a redefined one its macro's (no parameters).
    fn counter_env_own(&self) -> Option<Vec<(String, i32, Option<String>)>> {
        let doc = self.doc.as_ref()?;
        let mut own = Vec::with_capacity(doc.own_counters.len());
        for c in &doc.own_counters {
            let id = self.e.cs.get(&format!("the{}", c.name))?;
            let text = match self.e.eqtb.meaning(id) {
                Meaning::Macro(m) if m.text == c.the => None,
                Meaning::Macro(m) => match self.e.toks.get(m.text) {
                    [Token::END_MATCH, rest @ ..] => Some(the_text(self, rest)?),
                    _ => return None,
                },
                _ => return None,
            };
            own.push((c.name.to_string(), c.value, text));
        }
        Some(own)
    }

    /// `\thefigure`, `\thetable`, `\theequation` (`k` in `PREFIXED` order) at the value `n`
    /// when the document redefined it: the text `\protected@edef\@currentlabel` makes of the
    /// document's macro ([`CounterEnv::label`], the counter itself among the document's),
    /// `\thesection-\arabic{figure}` or `A.\arabic{table}`. `Some(None)` when the macro is
    /// the class's, `None` when its text cannot be read or reads a heading of a class whose
    /// `\thesection` is not `heading_number`'s.
    pub(super) fn own_float_the(&self, k: usize, n: i32) -> Option<Option<String>> {
        let name = super::PREFIXED[k];
        let Some(id) = self.e.cs.get(&format!("the{name}")) else { return Some(None) };
        let text = match self.e.eqtb.meaning(id) {
            Meaning::Unset | Meaning::Undefined | Meaning::Doc(_) => return Some(None),
            Meaning::Macro(m) => match self.e.toks.get(m.text) {
                [Token::END_MATCH, rest @ ..] => the_text(self, rest)?,
                _ => return None,
            },
            _ => return None,
        };
        let own = self.counter_env_own()?;
        let doc = self.doc.as_ref()?;
        if HEADINGS.iter().any(|h| text.contains(h)) && (doc.koma || doc.memoir || doc.ams.is_some() || doc.letter.is_some() || doc.ieee.is_some() || doc.titlesec.is_some()) {
            return None;
        }
        let mut list: Vec<(&str, i32, Option<String>)> = own.iter().map(|(n, v, t)| (n.as_str(), *v, t.clone())).collect();
        list.push((name, n, None));
        let sec: Vec<i64> = doc.counters.iter().map(|&c| i64::from(c)).collect();
        let env = CounterEnv { own: &list, sec: &sec, report: doc.report, appendix: doc.appendix };
        Some(Some(env.label(&text, 0)?))
    }

    /// `\thefigure` (`k` 0), `\thetable` (1), `\theequation` (2) at the value `n`: the
    /// document's text ([`Self::own_float_the`]), else the class's prefix (`Doc::the_prefix`)
    /// and `n`. `None` when the document's text cannot be read.
    pub(super) fn float_the(&self, k: usize, n: i32) -> Option<String> {
        match self.own_float_the(k, n)? {
            Some(t) => Some(t),
            None => Some(format!("{}{n}", self.doc.as_ref().map_or("", |d| d.the_prefix[k].as_str()))),
        }
    }

    /// `\@currentlabel` at an `\item` of enumerate depth `k + 1` when the document redefined
    /// one of `\theenumi`..`\theenumiv` ([`enum_current_label`], each `\theenum<j>` the
    /// document's macro or the list's own style, `enum_style`); `Some(None)` when none is
    /// redefined (the native job composes it), `None` when a text cannot be read.
    pub(super) fn own_enum_label(&self, k: usize) -> Option<Option<String>> {
        let mut texts: [Option<String>; 4] = Default::default();
        let mut any = false;
        for (j, text) in texts.iter_mut().enumerate() {
            let Some(id) = self.e.cs.get(&format!("the{}", ENUM_COUNTERS[j])) else { continue };
            // The native `\theenum<j>` (`the_enum`) is the kernel's.
            match self.e.eqtb.meaning(id) {
                Meaning::Unset | Meaning::Undefined | Meaning::Doc(_) => {}
                Meaning::Macro(m) => match self.e.toks.get(m.text) {
                    [Token::END_MATCH, rest @ ..] => {
                        *text = Some(the_text(self, rest)?);
                        any = true;
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
        if !any {
            return Some(None);
        }
        let own = self.counter_env_own()?;
        let doc = self.doc.as_ref()?;
        let mut list: Vec<(&str, i32, Option<String>)> = own.iter().map(|(n, v, t)| (n.as_str(), *v, t.clone())).collect();
        for (j, text) in texts.into_iter().enumerate() {
            let form = match self.enum_style(j as u8 + 1) {
                b'a' => "alph",
                b'A' => "Alph",
                b'i' => "roman",
                b'I' => "Roman",
                _ => "arabic",
            };
            list.push((ENUM_COUNTERS[j], self.rw(super::R_ENUMI + j), Some(text.unwrap_or_else(|| format!("\\{form}{{{}}}", ENUM_COUNTERS[j])))));
        }
        let sec: Vec<i64> = doc.counters.iter().map(|&c| i64::from(c)).collect();
        let env = CounterEnv { own: &list, sec: &sec, report: doc.report, appendix: doc.appendix };
        Some(Some(enum_current_label(&env, k)?))
    }

    /// `\protected@edef\@currentlabel{\p@name\the<name>}` of the document's counter `k`
    /// (`\p@name` is empty).
    pub(super) fn own_label(&self, k: usize) -> Option<String> {
        let own = self.counter_env_own()?;
        let doc = self.doc.as_ref()?;
        let list: Vec<(&str, i32, Option<String>)> = own.iter().map(|(n, v, t)| (n.as_str(), *v, t.clone())).collect();
        let sec: Vec<i64> = doc.counters.iter().map(|&c| i64::from(c)).collect();
        let env = CounterEnv { own: &list, sec: &sec, report: doc.report, appendix: doc.appendix };
        match &own[k].2 {
            Some(t) => env.label(t, 0),
            None => Some(own[k].1.to_string()),
        }
    }

    /// `\thechapter`, `\thesection`, ... in the standard classes: the heading's number
    /// (`heading_number`), its characters.
    fn the_heading(&mut self, k: usize) {
        let Some(doc) = self.doc.as_ref().filter(|d| !(d.koma || d.memoir || d.ams.is_some() || d.letter.is_some() || d.ieee.is_some() || d.titlesec.is_some())) else { return self.not_native("a heading's \\the in this class") };
        let Some(level) = k.checked_sub(usize::from(!doc.report)).filter(|&l| l < doc.counters.len()) else { return self.not_native("\\thechapter without chapters") };
        let text = super::heading_number(&doc.counters, level, doc.appendix);
        let t: Vec<Token> = text.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect();
        self.back_list(&t);
    }

    /// `\theenum<d>`: latex.ltx's `\@arabic\c@enumi`, `\@alph\c@enumii`, `\@roman\c@enumiii`,
    /// `\@Alph\c@enumiv`, or enumerate.sty's template's style (`enum_style`), the characters
    /// as `counter_form` writes them. Under enumitem's `label` key `\theenum<d>` is the key's
    /// text (`\enit@normlabel`): declined.
    fn the_enum(&mut self, d: u8) {
        let Some(doc) = self.doc.as_ref() else { return };
        if doc.lists.iter().any(|l| matches!(l.label, super::Label::Enit(e, _) if e == d)) {
            return self.not_native("\\theenum under enumitem's label key");
        }
        let n = self.rw(super::R_ENUMI + usize::from(d) - 1);
        let style = self.enum_style(d);
        let form = match style {
            b'a' => Form::Alph,
            b'A' => Form::CapAlph,
            b'i' => Form::Roman,
            b'I' => Form::CapRoman,
            _ => Form::Arabic,
        };
        if self.es_counter_differs(style, n) {
            return self.not_native("spanish's \\@roman or \\@alph of a list counter");
        }
        let Some(text) = form_text(form, n) else { return self.not_native("\\alph of a counter past 26") };
        if self.mode == Mode::Math && form != Form::Arabic {
            return self.not_native("a counter's letters in math");
        }
        let cat = if matches!(form, Form::Alph | Form::CapAlph) { Cat::Letter } else { Cat::Other };
        let t: Vec<Token> = text.chars().map(|c| Token::char(cat, c)).collect();
        self.back_list(&t);
    }

    /// `chapter`, `section`, `subsection` or `subsubsection` as an index into `Doc::counters`
    /// (report and book: `chapter` 0), in the standard classes, whose headings step them in
    /// `chapter_step` and `heading_step`.
    fn sectional_level(&self, name: &[Token]) -> Option<u8> {
        let d = self.doc.as_ref()?;
        if d.koma || d.memoir || d.ams.is_some() || d.letter.is_some() || d.ieee.is_some() || d.titlesec.is_some() {
            return None;
        }
        let text: String = name.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
        let k = ["chapter", "section", "subsection", "subsubsection"].iter().position(|c| *c == text)?;
        let k = k.checked_sub(usize::from(!d.report))?;
        (k < 3).then_some(k as u8)
    }

    /// The heading counter `level` stepped (`\@stpelt` of its `\cl@`, and of each counter
    /// below it): the document's counters within it or a lower level zeroed, and theirs.
    pub(super) fn reset_own_within(doc: &mut super::Doc, level: usize) {
        let mut reset: Vec<usize> = Vec::new();
        for j in 0..doc.own_counters.len() {
            let c = &doc.own_counters[j];
            if c.level.is_some_and(|l| usize::from(l) >= level) || c.within.is_some_and(|w| reset.contains(&usize::from(w))) {
                doc.own_counters[j].value = 0;
                reset.push(j);
            }
        }
    }

    /// `\stepcounter{name}`: `\addtocounter{name}\@ne`, then each counter it resets
    /// (`\cl@name`) `\@stpelt`: `\global\c@x\m@ne \stepcounter{x}`, which is zero, and its
    /// own resets in turn. `\refstepcounter` then sets `\@currentlabel` to `\p@name\the<name>`
    /// (`\protected@edef`, local), while `\the<name>` is still `\newcounter`'s; under
    /// hyperref it also makes an anchor, not ported here.
    fn step_counter(&mut self, refstep: bool) {
        let Some(k) = self.counter_arg() else { return self.not_native("\\stepcounter of a counter not the document's") };
        self.step_own(k, refstep);
    }

    /// `\stepcounter` (`refstep` false) or `\refstepcounter` of the document's counter `k`.
    /// hyperref's anchor name of the document's counter `k`: `name.\theHname`.
    pub(super) fn own_href(&self, k: usize) -> String {
        let Some(doc) = self.doc.as_ref() else { return String::new() };
        let counters: Vec<u32> = doc.counters.iter().map(|&c| c as u32).collect();
        format!("{}.{}", doc.own_counters[k].name, own_the_h(&doc.own_counters.iter().map(|c| (c.within.map(usize::from), c.level.map(usize::from), c.value)).collect::<Vec<_>>(), &counters, k))
    }

    pub(super) fn step_own(&mut self, k: usize, refstep: bool) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.own_counters[k].value = doc.own_counters[k].value.wrapping_add(1);
        // The counters reset by `k`, then theirs: each is zeroed (they come after `k`).
        let mut reset = vec![k];
        for j in k + 1..doc.own_counters.len() {
            if doc.own_counters[j].within.is_some_and(|w| reset.contains(&(w as usize))) {
                doc.own_counters[j].value = 0;
                reset.push(j);
            }
        }
        if !refstep {
            return;
        }
        let (name, value, the) = (doc.own_counters[k].name.clone(), doc.own_counters[k].value, doc.own_counters[k].the);
        let _ = the;
        let Some(label) = self.own_label(k) else { return self.not_native("\\refstepcounter of a counter whose \\the the native job cannot read") };
        if !self.hyper() {
            self.set_current_label(&label);
            return self.cref_step(&name);
        }
        // hyperref's `refstepcounter/target` plug: `\hyper@refstepcounter{name}`, that is
        // `\MakeLinkTarget{name}`, the anchor `name.\theHname` raised where it is
        // (`\__hyp_target_raise:n`).
        let Some(doc) = self.doc.as_ref() else { return };
        if self.mode == Mode::Math || doc.appendix {
            return self.not_native("hyperref: a counter's anchor in math or the appendix");
        }
        let href = self.own_href(k);
        let _ = value;
        self.store_current_label(&label);
        self.cref_step(&name);
        self.set_current_href(&href);
        if self.mode == Mode::Vertical {
            self.vertical_anchor(href);
        } else {
            self.raised_target(href);
        }
    }

    /// `\value{name}` read as a number (`\c@name`): the document's counters, the headings',
    /// the lists', `footnote`, `figure`, `table`, `equation`; `None` for any other (the page
    /// counter among them, which the output routine steps).
    pub(super) fn counter_value(&self, name: &[Token]) -> Option<i32> {
        if let Some(k) = self.own_counter(name) {
            return self.doc.as_ref().map(|d| d.own_counters[k].value);
        }
        if let Some(l) = self.sectional_level(name) {
            return self.doc.as_ref().map(|d| d.counters[usize::from(l)]);
        }
        if let Some(j) = enum_counter(name) {
            return Some(self.rw(super::R_ENUMI + j));
        }
        let text: String = name.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).collect();
        match text.trim() {
            "footnote" => self.doc.as_ref().map(|d| d.footnote),
            "figure" => Some(self.rw(super::R_FIGURE)),
            "table" => Some(self.rw(super::R_FIGURE + 1)),
            "equation" => self.doc.as_ref().filter(|d| d.sub_eq.is_none()).map(|d| d.equation),
            _ => None,
        }
    }

    /// `\arabic{name}` and its kin: `\expandafter\@arabic\csname c@name\endcsname`, the value's
    /// characters typeset (category 12 from `\number` and `\romannumeral`, 11 from `\@alph`'s
    /// letters, which typeset alike).
    fn counter_form(&mut self, form: Form) {
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let n = match self.own_counter(&name) {
            Some(k) => self.doc.as_ref().map_or(0, |d| d.own_counters[k].value),
            // A heading's counter (`\arabic{section}`), in the classes `the_heading` reads.
            None => match self.sectional_level(&name) {
                Some(l) => self.doc.as_ref().map_or(0, |d| d.counters[usize::from(l)]),
                // `\arabic{enumi}` in a list label of the document's (`own_list_label`).
                None => match enum_counter(&name) {
                    Some(j) => {
                        let n = self.rw(super::R_ENUMI + j);
                        let style = b"1iIaA"[form as usize];
                        if self.es_counter_differs(style, n) {
                            return self.not_native("spanish's \\@roman or \\@alph of a list counter");
                        }
                        n
                    }
                    // `\alph{footnote}` in a `\thefootnote` of the document's (`own_footnote_mark`).
                    None if name.len() == 8 && name.iter().map(|t| if t.cs_id().is_some() { '\\' } else { t.chr() }).eq("footnote".chars()) => self.doc.as_ref().map_or(0, |d| d.footnote),
                    None => return self.not_native("\\arabic of a counter not the document's or a heading's"),
                },
            },
        };
        let Some(text) = form_text(form, n) else { return self.not_native("\\alph of a counter past 26") };
        if self.mode == Mode::Math && form != Form::Arabic {
            return self.not_native("a counter's letters in math");
        }
        let cat = if matches!(form, Form::Alph | Form::CapAlph) { Cat::Letter } else { Cat::Other };
        let t: Vec<Token> = text.chars().map(|c| Token::char(cat, c)).collect();
        self.back_list(&t);
    }
}

/// `\p@enum<d>\theenum<d>` at enumerate depth `k + 1`, as latex.ltx composes it: `\p@enumii`
/// is `\theenumi`, `\p@enumiii` `\theenumi(\theenumii)`, `\p@enumiv` `\p@enumiii\theenumiii`;
/// each `\theenum<j>` read by `env`, whose own list names `enumi` to `enumiv` with their texts.
pub(super) fn enum_current_label(env: &CounterEnv, k: usize) -> Option<String> {
    let the = |j: usize| env.label(&format!("\\the{} ", ENUM_COUNTERS[j]), 0);
    let mut s = the(0)?;
    if k >= 1 {
        if k >= 2 {
            s.push('(');
        }
        s.push_str(&the(1)?);
        if k >= 2 {
            s.push(')');
            s.push_str(&the(2)?);
        }
        if k >= 3 {
            s.push_str(&the(3)?);
        }
    }
    Some(s)
}

/// `enumi` to `enumiv` as an index (`R_ENUMI + j`).
fn enum_counter(name: &[Token]) -> Option<usize> {
    ENUM_COUNTERS.iter().position(|c| c.len() == name.len() && c.chars().zip(name).all(|(a, t)| t.cs_id().is_none() && t.chr() == a))
}

/// `\theH<name>` of the document's counter `k` (each as `(within, level, value)`): latex.ltx's
/// `\@definecounter` makes it `\the\value{name}`, and `\@addtoreset{name}{within}` (hyperref's
/// patch alike) `\theH<within>.\the\value{name}`; a heading's `\theH<level>` is its counters
/// and those above it, in arabic (`theorem_href`).
pub(super) fn own_the_h(own: &[(Option<usize>, Option<usize>, i32)], sec: &[u32], k: usize) -> String {
    let (within, level, value) = own[k];
    match (within, level) {
        (Some(w), _) => format!("{}.{value}", own_the_h(own, sec, w)),
        (None, Some(l)) => {
            let mut s = String::new();
            for c in &sec[..=l] {
                s.push_str(&format!("{c}."));
            }
            format!("{s}{value}")
        }
        (None, None) => value.to_string(),
    }
}
