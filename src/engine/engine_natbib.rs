//! natbib on the GDL path (natbib.sty 8.31b): the package options, `\bibpunct`,
//! `\setcitestyle`, `\citestyle`, the `\bibstyle` that `\bibliographystyle` leaves in the
//! aux file, and the citation commands for keys no `\bibcite` defines.
//!
//! One pass and no aux file: pdfLaTeX's final run reads `\bibstyle{x}` from the aux file
//! in `\document`, before the `\AtBeginDocument` hooks (`latex.ltx` `\document`), so the
//! style's `\bibpunct` (natbib's `\bibstyle@x`) holds unless an option or the preamble
//! has made `\bibstyle` `\@gobble`. The job reads the document's `\bibliographystyle`
//! names off the source ([`styles`]) and applies them at `\begin{document}`.
//!
//! Every key is undefined: natbib's `thebibliography` and `\bibitem` (the `.bbl`) are
//! not native yet, so a document with one goes to the engine (the pre-scan). An
//! undefined key is natbib's `{\reset@font\bfseries ?}` in `\NAT@citex` (author-year)
//! and `{\reset@font\bfseries?}` and a space in `\NAT@citexnum` (numbers); neither sets
//! `\@citea`, so several keys run together.

use super::super::engine_catcode::Cat;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;

/// `s` as tokens: letters, others, spaces, and `~` active (a label as a `.bbl` holds it).
fn label_tokens(s: &str) -> Vec<Token> {
    s.chars().map(|c| match c {
        '~' => Token::char(Cat::Active, '~'),
        _ => super::label_tokens(&c.to_string())[0],
    }).collect()
}

/// A defined key's `\b@key` split by `\NAT@split`: `\NAT@num`, `\NAT@name`, `\NAT@date`
/// and `\NAT@all@names` (the names when the label has none of its own).
pub(super) struct Entry {
    num: String,
    name: Vec<Token>,
    date: Vec<Token>,
    all: Vec<Token>,
}

/// natbib's `\NAT@bare#1(#2)#3(@)#4\@nil` on a `\bibitem` label (`name(year)names`, then
/// `(@)(@)`): the name, the year and the full names (the name when there are none).
/// `None` for a label with commands, braces or no `(` (`\NAT@apalk`'s `name, year`), or
/// a year starting with `@`.
fn bare(label: &[Token]) -> Option<(Vec<Token>, Vec<Token>, Vec<Token>)> {
    if label.iter().any(|t| !(matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) || t.cat() == Some(Cat::Active) && t.chr() == '~')) {
        return None;
    }
    let is = |t: &Token, c: char| t.cat() == Some(Cat::Other) && t.chr() == c;
    let i = label.iter().position(|t| is(t, '('))?;
    let after = &label[i + 1..];
    let j = after.iter().position(|t| is(t, ')'))?;
    let (name, date, rest) = (&label[..i], &after[..j], &after[j + 1..]);
    if date.first().is_some_and(|t| is(t, '@')) || rest.iter().any(|t| is(t, '(') || is(t, ')') || is(t, '@')) {
        return None;
    }
    let all = if rest.is_empty() { name } else { rest };
    Some((name.to_vec(), date.to_vec(), all.to_vec()))
}

/// natbib's citation punctuation and mode: `\NAT@open`, `\NAT@close`, `\NAT@sep`,
/// `\NAT@aysep`, `\NAT@yrsep`, `\NAT@cmt` as tokens, `\ifNAT@numbers`, `\ifNAT@super`,
/// and whether `\bibstyle` is still `\csname bibstyle@#1\endcsname` (else `\@gobble`).
#[derive(PartialEq, Clone)]
pub(super) struct Natbib {
    pub open: Vec<Token>,
    pub close: Vec<Token>,
    pub sep: Vec<Token>,
    pub aysep: Vec<Token>,
    pub yrsep: Vec<Token>,
    pub cmt: Vec<Token>,
    pub numbers: bool,
    pub sup: bool,
    pub bibstyle: bool,
    /// An option that changes a defined key's citation or the bibliography in a way the
    /// native job does not follow (`sort`, `compress`, `longnamesfirst`, `nonamebreak`,
    /// `openbib`, `sectionbib`): defined keys and `thebibliography` are not native.
    pub other: Option<&'static str>,
    /// `\c@NAT@ctr`: the `\bibitem`s of the current `thebibliography` so far.
    pub ctr: u32,
    /// `\bibhang`: `\setlength{\bibhang}{1em}` in the font current when natbib loads.
    pub bibhang: i32,
}

/// natbib's options in the order `\DeclareOption` declares them: `\ProcessOptions` runs
/// the given ones in this order, not the document's.
const OPTIONS: [&str; 22] = ["numbers", "super", "authoryear", "round", "square", "angle", "curly", "comma", "semicolon", "colon", "nobibstyle", "bibstyle", "openbib", "sectionbib", "sort", "compress", "sort&compress", "mcite", "merge", "elide", "longnamesfirst", "nonamebreak"];

/// natbib's `\bibstyle@<name>`s: `\bibpunct{open}{close}{sep}{mode}{aysep}{yrsep}`.
/// `cospar`, `esa` and `nature` also set `\bibnumfmt`, which only the bibliography reads.
const STYLES: [(&str, [&str; 6]); 23] = [
    ("chicago", ["(", ")", ";", "a", ",", ","]),
    ("named", ["[", "]", ";", "a", ",", ","]),
    ("agu", ["[", "]", ";", "a", ",", ",~"]),
    ("copernicus", ["(", ")", ";", "a", ",", ","]),
    ("egu", ["(", ")", ";", "a", ",", ","]),
    ("egs", ["(", ")", ";", "a", ",", ","]),
    ("agsm", ["(", ")", ",", "a", "", ","]),
    ("kluwer", ["(", ")", ",", "a", "", ","]),
    ("dcu", ["(", ")", ";", "a", ";", ","]),
    ("aa", ["(", ")", ";", "a", "", ","]),
    ("pass", ["(", ")", ";", "a", ",", ","]),
    ("anngeo", ["(", ")", ";", "a", ",", ","]),
    ("nlinproc", ["(", ")", ";", "a", ",", ","]),
    ("cospar", ["/", "/", ",", "n", "", ""]),
    ("esa", ["(Ref.~", ")", ",", "n", "", ""]),
    ("nature", ["", "", ",", "s", "", "\\textsuperscript{,}"]),
    ("plain", ["[", "]", ",", "n", "", ","]),
    ("alpha", ["[", "]", ",", "n", "", ","]),
    ("abbrv", ["[", "]", ",", "n", "", ","]),
    ("unsrt", ["[", "]", ",", "n", "", ","]),
    ("plainnat", ["[", "]", ",", "a", ",", ","]),
    ("abbrvnat", ["[", "]", ",", "a", ",", ","]),
    ("unsrtnat", ["[", "]", ",", "a", ",", ","]),
];

