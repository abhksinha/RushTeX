//! a0poster.cls (v1.22b) on the GDL path: article with the poster's paper, its size
//! commands and `\veryHuge`, `\VeryHuge`, `\VERYHuge`.
//!
//! The class's parameters (`\textwidth`, `\textheight`, the zero head and margins, the
//! skips) and its size commands (`\fontsize{24.88}{30}\selectfont` for `\normalsize` and
//! kin: no display skips, no `\@listi`) are the class table (`rushtex-gen class --class
//! a0poster`, `data_class::A0_PAPERS`). What is code is here: the whatsits the class leaves
//! on the first page (its PostScript header's four `\write`s and `\closeout`, `\special
//! {header=./a0header.ps}`, and at `\begin{document}` `\special{papersize=..}`), the paper
//! size, and the three larger sizes.
//!
//! a0size.sty redeclares the Computer Modern shapes at the poster sizes; the native NFSS
//! table holds the kernel's, so a document that does not take all its fonts from lmodern
//! declines (`a0poster_fonts`). `posterdraft` declines in `class_options`.

use super::super::engine_node::{Kind, Node};
use super::super::engine_run::Machine;
use super::super::engine_whatsit::{CLOSE, NULL_STREAM, WRITE};

/// The paper (width, height) of each `data_class::A0_PAPERS` option, landscape as the class
/// sets it before `\ifportrait` swaps: `\setlength{\paperwidth}{118.82cm}` and kin.
const PAPERS: [[i32; 2]; 6] = [[221896897, 162227143], [221561268, 156558529], [156558529, 110761969], [110761969, 78279250], [78279250, 55380984], [221896897, 162227143]];

/// `\AtBeginDocument`'s `\special{papersize=w,h}` of each paper, landscape (a0b's is the
/// class's own 120cm by 91.51cm, not its paper).
const PAPERSIZE: [(&str, &str); 6] = [("120cm", "91.51cm"), ("118.82cm", "83.96cm"), ("83.96cm", "59.4cm"), ("59.4cm", "41.98cm"), ("41.98cm", "29.7cm"), ("120cm", "91.51cm")];

pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("veryHuge", |m| m.a0poster_size("74.3", "93")),
    ("VeryHuge", |m| m.a0poster_size("89.16", "112")),
    ("VERYHuge", |m| m.a0poster_size("107", "134")),
];

/// The commands a0poster.cls defines that the pre-scan takes under it.
pub(super) const A0POSTER_OWN: &[&str] = &["veryHuge", "VeryHuge", "VERYHuge"];

/// The paper of option key `options` (`\paperwidth`, `\paperheight` after `\ifportrait`).
pub(super) fn a0poster_paper(options: u32) -> [i32; 2] {
    let [w, h] = PAPERS[(options & 7) as usize];
    if options & 8 != 0 { [h, w] } else { [w, h] }
}

pub(super) const HOOKS: super::ClassHooks = super::ClassHooks {
    setup: |m, _| {
        if let Some(doc) = m.doc.as_mut() {
            doc.pkg.a0poster = true;
        }
        // article.cls's closing `\onecolumn` (`\clearpage`'s `\@@write\m@ne{}`), then the
        // class's `\write\Ausgabe{%%BeginFeature..}`, `{2 dict dup /PageSize..}`,
        // `{setpagedevice}`, `{%%EndFeature}` and `\closeout\Ausgabe`: done at the first
        // shipout (to a file the native job does not make), kept for their place in the list.
        for _ in 0..5 {
            m.cur.push(Node { kind: Kind::Whatsit, sub: WRITE, font: NULL_STREAM, a: 0, b: 0 });
        }
        m.cur.push(Node { kind: Kind::Whatsit, sub: CLOSE, font: NULL_STREAM, a: 0, b: 0 });
        let t = m.tex_tokens("\\special{header=./a0header.ps}");
        m.back_list(&t);
    },
    ..super::KERNEL_HOOKS
};

impl Machine<'_, '_> {
    /// a0poster's `\newcommand{\VeryHuge}{\fontsize{89.16}{112}\selectfont}` and kin.
    fn a0poster_size(&mut self, size: &str, skip: &str) {
        if !self.doc.as_ref().is_some_and(|d| d.pkg.a0poster) {
            return self.not_native("a0poster's size commands under another class");
        }
        let t = self.tex_tokens(&format!("\\fontsize{{{size}}}{{{skip}}}\\selectfont "));
        self.back_list(&t);
    }

    /// a0poster's `\\AtBeginDocument`: `\\special{papersize=w,h}` and `\\pagestyle{empty}`.
    pub(super) fn a0poster_begin(&mut self) {
        let Some(options) = self.doc.as_ref().filter(|d| d.pkg.a0poster).map(|d| d.options) else { return };
        let (w, h) = PAPERSIZE[(options & 7) as usize];
        let (w, h) = if options & 8 != 0 { (h, w) } else { (w, h) };
        if let Some(doc) = self.doc.as_mut() {
            doc.empty_style = true;
            doc.headings = false;
        }
        let t = self.tex_tokens(&format!("\\special{{papersize={w},{h}}}"));
        self.back_list(&t);
    }

    /// a0size.sty's Computer Modern shapes are not the native table's: every family must
    /// be lmodern's (`\usepackage{lmodern}`, which also sets the math fonts).
    pub(super) fn a0poster_fonts(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.pkg.a0poster && !d.packages.contains(&"lmodern")) {
            self.not_native("a0poster without lmodern (a0size.sty's Computer Modern shapes)");
        }
    }
}
