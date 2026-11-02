//! cleveref.sty (0.21.4) natively, with or without hyperref: `\cref`, `\Cref`,
//! `\crefrange`, `\Crefrange`, `\labelcref` and `\crefname`/`\Crefname` in the preamble,
//! English names, the options `capitalise` (`capitalize`) and `noabbrev`.
//!
//! cleveref reads a label's type from its `@cref` label, `[type][n][prefix]value`, which
//! `\cref@currentlabel` holds. The kernel's first aid for cleveref 0.21.4
//! (`latex2e-first-aid-for-external-files.ltx`) remakes it at every `\label` (the `label`
//! hook, `\firstaid@cref@updatelabeldata{\@currentcounter}`) from the counter the last
//! `\refstepcounter` stepped: the type is that counter, whatever cleveref's own patches
//! (amsthm's `\@thm[env]`, `\appendix`'s `appendix`) set. A theorem sharing `theorem`'s
//! counter is a `theorem`; a chapter after `\appendix` a `chapter`. The native job keeps the
//! counter beside `\@currentlabel` (`cref_step`, called where each native `\refstepcounter`
//! runs) and records it at `\label` (`cref_label_type`); a label ahead takes the counter of
//! the anchor the pre-scan predicted for it (`predicted_type`), checked at its `\label`.
//!
//! A reference is the name, `\nobreakspace`, then the label as `\creflabelformat` makes it:
//! `#2#1#3` (with hyperref the link around the value), `\textup{(#2#1#3)}` for an equation.
//! Several keys of one type are sorted by their values and runs of three or more
//! consecutive values are compressed into a range (`sort&compress`, the default): the plural
//! name, the items joined by `\crefpairconjunction` (` and\nobreakspace`),
//! `\crefmiddleconjunction` (`, `) and `\creflastconjunction` (` and\nobreakspace`), a range
//! `a\crefrangeconjunction b` (` to\nobreakspace`). `\labelcref` is the same without the
//! name (`\labelcrefformat`, `\@labelcrefdefinedefaultformats`). Keys of more than one type
//! decline.
//!
//! `\label` writes a second `\newlabel{key@cref}` (`\label@noarg`): one more `.aux` whatsit
//! after the first `\@esphack`, and another `\@bsphack`/`\@esphack` pair.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Reg, Region};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::label_tokens;

/// `\@currentcounter` as `counter|value`, with the `\@currentlabel` it was made with
/// (local, as `\refstepcounter`'s `\protected@edef`s are): a `\label` whose `\@currentlabel`
/// is not that value follows something other than `\refstepcounter` (amsmath's `\tag`,
/// `\tag@true`), and its type is unknown.
const CREF_TYPE: Reg = Reg::register(Region::Toks, 32702);

/// cleveref's natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@cref", |m| m.cref(CREF, false)), ("rt@Cref", |m| m.cref(BIG, false)), ("rt@crefrange", |m| m.cref(CREF, true)), ("rt@Crefrange", |m| m.cref(BIG, true)), ("rt@labelcref", |m| m.cref(LABEL, false)), ("rt@crefname", |m| m.cref_name(false)), ("rt@Crefname", |m| m.cref_name(true)), ("rt@creflink", |m| m.cref_link())];

/// What a reference prints before its labels: `\cref`'s name, `\Cref`'s, none (`\labelcref`).
const CREF: u8 = 0;
const BIG: u8 = 1;
const LABEL: u8 = 2;

