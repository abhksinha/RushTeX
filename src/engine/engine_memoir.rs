//! memoir.cls on the GDL path (`2026-10-31-memoir-moderncv.md`), its first slice: the class
//! table (`rushtex-gen class --class memoir`), the page styles memoir builds with
//! `\makepagestyle`, the column's `\@texttop` (`\@mkidx`) and `\@makecol`, and the
//! differences of its `\@startsection`/`\M@sect` from the kernel's (in `section_skip`,
//! `section_start`, `section_marks`). Everything else memoir.cls defines stops in the pre-scan
//! (`MEMOIR_NATIVE`).

use super::super::engine_eqtb::Glue;
use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::book::{label_digits, LEFT, RIGHT};
use super::fancy::HEAD_BOX;
use super::{COLUMN_WIDTH, NORMALSIZE};

/// `0.3` as TeX scans it (§452 `round_decimals`): `\footruleskip`'s
/// `0.3\normalbaselineskip`.
const POINT_THREE: i32 = 19661;

impl Machine<'_, '_> {
    pub(super) fn memoir(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.memoir)
    }

    /// `\mem@ps@HLH`'s box (memoir.cls) for one head or foot, its three slots as tokens:
    /// `\hb@xt@ W{\m@mhe@dreset \vbox{[foot rule] \hbox{\rlap{\parbox[b]{W}{\raggedright
    /// \strut{L}\strut}}\hfill \parbox[b]{W}{\centering\strut{C}\strut}\hfill \llap{\parbox
    /// [b]{W}{\raggedleft\strut{R}\strut}}} [head rule]}}` between `\makeheadposition`'s
    /// `\hss` and `\hss` (`\mem@ps@entry@wrap` is empty, so each slot is a group;
    /// `\mem@hvboxm`'s `\setbox0\vbox{..}\box0` is the `\vbox` itself; `\baselinestretch` is
    /// 1 already). The head rule is `\hrule\@width W\@height0pt \vskip-0pt`, the foot rule
    /// `\vskip-\footruleskip\vskip-0pt \hrule\@width W\@height0pt \vskip\footruleskip`.
    fn memoir_slots(&mut self, head: bool, slots: [Vec<Token>; 3]) -> Vec<Token> {
        let w = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let mut t = self.tex_tokens(&format!("\\hss\\hbox to{w}sp{{\\normalsize\\vbox{{"));
        if !head {
            // `\footruleskip` is `0.3\normalbaselineskip`, the class's `\baselineskip` at
            // `\normalsize`.
            let bs = super::glue_words(self.size_row(NORMALSIZE).baselineskip).width;
            let skip = crate::backend::scaled::xn_over_d(bs, POINT_THREE, 0x10000);
            t.extend(self.tex_tokens(&format!("\\begingroup\\vskip-{skip}sp\\vskip-0sp\\hrule width{w}sp height0sp\\vskip{skip}sp\\endgroup")));
        }
        let [l, c, r] = slots;
        t.extend(self.tex_tokens(&format!("\\hbox{{\\rlap{{\\parbox[b]{{{w}sp}}{{\\raggedright\\strut{{")));
        t.extend(l);
        t.extend(self.tex_tokens(&format!("}}\\strut}}}}\\hfill\\parbox[b]{{{w}sp}}{{\\centering\\strut{{")));
        t.extend(c);
        t.extend(self.tex_tokens(&format!("}}\\strut}}\\hfill\\llap{{\\parbox[b]{{{w}sp}}{{\\raggedleft\\strut{{")));
        t.extend(r);
        t.extend(self.tex_tokens("}\\strut}}}"));
        if head {
            t.extend(self.tex_tokens(&format!("\\hrule width{w}sp height0sp\\vskip-0sp")));
        }
        t.extend(self.tex_tokens("}}\\hss "));
        t
    }

    /// The head and foot of page `page` under memoir's `headings` (the class's last
    /// `\pagestyle`), as tokens (`start_foot`): the head into [`HEAD_BOX`], the foot into the
    /// output nest, in `\@outputpage`'s `\reset@font\normalsize`. Two-sided: odd head
    /// `{\slshape\rightmark}{}{\thepage}`, even head `{\thepage}{}{\slshape\leftmark}`;
    /// one-sided the odd head on every page. The feet are empty slots.
    pub(super) fn memoir_page_tokens(&mut self, page: i32) -> Vec<Token> {
        let Some(doc) = self.doc.as_mut() else { return Vec::new() };
        doc.head_page = true;
        let even = doc.class.flag("twoside") && page % 2 == 0;
        let digits = label_digits(&self.the_page(page));
        let mark = self.page_mark_text(if even { LEFT as u32 } else { RIGHT as u32 }, !even).unwrap_or_default();
        let mut sl = self.tex_tokens("\\slshape ");
        sl.extend(mark);
        let slots = if even { [digits, Vec::new(), sl] } else { [sl, Vec::new(), digits] };
        let mut t = self.tex_tokens(&format!("\\begingroup\\normalfont\\normalsize\\global\\setbox{HEAD_BOX}\\hbox{{"));
        t.extend(self.memoir_slots(true, slots));
        t.extend(self.tex_tokens("}"));
        t.extend(self.memoir_slots(false, [Vec::new(), Vec::new(), Vec::new()]));
        t.extend(self.tex_tokens("\\endgroup "));
        t
    }

    /// memoir's `\mem@makecol` with no footnotes, floats, side bar or side feet: `\setbox
    /// \@outputbox\box\@cclv`, then `\m@m@makecoltext`: `\vbox to\@colht{\@texttop \dimen@
    /// \dp\@outputbox \unvbox\@outputbox \vskip-\dimen@ \@textbottom}`, where `\@texttop` is
    /// `\@mkidx`: `\vbox to\z@{\rlap{\@rightidx \box\@indexbox}\vss}` (`\@leftidx`,
    /// `\hskip-\marginparsep \hskip-\marginparwidth`, on an even two-sided page), and
    /// `\@textbottom` `\relax` under the class's `\flushbottom`, `\vskip\z@\@plus.0001fil`
    /// under `\raggedbottom`.
    pub(super) fn memoir_column(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let class = doc.class;
        let (page, colht) = (doc.page, doc.floats.colht);
        if self.e.eqtb.box_reg(super::FOOTINS).is_some() || doc.floats.has_bottom() {
            return self.not_native("memoir: a column with footnotes or floats");
        }
        let Some(b) = self.e.eqtb.box_reg(255) else { return };
        let start = self.cur.len();
        let fil = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        if class.flag("twoside") && page % 2 == 0 {
            for w in [self.rw(super::R_MARGINPARSEP), self.rw(super::R_MARGINPARWIDTH)] {
                let n = self.e.nodes.glue_node(Glue { width: -w, ..Glue::default() }, 0);
                self.cur.push(n);
            }
        } else {
            for w in [self.e.eqtb.word(COLUMN_WIDTH), self.rw(super::R_MARGINPARSEP)] {
                let n = self.e.nodes.glue_node(Glue { width: w, ..Glue::default() }, 0);
                self.cur.push(n);
            }
        }
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let rlap = self.hpack(start, 0, true);
        let rlap = self.e.nodes.add_box(rlap);
        self.cur.push(Node::new(Kind::Box, rlap as i32));
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let d = self.dimen_param(super::super::engine_pack::BOX_MAX_DEPTH);
        let idx = self.vpack(start, 0, true, d);
        let idx = self.e.nodes.add_box(BoxRec { vertical: true, ..idx });
        self.cur.push(Node::new(Kind::Box, idx as i32));
        let r = self.e.nodes.boxes[b as usize];
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.eqtb.replace_box(255, None);
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
        let n = self.e.nodes.glue_node(Glue { width: -r.depth, ..Glue::default() }, 0);
        self.cur.push(n);
        if class.flag("raggedbottom") {
            // `.0001` is 7sp (§452).
            let n = self.e.nodes.glue_node(Glue { stretch: 7, stretch_order: 1, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        let column = self.vpack(start, colht, true, d);
        let column = self.e.nodes.add_box(BoxRec { vertical: true, ..column });
        if let Some(doc) = self.doc.as_mut() {
            doc.column = Some(column);
        }
        self.opcol();
    }
}