/// `s` as character tokens, as natbib's source has them (`~` active, a space a space).
/// `None` for a control sequence in it (`\textsuperscript`).
fn chars(s: &str) -> Option<Vec<Token>> {
    if s.contains('\\') {
        return None;
    }
    Some(s.chars().map(|c| Token::char(if c == ' ' { Cat::Space } else if c == '~' { Cat::Active } else if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other }, c)).collect())
}

impl Natbib {
    /// The state after `\usepackage[opts]{natbib}`: the defaults (`\NAT@open` `(`,
    /// `\NAT@close` `)`, `\NAT@sep` `;`, author-year, `\bibstyle` live), then
    /// `\ProcessOptions` in declaration order, then `\NAT@aysep`, `\NAT@yrsep` `,` and
    /// `\NAT@cmt` `, `, which natbib defines after it. Options it declares that change
    /// what the native citations do not implement (`super`, `angle`, `curly`, `mcite`,
    /// `merge`, `elide`) and unknown options (natbib's error) are `Err`.
    pub(super) fn options(opts: &str) -> Result<Natbib, &'static str> {
        let given: Vec<&str> = opts.split(',').map(str::trim).filter(|o| !o.is_empty()).collect();
        if given.iter().any(|o| !OPTIONS.contains(o)) {
            return Err("a natbib option it does not declare");
        }
        let c = |s: &str| chars(s).unwrap_or_default();
        let mut n = Natbib { open: c("("), close: c(")"), sep: c(";"), aysep: c(","), yrsep: c(","), cmt: c(", "), numbers: false, sup: false, bibstyle: true, other: None, ctr: 0, bibhang: 0 };
        for o in OPTIONS.iter().filter(|o| given.contains(o)) {
            match *o {
                "super" | "angle" | "curly" | "mcite" | "merge" | "elide" => return Err("a natbib option the native citations do not implement"),
                // `\ExecuteOptions{square,comma,nobibstyle}`.
                "numbers" => {
                    n.numbers = true;
                    (n.open, n.close, n.sep, n.bibstyle) = (c("["), c("]"), c(","), false);
                }
                // `\ExecuteOptions{round,semicolon,bibstyle}`.
                "authoryear" => {
                    n.numbers = false;
                    (n.open, n.close, n.sep, n.bibstyle) = (c("("), c(")"), c(";"), true);
                }
                "round" => (n.open, n.close, n.bibstyle) = (c("("), c(")"), false),
                "square" => (n.open, n.close, n.bibstyle) = (c("["), c("]"), false),
                "comma" => (n.sep, n.bibstyle) = (c(","), false),
                "semicolon" | "colon" => (n.sep, n.bibstyle) = (c(";"), false),
                "nobibstyle" => n.bibstyle = false,
                "bibstyle" => n.bibstyle = true,
                // The bibliography's, the sort of numbers (an undefined key does not sort)
                // and the names of defined keys: nothing an undefined citation shows.
                "sort" | "compress" | "sort&compress" | "longnamesfirst" | "nonamebreak" | "openbib" | "sectionbib" => n.other = Some("a natbib option that changes defined citations or the bibliography"),
                _ => {}
            }
        }
        Ok(n)
    }

    /// `\bibpunct[cmt]{open}{close}{sep}{mode}{aysep}{yrsep}`: the `\gdef`s, `n` numbers,
    /// `s` numbers and super, anything else author-year.
    fn punct(&mut self, args: [Vec<Token>; 7]) {
        let [cmt, open, close, sep, mode, aysep, yrsep] = args;
        // `\ifx #5n`: the first token of `mode`.
        let m = mode.first().map(|t| (t.cs_id().is_none()).then(|| t.chr()));
        (self.numbers, self.sup) = match m {
            Some(Some('n')) => (true, false),
            Some(Some('s')) => (true, true),
            _ => (false, self.sup),
        };
        (self.open, self.close, self.sep, self.aysep, self.yrsep, self.cmt) = (open, close, sep, aysep, yrsep, cmt);
    }

    /// `\bibstyle{name}` while it is live: `\csname bibstyle@name\endcsname`, whose
    /// `\bibpunct` makes `\bibstyle` `\@gobble` (`\NAT@@setcites` before the document).
    /// A name natbib does not define is `\relax`: nothing. `Err` for a style whose
    /// punctuation has a control sequence (`nature`).
    pub(super) fn style(&mut self, name: &str) -> Result<(), &'static str> {
        if !self.bibstyle {
            return Ok(());
        }
        let Some((_, p)) = STYLES.iter().find(|s| s.0 == name) else { return Ok(()) };
        let mut args: [Vec<Token>; 7] = Default::default();
        args[0] = chars(", ").unwrap_or_default();
        for (k, s) in p.iter().enumerate() {
            args[k + 1] = chars(s).ok_or("a natbib style with commands in its punctuation")?;
        }
        self.punct(args);
        self.bibstyle = false;
        Ok(())
    }
}