/// The types cleveref's English definitions name (`\Crefname@preamble` under its `english`
/// option): `\Cref`'s singular and plural. `\cref`'s are the same with the first letter
/// lower-cased unless `capitalise`; `abbrev` (the default) makes `\cref`'s equation and
/// figure names `ABBREV`'s.
const TYPES: [(&str, &str, &str); 22] = [
    ("equation", "Equation", "Equations"),
    ("figure", "Figure", "Figures"),
    ("table", "Table", "Tables"),
    ("page", "Page", "Pages"),
    ("part", "Part", "Parts"),
    ("chapter", "Chapter", "Chapters"),
    ("section", "Section", "Sections"),
    ("appendix", "Appendix", "Appendices"),
    ("enumi", "Item", "Items"),
    ("footnote", "Footnote", "Footnotes"),
    ("theorem", "Theorem", "Theorems"),
    ("lemma", "Lemma", "Lemmas"),
    ("corollary", "Corollary", "Corollaries"),
    ("proposition", "Proposition", "Propositions"),
    ("definition", "Definition", "Definitions"),
    ("result", "Result", "Results"),
    ("example", "Example", "Examples"),
    ("remark", "Remark", "Remarks"),
    ("note", "Note", "Notes"),
    ("algorithm", "Algorithm", "Algorithms"),
    ("listing", "Listing", "Listings"),
    ("line", "Line", "Lines"),
];
const ABBREV: [(&str, &str, &str); 2] = [("equation", "Eq.", "Eqs."), ("figure", "Fig.", "Figs.")];

/// `\@crefcopyformats` at `\begin{document}`: a type with no names of its own takes these
/// (`subsection` `section`'s, `subappendix` `appendix`'s, `enumii` `enumi`'s).
fn copied_from(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "subsection" | "subsubsection" => "section",
        "subappendix" | "subsubappendix" | "subsubsubappendix" => "appendix",
        "subfigure" => "figure",
        "subtable" => "table",
        "subequation" => "equation",
        "enumii" | "enumiii" | "enumiv" | "enumv" => "enumi",
        _ => return None,
    })
}

/// The package options the native job follows.
pub(super) fn options_ok(opts: &str) -> bool {
    opts.split(',').map(str::trim).filter(|o| !o.is_empty()).all(|o| matches!(o, "capitalise" | "capitalize" | "noabbrev" | "nameinlink"))
}

/// cleveref's state: the options, the document's `\crefname`s and `\Crefname`s (type,
/// singular, plural), the theorems' titles (`\cref@<env>@name@preamble`, `None` when not
/// plain text), each label's type (and the predicted types references ahead used, checked
/// at their `\label`), and the anchors of the links being typed, in order.
#[derive(Default, Clone, PartialEq)]
pub(super) struct Cleveref {
    capitalise: bool,
    abbrev: bool,
    /// `nameinlink`: the first link holds the name (`\@templabel@first`, `\@tempname`).
    name_in_link: bool,
    names: Vec<(String, String, String)>,
    big_names: Vec<(String, String, String)>,
    thm_titles: Vec<(String, Option<String>)>,
    types: std::collections::HashMap<String, String>,
    forward: std::collections::HashMap<String, String>,
    queue: std::collections::VecDeque<String>,
}

/// The type a label ahead will have, from the anchor name the pre-scan predicted for it
/// (hyperref's, made without hyperref too): the anchor's counter. After `\appendix` the
/// anchor of report's chapter is `appendix.A`, of its sections `appendix.A.1`, of article's
/// sections `appendix.A`: the counters `chapter` and `section`. Item and footnote anchors do
/// not name the counter, a starred heading's names none.
fn predicted_type(href: &str, report: bool) -> Option<String> {
    let (counter, number) = href.split_once('.')?;
    Some(match counter {
        "appendix" if report && !number.contains('.') => "chapter",
        "appendix" => "section",
        "Item" | "Hfootnote" | "Doc-Start" => return None,
        c if c.contains('*') => return None,
        c => c,
    }.to_string())
}

/// A value as numbers for sorting and ranges (`1.2` is `[1, 2]`); `None` for letters.
fn value_numbers(v: &str) -> Option<Vec<u32>> {
    v.split('.').map(|p| p.parse().ok()).collect()
}

/// `\MakeUppercase` (`\MakeLowercase` when `lower`) of a name's first token, as cleveref's
/// `\cref@othervariant` and its theorem name preambles (`\MakeLowercase #3`) apply them.
fn first_case(s: &str, lower: bool) -> String {
    let mut it = s.chars();
    it.next().map_or(String::new(), |f| if lower { f.to_ascii_lowercase() } else { f.to_ascii_uppercase() }.to_string() + it.as_str())
}

