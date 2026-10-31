//! biblatex on the GDL path (biblatex.sty 3.x): the package options, `\addbibresource`,
//! `\printbibliography`, `\nocite` and the citation commands, for a document whose keys
//! no `.bbl` defines.
//!
//! No corpus template ships a `.bbl`, and neither pdfLaTeX nor RushTeX runs biber or
//! BibTeX: pdfLaTeX's final run has no entry data, so every key is missing and
//! `\printbibliography` typesets nothing (`\blx@printbibliography`: `\ifdefvoid\blx@tempa
//! {\blx@warn@bibempty\endgroup}`, a warning only). A missing key is
//! `\abx@missing{key}`, `\mbox{\reset@font\bfseries key}` of the detokenized key
//! (`\blx@citeadd`), keys joined by `\multicitedelim`. The command's pre- and postnote
//! and its precode and postcode run only for keys with data (`\blx@citeprint`, from
//! `\forlistloop` over the keys found), so a missing key prints alone inside the
//! command's wrapper (`\mkbibparens`, `\mkbibbrackets` or none, from the style's
//! `\DeclareCiteCommand`).

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Code;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;

/// The citation styles whose commands are checked: the `style=` (or `citestyle=`) value.
/// `numeric` (biblatex's default) and `numeric-comp`, `authoryear` and its `-comp`,
/// `-ibid` and `-icomp`, and `alphabetic`: their `.cbx` files' wrappers and
/// `\multicitedelim` are the same within each group.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Style {
    Numeric,
    AuthorYear,
    Alphabetic,
}

/// biblatex's state once loaded: the citation style and the `autocite` option's command
/// (`inline` is `\parencite`, `plain` is `\cite`; biblatex.def's `\DeclareAutoCiteCommand`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Biblatex {
    pub style: Style,
    pub auto_plain: bool,
}

/// What a citation command puts around its keys.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    /// `\cite`: `\mkbibbrackets` in numeric and alphabetic (`numeric.cbx`, `alphabetic.cbx`),
    /// none in authoryear (`authoryear.cbx`).
    Cite,
    /// `\parencite`: `\mkbibparens` in authoryear, `\mkbibbrackets` otherwise.
    Paren,
    /// `\autocite`: `\parencite` or `\cite` by the `autocite` option.
    Auto,
    /// `\textcite`, `\citeauthor`, `\citetitle`, `\citeyear`: no wrapper for a missing key
    /// (numeric's `\cbx@textcite@init` prints `\cbx@savedcites` only when
    /// `textcitetotal` is positive, which only a key with data steps).
    Bare,
}

/// Options that change nothing a missing key or an empty bibliography typesets: names
/// and sorting go to biber, the trackers and `defernumbers` to the bibliography.
const NEUTRAL: [&str; 24] = ["maxnames", "minnames", "maxbibnames", "minbibnames", "maxcitenames", "mincitenames", "maxalphanames", "minalphanames", "giveninits", "firstinits", "uniquename", "uniquelist", "url", "doi", "isbn", "eprint", "dashed", "sortcites", "defernumbers", "backref", "abbreviate", "autopunct", "parentracker", "natbib"];

/// biblatex's `sorting=` templates (`biblatex.def`'s `\DeclareSortingTemplate`).
const SORTINGS: [&str; 9] = ["nty", "nyt", "nyvt", "anyt", "anyvt", "ynt", "ydnt", "none", "debug"];

/// `\printbibliography`'s keys (`blx@bib1`, `blx@bib2`): with no data, each only sets a
/// macro the empty bibliography never reads.
const BIB_KEYS: [&str; 17] = ["heading", "title", "type", "nottype", "subtype", "notsubtype", "keyword", "notkeyword", "category", "notcategory", "check", "env", "resetnumbers", "section", "segment", "filter", "prenote"];

impl Biblatex {
    /// `\usepackage[opts]{biblatex}`: the style and `autocite`; an option not known to
    /// change nothing here is refused (a style not checked, `autocite=footnote`,
    /// `language`, `refsection`).
    pub(super) fn options(opts: &str) -> Result<Biblatex, &'static str> {
        let mut b = Biblatex { style: Style::Numeric, auto_plain: false };
        let mut cite = None;
        for o in opts.split(',').map(str::trim).filter(|o| !o.is_empty()) {
            let (k, v) = o.split_once('=').map_or((o, "true"), |(k, v)| (k.trim(), v.trim()));
            let style = |v: &str| match v {
                "numeric" | "numeric-comp" => Ok(Style::Numeric),
                "authoryear" | "authoryear-comp" | "authoryear-ibid" | "authoryear-icomp" => Ok(Style::AuthorYear),
                "alphabetic" => Ok(Style::Alphabetic),
                _ => Err("a biblatex style"),
            };
            match k {
                "style" | "citestyle" => cite = Some(style(v)?),
                // The bibliography style draws nothing without entries; only checked ones load.
                "bibstyle" => {
                    style(v)?;
                }
                "backend" if matches!(v, "biber" | "bibtex" | "bibtex8" | "bibtexu") => {}
                "sorting" if SORTINGS.contains(&v) => {}
                "autocite" if v == "inline" => b.auto_plain = false,
                "autocite" if v == "plain" => b.auto_plain = true,
                _ if NEUTRAL.contains(&k) && v.bytes().all(|c| c.is_ascii_alphanumeric()) => {}
                _ => return Err("a biblatex option"),
            }
        }
        if let Some(s) = cite {
            b.style = s;
        }
        Ok(b)
    }
}