/// The `\bibliographystyle{name}` names of `src` outside comments, in order: the
/// `\bibstyle` lines its aux file holds for the final run.
pub(super) fn styles(src: &str) -> Vec<String> {
    const NAME: &str = "\\bibliographystyle";
    let mut out = Vec::new();
    // Each occurrence found by the substring search rather than by a walk of every line (the
    // document is read whole, so a walk costs per page): kept when no comment (a `%` not
    // after a backslash) starts before it on its line, its argument read up to the comment.
    for (at, _) in src.match_indices(NAME) {
        let start = src[..at].rfind('\n').map_or(0, |k| k + 1);
        let end = src[at..].find('\n').map_or(src.len(), |k| at + k);
        let b = &src.as_bytes()[start..end];
        let code_end = (0..b.len()).find(|&k| b[k] == b'%' && (k == 0 || b[k - 1] != b'\\')).map_or(end, |k| start + k);
        if code_end < at + NAME.len() {
            continue;
        }
        if let Some(name) = src[at + NAME.len()..code_end].trim_start().strip_prefix('{').and_then(|r| r.split_once('}')).map(|r| r.0) {
            out.push(name.trim().to_string());
        }
    }
    out
}

/// A citation command's natbib flags: `\ifNAT@swa` (parenthetical), `\ifNAT@par`
/// (brackets), `\NAT@ctype` (0 names and years, 1 `\citeauthor`, 2 `\citeyear`, 3 an
/// alias) and whether it takes `*` (`\@ifstar`).
#[derive(Clone, Copy)]
pub(super) struct Cite {
    pub swa: bool,
    pub par: bool,
    pub ctype: u8,
    pub star: bool,
}

/// `\citet`, `\citep`, `\citealt`, `\citealp`, `\citeauthor` (and their capitalised forms,
/// the same for an undefined key), `\citeyear`, `\citeyearpar`, `\citetalias`,
/// `\citepalias`.
pub(super) const CITET: Cite = Cite { swa: false, par: true, ctype: 0, star: true };
pub(super) const CITEP: Cite = Cite { swa: true, par: true, ctype: 0, star: true };
pub(super) const CITEALT: Cite = Cite { swa: false, par: false, ctype: 0, star: true };
pub(super) const CITEALP: Cite = Cite { swa: true, par: false, ctype: 0, star: true };
pub(super) const CITEAUTHOR: Cite = Cite { swa: false, par: false, ctype: 1, star: true };
pub(super) const CITEYEAR: Cite = Cite { swa: false, par: false, ctype: 2, star: false };
pub(super) const CITEYEARPAR: Cite = Cite { swa: true, par: true, ctype: 2, star: false };
pub(super) const CITETALIAS: Cite = Cite { swa: false, par: false, ctype: 3, star: false };
pub(super) const CITEPALIAS: Cite = Cite { swa: true, par: true, ctype: 3, star: false };

/// natbib's `\if*#1*` on a note: empty, or not. `None` when `\if` would expand its first
/// token (a command or an active character) or meet a `*`: not native.
fn note_empty(note: &[Token]) -> Option<bool> {
    match note.first() {
        None => Some(true),
        Some(t) if matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space)) && t.chr() != '*' => Some(false),
        _ => None,
    }
}

impl Machine<'_, '_> {
    /// The natbib state, or `None` (not loaded).
    fn natbib(&self) -> Option<&Natbib> {
        self.doc.as_ref().and_then(|d| d.pkg.natbib.as_deref())
    }

    /// `\usepackage[opts]{natbib}`, once.
    pub(super) fn natbib_load(&mut self, opts: &str) {
        if self.natbib().is_some() {
            return self.not_native("natbib loaded twice");
        }
        match Natbib::options(opts) {
            Ok(mut n) => {
                n.bibhang = self.glue_of("1em").width;
                self.load_package("natbib");
                if let Some(doc) = self.doc.as_mut() {
                    doc.pkg.natbib = Some(Box::new(n));
                }
            }
            Err(what) => self.not_native(what),
        }
    }

    /// natbib at `\begin{document}`: the aux file's `\bibstyle` lines (each live one, in
    /// order), then `\global\let\bibstyle\@gobble` and `\NAT@set@cites`.
    pub(super) fn natbib_begin_document(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(n) = doc.pkg.natbib.as_mut() else { return };
        for name in &doc.bibstyles {
            if let Err(what) = n.style(name) {
                return self.not_native(what);
            }
        }
        n.bibstyle = false;
    }

    /// A natbib citation (`\citep` and kin, [`Cite`]) of undefined keys. natbib's command is
    /// `\begingroup` and its flags, `\@ifstar`, then `\NAT@citetp`: `[post]` or
    /// `[pre][post]` (`\@ifnextchar[`), then the keys, into `\@citex` (`\NAT@citex` or
    /// `\NAT@citexnum`) and `\@cite` (`\NAT@cite`, `\NAT@citenum`, both the same here):
    /// `\NAT@@open`, `pre\NAT@spacechar`, the keys, `\NAT@cmt post`, `\NAT@@close` when
    /// parenthetical, else the keys alone; then `\endgroup`. `cite` is natbib's `\cite`
    /// (`\NAT@cites`): without `[` it is textual in author-year mode.
    pub(super) fn nat_cite(&mut self, mut kind: Cite, cite: bool) {
        if self.mode == Mode::Math {
            return self.not_native("a natbib citation in math");
        }
        // `\@ifstar{\NAT@fulltrue…}`: the full names (`\NAT@all@names`).
        let full = kind.star && self.next_is('*');
        let (mut pre, mut post) = (Vec::new(), Vec::new());
        if self.next_is('[') {
            let a = self.bracket_arg();
            if self.next_is('[') {
                (pre, post) = (a, self.bracket_arg());
            } else {
                post = a;
            }
        } else if cite && !self.natbib().is_some_and(|n| n.numbers) {
            kind.swa = false;
        }
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        self.nat_cite_keys(kind, &pre, &post, &keys, false, full);
    }