impl Machine<'_, '_> {
    /// `\usepackage[opts]{cleveref}`.
    pub(super) fn cleveref_load(&mut self, opts: &str) {
        let has = |o: &str| opts.split(',').any(|x| x.trim() == o);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.pkg.cref = Some(Box::new(Cleveref { capitalise: has("capitalise") || has("capitalize"), abbrev: !has("noabbrev"), name_in_link: has("nameinlink"), ..Default::default() }));
    }

    /// The kernel's `\refstepcounter{ty}` after the native one set `\@currentlabel`:
    /// `\@currentcounter`, kept with that value. Nothing without cleveref.
    pub(super) fn cref_step(&mut self, ty: &str) {
        if self.doc.as_ref().is_none_or(|d| d.pkg.cref.is_none()) {
            return;
        }
        let value: String = self.e.toks.get(self.e.eqtb.toks(super::CURRENT_LABEL)).iter().map(|t| t.chr()).collect();
        let span = self.e.toks.store(&label_tokens(&format!("{ty}|{value}")));
        self.e.eqtb.set_toks(CREF_TYPE, span, false);
    }

    /// A heading's `\refstepcounter` (`level` 0 the class's top numbered level): its
    /// counter, after `\appendix` too.
    pub(super) fn cref_heading_step(&mut self, level: usize) {
        let Some(doc) = self.doc.as_ref() else { return };
        let names: [&str; 3] = if doc.report { ["chapter", "section", "subsection"] } else { ["section", "subsection", "subsubsection"] };
        self.cref_step(names.get(level).copied().unwrap_or(""));
    }

    /// A theorem's `\refstepcounter`: the counter it numbers by (the kernel's and amsthm's
    /// `\@thm`), which a theorem sharing another's is that one's.
    pub(super) fn cref_theorem_step(&mut self, k: usize) {
        let Some(doc) = self.doc.as_ref() else { return };
        let ty = doc.theorems[doc.theorems[k].counter].env.clone();
        self.cref_step(&ty);
    }