impl Machine<'_, '_> {
    fn biblatex(&self) -> Option<Biblatex> {
        self.doc.as_ref().and_then(|d| d.biblatex.as_deref().copied())
    }

    /// `\usepackage[opts]{biblatex}`, once: its `\RequirePackage{url}` (biblatex.sty:44;
    /// etoolbox, kvoptions, logreq, ifthen and pdftexcmds define nothing the native job
    /// reads), then its commands.
    pub(super) fn biblatex_load(&mut self, opts: &str) {
        if self.biblatex().is_some() || self.doc.as_ref().is_some_and(|d| d.natbib.is_some()) {
            return self.not_native("biblatex loaded twice or with natbib");
        }
        match Biblatex::options(opts) {
            Ok(b) => {
                self.load_package("url");
                self.load_package("biblatex");
                if let Some(doc) = self.doc.as_mut() {
                    doc.biblatex = Some(Box::new(b));
                }
            }
            Err(what) => self.not_native(what),
        }
    }

    /// A biblatex citation command of missing keys. `\blx@citecmdinit` (`\leavevmode`),
    /// `\@ifstar`, `\blx@citeargs` (`[pre][post]`, `\@ifnextchar[` each), the keys; the
    /// notes are read and dropped. Each key is `\abx@missing{key}`, the keys after the
    /// first preceded by `\multicitedelim`: `\addsemicolon\space` (biblatex.def), in
    /// numeric `\addcomma\space` (`numeric.cbx`). The punctuation tracker's `\addcomma`
    /// after `\mbox` (space factor 1000, below `\blx@sf@threshold@low`) is the plain
    /// character. `\blx@setsfcodes` sets the sfcodes of `,;:.!?` to their usual values
    /// under `\nonfrenchspacing` and those of `([<>])` to 0: the space factor before the
    /// wrapper's `(` or `[` is 1000 (`\blx@leavevmode@cite`), so the document's own codes
    /// give the same. Under `\frenchspacing` (`\blx@setfrcodes`, 1003 to 1008) it is not
    /// native. A following punctuation mark taken by `\blx@checkpunct` comes back through
    /// `\blx@addpunct` after `)`, `]` or `\mbox` unchanged.
    pub(super) fn blx_cite(&mut self, kind: Kind) {
        let Some(b) = self.biblatex() else { return self.not_native("biblatex") };
        if self.mode == Mode::Math {
            return self.not_native("a biblatex citation in math");
        }
        if self.leave_vmode() {
            return;
        }
        let _ = self.next_is('*');
        if self.next_is('[') {
            self.bracket_arg();
            if self.next_is('[') {
                self.bracket_arg();
            }
        }
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        let text: String = keys.iter().map(|t| t.chr()).collect();
        let ok = |c: char| c.is_ascii_alphanumeric() || "-:._/+ ".contains(c);
        // `\detokenize` makes a subscript `_` a character like the rest. `\forcsvlist`
        // drops the spaces before a key, not those after it: `\cite{a ,b}` prints `a `.
        let list = || text.split(',').map(|k| k.trim_start_matches(' '));
        if keys.iter().any(|t| !matches!(t.cat(), Some(Cat::Letter | Cat::Other | Cat::Space | Cat::Subscript))) || list().any(|k| k.is_empty() || !k.chars().all(ok)) {
            return self.not_native("a biblatex citation with a blank key or other characters in its keys");
        }
        let delim = if b.style == Style::Numeric { ',' } else { ';' };
        let sf = |m: &Self, c: char| m.e.eqtb.code(Code::Sf, c as u32);
        if sf(self, '.') <= 2000 || sf(self, delim) != if delim == ',' { 1250 } else { 1500 } || sf(self, ')') != 0 || sf(self, ']') != 0 {
            return self.not_native("biblatex under \\frenchspacing or other sfcodes");
        }
        let kind = if kind == Kind::Auto { if b.auto_plain { Kind::Cite } else { Kind::Paren } } else { kind };
        let wrap = match kind {
            Kind::Cite if b.style == Style::AuthorYear => None,
            Kind::Paren if b.style == Style::AuthorYear => Some(('(', ')')),
            Kind::Cite | Kind::Paren => Some(('[', ']')),
            Kind::Auto | Kind::Bare => None,
        };
        let mut t = Vec::new();
        if let Some((open, _)) = wrap {
            t.push(Token::char(Cat::Other, open));
        }
        for (i, key) in list().enumerate() {
            if i > 0 {
                t.extend([Token::char(Cat::Other, delim), Token::char(Cat::Space, ' ')]);
            }
            t.extend([self.named_cs(named!("mbox")), Token::char(Cat::BeginGroup, '{'), self.doc_id(doc!("normalfont")), self.doc_id(doc!("bfseries"))]);
            t.extend(key.chars().map(|c| Token::char(if c == ' ' { Cat::Space } else { Cat::Other }, c)));
            t.push(Token::char(Cat::EndGroup, '}'));
        }
        if let Some((_, close)) = wrap {
            t.push(Token::char(Cat::Other, close));
        }
        // `\blx@checkpunct` (`\futurelet`, no space skipped) takes a following mark of
        // `\blx@autopunct` (`.,;:!?`) and `\blx@addpunct` sets it in the command's group,
        // under `\blx@setencodes`: `!` and `?` have sfcodes 3001 and 3002 there, so the
        // space after them stretches by that factor (the document's are 3000).
        if let Some(next) = self.get_token() {
            match [('!', 33, "3001"), ('?', 63, "3002")].into_iter().find(|p| next == Token::char(Cat::Other, p.0)) {
                Some((_, code, sf)) => {
                    let set = format!("\\begingroup\\sfcode{code}={sf} ");
                    t.extend(self.tex_tokens(&set));
                    t.push(next);
                    t.push(self.named_cs(named!("endgroup")));
                }
                None => self.input.back(next),
            }
        }
        self.back_list(&t);
    }

