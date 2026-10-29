//! url.sty's `\url` and `\path` on the native path (`2026-10-26-url-hyperref-recon.md`):
//! `\Url@FormatString`, a math formula in `\UrlFont` whose characters get the mathcodes
//! `\Url@MathSetup` gives them, built as tokens the engine's math typesets.

use super::super::engine_catcode::Cat;
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::{Reg, Region};

/// `\UrlFont` by `\urlstyle`: 0 `tt` (the default), 1 `rm`, 2 `sf`, 3 `same`; grouped as
/// the macro is.
pub(super) const URL_STYLE: Reg = Reg::register(Region::Int, 32711);

/// What `\Url@MathSetup` makes of an ASCII character in a typewriter font
/// (`\Url@OTnonTT` is `\relax` there): its math class (0 ord, 2 bin, 3 rel, 4 open), and
/// whether `\verbatim@nolig@list` adds `\sb{\/}`.
fn class(c: char) -> (u32, bool) {
    let class = match c {
        // `\UrlBreaks` (after `\UrlOrds`, so `'` is a break).
        '.' | '@' | '\\' | '/' | '!' | '_' | '|' | ';' | '>' | ']' | ')' | ',' | '?' | '&' | '\'' | '+' | '=' | '#' => 2,
        // `\UrlBigBreaks` (`\do@url@hyp` is empty without the `hyphens` option).
        ':' => 3,
        // `\UrlNoBreaks`.
        '(' | '[' | '{' | '<' => 4,
        _ => 0,
    };
    (class, matches!(c, '`' | '<' | '>' | ',' | '\'' | '-'))
}

/// A URI `\Hy@pstringdef` keeps as it is: no character it escapes or changes.
fn uri_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "./:-_~?=&+@,;!*#".contains(c))
}

impl Machine<'_, '_> {
    /// `\url{text}` (`\leavevmode\begingroup \Url`), or `\path` (with `\urlstyle{tt}`). The
    /// text is read as url.sty reads it: from the file with its specials made other when it
    /// has not been read yet, else from tokens as `\meaning` prints them. With hyperref
    /// `\url` is `\hyper@linkurl{\Hurl{text}}{text}` (`link`), `\nolinkurl` the `\Hurl`.
    pub(super) fn url_command(&mut self, path: bool, link: bool) {
        if self.leave_vmode() {
            return;
        }
        if self.mode == super::super::engine_run::Mode::Math {
            return self.not_native("\\url in math");
        }
        let text = match self.url_raw() {
            Some(t) if t.contains('{') => return self.not_native("\\url with braces inside"),
            Some(t) => t,
            None => {
                let mut t = Vec::new();
                match self.next_unblank() {
                    Some(x) if x.cat() == Some(Cat::BeginGroup) => {
                        self.input.back(x);
                        self.read_arg(&mut t);
                    }
                    _ => return self.not_native("\\url with a delimiter other than braces"),
                }
                match self.meaning_text(&t) {
                    Some(s) => s,
                    None => return self.not_native("\\url of an argument with parameters"),
                }
            }
        };
        if text.chars().any(|c| !(' '..='~').contains(&c)) {
            return self.not_native("\\url with a non-ASCII character or a line end");
        }
        let style = if path { 0 } else { self.e.eqtb.word(URL_STYLE) };
        let [rel, bin] = self.doc.as_ref().map_or([500, 700], |d| d.url_penalties);
        let mut tex = String::with_capacity(64 + text.len() * 12);
        tex.push_str(["\\ttfamily", "\\rmfamily", "\\sffamily", ""][style as usize & 3]);
        tex.push_str(&format!("\\medmuskip0mu\\thickmuskip0mu\\thinmuskip0mu\\relpenalty{rel}\\binoppenalty{bin}$\\fam0\\textfont0=\\font "));
        for c in text.chars() {
            match c {
                // `\meaning` prints a space as a space token, which math mode skips: the
                // active `\Url@space` is reached only with the `obeyspaces` option.
                ' ' => {}
                // `\Url@percent`.
                '%' => tex.push_str("\\mathbin{\\mathchar37}"),
                // OT1's typewriter `~`: `\lower.45ex\hbox{\m@th$\mathchar126$}`
                // (`\Url@OT1encSpecials`; T1's specials leave `~` an ordinary character).
                '~' if !self.t1() => tex.push_str("\\lower.45ex\\hbox{\\mathsurround0pt$\\mathchar126$}"),
                c => {
                    let (class, nolig) = class(c);
                    tex.push_str(&format!("\\mathchar{} ", class << 12 | c as u32));
                    if nolig {
                        tex.push_str("_{\\/}");
                    }
                }
            }
        }
        tex.push_str("\\mathsurround0pt$");
        let link = link && self.hyper();
        if link && !uri_ok(&text) {
            return self.not_native("hyperref: a \\url with characters its URI string escapes");
        }
        let mut t = Vec::new();
        if link {
            if let Some(doc) = self.doc.as_mut() {
                doc.uri = Some(text);
            }
            t.push(self.doc_id(super::START_LINK_NATIVE));
        }
        t.push(self.named_cs(named!("begingroup")));
        // `tex_tokens` reads `$` and `_` as other characters; the url's own characters are
        // all `\mathchar`s, so each one here is the formula's.
        t.extend(self.tex_tokens(&tex).into_iter().map(|x| match x {
            x if x == Token::char(Cat::Other, '$') => Token::char(Cat::MathShift, '$'),
            x if x == Token::char(Cat::Other, '_') => Token::char(Cat::Subscript, '_'),
            x => x,
        }));
        t.push(self.named_cs(named!("endgroup")));
        if link {
            t.push(self.doc_id(super::END_LINK_NATIVE));
        }
        self.back_list(&t);
    }