    /// `\citenum{keys}`: `\NAT@citexnum[][]` in a group with `\NAT@swatrue` and
    /// `\NAT@parfalse`, whatever the mode.
    pub(super) fn nat_citenum(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("a natbib citation in math");
        }
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        if keys.first().is_some_and(|t| *t == Token::char(Cat::Other, '[')) && keys.len() == 1 {
            return self.not_native("\\citenum with an optional argument");
        }
        self.nat_cite_keys(Cite { swa: true, par: false, ctype: 0, star: false }, &[], &[], &keys, true, false);
    }

    /// The citation's tokens, all keys undefined. Author-year (`\NAT@citex`): each key
    /// `{\reset@font\bfseries ?}`; when textual, `\NAT@cmt post` after them (`\NAT@date`
    /// is empty: no `\NAT@@close`). Numbers (`\NAT@citexnum`): each key
    /// `{\reset@font\bfseries?}` and the space after it in natbib.sty; when textual,
    /// `\NAT@cmt post` (`\NAT@ctype` 0 only) and `\NAT@mbox{\NAT@@close}`.
    fn nat_cite_keys(&mut self, kind: Cite, pre: &[Token], post: &[Token], keys: &[Token], num: bool, full: bool) {
        let Some(n) = self.natbib().cloned() else { return self.not_native("natbib") };
        if n.sup {
            return self.not_native("natbib's super mode");
        }
        let text: String = keys.iter().map(|t| t.chr()).collect();
        if keys.iter().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) || text.split(',').any(|k| k.trim().is_empty()) {
            return self.not_native("a natbib citation with a blank key or commands in its keys");
        }
        let (Some(pre_empty), Some(post_empty)) = (note_empty(pre), note_empty(post)) else {
            return self.not_native("a natbib note that starts with a command");
        };
        let numbers = n.numbers || num;
        let open_close = n.open.iter().chain(&n.close).chain(&n.cmt).chain(&n.sep).chain(&n.aysep);
        if open_close.clone().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) {
            return self.not_native("natbib punctuation with commands");
        }
        // `\@citeb` is each key with its leading spaces dropped (`\@firstofone`).
        let names: Vec<&str> = text.split(',').map(|k| k.trim_start_matches(' ')).collect();
        let entries: Vec<Option<Entry>> = names.iter().map(|k| self.nat_entry(k)).collect();
        if entries.iter().any(Option::is_some) || self.doc.as_ref().is_some_and(|d| d.labels.keys().chain(d.predicted.keys()).any(|k| k.starts_with(super::BIB))) {
            let Some(entries) = entries.into_iter().collect::<Option<Vec<Entry>>>() else {
                return self.not_native("a natbib citation of a key no \\bibitem defines");
            };
            return self.nat_cite_defined(&n, kind, pre, post, &names, &entries, num, full);
        }
        let space = Token::char(Cat::Space, ' ');
        let undefined = [Token::char(Cat::BeginGroup, '{'), self.doc_id(doc!("normalfont")), self.doc_id(doc!("bfseries")), Token::char(Cat::Other, '?'), Token::char(Cat::EndGroup, '}')];
        let mut body = Vec::new();
        for _ in text.split(',') {
            body.extend(undefined);
            if numbers {
                body.push(space);
            }
        }
        // `\NAT@cmt#2` when textual (`\NAT@citexnum`: for `\NAT@ctype` 0 only).
        if !kind.swa && !post_empty && (!numbers || kind.ctype == 0) {
            body.extend(&n.cmt);
            body.extend(post);
        }
        // `\NAT@mbox{\NAT@@close}`, with `\NAT@mbox` `\mbox`.
        if !kind.swa && numbers {
            body.extend([self.named_cs(named!("mbox")), Token::char(Cat::BeginGroup, '{')]);
            if kind.par {
                body.extend(&n.close);
            }
            body.push(Token::char(Cat::EndGroup, '}'));
        }
        let mut t = vec![self.named_cs(named!("begingroup"))];
        if kind.swa {
            if kind.par {
                t.extend(&n.open);
            }
            if !pre_empty {
                t.extend(pre);
                t.push(self.doc_id(doc!("rt@space")));
            }
            t.extend(body);
            if !post_empty {
                t.extend(&n.cmt);
                t.extend(post);
            }
            if kind.par {
                t.extend(&n.close);
            }
        } else {
            t.extend(body);
        }
        t.push(self.named_cs(named!("endgroup")));
        self.back_list(&t);
    }

    /// A defined key's `\b@key`, as `\NAT@parse` splits it: its `\bibitem`'s label
    /// (kept in memory, or the pre-scan's for a `\bibitem` ahead) split as `\NAT@bare`
    /// splits it, and its place in the list (`\NAT@num`). `None` for a key no `\bibitem`
    /// defines, or one whose value the native citations cannot read.
    fn nat_entry(&mut self, key: &str) -> Option<Entry> {
        let doc = self.doc.as_mut()?;
        let (lk, nk) = (format!("{}{key}", super::BIB), format!("{}{}{key}", super::BIB, super::BIB));
        let label: Vec<Token> = match doc.labels.get(&lk) {
            Some(v) => v.clone(),
            None => {
                let p = doc.predicted.get(&lk)?.clone();
                doc.forward.insert(lk.clone(), p.clone());
                label_tokens(&p)
            }
        };
        let num: String = match doc.labels.get(&nk) {
            Some(v) => v.iter().map(|t| t.chr()).collect(),
            None => {
                let p = doc.predicted.get(&nk)?.clone();
                doc.forward.insert(nk.clone(), p.clone());
                p
            }
        };
        doc.used.insert(lk);
        doc.used.insert(nk);
        let (name, date, all) = bare(&label)?;
        Some(Entry { num, name, date, all })
    }

    /// `\hyper@natlinkstart{key}` (hyperref: `\hyper@linkstart{cite}{cite.key}`, its
    /// `\pdfstartlink` and `\Hy@colorlink`'s `\begingroup`); nothing without hyperref.
    fn nat_link_start(&mut self, key: &str, t: &mut Vec<Token>) {
        if self.hyper() {
            t.extend([self.doc_id(doc!("rt@natlink")), Token::char(Cat::BeginGroup, '{')]);
            t.extend(label_tokens(key));
            t.extend([Token::char(Cat::EndGroup, '}'), self.named_cs(named!("begingroup"))]);
        }
    }

    /// `\hyper@natlinkend` (`\Hy@endcolorlink`'s `\endgroup`, `\pdfendlink`).
    fn nat_link_end(&mut self, t: &mut Vec<Token>) {
        if self.hyper() {
            t.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
        }
    }

    /// The citation of keys all defined. natbib's `\@citex` with `\NAT@ctype` 0, each key
    /// in `\NAT@hyper@`'s link. Author-year (`\NAT@citex`): parenthetical, `\@citea
    /// \NAT@hyper@{\NAT@nmfmt{\NAT@nm}\hyper@natlinkbreak{\NAT@aysep\NAT@spacechar}{key}
    /// \NAT@date}` then `\NAT@def@citea`; textual, the break `\NAT@spacechar\NAT@@open pre
    /// \NAT@spacechar`, then `\NAT@def@citea@close`, and after the keys `\NAT@cmt post
    /// \NAT@@close`. Numbers (`\NAT@citexnum`): parenthetical, `\@citea\mbox{\NAT@hyper@
    /// {{num}}}` then `\NAT@def@citea`; textual, `\@citea name\NAT@spacechar\NAT@mbox{\relax
    /// \NAT@@open}pre\NAT@spacechar\NAT@mbox{\NAT@hyper@{{num}}}` then `\NAT@def@citea@box`,
    /// after the keys `\NAT@cmt post\NAT@mbox{\NAT@@close}`. Then `\NAT@cite`
    /// (`\NAT@citenum`): `\NAT@@open pre\NAT@spacechar body\NAT@cmt post\NAT@@close` when
    /// parenthetical, else the body; then `\endgroup`.
    #[allow(clippy::too_many_arguments)]
    fn nat_cite_defined(&mut self, n: &Natbib, kind: Cite, pre: &[Token], post: &[Token], keys: &[&str], entries: &[Entry], num: bool, full: bool) {
        if let Some(what) = n.other {
            return self.not_native(what);
        }
        if kind.ctype != 0 || num {
            return self.not_native("a natbib citation of a defined key other than \\citet, \\citep, \\cite");
        }
        // `\pdfstartlink` in vertical mode is an error.
        if self.hyper() && self.mode == Mode::Vertical {
            return self.not_native("a natbib citation with hyperref that starts a paragraph");
        }
        let numbers = n.numbers;
        let (pre_empty, post_empty) = (pre.is_empty(), post.is_empty());
        let space = self.doc_id(doc!("rt@space"));
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mbox = self.named_cs(named!("mbox"));
        let relax = self.tex_tokens("\\relax")[0];
        let penalty = self.tex_tokens("\\penalty1000");
        let mut body = Vec::new();
        let mut last: Option<&[Token]> = None;
        for (k, (key, e)) in keys.iter().zip(entries).enumerate() {
            let nm: &[Token] = if full { &e.all } else { &e.name };
            if nm.is_empty() || e.date.is_empty() {
                return self.not_native("a natbib entry with no name or no year");
            }
            // The same names as the key before: `\NAT@yrsep` and the year alone.
            if last == Some(nm) {
                return self.not_native("a natbib citation of two entries with the same names");
            }
            last = Some(nm);
            // `\@citea`.
            if k > 0 {
                match (numbers, kind.swa) {
                    (false, true) => {}
                    (false, false) if kind.par => body.extend(&n.close),
                    (false, false) => {}
                    (true, true) => {}
                    (true, false) => {
                        body.extend([mbox, open]);
                        if kind.par {
                            body.extend(&n.close);
                        }
                        body.push(close);
                    }
                }
                body.extend(&n.sep);
                if numbers {
                    body.extend(penalty.iter().copied());
                }
                body.push(space);
            }
            let num = label_tokens(&e.num);
            if numbers {
                if !kind.swa {
                    // `\NAT@test{\@ne}` is `\NAT@nm`: the name with its braces.
                    body.push(open);
                    body.extend(nm);
                    body.extend([close, space, mbox, open, relax]);
                    if kind.par {
                        body.extend(&n.open);
                    }
                    body.push(close);
                    if !pre_empty {
                        body.extend(pre);
                        body.push(space);
                    }
                }
                body.extend([mbox, open]);
                self.nat_link_start(key, &mut body);
                body.push(open);
                body.extend(num);
                body.push(close);
                self.nat_link_end(&mut body);
                body.push(close);
                continue;
            }
            // `\NAT@nmfmt{\NAT@nm}`: `{\NAT@up{name}}`, `\NAT@up` `\relax`.
            self.nat_link_start(key, &mut body);
            body.extend([open, relax, open]);
            body.extend(nm);
            body.extend([close, close]);
            // `\hyper@natlinkbreak{x}{key}`: `\hyper@linkend x \hyper@linkstart{cite}{cite.key}`.
            self.nat_link_end(&mut body);
            if kind.swa {
                body.extend(&n.aysep);
                body.push(space);
            } else {
                body.push(space);
                if kind.par {
                    body.extend(&n.open);
                }
                if !pre_empty {
                    body.extend(pre);
                    body.push(space);
                }
            }
            self.nat_link_start(key, &mut body);
            body.extend(e.date.iter().copied());
            self.nat_link_end(&mut body);
        }
        if !kind.swa {
            if !post_empty {
                body.extend(&n.cmt);
                body.extend(post);
            }
            if numbers {
                body.extend([mbox, open]);
                if kind.par {
                    body.extend(&n.close);
                }
                body.push(close);
            } else if kind.par {
                body.extend(&n.close);
            }
        }
        let mut t = vec![self.named_cs(named!("begingroup"))];
        if kind.swa {
            if kind.par {
                t.extend(&n.open);
            }
            if !pre_empty {
                t.extend(pre);
                t.push(space);
            }
            t.extend(body);
            if !post_empty {
                t.extend(&n.cmt);
                t.extend(post);
            }
            if kind.par {
                t.extend(&n.close);
            }
        } else {
            t.extend(body);
        }
        t.push(self.named_cs(named!("endgroup")));
        self.back_list(&t);
    }

    /// `\hyper@natlinkstart{key}` with hyperref: the link to `cite.key`, its colour.
    fn nat_link_native(&mut self) {
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let name: String = key.iter().map(|t| t.chr()).collect();
        if let Some(doc) = self.doc.as_mut() {
            doc.link = Some((super::hyperref::CITE, format!("cite.{name}")));
        }
        self.start_link();
    }

    /// natbib's `thebibliography{widest}`: `\bibsection`, `\parindent\z@`, then `\list`
    /// (`nat_bib_open`). `\bibsection` is article's `\section*{\refname\@mkboth{…}{…}}`,
    /// report's and book's `\chapter*{\bibname\@mkboth{…}{…}}` (the marks in the title: not
    /// native unless `\@mkboth` gobbles them), KOMA's `\bib@heading` (`bib_marks`).
    pub(super) fn nat_bib_begin(&mut self) {
        let Some(n) = self.natbib() else { return };
        if let Some(what) = n.other {
            return self.not_native(what);
        }
        // natbib's `\bibnumfmt` and author-year `\hfill` are its own `\@biblabel`s.
        let biblabel = self.e.cs.intern("@biblabel");
        if !matches!(self.e.eqtb.meaning(biblabel), super::Meaning::Unset | super::Meaning::Undefined) {
            return self.not_native("natbib with a \\@biblabel of the document's own");
        }
        let (report, koma_book) = self.doc.as_ref().map_or((false, false), |d| (d.report, d.pkg.koma.book));
        if report && self.babel_date().is_some_and(|k| k > super::babel::BRITISH) {
            return self.not_native("babel: thebibliography's \\bibname in another language");
        }
        // The marks in the title (`rt@natbibmk`), where its paragraph is set: fancyhdr's
        // `\markboth`, as report's `\tableofcontents` has them, or book's `headings`.
        // Under a titlesec heading `\ttl@finmarks` puts them after it (`fancy_finmarks`).
        let titlesec = self.doc.as_ref().is_some_and(|d| d.titlesec.as_ref().is_some_and(|t| !d.report || t.chapter));
        let headings = !koma_book && self.doc.as_ref().is_some_and(|d| d.headings);
        let mut fancy = !koma_book && self.fancy_on_now();
        if headings && titlesec {
            return self.not_native("natbib: the marks of thebibliography's titlesec heading");
        }
        if fancy && titlesec {
            let name = self.tex_tokens(if report { "Bibliography" } else { "References" });
            if let Some(f) = self.doc.as_mut().and_then(|d| d.fancy.as_mut()) {
                f.mkboth = Some(name);
            }
            fancy = false;
        }
        // book's `headings`: `\markboth` in the title, as its `\tableofcontents` has it.
        let fancy = fancy || headings;
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        self.begin_group(super::SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = if report {
            let mut t = vec![self.doc_id(doc!("chapter")), Token::char(Cat::Other, '*'), open];
            // A `\renewcommand{\bibname}` of the document's (`own_head_name`).
            let Some(own) = self.own_head_name("bibname") else { return };
            match own {
                Some(cs) => t.push(cs),
                None => t.extend(label_tokens("Bibliography")),
            }
            if fancy {
                t.push(self.doc_id(doc!("rt@natbibmk")));
            }
            t.push(close);
            if koma_book {
                t.push(self.doc_id(doc!("rt@bibmk")));
            }
            t
        } else {
            let mut t = vec![self.doc_id(doc!("section")), Token::char(Cat::Other, '*'), open, self.named_cs(named!("refname"))];
            if fancy {
                t.push(self.doc_id(doc!("rt@natbibmk")));
            }
            t.push(close);
            t
        };
        // `\settowidth\labelwidth{\@biblabel{#1}}` in numbers mode, `\bibnumfmt` the
        // kernel's `[#1]`.
        t.extend([self.doc_id(doc!("rt@bibbox")), Token::char(Cat::Other, '[')]);
        t.extend(arg);
        t.extend([Token::char(Cat::Other, ']'), close, self.doc_id(doc!("rt@natbibopen"))]);
        self.back_list(&t);
    }

    /// natbib's `\list{…}{\@bibsetup{#1}\global\c@NAT@ctr\z@}`: `\NAT@bibsetnum` (numbers)
    /// `\labelwidth` the label's width, `\leftmargin\labelwidth+\labelsep`; `\NAT@bibsetup`
    /// (author-year) `\leftmargin\bibhang` (1em) `\itemindent-\leftmargin`; both
    /// `\itemsep\bibsep` (`\@listi`'s `\itemsep` plus `\parsep`, as natbib took them at
    /// load) `\parsep\z@`. Then `\sloppy\clubpenalty4000\widowpenalty4000\sfcode`\.\@m`;
    /// `\@clubpenalty` stays (the kernel's environment sets it, natbib's does not).
    fn nat_bib_open(&mut self) {
        use super::{Label, List, Make, R_ENUMI, R_ITEMINDENT, R_ITEMSEP, R_LABELSEP, R_LABELWIDTH, R_LEFTMARGIN, R_PARSEP, TEMP_BOX};
        let w = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        self.e.eqtb.replace_box(TEMP_BOX, None);
        self.set_dimen_named(param!("parindent"), 0);
        let list = List { label: Label::Bib, make: Make::Mklab, numbered: false, counter: R_ENUMI + 3, centercr: false, auto_item: false, endpe_false: false };
        if !self.list_open(list) {
            return;
        }
        let Some(numbers) = self.natbib().map(|n| n.numbers) else { return };
        if numbers {
            self.set_rw(R_LABELWIDTH, w, false);
            let sep = self.rw(R_LABELSEP);
            self.set_rw(R_LEFTMARGIN, w + sep, false);
        } else {
            let hang = self.natbib().map_or(0, |n| n.bibhang);
            self.set_rw(R_LEFTMARGIN, hang, false);
            self.set_rw(R_ITEMINDENT, -hang, false);
        }
        let bibsep = super::glue_add(self.rg(R_ITEMSEP), self.rg(R_PARSEP));
        self.set_rg(R_ITEMSEP, bibsep);
        self.set_rg(R_PARSEP, super::Glue::default());
        if let Some(n) = self.doc.as_mut().and_then(|d| d.pkg.natbib.as_mut()) {
            n.ctr = 0;
        }
        let t = self.doc_id(doc!("rt@sloppy"));
        self.input.back(t);
        self.trivlist(|m| m.list_tail());
    }

    /// natbib's `\bibitem[label]{key}` (`\@lbibitem`): `\global\advance\c@NAT@ctr\@ne`,
    /// `\item[\hfil\NAT@anchor{key}{\NAT@num}]` (`\NAT@anchor`: hyperref's raised anchor
    /// `cite.key`, then `\@biblabel{num}`, which is `\hfill` in author-year mode and
    /// `[num]` in numbers mode); `\NAT@num` is the aux file's (the same count) when the
    /// label is given. Then `\NAT@wrout`'s `\bibcite{key}{{num}{year}{{name}}{{names}}}`,
    /// kept in memory, and `\ignorespaces`. A label natbib's `\NAT@bare` does not split
    /// as `name(year)names`, or one with commands, is not native.
    pub(super) fn nat_bibitem(&mut self) {
        if !self.next_is('[') {
            return self.not_native("natbib: a \\bibitem with no label");
        }
        let label = self.bracket_arg();
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let key: String = key.iter().map(|t| t.chr()).collect();
        if self.doc.as_ref().and_then(|d| d.lists.last()).is_none_or(|l| l.label != super::Label::Bib) {
            return self.not_native("a \\bibitem outside thebibliography");
        }
        if bare(&label).is_none() {
            return self.not_native("natbib: a \\bibitem label it does not split as name(year)");
        }
        let Some(n) = self.doc.as_mut().and_then(|d| d.pkg.natbib.as_mut()) else { return };
        n.ctr += 1;
        let (ctr, numbers) = (n.ctr, n.numbers);
        let (lk, nk) = (format!("{}{key}", super::BIB), format!("{}{}{key}", super::BIB, super::BIB));
        let num = label_tokens(&ctr.to_string());
        if !self.defined_once(&lk, &label) || !self.defined_once(&nk, &num) {
            return;
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.labels.insert(lk, label);
        doc.labels.insert(nk, num.clone());
        let mut l = self.tex_tokens("\\hfil");
        if self.hyper() {
            l.extend(self.text_raised_link(format!("cite.{key}")));
        }
        if numbers {
            l.push(Token::char(Cat::Other, '['));
            l.extend(num);
            l.push(Token::char(Cat::Other, ']'));
        } else {
            l.extend(self.tex_tokens("\\hfill"));
        }
        self.item_start(Some(l));
    }

    /// `\citetext{text}`: `\NAT@open text\NAT@close`.
    pub(super) fn nat_citetext(&mut self) {
        let Some(n) = self.natbib().cloned() else { return self.not_native("natbib") };
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let mut t = n.open;
        t.extend(arg);
        t.extend(n.close);
        self.back_list(&t);
    }

    /// `\bibpunct[cmt]{open}{close}{sep}{mode}{aysep}{yrsep}` (`[, ]` by default), then
    /// `\NAT@@setcites`: `\bibstyle` gobbles in the preamble. Arguments with commands
    /// are not native.
    pub(super) fn nat_bibpunct(&mut self) {
        let mut args: [Vec<Token>; 7] = Default::default();
        args[0] = chars(", ").unwrap_or_default();
        if self.next_is('[') {
            args[0] = self.bracket_arg();
        }
        for a in args.iter_mut().skip(1) {
            self.read_arg(a);
        }
        if args.iter().flatten().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) {
            return self.not_native("\\bibpunct with commands");
        }
        let Some(n) = self.doc.as_mut().and_then(|d| d.pkg.natbib.as_mut()) else { return self.not_native("natbib") };
        n.punct(args);
        n.bibstyle = false;
    }

    /// `\setcitestyle{list}` in the preamble: each item of `\@for` (not trimmed) compared
    /// with the keywords; `key=value` items and the keywords that change what is not
    /// native (`angle`, `curly`, `super`) are not native. The macro's text starts with a
    /// space (natbib.sty's line end), so in the body it is not native either.
    pub(super) fn nat_setcitestyle(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\setcitestyle in the body");
        }
        if arg.iter().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) {
            return self.not_native("\\setcitestyle with commands");
        }
        let text: String = arg.iter().map(|t| t.chr()).collect();
        let Some(n) = self.doc.as_mut().and_then(|d| d.pkg.natbib.as_mut()) else { return self.not_native("natbib") };
        let c = |s: &str| chars(s).unwrap_or_default();
        let mut bad = false;
        for item in text.split(',') {
            match item {
                "round" => (n.open, n.close) = (c("("), c(")")),
                "square" => (n.open, n.close) = (c("["), c("]")),
                "semicolon" | "colon" => n.sep = c(";"),
                "comma" => n.sep = c(","),
                "authoryear" => n.numbers = false,
                "numbers" => (n.numbers, n.sup) = (true, false),
                "angle" | "curly" | "super" => bad = true,
                _ if item.contains('=') => bad = true,
                _ => {}
            }
        }
        if bad {
            return self.not_native("a \\setcitestyle key the native citations do not implement");
        }
        n.bibstyle = false;
    }

    /// `\citestyle{name}`: `\@citestyle{name}` (natbib's `\bibstyle@name`, gobbling
    /// `\bibstyle`), then `\let\bibstyle\@gobble`.
    pub(super) fn nat_citestyle(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| t.chr()).collect();
        if arg.iter().any(|t| t.cs_id().is_some()) {
            return self.not_native("\\citestyle with commands");
        }
        let Some(n) = self.doc.as_mut().and_then(|d| d.pkg.natbib.as_mut()) else { return self.not_native("natbib") };
        n.bibstyle = true;
        let r = n.style(&name);
        n.bibstyle = false;
        if let Err(what) = r {
            self.not_native(what);
        }
    }

    /// `\defcitealias{key}{text}`: `\@namedef{al@key}{text}`, read only by a defined key's
    /// alias citation; no node.
    pub(super) fn nat_defcitealias(&mut self) {
        let mut a = Vec::new();
        self.read_arg(&mut a);
        self.read_arg(&mut a);
    }
}


