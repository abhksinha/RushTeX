//! hyperref under the classes other than article, report and book (`2026-10-31-w9-hyperref-classes.md`):
//! one layer of anchor and bookmark hooks the class code calls, so that each class's
//! headings, contents and `\MakeLinkTarget` calls get hyperref's anchors in the order hpdftex.def,
//! hyperref-linktarget.dtx and the class's fix file (memhfixc.sty) put them.

use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};

impl Machine<'_, '_> {
    /// The deepest level whose `\addcontentsline` writes a bookmark: hyperref's
    /// `\let\Hy@bookmarksdepth\c@tocdepth` (hyperref.sty), the class's `tocdepth` (article 3,
    /// report and book 2, memoir 1); `\Hy@writebookmark` skips a deeper entry (no outline, no
    /// `.out` write). `level` is the native job's (article's `section` 0, report's `chapter` 0).
    pub(super) fn bookmark_level(&self, level: usize) -> bool {
        let Some(doc) = self.doc.as_ref() else { return false };
        let latex = level + !doc.report as usize;
        latex <= if doc.memoir { 1 } else if doc.report { 2 } else { 3 }
    }

    /// hyperref's bookmark `\protected@write` of an `\addcontentsline` at `level` (before the
    /// contents line's own write), when hyperref is loaded and the level is in the bookmarks.
    /// The KOMA-Script classes load the bookmark package with hyperref ("loading recommended
    /// package `bookmark'", scrreprt.cls): its `\BKM@entry` goes to the `.aux` file with an
    /// `\immediate\write`, so no write node.
    pub(super) fn bookmark_write(&mut self, level: usize) {
        if self.hyper() && self.bookmark_level(level) && !self.doc.as_ref().is_some_and(|d| d.koma) {
            self.aux_write(0);
        }
    }