    /// `\newtheorem{env}{Title}` after cleveref (its `\@othm`, `\@xnthm`, `\@ynthm`
    /// patches): `\cref@env@name@preamble`, `\MakeLowercase Title`, and `\Cref`'s
    /// `\MakeUppercase Title`; no plural.
    pub(super) fn cref_theorem(&mut self, env: &str, title: &[Token]) {
        let text = title.iter().all(|t| t.cs_id().is_none() && matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))).then(|| title.iter().map(|t| t.chr()).collect::<String>());
        let Some(c) = self.doc.as_mut().and_then(|d| d.pkg.cref.as_mut()) else { return };
        c.thm_titles.retain(|t| t.0 != env);
        c.thm_titles.push((env.to_string(), text.filter(|t| !t.is_empty())));
    }

    /// At `\label{key}`: the key's type from `\cref@currentlabel` (empty when unknown); a
    /// type a reference ahead took from the pre-scan must be this one.
    pub(super) fn cref_label_type(&mut self, key: &str, value: &[Token]) {
        if self.doc.as_ref().is_none_or(|d| d.pkg.cref.is_none()) {
            return;
        }
        let stored: String = self.e.toks.get(self.e.eqtb.toks(CREF_TYPE)).iter().map(|t| t.chr()).collect();
        let value: String = value.iter().map(|t| t.chr()).collect();
        let ty = stored.split_once('|').filter(|(_, v)| *v == value).map_or(String::new(), |(t, _)| t.to_string());
        let Some(c) = self.doc.as_mut().and_then(|d| d.pkg.cref.as_mut()) else { return };
        if c.forward.get(key).is_some_and(|p| *p != ty) {
            return self.not_native("cleveref: a label's type the pre-scan predicted differently");
        }
        c.types.insert(key.to_string(), ty);
    }

    /// `\crefname{type}{singular}{plural}` (`\Crefname` when `big`) in the preamble: plain
    /// text only.
    pub(super) fn cref_name(&mut self, big: bool) {
        let mut a = [Vec::new(), Vec::new(), Vec::new()];
        for x in a.iter_mut() {
            self.read_arg(x);
        }
        let text = |t: &[Token]| -> Option<String> { t.iter().all(|t| t.cs_id().is_none() && matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))).then(|| t.iter().map(|t| t.chr()).collect()) };
        let (Some(ty), Some(one), Some(more)) = (text(&a[0]), text(&a[1]), text(&a[2])) else {
            return self.not_native("cleveref: a \\crefname that is not plain text");
        };
        let Some(c) = self.doc.as_mut().and_then(|d| d.pkg.cref.as_mut()) else { return };
        let list = if big { &mut c.big_names } else { &mut c.names };
        list.retain(|n| n.0 != ty);
        list.push((ty, one, more));
    }

    /// The type whose names `ty` takes: the one it copies (`copied_from`) unless the
    /// document named `ty` itself.
    fn cref_canon(&self, ty: &str) -> String {
        let own = self.doc.as_ref().and_then(|d| d.pkg.cref.as_ref()).is_some_and(|c| c.names.iter().chain(c.big_names.iter()).any(|n| n.0 == ty));
        match copied_from(ty) {
            Some(from) if !own => from.to_string(),
            _ => ty.to_string(),
        }
    }

    /// The name of `ty` (`plural`, `\Cref`'s when `big`), `None` when cleveref has none.
    /// The document's `\crefname` gives `\Cref` its name with the first letter upper-cased,
    /// a `\Crefname` gives `\cref` its own lower-cased (`\cref@othervariant`, unless
    /// `capitalise`), unless the other is given too; a type with neither takes the names it
    /// copies (`copied_from`), then the English ones, then its theorem's title (singular
    /// only).
    fn cref_type_name(&self, ty: &str, big: bool, plural: bool) -> Option<String> {
        let canon = self.cref_canon(ty);
        let ty = canon.as_str();
        let c = self.doc.as_ref()?.pkg.cref.as_ref()?;
        let pick = |n: &(String, String, String)| if plural { n.2.clone() } else { n.1.clone() };
        let small = c.names.iter().find(|n| n.0 == ty);
        let large = c.big_names.iter().find(|n| n.0 == ty);
        match (big, small, large) {
            (true, _, Some(n)) | (false, Some(n), _) => return Some(pick(n)),
            (true, Some(n), None) => return Some(first_case(&pick(n), false)),
            (false, None, Some(n)) => return Some(if c.capitalise { pick(n) } else { first_case(&pick(n), true) }),
            _ => {}
        }
        if let Some(&(_, one, more)) = TYPES.iter().find(|t| t.0 == ty) {
            let (one, more) = match ABBREV.iter().find(|t| t.0 == ty) {
                Some(&(_, a, b)) if !big && c.abbrev => (a, b),
                _ => (one, more),
            };
            let name = if plural { more } else { one };
            return Some(if big || c.capitalise { name.to_string() } else { first_case(name, true) });
        }
        let title = c.thm_titles.iter().find(|t| t.0 == ty)?.1.as_ref()?;
        (!plural).then(|| first_case(title, !big && !c.capitalise))
    }

    /// A label's value, anchor (with hyperref) and type, as `\ref` reads them
    /// (`reference`): the value now, or the pre-scan's for a label ahead.
    fn cref_label(&mut self, key: &str) -> Option<(String, String, String)> {
        let report = self.doc.as_ref()?.report;
        let hyper = self.hyper();
        let doc = self.doc.as_mut()?;
        let v: String = match doc.labels.get(key) {
            Some(v) => v.iter().map(|t| t.chr()).collect(),
            None => {
                // A tag of tokens (`tag_text`'s U+FFFF) is not a text cleveref can read.
                let p = doc.predicted.get(key).filter(|p| !p.starts_with('\u{ffff}'))?.clone();
                doc.forward.insert(key.to_string(), p.clone());
                p
            }
        };
        let href = doc.label_hrefs.get(key).or_else(|| doc.predicted_hrefs.get(key)).cloned();
        let c = doc.pkg.cref.as_mut()?;
        let ty = match c.types.get(key) {
            Some(t) => t.clone(),
            None => {
                let t = predicted_type(href.as_deref()?, report)?;
                c.forward.insert(key.to_string(), t.clone());
                t
            }
        };
        let href = match href {
            Some(h) => h,
            None if !hyper => String::new(),
            None => return None,
        };
        doc.used.insert(key.to_string());
        (!ty.is_empty()).then_some((v, href, ty))
    }

    /// `\cref{keys}`, `\Cref{keys}`, `\labelcref{keys}` (`kind`), `\crefrange{a}{b}` and
    /// `\Crefrange` (`range`).
    pub(super) fn cref(&mut self, kind: u8, range: bool) {
        if self.next_is('*') {
            return self.not_native("cleveref: a starred reference");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut keys: Vec<String> = vec![arg.iter().map(|t| t.chr()).collect()];
        if range {
            let mut b = Vec::new();
            self.read_arg(&mut b);
            keys.push(b.iter().map(|t| t.chr()).collect());
        } else {
            keys = keys[0].split(',').map(|k| k.trim().to_string()).collect();
        }
        if arg.iter().any(|t| t.cs_id().is_some()) || keys.iter().any(|k| k.is_empty()) {
            return self.not_native("cleveref: a key with commands or a blank key");
        }
        let mut items = Vec::new();
        let mut ty: Option<String> = None;
        for key in &keys {
            let Some((value, href, t)) = self.cref_label(key) else {
                return self.not_native("cleveref: a reference with no \\label, or whose type is unknown");
            };
            // `subsection` and `section` (`copied_from`) group as one.
            let t = self.cref_canon(&t);
            if ty.as_ref().is_some_and(|x| *x != t) {
                return self.not_native("cleveref: references of more than one type");
            }
            ty = Some(t);
            items.push((value, href));
        }
        let Some(ty) = ty else { return };
        // `sort&compress`: by value, then runs of three or more consecutive values as ranges.
        let mut groups: Vec<(usize, usize)> = Vec::new();
        if range {
            groups.push((0, 1));
        } else if items.len() == 1 {
            groups.push((0, 0));
        } else {
            let Some(nums) = items.iter().map(|i| value_numbers(&i.0)).collect::<Option<Vec<_>>>() else {
                return self.not_native("cleveref: several references whose values are not numbers");
            };
            let mut order: Vec<usize> = (0..items.len()).collect();
            order.sort_by(|&a, &b| nums[a].cmp(&nums[b]));
            if order.windows(2).any(|w| nums[w[0]] == nums[w[1]]) {
                return self.not_native("cleveref: a key given twice");
            }
            let next = |a: &[u32], b: &[u32]| a.len() == b.len() && a[..a.len() - 1] == b[..b.len() - 1] && a[a.len() - 1] + 1 == b[b.len() - 1];
            let mut k = 0;
            while k < order.len() {
                let mut j = k;
                while j + 1 < order.len() && next(&nums[order[j]], &nums[order[j + 1]]) {
                    j += 1;
                }
                if j - k >= 2 {
                    groups.push((order[k], order[j]));
                } else {
                    for &o in &order[k..=j] {
                        groups.push((o, o));
                    }
                }
                k = j + 1;
            }
        }
        let plural = items.len() > 1;
        let name = if kind == LABEL {
            String::new()
        } else {
            let Some(name) = self.cref_type_name(&ty, kind == BIG, plural) else {
                return self.not_native("cleveref: a type with no name (or no plural name)");
            };
            name
        };
        let nbsp = self.named_cs(named!("nobreakspace"));
        let space = Token::char(Cat::Space, ' ');
        let hyper = self.hyper();
        let mut t = Vec::new();
        // `\@tempname\nobreakspace` unless the name is empty; under `nameinlink` (with
        // hyperref) inside the first label's link, which starts before it.
        let in_link = hyper && self.doc.as_ref().and_then(|d| d.pkg.cref.as_ref()).is_some_and(|c| c.name_in_link);
        let mut lead = Vec::new();
        if !name.is_empty() {
            lead.extend(self.tex_tokens(&name));
            lead.push(nbsp);
        }
        if !in_link {
            t.append(&mut lead);
        }
        let mut queue = Vec::new();
        let conj = |m: &mut Self, t: &mut Vec<Token>, word: &str| {
            if word == "," {
                t.extend([Token::char(Cat::Other, ','), space]);
            } else {
                t.push(space);
                t.extend(m.tex_tokens(word));
                t.push(nbsp);
            }
        };
        for (g, &(a, b)) in groups.iter().enumerate() {
            if g > 0 {
                conj(self, &mut t, if g + 1 < groups.len() { "," } else { "and" });
            }
            let lead = std::mem::take(&mut lead);
            self.cref_item(&ty, &items[a].0, hyper, &lead, in_link, &mut t);
            queue.push(items[a].1.clone());
            if a != b {
                conj(self, &mut t, "to");
                self.cref_item(&ty, &items[b].0, hyper, &[], in_link, &mut t);
                queue.push(items[b].1.clone());
            }
        }
        if let Some(c) = self.doc.as_mut().and_then(|d| d.pkg.cref.as_mut()).filter(|_| hyper) {
            c.queue.extend(queue);
        }
        self.back_list(&t);
    }

    /// One label as `\creflabelformat` makes it: `#2#1#3`, or `\textup{(#2#1#3)}` for an
    /// equation; with hyperref `#2` is `\hyper@@link[link]`'s start (`rt@creflink`, the next
    /// anchor of the queue), `#3` its end, without it both empty. Under `nameinlink`
    /// (`in_link`) an equation's is `#2\textup{(#1)}#3`, and `lead` (the name) goes after the
    /// first link's start.
    fn cref_item(&mut self, ty: &str, value: &str, hyper: bool, lead: &[Token], in_link: bool, t: &mut Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        if hyper && in_link {
            let eq = ty == "equation";
            t.extend([self.doc_id(super::LEAVE), self.doc_id(doc!("rt@creflink")), self.named_cs(named!("begingroup"))]);
            t.extend_from_slice(lead);
            if eq {
                t.extend([self.doc_id(doc!("textup")), open, Token::char(Cat::Other, '(')]);
            }
            t.extend(label_tokens(value));
            if eq {
                t.extend([Token::char(Cat::Other, ')'), close, self.named_cs(named!("relax"))]);
            }
            t.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
            return;
        }
        if ty == "equation" {
            t.extend([self.doc_id(doc!("textup")), open, Token::char(Cat::Other, '(')]);
        }
        if hyper {
            t.extend([self.doc_id(super::LEAVE), self.doc_id(doc!("rt@creflink")), self.named_cs(named!("begingroup"))]);
            t.extend(label_tokens(value));
            t.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
        } else {
            t.extend(label_tokens(value));
        }
        if ty == "equation" {
            // `\check@icr` meets cleveref's macros, not a following `.` or `,`: the
            // italic correction always (`\relax` stands in for them).
            t.extend([Token::char(Cat::Other, ')'), close, self.named_cs(named!("relax"))]);
        }
    }

    /// The start of the next link of a reference: its anchor, then hyperref's link start.
    fn cref_link(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(name) = doc.pkg.cref.as_mut().and_then(|c| c.queue.pop_front()) else {
            return self.not_native("cleveref: a link with no anchor");
        };
        doc.link = Some((super::hyperref::LINK, name));
        let t = self.doc_id(super::START_LINK_NATIVE);
        self.input.back(t);
    }
}
