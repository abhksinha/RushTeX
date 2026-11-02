//! hyperref on the native path (`2026-10-26-hyperref-design.md`): its anchors, links and
//! document entries, appended as the pdfTeX primitives hpdftex.def runs would append them.

use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_catcode::Cat;
use super::super::engine_token::Token;
use super::Glue;
use super::super::engine_run::Machine;

/// 1bp in sp: `\Hy@EveryPageAnchor`'s `\kern-1bp`.
const ONE_BP: i32 = 65781;

/// The link kinds `\hyper@linkstart{kind}` names: `link`, `cite`, `url`.
pub(super) const LINK: usize = 0;
pub(super) const CITE: usize = 1;
pub(super) const URL_LINK: usize = 2;
/// Their border colours, `\@<kind>bordercolor`.
const BORDER_COLORS: [&str; 3] = ["1 0 0", "0 1 0", "0 1 1"];

/// The options the native job takes (`hyper_keys`): `hidelinks` (border 0, no colour),
/// `colorlinks` (border 0 and `\Hy@colorlink` at `\begin{document}`), the link colours by
/// kind (names, made literals at `\begin{document}`).
#[derive(PartialEq, Clone)]
pub(super) struct HyperOpts {
    hide: bool,
    colorlinks: bool,
    colors: [String; 3],
    /// `pdfauthor`, `pdftitle`, `pdfsubject`, `pdfcreator`, `pdfproducer`, `pdfkeywords` as
    /// `\pdfstringdef` wrote them, in `\PDF@FinishDoc`'s order.
    info: [Option<String>; 6],
    /// `pdfborder` (`\@pdfborder`, `0 0 1` by default): a link's `/Border` without
    /// `colorlinks` or `hidelinks`.
    border: String,
    /// `bookmarksopen`: `\Hy@writebookmark`'s `count` positive (open) at every level
    /// (`bookmarksopenlevel` is `\maxdimen`).
    open: bool,
    /// `pdfstartview` (`Fit` by default): the view of the catalog's `/OpenAction`.
    view: String,
    /// `pdfpagemode` (`UseOutlines` with bookmarks, hyperref's default).
    mode: String,
    /// `bookmarksnumbered`: `\numberline` is `\Hy@numberline` (`#1 `) in the bookmark
    /// titles, not `\@gobble` (hpdftex.def:1651).
    numbered: bool,
    /// `pdfdisplaydoctitle`: `/DisplayDocTitle true` in the catalog's `/ViewerPreferences`.
    doctitle: bool,
    /// `pdfusetitle`: `\Hy@UseMaketitleInfos` fills an empty `pdftitle` and `pdfauthor`
    /// from `\title` and `\author` (hyperref.sty:6215-6278).
    pub(super) usetitle: bool,
}

impl Default for HyperOpts {
    fn default() -> Self {
        Self { hide: false, colorlinks: false, colors: ["red", "green", "magenta"].map(String::from), info: Default::default(), border: "0 0 1".into(), open: false, view: "Fit".into(), mode: "UseOutlines".into(), numbered: false, doctitle: false, usetitle: false }
    }
}

const INFO_KEYS: [&str; 6] = ["pdfauthor", "pdftitle", "pdfsubject", "pdfcreator", "pdfproducer", "pdfkeywords"];

/// `\pdfstringdef` of plain text (hyperref's `unicode`): `\376\377`, then each character as
/// `\000c`, a space `\000\040`; a UTF-8 character of the BMP (the source's bytes, one to a
/// `char`, as for bookmark titles) both UTF-16BE bytes as octal escapes (`bookmark_char`).
/// Text with characters it escapes or changes is `None`.
fn pdf_string(v: &str) -> Option<String> {
    if v.is_empty() {
        return Some(String::new());
    }
    if v.contains("--") || v.contains("``") || v.contains("''") || v.contains("  ") {
        return None;
    }
    let bytes = v.chars().map(|c| u8::try_from(c as u32)).collect::<Result<Vec<u8>, _>>().ok()?;
    let v = String::from_utf8(bytes).ok()?;
    let mut out = String::from("\\376\\377");
    for c in v.chars() {
        match c {
            ' ' => out.push_str("\\000\\040"),
            '(' | ')' | '\\' | '%' | '#' | '$' | '&' | '^' | '_' | '~' | '{' | '}' | '`' => return None,
            c if c.is_ascii_graphic() => {
                out.push_str("\\000");
                out.push(c);
            }
            // U+00A0 is utf8.def's `\nobreakspace`, which `\pdfstringdef` changes.
            c if c > '\u{a0}' && c <= '\u{ffff}' => bookmark_char(c, &mut out),
            _ => return None,
        }
    }
    Some(out)
}

/// The sectioning counters by heading level: article's levels start at `section`, report's
/// at `chapter`.
fn heading_names(report: bool) -> &'static [&'static str] {
    if report { &["chapter", "section", "subsection"] } else { &["section", "subsection", "subsubsection"] }
}

/// The anchor of a numbered heading at `level` with the sectioning counters `counters`:
/// `<prefix>.\theH<counter>`, the counters in arabic. hyperref's `\appendix` makes
/// `\theHchapter` (article: `\theHsection`) `\Hy@AlphNoErr` (`A` to `Z`, else `Alph<n>`),
/// and the prefix of `chapter` (`\Hy@chapapp`) and of `section` (`target-prefix=
/// {section}{\Hy@appendixstring}`) `appendix`: report's `appendix.A`, `appendix.A.1`,
/// `subsection.A.1.1`, article's `appendix.A`, `subsection.A.1`.
pub(super) fn heading_anchor_name<T: Copy + Into<i64>>(report: bool, appendix: bool, level: usize, counters: &[T]) -> String {
    let name = heading_names(report)[level];
    let mut out = String::from(if appendix && matches!(name, "chapter" | "section") { "appendix" } else { name });
    for (k, &c) in counters[..=level].iter().enumerate() {
        out.push('.');
        out.push_str(&if k == 0 { the_h_top(c, appendix) } else { c.into().to_string() });
    }
    out
}