/// natbib's natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@citet", |m| m.nat_cite(CITET, false)),
    ("rt@citep", |m| m.nat_cite(CITEP, false)),
    ("rt@natcite", |m| m.nat_cite(CITEP, true)),
    ("rt@citealt", |m| m.nat_cite(CITEALT, false)),
    ("rt@citealp", |m| m.nat_cite(CITEALP, false)),
    ("rt@citeauthor", |m| m.nat_cite(CITEAUTHOR, false)),
    // `\citefullauthor` is `\citeauthor*`.
    ("rt@citefullauthor", |m| m.nat_cite(Cite { star: false, ..CITEAUTHOR }, false)),
    ("rt@citeyear", |m| m.nat_cite(CITEYEAR, false)),
    ("rt@citeyearpar", |m| m.nat_cite(CITEYEARPAR, false)),
    ("rt@citetalias", |m| m.nat_cite(CITETALIAS, false)),
    ("rt@citepalias", |m| m.nat_cite(CITEPALIAS, false)),
    ("rt@citenum", |m| m.nat_citenum()),
    ("rt@citetext", |m| m.nat_citetext()),
    ("rt@bibpunct", |m| m.nat_bibpunct()),
    ("rt@setcitestyle", |m| m.nat_setcitestyle()),
    ("rt@citestyle", |m| m.nat_citestyle()),
    ("rt@defcitealias", |m| m.nat_defcitealias()),
    ("rt@natlink", |m| m.nat_link_native()),
    ("rt@natbibopen", |m| m.nat_bib_open()),
    // `\@mkboth{\MakeUppercase{\refname}}{…}` (`\bibname` under report) in `\bibsection`'s
    // title, fancyhdr's `\markboth`.
    ("rt@natbibmk", |m| {
        let report = m.doc.as_ref().is_some_and(|d| d.report);
        let name = if report { "Bibliography" } else { "References" };
        if m.fancy_on_now() {
            return m.fancy_mkboth_upper(name);
        }
        m.mkboth_upper(name);
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &[Token]) -> String {
        t.iter().map(|t| t.chr()).collect()
    }

    /// `\ProcessOptions` runs natbib's options in declaration order: `authoryear`
    /// (`round,semicolon,bibstyle`) before `round` and `square` (each `nobibstyle`).
    #[test]
    fn options_in_declaration_order() {
        let n = Natbib::options("round,authoryear").expect("declared");
        assert!(!n.bibstyle && !n.numbers && text(&n.open) == "(");
        let n = Natbib::options("square, authoryear").expect("declared");
        assert!(!n.bibstyle && text(&n.open) == "[" && text(&n.sep) == ";");
        let n = Natbib::options("sort&compress,numbers").expect("declared");
        assert!(n.numbers && !n.bibstyle && text(&n.close) == "]" && text(&n.sep) == ",");
        let n = Natbib::options("authoryear").expect("declared");
        assert!(n.bibstyle && text(&n.cmt) == ", ");
        assert!(Natbib::options("super").is_err() && Natbib::options("nosuch").is_err());
    }

    /// A live `\bibstyle` takes the first style natbib defines and then gobbles.
    #[test]
    fn bibstyle_first_defined_wins() {
        let mut n = Natbib::options("").expect("no options");
        n.style("ieeetr").expect("undefined: relax");
        assert!(n.bibstyle);
        n.style("plain").expect("defined");
        assert!(!n.bibstyle && n.numbers && text(&n.open) == "[");
        n.style("chicago").expect("gobbled");
        assert!(n.numbers);
        assert_eq!(styles("\\bibliographystyle{a}% \\bibliographystyle{b}\n\\bibliographystyle {c}\n\\%\\bibliographystyle{d}"), ["a", "c", "d"]);
        assert_eq!(styles("x\\bibliographystyle{e%}\r\n%\\bibliographystyle{g}\n\\bibliographystyle{f}\r\n\\bibliographystyle"), ["f"]);
    }

    /// `\NAT@bare` splits `name(year)names`; full names default to the name; `name, year`
    /// labels (`\NAT@apalk`) and a year starting with `@` are not split here.
    #[test]
    fn bare_splits_labels_as_natbib() {
        let (n, d, a) = bare(&label_tokens("Goossens et~al.(1993)Goossens, Mittelbach, and Samarin")).expect("split");
        assert_eq!((text(&n), text(&d), text(&a)), ("Goossens et~al.".into(), "1993".into(), "Goossens, Mittelbach, and Samarin".into()));
        assert!(n[11].cat() == Some(Cat::Active));
        let (n, d, a) = bare(&label_tokens("Knuth(1984)")).expect("split");
        assert_eq!((text(&n), text(&d), text(&a)), ("Knuth".into(), "1984".into(), "Knuth".into()));
        assert!(bare(&label_tokens("Knuth, 1984")).is_none() && bare(&label_tokens("K(@x)")).is_none());
    }

    /// natbib's `.bbl` preamble is left out of the splice; the rest stays as it was.
    #[test]
    fn splice_drops_the_natbib_bbl_preamble() {
        let bbl = format!("\\begin{{thebibliography}}{{1}}\n{}\n\\bibitem[Knuth(1984)]{{k}}\nD.\n\n\\end{{thebibliography}}\n", super::super::NATBIB_BBL_PREAMBLE);
        let src = "\\begin{document}\nx\n\\bibliography{refs}\n\\end{document}\n";
        assert_eq!(super::super::splice_bibliography(src, Some(&bbl)), "\\begin{document}\nx\n\\begin{thebibliography}{1}\n\n\\bibitem[Knuth(1984)]{k}\nD.\n\n\\end{thebibliography}\n\\end{document}\n");
    }
}
