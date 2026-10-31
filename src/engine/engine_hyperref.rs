//! hyperref on the native path (`2026-10-26-hyperref-design.md`): its anchors, links and
//! document entries, appended as the pdfTeX primitives hpdftex.def runs would append them.

use super::super::engine_node::{BoxRec, Kind, Node};
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
}

impl Default for HyperOpts {
    fn default() -> Self {
        Self { hide: false, colorlinks: false, colors: ["red", "green", "magenta"].map(String::from), info: Default::default() }
    }
}

const INFO_KEYS: [&str; 6] = ["pdfauthor", "pdftitle", "pdfsubject", "pdfcreator", "pdfproducer", "pdfkeywords"];

/// `\pdfstringdef` of plain text (hyperref's `unicode`): `\376\377`, then each character as
/// `\000c`, a space `\000\040`. Text with characters it escapes or changes is `None`.
fn pdf_string(v: &str) -> Option<String> {
    if v.is_empty() {
        return Some(String::new());
    }
    if v.contains("--") || v.contains("``") || v.contains("''") || v.contains("  ") {
        return None;
    }
    let mut out = String::from("\\376\\377");
    for c in v.chars() {
        match c {
            ' ' => out.push_str("\\000\\040"),
            '(' | ')' | '\\' | '%' | '#' | '$' | '&' | '^' | '_' | '~' | '{' | '}' | '`' => return None,
            c if c.is_ascii_graphic() => {
                out.push_str("\\000");
                out.push(c);
            }
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
        self.add_catalog("/PageMode/UseOutlines", Some((1, "/Fit")));
        self.add_catalog("", None);
        // `\PDF@FinishDoc`: `/Producer` only when set (pdfTeX writes its own).
        let info = self.doc.as_ref().map(|d| d.hy.info.clone()).unwrap_or_default();
        let field = |i: usize, default: &str| info[i].clone().unwrap_or_else(|| default.to_string());
        let mut text = format!("/Author({})/Title({})/Subject({})/Creator({})", field(0, ""), field(1, ""), field(2, ""), field(3, "LaTeX with hyperref"));
        if let Some(p) = &info[4] {
            text.push_str(&format!("/Producer({p})"));
        }
        text.push_str(&format!("/Keywords({})", field(5, "")));
        self.add_info(&text);
        // hyperref's `\addcontentsline` writes no bookmark while `\@currentHref` is empty.
        let entries: Option<Vec<(usize, String, String)>> = self.doc.as_ref().and_then(|d| d.contents.as_ref()).map(|t| t.iter().filter(|e| e.number != "#").map(|e| (e.level, e.number.clone(), e.title.clone())).collect());
        if let Some(entries) = entries {
            if let Err(what) = self.bookmarks(&entries) {
                return self.not_native(what);
            }
        }
        self.append_dest("Doc-Start".to_string());
        self.cur.push(Node::new(Kind::Penalty, 10000));
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

    /// `\ReadBookmarks` at `\begin{document}`: one `\pdfoutline goto name{anchor} count-n
    /// {title}` per contents entry, in order, n its direct children, the title as
    /// `\pdfstringdef` makes it (UTF-16BE with its mark, in octal escapes). `Err` for what is
    /// not ported: a title that is not plain text, a level jump.
    pub(super) fn bookmarks(&mut self, entries: &[(usize, String, String)]) -> Result<(), &'static str> {
        let report = self.doc.as_ref().is_some_and(|d| d.report);
        let mut prev = None;
        for (level, _, _) in entries {
            if *level > prev.map_or(0, |p: usize| p + 1) {
                return Err("hyperref: a bookmark level jump");
            }
            prev = Some(*level);
        }
        for (i, (level, number, title)) in entries.iter().enumerate() {
            let children = entries[i + 1..].iter().take_while(|e| e.0 > *level).filter(|e| e.0 == level + 1).count() as i32;
            let mut text = String::from("\\376\\377");
            let mut space = false;
            // The source's bytes, one to a character: UTF-8 (`\usepackage[utf8]{inputenc}`, the
            // kernel's default).
            let bytes = title.chars().map(|c| u8::try_from(c as u32)).collect::<Result<Vec<u8>, _>>();
            let Some(title) = bytes.ok().and_then(|b| String::from_utf8(b).ok()) else {
                return Err("hyperref: a bookmark title that is not plain text");
            };
            let mut chars = title.trim().chars();
            while let Some(c) = chars.next() {
                let c = match c {
                    ' ' | '\n' => {
                        space = true;
                        continue;
                    }
                    // `\&` and its kin are their characters.
                    '\\' => match chars.next() {
                        Some(c @ ('&' | '%' | '#' | '$' | '_')) => c,
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
            let Some(name) = heading_href(report, *level, number) else {
                return Err("hyperref: a heading number that is not arabic");
            };
            self.append_outline(&name, -children, &text);
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
        let border = if doc.hy.hide || doc.hy.colorlinks { 0 } else { 1 };
        let attr = format!("/Border[0 0 {border}]/H/I/C[{}]", BORDER_COLORS[kind]);
        let color = doc.hy.colorlinks.then(|| doc.hy.colors[kind].clone());
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
        if self.doc.as_ref().is_some_and(|d| d.hy.colorlinks) {
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
    fn equation_anchor_step(&mut self) -> Option<String> {
        let doc = self.doc.as_mut()?;
        if doc.appendix {
            self.not_native("hyperref: an equation in the appendix");
            return None;
        }
        doc.equation += 1;
        let counters: Vec<u32> = doc.counters.iter().map(|&c| c as u32).collect();
        // hyperref's `\theHequation` is `\arabic{equation}`: a prefixed `\theequation` names no anchor.
        // report's `\@addtoreset{equation}{chapter}`: `\theHchapter.` first (latex.ltx).
        let name = super::theorem_href("equation", &counters, doc.eq_within.or(doc.report.then_some(0)), doc.equation as u32);
        let n = self.equation_text();
        self.store_current_label(&n);
        self.set_current_href(&name);
        Some(name)
    }

    /// amsmath's `\incr@eqnum` in a display's tag (`\make@display@tag`): with hyperref the
    /// target raised where the tag box is being built, as nodes: `\penalty\@M`, the smashed
    /// `\raise\normalbaselineskip\hbox{dest}`, `\kern0pt` (the space factor kept).
    pub(super) fn tag_equation_step(&mut self) {
        if !self.hyper() {
            return self.equation_step();
        }
        let Some(name) = self.equation_anchor_step() else { return };
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
            let name = || match value {
                Some(v) if !v.is_empty() && v.chars().all(|c| c.is_ascii_alphabetic()) => Ok(v.to_string()),
                _ => Err("hyperref: a link colour that is not a colour name"),
            };
            match key {
                "pdftex" | "final" if value.is_none() => {}
                "hidelinks" if value.is_none() => {
                    doc.hy.hide = true;
                    doc.hy.colorlinks = false;
                }
                "colorlinks" => doc.hy.colorlinks = flag()?,
                "linkcolor" => doc.hy.colors[LINK] = name()?,
                "citecolor" => doc.hy.colors[CITE] = name()?,
                "urlcolor" => doc.hy.colors[URL_LINK] = name()?,
                "allcolors" => doc.hy.colors = [name()?, name()?, name()?],
                // Kinds and anchors the native job does not link.
                "filecolor" | "anchorcolor" | "menucolor" | "runcolor" => {
                    name()?;
                }
                k if INFO_KEYS.contains(&k) => {
                    let i = INFO_KEYS.iter().position(|&x| x == k).unwrap_or(0);
                    doc.hy.info[i] = Some(value.and_then(pdf_string).ok_or("hyperref: a document property with characters \\pdfstringdef changes")?);
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
        if !self.doc.as_ref().is_some_and(|d| d.hy.colorlinks) {
            return;
        }
        if !self.colored() {
            self.load_package("color");
        }
        for k in [LINK, CITE, URL_LINK] {
            let Some(name) = self.doc.as_ref().map(|d| d.hy.colors[k].clone()) else { return };
            let Some(lit) = self.color_literal(None, &name, false) else {
                return self.not_native("hyperref: a link colour the native job does not know");
            };
            if let Some(doc) = self.doc.as_mut() {
                doc.hy.colors[k] = lit;
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