/// `\theHchapter` (article: `\theHsection`) of counter value `c`: arabic, after hyperref's
/// `\appendix` `\Hy@AlphNoErr` (`A` to `Z`, else `Alph<n>`).
pub(super) fn the_h_top<T: Into<i64>>(c: T, appendix: bool) -> String {
    match c.into() {
        c @ 1..=26 if appendix => ((b'A' + c as u8 - 1) as char).to_string(),
        c if appendix => format!("Alph{c}"),
        c => c.to_string(),
    }
}

/// The anchor of a starred chapter: `chapter*.N`, after hyperref's `\appendix` (`\Hy@chapapp`)
/// `appendix*.N`. A starred section keeps `section*.N`.
pub(super) fn star_chapter_anchor(appendix: bool, n: impl std::fmt::Display) -> String {
    format!("{}*.{n}", if appendix { "appendix" } else { "chapter" })
}

/// The anchor of a numbered heading at `level` whose number is `number`, as
/// `heading_anchor_name` makes it: an appendix number starts with its letter; `None` for a
/// number that is neither.
fn heading_href(report: bool, level: usize, number: &str) -> Option<String> {
    let names = heading_names(report);
    // An unnumbered contents entry ("*N", book's chapters outside the main matter): its
    // `\Hy@MakeCurrentHrefAuto{chapter*}` anchor.
    // `\addcontentsline`'s ("#href"): `\@currentHref` when it was written.
    if let Some(h) = number.strip_prefix('#') {
        return (!h.is_empty()).then(|| h.to_string());
    }
    if let Some(n) = number.strip_prefix('*') {
        return names.get(level).map(|k| format!("{k}*.{n}"));
    }
    if level >= names.len() {
        return None;
    }
    let mut parts = number.split('.');
    let first = parts.next().unwrap_or("");
    let appendix = first.len() == 1 && first.as_bytes()[0].is_ascii_uppercase();
    let arabic = |n: &str| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
    if !(appendix || arabic(first)) || !parts.all(arabic) {
        return None;
    }
    let name = names[level];
    Some(format!("{}.{number}", if appendix && matches!(name, "chapter" | "section") { "appendix" } else { name }))
}