    /// hyperref's `\href{URL}{text}`: both read under `\hyper@normalise`, so the URL from the
    /// file and a text without the characters it changes; a `scheme:` URL is
    /// `\hyper@linkurl{text}{URL}`: `\leavevmode`, `\pdfstartlink` with the URI action, the
    /// text in a group, `\pdfendlink`.
    pub(super) fn href(&mut self) {
        if self.leave_vmode() {
            return;
        }
        // The options are looked for on the raw line: a token read would end the raw URL.
        let raw = self.input.raw_peek();
        if raw.as_ref().is_some_and(|r| r.trim_start_matches(' ').starts_with('[')) || raw.is_none() && self.next_is('[') {
            return self.not_native("hyperref: \\href with options");
        }
        let Some(uri) = self.url_raw().or_else(|| self.url_tokens()) else { return self.not_native("hyperref: an \\href not read from the file") };
        let scheme = uri.split_once(':').map(|(s, _)| s);
        if !uri_ok(&uri) || uri.contains('#') || !scheme.is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()) && s != "file" && s != "run") {
            return self.not_native("hyperref: an \\href that is not a plain scheme: URL");
        }
        // The text's line, up to its closing brace, holds none of `\hyper@normalise`'s
        // characters; a text read already is the same tokens for pdfLaTeX.
        let Some(rest) = self.input.raw_peek() else { return self.href_text(uri) };
        let mut depth = 0;
        let mut closed = false;
        for c in rest.trim_start_matches(' ').chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                }
                '%' | '#' | '_' | '&' | '$' | '~' => break,
                _ if depth == 0 => break,
                _ => {}
            }
        }
        if !closed {
            return self.not_native("hyperref: an \\href text across lines or with special characters");
        }
        self.href_text(uri);
    }

    /// `\hyper@linkurl{text}{URL}` after the URL: the text in a group between the link's
    /// start and end; none for an empty text.
    fn href_text(&mut self, uri: String) {
        let mut text = Vec::new();
        self.read_arg(&mut text);
        // `\hyper@link@`: an empty text makes no link ("Suppressing empty link").
        if text.is_empty() {
            return;
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.uri = Some(uri);
        }
        let mut t = vec![self.doc_id(super::START_LINK_NATIVE), self.named_cs(named!("begingroup"))];
        t.extend(text);
        t.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
        self.back_list(&t);
    }

    /// An `\href` URL read already, as an argument of another command: its tokens were made
    /// with the file's category codes for pdfLaTeX too, so letters and other characters are
    /// the URL as written; `None` for anything else (a command, a space, a parameter).
    fn url_tokens(&mut self) -> Option<String> {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        t.iter().map(|x| matches!(x.cat(), Some(Cat::Letter | Cat::Other)).then(|| x.chr())).collect()
    }

    /// The braced text after `\url` straight from the file (url.sty's `\dospecials` other,
    /// spaces as spaces; a line end in it stays `\n`), or `None` when it was read already.
    fn url_raw(&mut self) -> Option<String> {
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::super::engine_names::END_LINE_CHAR });
        let peek = self.input.raw_peek()?;
        if !peek.trim_start_matches(' ').starts_with('{') {
            return None;
        }
        let text = self.input.raw_until("}", elc)?;
        text.trim_start_matches(' ').strip_prefix('{').map(str::to_string)
    }

    /// Tokens as `\meaning` prints them: characters, and a control word as `\name ` (a
    /// control symbol without the space). `None` for a parameter token.
    fn meaning_text(&self, t: &[Token]) -> Option<String> {
        let mut s = String::new();
        for x in t {
            match x.cs_id() {
                Some(id) => {
                    let name = self.e.cs.name(id);
                    s.push('\\');
                    s.push_str(name);
                    if name.chars().all(|c| c.is_ascii_alphabetic()) {
                        s.push(' ');
                    }
                }
                None if x.cat() == Some(Cat::Param) => return None,
                None => s.push(x.chr()),
            }
        }
        Some(s)
    }

    /// `\urlstyle{tt|rm|sf|same}`.
    pub(super) fn url_style(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let name: String = t.iter().filter(|x| x.cs_id().is_none()).map(|x| x.chr()).collect();
        let Some(i) = ["tt", "rm", "sf", "same"].iter().position(|s| *s == name.trim()) else {
            return self.not_native("\\urlstyle of another style");
        };
        self.e.eqtb.set_word(URL_STYLE, i as i32, false);
    }
}
