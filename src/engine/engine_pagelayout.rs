//! Page layout changes in the body: geometry.sty's (v5.9) `\newgeometry` and
//! `\restoregeometry`; the page colour (`\pagecolor`, color.sty, xcolor and pagecolor.sty).
//!
//! `\newgeometry{keys}` is `\clearpage`, `\Gm@restore@org` (the lengths as geometry found
//! them at load, `\paperwidth` and `\paperheight` as at `\begin{document}`), `\Gm@initnewgm`
//! (no key given), the keys (the preamble-only ones skipped with a warning), `\Gm@process`,
//! then `\Gm@changelayout`: `\@colht`, `\@colroom` and `\vsize` the new `\textheight`,
//! `\columnwidth`, `\hsize` and `\linewidth` the new `\textwidth`. `\restoregeometry` is
//! `\clearpage`, `\Gm@restore@pkg` (the lengths of `\begin{document}`) and
//! `\Gm@changelayout`. The output routine reads the class lengths each page, so the next
//! page takes them.

use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};
use super::{data_class, COLUMN_WIDTH, GEOMETRY, R_LINEWIDTH};

/// The natives of `\newgeometry` and `\restoregeometry` in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@newgeometry", |m| m.new_geometry()),
    ("rt@restoregeometry", |m| m.restore_geometry()),
    ("rt@newgeometry@", |m| m.new_geometry_apply()),
    ("rt@pagecolor", |m| m.page_color()),
    ("rt@nopagecolor", |m| m.no_page_color()),
];

/// geometry's state for `\newgeometry` (`Packages::gm`): the head lengths of
/// `\Gm@restore@org` (`\headheight`, `\headsep`, `\footskip` as geometry found them at
/// load), the `\Gm@restore@pkg` layout (taken at the first change), and a pending change's
/// keys (`restore` for `\restoregeometry`).
#[derive(Default, Clone, PartialEq)]
pub(super) struct GmState {
    pub org: [i32; 3],
    pkg: Option<[i32; 8]>,
    list: Option<String>,
    restore: bool,
}

/// The lengths `\Gm@changelayout` and the output routine read: [`data_class::CLASS_DIMENS`]
/// up to `\footskip`.
const LAYOUT: usize = 8;

impl Machine<'_, '_> {
    /// `\newgeometry{keys}`: the keys kept, then `\clearpage` and `rt@newgeometry@`.
    pub(super) fn new_geometry(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut list = String::new();
        for x in &t {
            match x.cs_id() {
                Some(id) => {
                    list.push('\\');
                    list.push_str(self.e.cs.name(id));
                    list.push(' ');
                }
                None => list.push(x.chr()),
            }
        }
        // `\ifGm@preamble` keys (paper, `landscape`, `mag`, ...) and the ones the native
        // geometry does not read at all.
        if ["paper", "landscape", "portrait", "mag", "truedimen", "layout", "driver", "pass", "reset", "showframe", "twoside", "twocolumn", "onecolumn", "bindingoffset"].iter().any(|k| list.contains(k)) {
            return self.not_native("\\newgeometry with a preamble key");
        }
        self.layout_change(Some(list));
    }

    /// `\restoregeometry`.
    pub(super) fn restore_geometry(&mut self) {
        self.layout_change(None);
    }

    fn layout_change(&mut self, list: Option<String>) {
        let Some(doc) = self.doc.as_mut() else { return };
        if !doc.body || doc.geometry.is_none() {
            return self.not_native("\\newgeometry in the preamble");
        }
        if doc.options & data_class::TWOCOLUMN != 0 || doc.fancy.is_some() || doc.koma || doc.memoir || doc.packages.iter().any(|p| matches!(*p, "multicol" | "longtable" | "hyperref" | "fancyhdr")) {
            return self.not_native("\\newgeometry with two columns, fancyhdr, multicol, longtable, hyperref or a KOMA or memoir class");
        }
        doc.pkg.gm.restore = list.is_none();
        doc.pkg.gm.list = Some(list.unwrap_or_default());
        let mut t = self.tex_tokens("\\clearpage");
        t.push(self.doc_id(doc!("rt@newgeometry@")));
        self.back_list(&t);
    }