/// The keys of a key-value list: commas inside braces stay in the value.
fn key_items(list: &str) -> Vec<&str> {
    let (mut items, mut depth, mut from) = (Vec::new(), 0, 0);
    for (i, c) in list.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&list[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    items.push(&list[from..]);
    items
}

impl Machine<'_, '_> {
    pub(super) fn hyper(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"hyperref"))
    }

    /// `\Hy@EveryPageAnchor` in `shipout/before`: `\vbox{\offinterlineskip \vbox to0pt{\kern
    /// \voffset \kern\topmargin \kern-1bp \hbox to0pt{\kern\hoffset \kern<margin> \kern-1bp
    /// \pdfdest name{page.N} xyz \penalty10000 \hss}\vss} \box\ShipoutBox}`, `page` the page
    /// number being shipped and `margin` its side margin.
    pub(super) fn page_anchor(&mut self, shipped: BoxRec, page: i32, top: i32, margin: i32, d: i32) -> BoxRec {
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let offsets = [param!("voffset"), param!("hoffset")].map(|r| self.e.eqtb.word(r));
        let start = self.cur.len();
        let explicit = |w| Node { sub: 1, ..Node::new(Kind::Kern, w) };
        self.cur.extend([explicit(offsets[1]), explicit(margin), explicit(-ONE_BP)]);
        let name = format!("page.{}", self.the_page(page));
        self.append_dest(name);
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let h = self.hpack(start, 0, true);
        let h = self.e.nodes.add_box(h);
        self.cur.extend([explicit(offsets[0]), explicit(top), explicit(-ONE_BP), Node::new(Kind::Box, h as i32)]);
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let v = self.vpack(start, 0, true, d);
        let v = self.e.nodes.add_box(BoxRec { vertical: true, ..v });
        self.cur.push(Node::new(Kind::Box, v as i32));
        // `\offinterlineskip`: the page box after `\lineskip` glue of zero.
        let n = self.e.nodes.glue_node(Glue::default(), super::super::engine_pack::LINE_SKIP as u8 + 1);
        self.cur.push(n);
        let b = self.e.nodes.add_box(shipped);
        self.cur.push(Node::new(Kind::Box, b as i32));
        self.vpack(start, 0, false, d)
    }

    /// hyperref's `\Hy@AtBeginDocument`: `\PDF@SetupDoc` (the catalog), the `Doc-Start` anchor
    /// in vertical mode (`\pdfdest` and `\penalty10000`, which an empty page drops), and
    /// `\PDF@FinishDoc`'s `\pdfinfo` (at the first shipout; nothing else writes Info).
    pub(super) fn hyper_begin_document(&mut self) {
        // hpdftex.def's `\pdflinkmargin1pt` (global, at load: no preamble box holds a link).
        self.e.eqtb.set_word(param!("pdflinkmargin"), 65536, true);
        let (mode, view) = self.doc.as_ref().map(|d| (d.pkg.hy.mode.clone(), d.pkg.hy.view.clone())).unwrap_or_default();
        self.add_catalog(&format!("/PageMode/{mode}"), Some((1, &format!("/{view}"))));
        // `\PDF@SetupDoc`'s second `\pdfcatalog`: `/ViewerPreferences<<\Hy@temp>>` when
        // `\Hy@temp` is not empty.
        let doctitle = self.doc.as_ref().is_some_and(|d| d.pkg.hy.doctitle);
        self.add_catalog(if doctitle { "/ViewerPreferences<</DisplayDocTitle true>>" } else { "" }, None);
        // `\PDF@FinishDoc`: `\Hy@UseMaketitleInfos`, then `/Producer` only when set (pdfTeX
        // writes its own).
        if self.doc.as_ref().is_some_and(|d| d.pkg.hy.usetitle) {
            if let Err(what) = self.use_title() {
                return self.not_native(what);
            }
        }
        let info = self.doc.as_ref().map(|d| d.pkg.hy.info.clone()).unwrap_or_default();
        let field = |i: usize, default: &str| info[i].clone().unwrap_or_else(|| default.to_string());
        let mut text = format!("/Author({})/Title({})/Subject({})/Creator({})", field(0, ""), field(1, ""), field(2, ""), field(3, "LaTeX with hyperref"));
        if let Some(p) = &info[4] {
            text.push_str(&format!("/Producer({p})"));
        }
        text.push_str(&format!("/Keywords({})", field(5, "")));
        self.add_info(&text);
        // hyperref's `\addcontentsline` writes no bookmark while `\@currentHref` is empty.
        // An entry past the bookmarks' depth writes none; an unnumbered heading's ("" with an
        // anchor) links to its `section*.N` ("#href").
        let entries: Option<Vec<(usize, String, String)>> = self.doc.as_ref().and_then(|d| d.contents.as_ref()).map(|t| t.iter().filter(|e| e.number != "#" && e.level < super::LOF && self.bookmark_level(e.level)).map(|e| (e.level, if e.href.is_empty() { e.number.clone() } else { format!("#{}", e.href) }, e.title.clone())).collect());
        if let Some(entries) = entries {
            if let Err(what) = self.bookmarks(&entries) {
                return self.not_native(what);
            }
        }
        self.append_dest("Doc-Start".to_string());
        self.cur.push(Node::new(Kind::Penalty, 10000));
    }

    /// `\Hy@UseMaketitleString` for `title` and `author`: an empty `\@pdftitle` or
    /// `\@pdfauthor` becomes `\pdfstringdef` of the argument `\title` or `\author` gave, with
    /// `\\` (and `\newline`) `\Hy@newline@title` (` #1`) or `\Hy@newline@author` (`, #1`),
    /// `\and` `; ` and `\thanks` gobbled. hyperref captures them as `\title` runs and writes
    /// Info at the first shipout; here they are read at `\begin{document}`, so a `\title` or
    /// `\author` in the body declines (`store`), as do a class with its own title commands
    /// and authblk.
    fn use_title(&mut self) -> Result<(), &'static str> {
        let Some(doc) = self.doc.as_ref() else { return Ok(()) };
        // amsart's `\title` and `\author` are its own (`ams_title`); the letter classes have none.
        if doc.ams.is_some() || doc.letter.is_some() || doc.authblk.is_some() {
            return Err("hyperref: pdfusetitle under amsart, a letter class, or authblk");
        }
        let args = [doc.stored[0].clone(), doc.stored[1].clone()];
        for (arg, sep, slot) in [(&args[0], " ", 1), (&args[1], ", ", 0)] {
            if self.doc.as_ref().is_some_and(|d| d.pkg.hy.info[slot].as_deref().is_some_and(|v| !v.is_empty())) {
                continue;
            }
            let mut text = String::new();
            let mut i = 0;
            while i < arg.len() {
                let t = arg[i];
                i += 1;
                if t.cs_id().is_none() {
                    match t.cat() {
                        Some(Cat::BeginGroup | Cat::EndGroup) => {}
                        Some(Cat::Space) => text.push(' '),
                        _ => text.push(t.chr()),
                    }
                    continue;
                }
                match self.name_of(t).as_str() {
                    "\\\\" | "\\newline" => {
                        if arg.get(i).is_some_and(|n| n.cs_id().is_none() && matches!(n.chr(), '[' | '*')) {
                            return Err("hyperref: pdfusetitle with \\\\[ or \\\\*");
                        }
                        // `#1` is the next argument: spaces before it are skipped.
                        while arg.get(i).is_some_and(|n| n.cat() == Some(Cat::Space)) {
                            i += 1;
                        }
                        text.push_str(sep);
                    }
                    "\\and" => text.push_str("; "),
                    "\\thanks" => {
                        // `\@gobble`: one argument.
                        while arg.get(i).is_some_and(|n| n.cat() == Some(Cat::Space)) {
                            i += 1;
                        }
                        if arg.get(i).is_some_and(|n| n.cat() == Some(Cat::BeginGroup)) {
                            let mut depth = 0;
                            while let Some(n) = arg.get(i) {
                                i += 1;
                                match n.cat() {
                                    Some(Cat::BeginGroup) => depth += 1,
                                    Some(Cat::EndGroup) => depth -= 1,
                                    _ => {}
                                }
                                if depth == 0 {
                                    break;
                                }
                            }
                        } else {
                            i += 1;
                        }
                    }
                    _ => return Err("hyperref: pdfusetitle with a command in \\title or \\author"),
                }
            }
            let v = pdf_string(&text).ok_or("hyperref: a document property with characters \\pdfstringdef changes")?;
            if let Some(d) = self.doc.as_mut() {
                d.pkg.hy.info[slot] = Some(v);
            }
        }
        Ok(())
    }

    /// `\hyper@anchorstart{name}` in vertical mode (`\new@pdflink`): `\Hy@SaveLastskip`,
    /// `\pdfdest`, `\Hy@RestoreLastskip`. After glue g: the dest, `\nobreak`, `\vskip-g
    /// \vskip g`; after a box: the dest alone; else the dest and `\penalty10000`.
    pub(super) fn vertical_anchor(&mut self, name: String) {
        // `\ifdim\lastskip=\z@` (the width only), `\ifnum\lastnodetype=1` (an hlist), both
        // from the page builder once the contributions are gone (§424).
        let g = self.last_skip();
        let hlist = match self.cur.last().filter(|_| self.cur.len() > self.start as usize) {
            Some(n) => n.kind == Kind::Box && !self.e.nodes.boxes[n.a as usize].vertical,
            None => self.page_last_type() == Some(1),
        };
        self.append_dest(name);
        if g.width != 0 {
            self.cur.push(Node::new(Kind::Penalty, 10000));
            let n = self.e.nodes.glue_node(super::neg(g), 0);
            self.cur.push(n);
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        } else if !hlist {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
    }

    /// The anchor of a numbered heading at `level` (0 `section`, report's `chapter`):
    /// `section.\theHsection`, the counters in arabic (the kernel's `\theH<level>`).
    pub(super) fn heading_anchor(&mut self, level: usize, counters: &[i32]) {
        let (report, appendix) = self.doc.as_ref().map_or((false, false), |d| (d.report, d.appendix));
        let name = heading_anchor_name(report, appendix, level, counters);
        self.set_current_href(&name);
        self.vertical_anchor(name);
    }

    /// `\hyper@makecurrent`: `\@currentHref`, local.
    /// `\hyper@makecurrent`: `\global\let\@currentHref` (hyperref's default, no
    /// `localanchorname`), so a `\label` after the group still takes it.
    pub(super) fn set_current_href(&mut self, name: &str) {
        self.store_current_href(name, true);
    }

    pub(super) fn store_current_href(&mut self, name: &str, global: bool) {
        let span = self.e.toks.store(&super::label_tokens(name));
        self.e.eqtb.set_toks(super::CURRENT_HREF, span, global);
    }

    /// A bookmark title holding math, as `\pdfstringdef` reads its tokens (checked against
    /// pdfLaTeX's `.out`): math shifts, `^`, `_` and braces in math go ("Token not allowed
    /// ... removing"), as does an unexpandable `\mathchardef` command (`\beta`, `\leq`) with
    /// the spaces its name swallowed; every other space token stays, the first and the last
    /// too (`$\beta$ x` is " x"). An empty result is an empty string without the mark.
    fn bookmark_math(&self, title: &str) -> Result<String, &'static str> {
        const NOT: &str = "hyperref: a bookmark title that is not plain text";
        let mut out = String::new();
        let (mut math, mut space) = (false, false);
        let mut chars = title.chars().peekable();
        while let Some(c) = chars.next() {
            let was_space = std::mem::take(&mut space);
            match c {
                ' ' | '\n' | '\t' => {
                    if !was_space {
                        out.push_str("\\000\\040");
                    }
                    space = true;
                }
                '$' if chars.peek() == Some(&'$') => return Err(NOT),
                '$' => math = !math,
                '^' | '_' | '{' | '}' if math => {}
                '\\' => {
                    let mut word = String::new();
                    while let Some(&l) = chars.peek().filter(|l| l.is_ascii_alphabetic()) {
                        word.push(l);
                        chars.next();
                    }
                    if !math || word.is_empty() || self.si_math_char(&word).is_none() {
                        return Err(NOT);
                    }
                    while chars.peek().is_some_and(|l| matches!(l, ' ' | '\t' | '\n')) {
                        chars.next();
                    }
                }
                'A'..='Z' | 'a'..='z' | '0'..='9' | '!' | '(' | ')' | '*' | '+' | ',' | '-' | '.' | '/' | ':' | ';' | '<' | '=' | '>' | '?' | '[' | ']' | '|' => bookmark_char(c, &mut out),
                _ => return Err(NOT),
            }
        }
        if math {
            return Err(NOT);
        }
        Ok(if out.is_empty() { out } else { format!("\\376\\377{out}") })
    }

    /// `\ReadBookmarks` at `\begin{document}`: one `\pdfoutline goto name{anchor} count-n
    /// {title}` per contents entry, in order, n its direct children, the title as
    /// `\pdfstringdef` makes it (UTF-16BE with its mark, in octal escapes). `Err` for what is
    /// not ported: a title that is not plain text.
    pub(super) fn bookmarks(&mut self, entries: &[(usize, String, String)]) -> Result<(), &'static str> {
        let report = self.doc.as_ref().is_some_and(|d| d.report);
        // `\@@writetorep`'s level check (hpdftex.def): the first bookmark keeps its level
        // (it sets `\Hy@levelcheck`); a later one more than one below the last is "level
        // fixed" to the last plus one. A bookmark's parent is the last at its level less one
        // (`\Parent<n>`), so its children are the bookmarks after it one level below, up to
        // one at its level or above.
        let mut levels: Vec<usize> = Vec::with_capacity(entries.len());
        for (level, _, _) in entries {
            let fixed = match levels.last() {
                Some(&p) if *level > p + 1 => p + 1,
                _ => *level,
            };
            levels.push(fixed);
        }
        for (i, (_, number, title)) in entries.iter().enumerate() {
            let level = &levels[i];
            let children = levels[i + 1..].iter().take_while(|&&l| l > *level).filter(|&&l| l == level + 1).count() as i32;
            let mut text = String::from("\\376\\377");
            let mut space = false;
            // The source's bytes, one to a character: UTF-8 (`\usepackage[utf8]{inputenc}`, the
            // kernel's default).
            let bytes = title.chars().map(|c| u8::try_from(c as u32)).collect::<Result<Vec<u8>, _>>();
            let Some(title) = bytes.ok().and_then(|b| String::from_utf8(b).ok()) else {
                return Err("hyperref: a bookmark title that is not plain text");
            };
            // `\pdfstringdef`'s `\HyPsd@Subst`s (hyperref.sty): `---` `\textemdash`, `--`
            // `\textendash`, ``!` `` and ``?` `` the inverted marks.
            // `\Hy@numberline`'s `#1 ` before the entry's text.
            let title = if self.doc.as_ref().is_some_and(|d| d.pkg.hy.numbered) && !number.is_empty() { format!("{number} {title}") } else { title };
            let title = title.replace("---", "\u{2014}").replace("--", "\u{2013}").replace("!`", "\u{a1}").replace("?`", "\u{bf}");
            if title.contains('$') {
                let text = self.bookmark_math(&title)?;
                let Some(name) = heading_href(report, entries[i].0, number) else {
                    return Err("hyperref: a heading number that is not arabic");
                };
                let open = self.doc.as_ref().is_some_and(|d| d.pkg.hy.open);
                self.append_outline(&name, if open { children } else { -children }, &text);
                continue;
            }
            let mut chars = title.chars().peekable();
            // The open arguments of `\texttt{..}` and its kin, which `\pdfstringdef` lets to
            // `\@firstofone` (hyperref.sty's `\HyPsd@Init`): their braces go, the text stays.
            let mut open = 0u32;
            while let Some(c) = chars.next() {
                let c = match c {
                    ' ' | '\n' => {
                        space = true;
                        continue;
                    }
                    '}' if open > 0 => {
                        open -= 1;
                        continue;
                    }
                    // `\pdfstringdef` expands the text: a group's braces go (`{[}`).
                    '{' => continue,
                    '}' if open == 0 => continue,
                    // `\&` and its kin are their characters.
                    '\\' => match chars.next() {
                        Some(c @ ('&' | '%' | '#' | '$' | '_')) => c,
                        Some(c) if c.is_ascii_alphabetic() => {
                            let mut word = String::from(c);
                            while let Some(&l) = chars.peek().filter(|l| l.is_ascii_alphabetic()) {
                                word.push(l);
                                chars.next();
                            }
                            while chars.peek() == Some(&' ') {
                                chars.next();
                            }
                            if !matches!(word.as_str(), "emph" | "textnormal" | "textrm" | "textsf" | "texttt" | "textbf" | "textmd" | "textit" | "textsc" | "textsl" | "textup") || chars.next() != Some('{') {
                                return Err("hyperref: a bookmark title that is not plain text");
                            }
                            open += 1;
                            continue;
                        }
                        _ => return Err("hyperref: a bookmark title that is not plain text"),
                    },
                    'A'..='Z' | 'a'..='z' | '0'..='9' | '!' | '"' | '\'' | '(' | ')' | '*' | '+' | ',' | '-' | '.' | '/' | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | ']' | '`' | '|' => c,
                    c if c > '\u{7f}' && c <= '\u{ffff}' => c,
                    _ => return Err("hyperref: a bookmark title that is not plain text"),
                };
                if std::mem::take(&mut space) {
                    text.push_str("\\000\\040");
                }
                bookmark_char(c, &mut text);
            }
            // Every space token stays, the first and the last too (pdfLaTeX's `.out`: `{ends }`
            // is "ends ", `{\texttt{a }}` "a "); an empty title is an empty string, no mark.
            if space {
                text.push_str("\\000\\040");
            }
            if text == "\\376\\377" {
                text.clear();
            }
            let Some(name) = heading_href(report, entries[i].0, number) else {
                return Err("hyperref: a heading number that is not arabic");
            };
            // `\@bookmarkopenstatus`: `-` unless `bookmarksopen`.
            let open = self.doc.as_ref().is_some_and(|d| d.pkg.hy.open);
            self.append_outline(&name, if open { children } else { -children }, &text);
        }
        Ok(())
    }

    /// hyperref's `\@setref` for `\ref` and `\pageref`: `\hyper@@link{}{anchor}{text}`, so
    /// `\leavevmode \pdfstartlink … \begingroup text \endgroup \pdfendlink` (`\Hy@colorlink`
    /// is the group), the label's anchor the target; the kernel's `\null` goes.
    pub(super) fn ref_link(&mut self, key: &str, text: Vec<Token>) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(name) = doc.label_hrefs.get(key).or_else(|| doc.predicted_hrefs.get(key)).cloned() else {
            return self.not_native("hyperref: a \\ref whose anchor is unknown");
        };
        doc.used.insert(key.to_string());
        // `\hyper@link@`: empty text, no link ("Suppressing empty link"); an empty anchor,
        // the text alone.
        if text.is_empty() {
            return;
        }
        if name.is_empty() {
            return self.back_list(&text);
        }
        doc.link = Some((LINK, name));
        let mut list = vec![self.doc_id(super::LEAVE), self.doc_id(super::START_LINK_NATIVE), self.named_cs(named!("begingroup"))];
        list.extend(text);
        list.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
        self.back_list(&list);
    }

    /// hyperref's `\contentsline` (`linktoc=section`): `\l@<level>{\Hy@toclinkstart{#2}
    /// \Hy@toclinkend}`, so the entry's `\numberline` and title inside `\pdfstartlink`, a
    /// group, `\pdfendlink`; the target the heading's `\@currentHref` when it was written.
    pub(super) fn toc_link_start(&mut self, level: usize, number: &str) {
        let Some(doc) = self.doc.as_mut() else { return };
        let Some(name) = heading_href(doc.report, level, number) else {
            return self.not_native("hyperref: a contents entry whose number is not arabic");
        };
        doc.link = Some((LINK, name));
        self.start_link();
        self.begin_group(super::SIMPLE_GROUP);
    }

    /// A caption's entry in a list of figures or tables: the link to its anchor
    /// (`\contentsline`'s fourth argument), as [`Self::toc_link_start`].
    pub(super) fn toc_link_start_named(&mut self, name: String) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.link = Some((LINK, name));
        self.start_link();
        self.begin_group(super::SIMPLE_GROUP);
    }

    /// The group's end and `\pdfendlink` of [`Self::toc_link_start`].
    pub(super) fn toc_link_end(&mut self) {
        self.end_group(super::SIMPLE_GROUP);
        self.end_link();
    }

    /// The link `ref_link` prepared, started where the text begins (after `\leavevmode`).
    pub(super) fn start_link(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let (kind, target) = match (doc.uri.take(), doc.link.take()) {
            (Some(uri), _) => (URL_LINK, Err(uri)),
            (None, Some((kind, name))) => (kind, Ok(name)),
            (None, None) => return,
        };
        // `\Hy@setpdfborder` (`0 0 0` under `hidelinks` and `colorlinks`),
        // `\Hy@setpdfhighlight`, `/C[\CurrentBorderColor]`.
        let border = if doc.pkg.hy.hide || doc.pkg.hy.colorlinks { "0 0 0" } else { doc.pkg.hy.border.as_str() };
        let attr = format!("/Border[{border}]/H/I/C[{}]", BORDER_COLORS[kind]);
        let color = doc.pkg.hy.colorlinks.then(|| doc.pkg.hy.colors[kind].clone());
        match target {
            // `\hyper@linkurl`'s `user{/Subtype/Link/A<<…/S/URI…>>}`.
            Err(uri) => self.append_uri_link(&attr, &uri),
            Ok(name) => self.append_start_link(&attr, &name),
        }
        // `\Hy@colorlink\@<kind>color`: `\begingroup\color{…}`, popped by `end_link`.
        if let Some(c) = color {
            self.color_stack(Some(c.as_str()));
        }
    }

    /// `\pdfendlink`.
    pub(super) fn end_link(&mut self) {
        // `\Hy@endcolorlink`'s `\endgroup`: the colour's `\reset@color`.
        if self.doc.as_ref().is_some_and(|d| d.pkg.hy.colorlinks) {
            self.color_stack(None);
        }
        self.cur.push(Node { kind: Kind::Whatsit, sub: super::super::engine_pdfdoc::END_LINK, font: 0, a: 0, b: 0 });
    }

    /// The kernel `equation`'s `\refstepcounter{equation}`; with hyperref (no amsmath, whose
    /// patch differs) `\hyper@makecurrent{equation}` and `\mathopen{\Hy@raisedlink{
    /// \hyper@anchorstart{equation.N}}}`.
    pub(super) fn equation_env_step(&mut self) {
        if !self.hyper() {
            return self.equation_step();
        }
        if self.ams() {
            // amsmath's `\incr@eqnum` after `\mathdisplay`: the kernel's `\refstepcounter`
            // target, `\__hyp_target_raise` in the display (`rt@mathopen` follows).
            let Some(name) = self.equation_anchor_step() else { return };
            if let Some(doc) = self.doc.as_mut() {
                doc.anchor = Some(name);
            }
            let mut t = self.tex_tokens("\\penalty10000 \\smash{\\raise\\baselineskip\\hbox{");
            t.push(self.doc_id(super::ANCHOR));
            t.extend(self.tex_tokens("}}\\kern0pt "));
            // Under cleveref the display has one more empty box here than hyperref's own
            // target gives (measured against pdfLaTeX's box dump, `engine_cleveref`).
            if self.doc.as_ref().is_some_and(|d| d.pkg.cref.is_some()) {
                t.extend(self.tex_tokens("\\hbox{}"));
            }
            return self.back_list(&t);
        }
        if self.doc.as_ref().is_some_and(|d| d.eq_within.is_some()) {
            return self.not_native("hyperref: a kernel equation numbered within");
        }
        let Some(name) = self.equation_anchor_step() else { return };
        let t = self.raised_link(Some(name));
        self.back_list(&t);
    }

    /// `\refstepcounter{equation}` with hyperref: the counter, `\@currentlabel`, and
    /// `\@currentHref` `equation.\theHequation` (`theorem_href`'s form), whose name it returns.
    pub(super) fn equation_anchor_step(&mut self) -> Option<String> {
        if self.koma_front_numbers("an equation number") || self.equation_before_chapter() {
            return None;
        }
        let doc = self.doc.as_mut()?;
        if doc.appendix {
            self.not_native("hyperref: an equation in the appendix");
            return None;
        }
        doc.equation += 1;
        let counters: Vec<u32> = doc.counters.iter().map(|&c| c as u32).collect();
        // hyperref's `\theHequation` is `\arabic{equation}`: a prefixed `\theequation` names no anchor.
        // report's `\@addtoreset{equation}{chapter}`: `\theHchapter.` first (latex.ltx).
        // In `subequations`, `\theHparentequation\alph{equation}`.
        let name = match &doc.sub_eq_h {
            Some(h) => format!("equation.{h}{}", super::alph(doc.equation, b'a').collect::<String>()),
            None => super::theorem_href("equation", &counters, doc.eq_within.or(doc.report.then_some(0)), doc.equation as u32),
        };
        let e = doc.equation;
        if self.doc.as_ref().is_some_and(|d| d.sub_eq.is_none()) && self.own_float_the(2, e).is_none() {
            self.not_native("a \\theequation the native job cannot read");
            return None;
        }
        let n = self.equation_text();
        self.store_current_label(&n);
        self.cref_step("equation");
        self.set_current_href(&name);
        Some(name)
    }

    /// `subequations` under hyperref: its `cmd/subequations/before` hook (`\stepcounter
    /// {equation}\protected@edef\theHparentequation{\theHequation}\addtocounter{equation}
    /// {-1}`), amsmath's `\refstepcounter{equation}` with its `\MakeLinkTarget` anchor where
    /// it stands (`vertical_anchor`, or raised in a paragraph), and the `after` hook's
    /// `\def\theHequation{\theHparentequation\alph{equation}}` (`Doc::sub_eq_h`). Not native
    /// in math. Under cleveref the steps are `equation_step`'s (`cref_step`).
    pub(super) fn subequations_anchor(&mut self) {
        if self.mode == super::super::engine_run::Mode::Math {
            return self.not_native("hyperref: subequations in math");
        }
        let Some(name) = self.equation_anchor_step() else { return };
        if self.mode == super::super::engine_run::Mode::Vertical {
            self.vertical_anchor(name.clone());
        } else {
            self.raised_target(name.clone());
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.sub_eq_h = Some(name["equation.".len()..].to_string());
        }
    }

    /// amsmath's `\incr@eqnum` in a display's tag (`\make@display@tag`): with hyperref the
    /// target raised where the tag box is being built, as nodes: `\penalty\@M`, the smashed
    /// `\raise\normalbaselineskip\hbox{dest}`, `\kern0pt` (the space factor kept).
    pub(super) fn tag_equation_step(&mut self) {
        if !self.hyper() {
            return self.equation_step();
        }
        let Some(name) = self.equation_anchor_step() else { return };
        self.raised_target(name);
    }

    /// `\__hyp_target_raise:n` of an anchor in horizontal mode, as nodes: `\penalty\@M`, the
    /// smashed `\raise\normalbaselineskip\hbox{dest}`, `\kern0pt`, the space factor kept.
    pub(super) fn raised_target(&mut self, name: String) {
        let bs = match self.e.eqtb.word(super::NORMAL_BASELINE) {
            0 => self.glue_param(super::BASELINE_SKIP).width,
            v => v,
        };
        let sf = self.aux;
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let start = self.cur.len();
        self.append_dest(name);
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let inner = self.hpack(start, 0, false);
        let inner = self.e.nodes.add_box(BoxRec { shift: -bs, ..inner });
        self.cur.push(Node::new(Kind::Box, inner as i32));
        let outer = self.hpack(start, 0, false);
        let outer = self.e.nodes.add_box(BoxRec { height: 0, depth: 0, ..outer });
        self.cur.extend([Node::new(Kind::Box, outer as i32), Node { sub: 1, ..Node::new(Kind::Kern, 0) }]);
        self.aux = sf;
    }

    /// `\mathopen` or (no anchor) `\mathclose` of `\Hy@raisedlink{…}` in math:
    /// `\penalty\@M \smash{\raise\baselineskip\hbox{…}}`, the anchor `rt@anchor`.
    pub(super) fn raised_link(&mut self, anchor: Option<String>) -> Vec<Token> {
        let open = anchor.is_some();
        let mut t = self.tex_tokens(if open { "\\mathopen{\\penalty10000 \\smash{\\raise\\baselineskip\\hbox{" } else { "\\mathclose{\\penalty10000 \\smash{\\raise\\baselineskip\\hbox{" });
        if let Some(name) = anchor {
            if let Some(doc) = self.doc.as_mut() {
                doc.anchor = Some(name);
            }
            t.push(self.doc_id(super::ANCHOR));
        }
        t.extend(self.tex_tokens("}}}"));
        t
    }

    /// `\Hy@raisedlink{\hyper@@anchor{name}{\relax}}` in text: `\penalty\@M \smash{\raise
    /// \baselineskip\hbox{anchor}}` (the space factor kept), then `\relax`.
    pub(super) fn text_raised_link(&mut self, name: String) -> Vec<Token> {
        if let Some(doc) = self.doc.as_mut() {
            doc.anchor = Some(name);
        }
        let mut t = self.tex_tokens("\\penalty10000 \\smash{\\raise\\baselineskip\\hbox{");
        t.push(self.doc_id(super::ANCHOR));
        t.extend(self.tex_tokens("}}\\relax "));
        t
    }

    /// `\hyper@anchorstart{name}` where it is, the name `raised_link` left: vertical as
    /// `vertical_anchor`; horizontal, `\new@pdflink`'s dest, then after glue g `\nobreak
    /// \hskip-g \hskip g`, else `\penalty10000`.
    pub(super) fn anchor_here(&mut self) {
        let Some(name) = self.doc.as_mut().and_then(|d| d.anchor.take()) else { return };
        if self.mode == super::super::engine_run::Mode::Vertical {
            return self.vertical_anchor(name);
        }
        let g = self.last_skip();
        self.append_dest(name);
        self.cur.push(Node::new(Kind::Penalty, 10000));
        if g.width != 0 {
            let n = self.e.nodes.glue_node(super::neg(g), 0);
            self.cur.push(n);
            let n = self.e.nodes.glue_node(g, 0);
            self.cur.push(n);
        }
    }

    /// hyperref's options (`\usepackage[…]{hyperref}`, `\hypersetup` in the preamble), the
    /// keys `HyperOpts` holds; the driver and `final` are the defaults, other keys are not
    /// native.
    pub(super) fn hyper_keys(&mut self, list: &str) -> Result<(), &'static str> {
        let Some(doc) = self.doc.as_mut() else { return Ok(()) };
        for item in key_items(list).into_iter().map(str::trim).filter(|i| !i.is_empty()) {
            let (key, value) = match item.split_once('=') {
                // keyval strips one level of braces around the value, and the spaces.
                Some((k, v)) => {
                    let v = v.trim();
                    (k.trim(), Some(v.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(v).trim()))
                }
                None => (item, None),
            };
            let flag = || match value {
                None | Some("true") => Ok(true),
                Some("false") => Ok(false),
                _ => Err("hyperref: an option value that is not true or false"),
            };
            // A name, or xcolor's `c!p!c2` expression (`\Hy@colorlink` gives it to `\color`
            // as is); `hyper_colors` computes it at `\begin{document}`, declining without xcolor.
            let name = || match value {
                Some(v) if v.starts_with(|c: char| c.is_ascii_alphabetic()) && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '!' || c == '.') => Ok(v.to_string()),
                _ => Err("hyperref: a link colour that is not a colour name"),
            };
            match key {
                "pdftex" | "final" if value.is_none() => {}
                "hidelinks" if value.is_none() => {
                    doc.pkg.hy.hide = true;
                    doc.pkg.hy.colorlinks = false;
                }
                "colorlinks" => doc.pkg.hy.colorlinks = flag()?,
                "linkcolor" => doc.pkg.hy.colors[LINK] = name()?,
                "citecolor" => doc.pkg.hy.colors[CITE] = name()?,
                "urlcolor" => doc.pkg.hy.colors[URL_LINK] = name()?,
                "allcolors" => doc.pkg.hy.colors = [name()?, name()?, name()?],
                // Kinds and anchors the native job does not link.
                "filecolor" | "anchorcolor" | "menucolor" | "runcolor" => {
                    name()?;
                }
                // hyperref's defaults under pdfTeX (`unicode` since 2022, `bookmarks`,
                // `pdfpagelabels`); `breaklinks` changes nothing pdfTeX writes (it breaks a
                // link's annotation at line ends either way).
                "unicode" | "bookmarks" | "pdfpagelabels" | "breaklinks" => {
                    if !flag()? {
                        return Err("hyperref: a default option turned off");
                    }
                }
                "bookmarksopen" => doc.pkg.hy.open = flag()?,
                "bookmarksnumbered" => doc.pkg.hy.numbered = flag()?,
                "pdfdisplaydoctitle" => doc.pkg.hy.doctitle = flag()?,
                "pdfusetitle" => doc.pkg.hy.usetitle = flag()?,
                // `plainpages=false`, hyperref's default: page anchors `page.<\thepage>`.
                "plainpages" => {
                    if flag()? {
                        return Err("hyperref: plainpages");
                    }
                }
                // `\define@key{Hyp}{pdfborder}{\def\@pdfborder{#1}}`: written as given.
                "pdfborder" => match value {
                    Some(v) if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == ' ' || c == '.') => doc.pkg.hy.border = v.split_whitespace().collect::<Vec<_>>().join(" "),
                    _ => return Err("hyperref: a pdfborder that is not numbers"),
                },
                // `\Hy@pstringdef\@pdfstartview{#1}`, a view without its coordinates.
                "pdfstartview" => match value.map(|v| v.trim_start_matches('/')) {
                    Some(v @ ("Fit" | "FitH" | "FitV" | "FitB" | "FitBH" | "FitBV")) => doc.pkg.hy.view = v.to_string(),
                    _ => return Err("hyperref: a pdfstartview with coordinates"),
                },
                "pdfpagemode" => match value {
                    Some(v @ ("UseNone" | "UseOutlines" | "UseThumbs" | "FullScreen" | "UseOC" | "UseAttachments")) => doc.pkg.hy.mode = v.to_string(),
                    _ => return Err("hyperref: a pdfpagemode hyperref does not know"),
                },
                k if INFO_KEYS.contains(&k) => {
                    let i = INFO_KEYS.iter().position(|&x| x == k).unwrap_or(0);
                    doc.pkg.hy.info[i] = Some(value.and_then(pdf_string).ok_or("hyperref: a document property with characters \\pdfstringdef changes")?);
                }
                _ => return Err("hyperref: an option the native job does not take"),
            }
        }
        Ok(())
    }

    /// `\hypersetup{keys}`, in the preamble.
    pub(super) fn hypersetup(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("hyperref: \\hypersetup after \\begin{document}");
        }
        let Some(text) = t.iter().map(|x| x.cs_id().is_none().then(|| x.chr())).collect::<Option<String>>() else {
            return self.not_native("hyperref: \\hypersetup with commands");
        };
        if let Err(what) = self.hyper_keys(&text) {
            self.not_native(what);
        }
    }

    /// hyperref's `\begin{document}` options: with `colorlinks`, `\RequirePackage{color}` and
    /// each link colour's literal.
    pub(super) fn hyper_colors(&mut self) {
        if !self.doc.as_ref().is_some_and(|d| d.pkg.hy.colorlinks) {
            return;
        }
        if !self.colored() {
            self.load_package("color");
        }
        for k in [LINK, CITE, URL_LINK] {
            let Some(name) = self.doc.as_ref().map(|d| d.pkg.hy.colors[k].clone()) else { return };
            let Some(lit) = self.color_literal(None, &name, false) else {
                return self.not_native("hyperref: a link colour the native job does not know");
            };
            if let Some(doc) = self.doc.as_mut() {
                doc.pkg.hy.colors[k] = lit;
            }
        }
    }
}

/// One character of a bookmark title as `\pdfstringdef` writes it (UTF-16BE): an ASCII
/// character `\000` and itself, or its octal escape when it is a space, a parenthesis or one
/// of `#$%&_`; any other both bytes as octal escapes.
fn bookmark_char(c: char, text: &mut String) {
    let u = c as u32;
    if u < 0x80 {
        text.push_str("\\000");
        if matches!(c, ' ' | '(' | ')' | '#' | '$' | '%' | '&' | '_') {
            text.push_str(&format!("\\{u:03o}"));
        } else {
            text.push(c);
        }
    } else {
        text.push_str(&format!("\\{:03o}\\{:03o}", u >> 8, u & 0xff));
    }
}
