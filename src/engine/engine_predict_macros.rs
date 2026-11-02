//! The contents prediction ([`super::predict`]) through the document's own heading macros:
//! a `\newcommand{\logday}[2]{\section{#1}…}` used in the body stops the walk (its text
//! counts), so a `\tableofcontents` could not be read whole. A macro whose text is plain
//! (`\newcommand`'s `[n]` and no default, a balanced text without `@` names or other
//! macros of its own, defined once) is expanded in a copy of the source before the walk, as
//! TeX's `\@argdef` would substitute its arguments. Every prediction is still checked when
//! the run meets it (`contents_write`, `label`), so a wrong expansion only sends the
//! document to the engine.

/// The commands whose use in a macro's text makes it worth expanding: those that write a
/// contents line, `\refstepcounter` (a label of the document's own counter, `engine_counter`),
/// and amsmath's `\tag` (a display row's label its text, hyperref's `AMS.N` counted).
const HEADINGS: [&str; 10] = ["part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph", "addcontentsline", "refstepcounter", "tag"];

/// One plain macro: its name, number of arguments and text.
struct Plain<'a> {
    name: &'a str,
    args: usize,
    text: &'a str,
}

/// `src` with the body's uses of plain heading macros expanded, or `None` when there are none.
pub(super) fn expand_heading_macros(src: &str) -> Option<String> {
    let start = super::doc_start(src)?;
    let macros = plain_macros(&src[..start]);
    if macros.is_empty() {
        return None;
    }
    let mut body = src[start..].to_string();
    let mut changed = false;
    // Nested uses (a macro's text using another) expand on a later pass.
    for _ in 0..4 {
        let Some(next) = expand_once(&body, &macros) else { break };
        body = next;
        changed = true;
    }
    changed.then(|| format!("{}{body}", &src[..start]))
}

/// The preamble's `\newcommand`/`\renewcommand{\name}[n]{text}` whose text uses a heading
/// command, has no `@` name and no `#` past `#n`; a name defined twice (or by `\def`) is left
/// out, as is one whose text uses itself.
fn plain_macros(pre: &str) -> Vec<Plain<'_>> {
    let mut found: Vec<Plain> = Vec::new();
    let mut twice: Vec<&str> = Vec::new();
    let mut rest = pre;
    while let Some(k) = rest.find('\\') {
        rest = &rest[k + 1..];
        let n = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        let cmd = &rest[..n];
        if cmd == "def" || cmd == "let" || cmd == "providecommand" {
            let r = rest[n..].trim_start().trim_start_matches('{');
            if let Some(r) = r.strip_prefix('\\') {
                twice.push(&r[..r.bytes().take_while(u8::is_ascii_alphabetic).count()]);
            }
            continue;
        }
        if cmd != "newcommand" && cmd != "renewcommand" {
            continue;
        }
        let r = rest[n..].trim_start();
        let (name, r) = match r.strip_prefix("{\\").and_then(|r| r.split_once('}')) {
            Some(x) => x,
            None => match r.strip_prefix('\\') {
                Some(r) => r.split_at(r.bytes().take_while(u8::is_ascii_alphabetic).count()),
                None => continue,
            },
        };
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        if found.iter().any(|m| m.name == name) {
            twice.push(name);
        }
        let mut r = r.trim_start();
        let mut args = 0;
        if let Some(o) = r.strip_prefix('[') {
            let Some((a, after)) = o.split_once(']') else { continue };
            let Ok(a) = a.trim().parse::<usize>() else { continue };
            args = a;
            r = after.trim_start();
        }
        // An optional argument's default is not plain.
        if r.starts_with('[') || args > 9 {
            twice.push(name);
            continue;
        }
        let Some(text) = r.strip_prefix('{').and_then(|t| super::balanced(t).map(|e| &t[..e])) else { continue };
        rest = &r[1 + text.len()..];
        let uses = |c: &str| text.match_indices('\\').any(|(i, _)| text[i + 1..].starts_with(c) && !text[i + 1 + c.len()..].starts_with(|x: char| x.is_ascii_alphabetic()));
        let hashes_ok = text.match_indices('#').all(|(i, _)| text[i + 1..].bytes().next().is_some_and(|d| d.is_ascii_digit() && (1..=args).contains(&((d - b'0') as usize))));
        if HEADINGS.iter().any(|h| uses(h)) && !text.contains('@') && hashes_ok && !uses(name) {
            found.push(Plain { name, args, text });
        } else {
            twice.push(name);
        }
    }
    found.retain(|m| !twice.contains(&m.name));
    found
}

/// One pass over `body`: each use `\name{a1}…{an}` of a plain macro replaced by its text with
/// the arguments put in; `None` when none was.
fn expand_once(body: &str, macros: &[Plain]) -> Option<String> {
    let mut out = String::with_capacity(body.len());
    let mut changed = false;
    let mut i = 0;
    while let Some(k) = body[i..].find('\\') {
        let at = i + k;
        out.push_str(&body[i..at]);
        let r = &body[at + 1..];
        let n = r.bytes().take_while(u8::is_ascii_alphabetic).count();
        // `\\` and other control symbols: kept, with the character they name.
        if n == 0 {
            let w = r.chars().next().map_or(0, char::len_utf8);
            out.push_str(&body[at..at + 1 + w]);
            i = at + 1 + w;
            continue;
        }
        let name = &r[..n];
        if let Some(m) = macros.iter().find(|m| m.name == name) {
            if let Some((args, len)) = read_args(&r[n..], m.args) {
                out.push_str(&substitute(m.text, &args));
                i = at + 1 + n + len;
                changed = true;
                continue;
            }
        }
        out.push_str(&body[at..at + 1 + n]);
        i = at + 1 + n;
    }
    out.push_str(&body[i..]);
    changed.then_some(out)
}

/// `n` braced arguments at the start of `s` (spaces and one line end before each, as TeX
/// skips them; a blank line is a `\par` and ends it): their texts and the bytes read.
fn read_args(s: &str, n: usize) -> Option<(Vec<&str>, usize)> {
    let mut args = Vec::with_capacity(n);
    let mut p = 0;
    for _ in 0..n {
        let skip = s[p..].bytes().take_while(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')).count();
        if s[p..p + skip].matches('\n').count() > 1 {
            return None;
        }
        p += skip;
        let t = s[p..].strip_prefix('{')?;
        let e = super::balanced(t)?;
        args.push(&t[..e]);
        p += e + 2;
    }
    Some((args, p))
}

/// A macro's text with `#k` replaced by argument `k`. TeX substitutes tokens; in text, a
/// control word next to letters would join them (`\hfill#3` with `Prep`), so a space, which
/// the tokenizer drops after a control word, goes between them.
fn substitute(text: &str, args: &[&str]) -> String {
    // Whether `s` ends with a control word (`\name`).
    let ends_cs = |s: &str| {
        let n = s.bytes().rev().take_while(u8::is_ascii_alphabetic).count();
        n > 0 && s[..s.len() - n].ends_with('\\')
    };
    let letter = |s: &str| s.starts_with(|c: char| c.is_ascii_alphabetic());
    let mut out = String::with_capacity(text.len() + args.iter().map(|a| a.len()).sum::<usize>());
    let mut rest = text;
    while let Some(k) = rest.find('#') {
        out.push_str(&rest[..k]);
        let a = args[(rest.as_bytes()[k + 1] - b'1') as usize];
        if ends_cs(&out) && letter(a) {
            out.push(' ');
        }
        out.push_str(a);
        rest = &rest[k + 2..];
        if ends_cs(a) && letter(rest) {
            out.push(' ');
        }
    }
    out.push_str(rest);
    out
}