    /// `\nocite{keys}`: `\@bsphack`, aux and `.bcf` writes (`\immediate`), `\@esphack`.
    pub(super) fn blx_nocite(&mut self) {
        let mut keys = Vec::new();
        self.read_arg(&mut keys);
        if self.mode == Mode::Horizontal {
            let saved = (self.last_skip(), self.aux);
            if let Some(doc) = self.doc.as_mut() {
                doc.sphack.push(saved);
            }
            self.esphack_end();
        } else if self.mode == Mode::Vertical && self.doc.as_ref().is_some_and(|d| d.nobreak || d.noskipsec) {
            self.cur.push(super::super::engine_node::Node::new(super::super::engine_node::Kind::Penalty, 10000));
        }
    }

    /// `\addbibresource[options]{file}`, in the preamble (`\@onlypreamble`): the file goes
    /// to the `.bcf` only.
    pub(super) fn blx_addbibresource(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\addbibresource after \\begin{document}");
        }
        if self.next_is('[') {
            self.bracket_arg();
        }
        let mut file = Vec::new();
        self.read_arg(&mut file);
    }

    /// `\printbibliography[options]` with no entry data: `\begingroup`, the keys set, the
    /// "Empty bibliography" warning, `\endgroup`. Nothing is typeset; the `[` test skips
    /// spaces (`\@ifnextchar`).
    pub(super) fn blx_printbibliography(&mut self) {
        if self.doc.as_ref().is_none_or(|d| !d.body) {
            return self.not_native("\\printbibliography in the preamble");
        }
        if self.next_is('[') {
            let opts: String = self.bracket_arg().iter().map(|t| t.chr()).collect();
            let mut depth = 0;
            let mut items = vec![String::new()];
            for c in opts.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        items.push(String::new());
                        continue;
                    }
                    _ => {}
                }
                if let Some(last) = items.last_mut() {
                    last.push(c);
                }
            }
            if items.iter().map(|i| i.split('=').next().unwrap_or("").trim()).any(|k| !k.is_empty() && !BIB_KEYS.contains(&k)) {
                self.not_native("a \\printbibliography option");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options() {
        assert_eq!(Biblatex::options("").unwrap().style, Style::Numeric);
        let b = Biblatex::options("style=authoryear, backend=bibtex,sorting=nyt, maxbibnames=99").unwrap();
        assert_eq!((b.style, b.auto_plain), (Style::AuthorYear, false));
        assert!(Biblatex::options("autocite=plain,citestyle=alphabetic").unwrap().auto_plain);
        assert!(Biblatex::options("style=apa").is_err());
        assert!(Biblatex::options("autocite=footnote").is_err());
        assert!(Biblatex::options("language=american").is_err());
    }
}