    /// `\MakeLinkTarget[prefix]{}` (latex.ltx's ltmeta: `\special{}` in vertical mode without
    /// hyperref). hyperref's (hyperref-linktarget.dtx `\__hyp_target_counter_anon:n`):
    /// `\int_gincr:N\Hy@linkcounter`, `\@currentHref` globally `prefix*.N` (the prefix as
    /// given, not `\Hy@chapapp`), and `\Hy@raisedlink`'s anchor, in vertical mode the
    /// `\hyper@anchorstart` of `vertical_anchor`.
    pub(super) fn link_target(&mut self, prefix: &str) {
        if self.mode != Mode::Vertical {
            return self.not_native("\\MakeLinkTarget in a paragraph");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if !doc.packages.contains(&"hyperref") {
            self.cur.push(Node { kind: Kind::Whatsit, sub: super::super::engine_whatsit::SPECIAL, font: 0, a: 0, b: 0 });
            return;
        }
        doc.link_counter += 1;
        let name = format!("{prefix}*.{}", doc.link_counter);
        self.set_current_href(&name);
        self.vertical_anchor(name);
    }

    /// hpdftex.def's `\@schapter` and, with `secnumdepth` below 0 or outside the main matter,
    /// its `\@chapter`: `\Hy@MakeCurrentHrefAuto{\Hy@chapapp*}` (`chapter*.N`, after
    /// hyperref's `\appendix` `appendix*.N`) and `\Hy@raisedlink`'s anchor, before the class's
    /// own `\@chapter`/`\@schapter` runs (memoir's then makes its `\MakeLinkTarget[chapter]{}`).
    pub(super) fn unnumbered_chapter_anchor(&mut self) {
        let Some(doc) = self.doc.as_mut().filter(|d| d.packages.contains(&"hyperref")) else { return };
        doc.link_counter += 1;
        let name = super::hyperref::star_chapter_anchor(doc.appendix, doc.link_counter);
        self.set_current_href(&name);
        self.vertical_anchor(name);
    }

    /// A numbered chapter's `\refstepcounter{chapter}` anchor, `chapter.\theHchapter`, in
    /// vertical mode (hpdftex's `\@chapter` adds none of its own in the main matter).
    pub(super) fn numbered_chapter_anchor(&mut self) {
        if self.hyper() {
            let counters = self.doc.as_ref().map_or([0; 3], |d| d.counters);
            self.heading_anchor(0, &counters);
        }
    }

    /// A contents line's `\contentsline` link (`linktoc=section`) opened before its number:
    /// to the entry's anchor (`\@currentHref` when it was written) when the pre-scan named
    /// one, else to the numbered heading's.
    pub(super) fn contents_link_start(&mut self, level: usize, number: &str) {
        if !self.hyper() {
            return;
        }
        match self.toc_current_href() {
            Some(name) => self.toc_link_start_named(name),
            None => self.toc_link_start(level, number),
        }
    }

    /// An unnumbered section's anchor with hyperref: hpdftex's `\@ssect` (and its `\@sect` past
    /// `secnumdepth`, memhfixc's `\M@sect`) `\Hy@MakeCurrentHrefAuto{section*}`, the anchor first
    /// in the title (`section_anchor_tokens`). KOMA-Script's `\scr@startsection` passes
    /// hpdftex's patches: a starred section makes no anchor; past `secnumdepth` it runs
    /// `\hy@insteadofrefstepcounter{<level>}` (`<level>*.N`, vertical), not ported.
    pub(super) fn unnumbered_section_anchor(&mut self) {
        let Some(doc) = self.doc.as_mut().filter(|d| d.packages.contains(&"hyperref")) else { return };
        if doc.koma {
            if doc.toc {
                self.not_native("KOMA-Script: an unnumbered section past secnumdepth under hyperref");
            }
            return;
        }
        doc.link_counter += 1;
        let name = format!("section*.{}", doc.link_counter);
        doc.star_anchor = Some(name.clone());
        self.set_current_href(&name);
    }

    /// An unnumbered heading's `\Hy@SectionAnchorHref{\@currentHref}` (hpdftex.def's `\@ssect`,
    /// its `\@sect` past `secnumdepth`, memhfixc's `\M@sect`), put first in the title the class
    /// sets: `\Hy@@SectionAnchor`'s `\hbox to0pt{\kern-\Hy@SectionHShift \Hy@raisedlink{
    /// \hyper@anchorstart{…}\hyper@anchorend}\hss}` (the indent 0); nothing without one.
    pub(super) fn section_anchor_tokens(&mut self) -> Vec<super::super::engine_token::Token> {
        self.section_anchor_tokens_at(0)
    }

    /// [`Self::section_anchor_tokens`] with `\Hy@SectionHShift` `shift` (scaled points, the
    /// heading's `#3`: a run-in `\subparagraph`'s `\parindent`).
    pub(super) fn section_anchor_tokens_at(&mut self, shift: i32) -> Vec<super::super::engine_token::Token> {
        let Some(name) = self.doc.as_mut().and_then(|d| d.star_anchor.take()) else { return Vec::new() };
        let mut t = if shift == 0 { self.tex_tokens("\\hbox to0pt{\\kern0pt ") } else { self.tex_tokens(&format!("\\hbox to0pt{{\\kern-{shift}sp ")) };
        t.extend(self.text_raised_link(name));
        t.extend(self.tex_tokens("\\hss}"));
        t
    }

    /// `\normalcolor` as the class fonts use it (KOMA's `\sectfont`, `\usekomafont{pagenumber}`,
    /// ...): `\relax` until a colour package is loaded (hyperref's `colorlinks` loads color),
    /// then color.sty's `\let\current@color\default@color \set@color`: a push of the default
    /// colour and `\aftergroup\reset@color`, the pop at the group's end.
    pub(super) fn normal_color(&mut self) {
        if !self.colored() {
            return;
        }
        self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
        self.push_default_color();
        // `\aftergroup` at once (tokens put back now would run after the caller's).
        let level = self.e.eqtb.level();
        if level > 1 {
            let t = self.doc_id(doc!("rt@resetcolor"));
            self.after_group.push((level, t));
        }
    }

    /// The link's end of [`Self::contents_link_start`].
    pub(super) fn contents_link_end(&mut self) {
        if self.hyper() {
            self.toc_link_end();
        }
    }
}