    /// After the `\clearpage`: the new lengths, then `\Gm@changelayout`.
    pub(super) fn new_geometry_apply(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let (Some(list), Some(old)) = (doc.pkg.gm.list.take(), doc.geometry) else { return };
        let restore = std::mem::take(&mut doc.pkg.gm.restore);
        let (org, twoside) = (doc.pkg.gm.org, doc.options & 16 != 0);
        let now: [i32; LAYOUT] = std::array::from_fn(|i| self.e.eqtb.word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 }));
        let Some(doc) = self.doc.as_mut() else { return };
        let pkg = *doc.pkg.gm.pkg.get_or_insert(now);
        let v = if restore {
            pkg
        } else {
            let mut g = super::super::engine_geometry::Geometry::new(old.page());
            if let Err(what) = g.keys(&list, &mut |v| self.length_of(v)) {
                return self.not_native(what);
            }
            if g.footins.is_some() || g.columnsep.is_some() {
                return self.not_native("\\newgeometry with footnotesep or columnsep");
            }
            let (dims, head) = g.process(org, twoside);
            let mut v = [0; LAYOUT];
            v[..5].copy_from_slice(&dims);
            v[5..].copy_from_slice(&head);
            v
        };
        for (i, &x) in v.iter().enumerate() {
            self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: GEOMETRY + i as u32 }, x, false);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.class.vals[..LAYOUT].copy_from_slice(&v);
        }
        // `\Gm@changelayout`.
        let (width, height) = (v[0], v[1]);
        self.floats_begin(height);
        self.set_dimen_named(param!("vsize"), height);
        self.e.eqtb.set_word(COLUMN_WIDTH, width, false);
        self.set_dimen_named(param!("hsize"), width);
        self.set_rw(R_LINEWIDTH, width, false);
    }

    /// color.sty's and xcolor's `\pagecolor[model]{spec}`: `\color` in a group with
    /// `\set@color` made pdftex.def's `\set@page@color` (`\current@page@color` the colour,
    /// `\ifGPT@pagecolor` true) and `\ignorespaces` the group's end, so it leaves nothing in
    /// the list and the spaces after it stay. pagecolor.sty's own `\pagecolor` wraps it in
    /// `\@bsphack`/`\@esphack`: in a paragraph that is not native.
    pub(super) fn page_color(&mut self) {
        if self.mode != Mode::Vertical && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"pagecolor")) {
            return self.not_native("pagecolor.sty's \\pagecolor in a paragraph");
        }
        let text = |t: &[super::Token]| -> Option<String> { t.iter().map(|t| if t.cs_id().is_some() { None } else { Some(t.chr()) }).collect() };
        let model = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut spec = Vec::new();
        self.read_arg(&mut spec);
        let (Some(spec), model) = (text(&spec), model.map(|m| text(&m))) else {
            return self.not_native("\\pagecolor of a macro");
        };
        let model = match model {
            Some(None) => return self.not_native("\\pagecolor of a macro"),
            Some(Some(m)) => Some(m),
            None => None,
        };
        let Some(lit) = self.color_literal(model.as_deref(), spec.trim(), false) else {
            return self.not_native("\\pagecolor of a colour not native");
        };
        if let Some(doc) = self.doc.as_mut() {
            doc.pkg.page_color = Some(lit);
        }
    }

    /// `\nopagecolor`: pdftex.def's `\no@page@color` (`\GPT@pagecolorfalse`).
    pub(super) fn no_page_color(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.pkg.page_color = None;
        }
    }

    /// pdftex.def's part of the shipout background (`\@kernel@before@shipout@background`)
    /// while `\ifGPT@pagecolor`: `\put(-\hoffset,-\pdfpageheight+\voffset){\pdfcolorstack
    /// push{colour}\aftergroup\reset@color\rule{\pdfpagewidth}{\pdfpageheight}}`, first in
    /// the background picture: `\raise` by the page height of an `\hbox to\z@{\kern..
    /// push \hbox{rule}\hss}`, the pop after it. `false` when `\hoffset` or `\voffset` is set
    /// (the picture's coordinates through `\unitlength` are not ported).
    pub(super) fn page_color_put(&mut self, lit: &str) -> bool {
        let [ho, vo] = [param!("hoffset"), param!("voffset")].map(|r| self.e.eqtb.word(r));
        if ho != 0 || vo != 0 {
            return false;
        }
        let [w, h] = [0, 1].map(|i| self.e.eqtb.word(Reg { region: Region::Dimen, slot: super::PDF_PAGE_WIDTH + i }));
        let s = self.cur.len();
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -ho) });
        self.color_stack(Some(lit));
        let r = self.e.nodes.rule_node([w, h, 0]);
        let k = self.cur.len();
        self.cur.push(r);
        let rule = self.hpack(k, 0, false);
        let rule = self.e.nodes.add_box(rule);
        self.cur.push(Node::new(Kind::Box, rule as i32));
        let hss = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let n = self.e.nodes.glue_node(hss, 0);
        self.cur.push(n);
        let mut put = self.hpack(s, 0, true);
        put.shift = h - vo;
        let put = self.e.nodes.add_box(put);
        self.cur.push(Node::new(Kind::Box, put as i32));
        self.color_stack(None);
        true
    }
}
