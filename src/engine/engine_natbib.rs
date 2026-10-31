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
        let mut n = Natbib { open: c("("), close: c(")"), sep: c(";"), aysep: c(","), yrsep: c(","), cmt: c(", "), numbers: false, sup: false, bibstyle: true };
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
        self.doc.as_ref().and_then(|d| d.natbib.as_deref())
    }

    /// `\usepackage[opts]{natbib}`, once.
    pub(super) fn natbib_load(&mut self, opts: &str) {
        if self.natbib().is_some() {
            return self.not_native("natbib loaded twice");
        }
        match Natbib::options(opts) {
            Ok(n) => {
                self.load_package("natbib");
                if let Some(doc) = self.doc.as_mut() {
                    doc.natbib = Some(Box::new(n));
                }
            }
            Err(what) => self.not_native(what),
        }
    }

    /// natbib at `\begin{document}`: the aux file's `\bibstyle` lines (each live one, in
    /// order), then `\global\let\bibstyle\@gobble` and `\NAT@set@cites`.
    pub(super) fn natbib_begin_document(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(n) = doc.natbib.as_mut() else { return };
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
        if kind.star {
            let _ = self.next_is('*');
        }
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
        self.nat_cite_keys(kind, &pre, &post, &keys, false);
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
        self.nat_cite_keys(Cite { swa: true, par: false, ctype: 0, star: false }, &[], &[], &keys, true);
    }

    /// The citation's tokens, all keys undefined. Author-year (`\NAT@citex`): each key
    /// `{\reset@font\bfseries ?}`; when textual, `\NAT@cmt post` after them (`\NAT@date`
    /// is empty: no `\NAT@@close`). Numbers (`\NAT@citexnum`): each key
    /// `{\reset@font\bfseries?}` and the space after it in natbib.sty; when textual,
    /// `\NAT@cmt post` (`\NAT@ctype` 0 only) and `\NAT@mbox{\NAT@@close}`.
    fn nat_cite_keys(&mut self, kind: Cite, pre: &[Token], post: &[Token], keys: &[Token], num: bool) {
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
        let open_close = n.open.iter().chain(&n.close).chain(&n.cmt);
        if open_close.clone().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space))) {
            return self.not_native("natbib punctuation with commands");
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
        let Some(n) = self.doc.as_mut().and_then(|d| d.natbib.as_mut()) else { return self.not_native("natbib") };
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
        let Some(n) = self.doc.as_mut().and_then(|d| d.natbib.as_mut()) else { return self.not_native("natbib") };
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
        let Some(n) = self.doc.as_mut().and_then(|d| d.natbib.as_mut()) else { return self.not_native("natbib") };
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
}
