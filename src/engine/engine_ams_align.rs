//! amsmath's `align` and `align*` (amsmath.sty `\start@align`, `\align@`, `\math@cr`,
//! `\measure@`, `\calc@shift@align`, `\place@tag`), in one pass.
//!
//! amsmath sets the body twice: a measuring `\halign` finds the column widths, each row's
//! field widths and tag width, from which `\eqnshift@` (the glue before the first column),
//! `\alignsep@` (between column pairs) and `\tagshift@` follow; then the real `\halign`
//! uses them. Here the body is set once. `\maxfields@` is counted from its tokens before
//! the alignment opens; the rows record their field widths, depths and tags as they are
//! built; at the alignment's end [`align_shifts`] runs amsmath's arithmetic, and
//! `align_retab` writes the glue and the tag entries into the rows already built.

use super::super::engine_align::NO_ALIGN_GROUP;
use super::super::engine_box::fil_glue;
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_pack::{BASELINE_SKIP, LINE_SKIP, LINE_SKIP_LIMIT};
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::END_TEMPLATE_ID;
use super::{AL_ALIGN_CR, AL_END, AL_FIELD, AL_HALIGN, AL_START, AL_TAG, AL_TAG_KEEP, AL_TAG_NUM};

/// `\strutbox@`, a hidden box register.
const STRUT_BOX: u32 = 32703;
/// latex.ltx's `\jot` and `\normallineskiplimit`; the native path takes no assignment to them.
const JOT: i32 = 3 * 65536;
const NORMAL_LINE_SKIP_LIMIT: i32 = 0;
/// `\interdisplaylinepenalty`, a count register of latex.ltx that amsmath sets to `\@M`;
/// the native path takes no assignment to it.
const INTER_DISPLAY_LINE_PENALTY: i32 = 10000;
/// amsmath's `\minalignsep`.
const MIN_ALIGN_SEP: i32 = 10 * 65536;

/// One `align` being read.
pub(super) struct AmsAlign {
    starred: bool,
    /// `\maxfields@`, counted from the body and made even.
    maxfields: usize,
    /// `doc.tabs.len()` when it opened: `\\` is `\math@cr` while no `tabular` opened since.
    tabs: usize,
    /// `\if@eqnsw`, `\ifdt@p`, `\@eqpen`.
    pub(super) eqnsw: bool,
    dtp: bool,
    eqpen: i32,
    rows: Vec<Row>,
    row: Row,
}

/// What a row gives the measuring pass: its field widths (`\field@lengths`), the depth of
/// its deepest field (`\lineht@`) and its tag's width (`\tag@lengths`, 0 for none).
#[derive(Default)]
struct Row {
    fields: Vec<i32>,
    lineht: i32,
    tag: i32,
}

/// The numbers `\measure@` and `\calc@shift@align` settle for `align` (`\xatlevel@` 1, tags
/// right, no `fleqn`).
#[derive(Debug, PartialEq)]
struct Shifts {
    eqnshift: i32,
    alignsep: i32,
    tagshift: i32,
    /// `\totwidth@` after its `\minalignsep`s, for `\black@`.
    totwidth: i32,
    /// Whether any tag must go below its row (`\shift@tag` 1).
    drops: bool,
}

/// `\measure@` after the measuring `\halign`, then `\calc@shift@align`: `cols` are the
/// field columns' widths (`\maxcol@width`), `rows` each row's field widths and tag width.
fn align_shifts(dw: i32, cols: &[i32], rows: &[(&[i32], i32)], mintagsep: i32) -> Shifts {
    let maxcol = |c: usize| cols.get(c - 1).copied().unwrap_or(0);
    let totwidth: i32 = cols.iter().sum();
    let pairs = (cols.len() / 2) as i32;
    // `\ifcase\xatlevel@` `\or`: `\@tempcnta` pairs plus one, `\@tempcntb` pairs less one.
    let (cnta, cntb) = (pairs + 1, pairs - 1);
    let mut eqnshift = (dw - totwidth) / cnta;
    let mut alignsep = eqnshift;
    if alignsep < MIN_ALIGN_SEP {
        alignsep = MIN_ALIGN_SEP;
        if eqnshift > 0 {
            eqnshift = (dw - totwidth - cntb * alignsep) / 2;
        }
    }
    eqnshift = eqnshift.max(0);
    let mut drops = false;
    // `\calc@shift@align`: the rows from the last, `\x@calc@shift@rc` for each with a tag.
    for &(fields, tagwidth) in rows.iter().rev() {
        if tagwidth <= 0 {
            continue;
        }
        let (mut dimb, mut dimc) = (0, 0);
        for (k, &a) in fields.iter().enumerate() {
            let column = k + 1;
            // `\x@rcalc@width`.
            if a > 0 {
                dimc += dimb;
                if column % 2 == 1 {
                    dimc += maxcol(column);
                    dimb = 0;
                } else {
                    dimc += a;
                    dimb = maxcol(column) - a;
                }
            } else {
                dimb += maxcol(column);
            }
        }
        let column = ((fields.len() as i32) - 1) / 2;
        let (mut a, mut b) = (cnta, cntb);
        if b > column {
            a = a - b + column;
            b = column;
        }
        let dima = dimc + tagwidth;
        if MIN_ALIGN_SEP * b + 2 * mintagsep + dima > dw {
            drops = true;
            continue;
        }
        if eqnshift + dima + b * alignsep + tagwidth > dw {
            let d = if a > 0 { (dw - dima) / a } else { 0 };
            if d < MIN_ALIGN_SEP {
                alignsep = MIN_ALIGN_SEP;
                eqnshift = (dw - dima - b * alignsep) / 2;
            } else {
                if d < eqnshift {
                    eqnshift = d.max(0);
                }
                if d < alignsep {
                    alignsep = d;
                }
            }
        }
    }
    let tagshift = totwidth + cntb * alignsep + eqnshift - dw;
    Shifts { eqnshift, alignsep, tagshift, totwidth: totwidth + cntb * MIN_ALIGN_SEP, drops }
}

impl Machine<'_, '_> {
    /// `\begin{align}` (`align*`): `\start@align`'s `\collect@body`, here a count of the
    /// body's fields per row for `\maxfields@`, the body put back; `\dollardollar@begin`,
    /// then [`Self::align_start`] in the display.
    pub(super) fn align_begin(&mut self, starred: bool) {
        if self.mode == Mode::Math {
            return self.not_native("an align inside math");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        self.ams_display(3);
        let name = if starred { "align*" } else { "align" };
        let (begin, end, nl) = (self.e.cs.intern("begin"), self.e.cs.intern("end"), self.e.cs.intern("\\"));
        let mut body = Vec::new();
        let (mut depth, mut fields, mut maxfields) = (0i32, 1usize, 0usize);
        // Open `array`s and amsmath's environments built on it: their `&` and `\\` are
        // their own.
        let mut inner = 0usize;
        loop {
            let Some(t) = self.get_token() else { return self.not_native("an align without its end") };
            if t == Token::cs(end) || t == Token::cs(begin) {
                let mut arg = Vec::new();
                self.read_arg(&mut arg);
                let env: String = arg.iter().map(|t| t.chr()).collect();
                if t == Token::cs(end) && env == name && inner == 0 {
                    break;
                }
                if !matches!(env.as_str(), "array" | "cases" | "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix") {
                    return self.not_native("an environment inside align");
                }
                if t == Token::cs(end) {
                    let Some(n) = inner.checked_sub(1) else { return self.not_native("an \\end inside align without its \\begin") };
                    inner = n;
                } else {
                    inner += 1;
                }
                body.push(t);
                body.push(Token::char(Cat::BeginGroup, '{'));
                body.extend(arg);
                body.push(Token::char(Cat::EndGroup, '}'));
                continue;
            }
            match t.cs_id() {
                None if t.cat() == Some(Cat::BeginGroup) => depth += 1,
                None if t.cat() == Some(Cat::EndGroup) => depth -= 1,
                None if t.cat() == Some(Cat::AlignTab) && depth == 0 && inner == 0 => fields += 1,
                Some(c) if c == nl && depth == 0 && inner == 0 => {
                    maxfields = maxfields.max(fields);
                    fields = 1;
                }
                Some(c) if self.doc.as_ref().is_some_and(|d| d.par_id == c) => return self.not_native("a paragraph in align"),
                Some(c) if self.hides_tab(c, 8) => return self.not_native("a macro with & in align"),
                _ => {}
            }
            body.push(t);
        }
        maxfields = maxfields.max(fields);
        maxfields += maxfields % 2;
        let tabs = self.doc.as_ref().map_or(0, |d| d.tabs.len());
        if let Some(doc) = self.doc.as_mut() {
            doc.align = Some(Box::new(AmsAlign { starred, maxfields, tabs, eqnsw: !starred, dtp: true, eqpen: INTER_DISPLAY_LINE_PENALTY, rows: Vec::new(), row: Row::default() }));
        }
        let mut t = Vec::new();
        self.tex("$$", &mut t);
        t.push(self.doc_id(AL_START));
        t.extend(body);
        t.push(Token::cs(end));
        t.push(Token::char(Cat::BeginGroup, '{'));
        t.extend(name.chars().map(|c| Token::char(if c == '*' { Cat::Other } else { Cat::Letter }, c)));
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// Whether macro `c`, or one its text names (to `depth` levels), holds `&` or `\\`.
    fn hides_tab(&self, c: u32, depth: u8) -> bool {
        let Meaning::Macro(m) = self.e.eqtb.meaning(c) else { return false };
        self.e.toks.get(m.text).iter().any(|t| match t.cs_id() {
            None => t.cat() == Some(Cat::AlignTab),
            Some(x) => self.e.cs.name(x) == "\\" || depth == 0 || x != c && self.hides_tab(x, depth - 1),
        })
    }

    /// In the display: `\align@`'s `\displ@y@`, whose `\spread@equation` first does
    /// `\reset@strutbox@`: `\global\setbox\strutbox@\hbox{\lower.5\normallineskiplimit
    /// \vbox{\kern-\normallineskiplimit\copy\strutbox}}`; then [`Self::align_halign`].
    pub(super) fn align_start(&mut self) {
        let (ht, dp) = (self.strut_height(), self.strut_depth());
        let mut t = Vec::new();
        self.tex(&format!("\\global\\setbox{STRUT_BOX}\\hbox{{\\lower{}sp\\vbox{{\\kern{}sp\\hbox{{\\vrule height{ht}sp depth{dp}sp width0sp}}}}}}", NORMAL_LINE_SKIP_LIMIT / 2, -NORMAL_LINE_SKIP_LIMIT), &mut t);
        t.push(self.doc_id(AL_HALIGN));
        self.back_list(&t);
    }

    /// `\openup\jot`, `\everycr` the rows' `\noalign` (`\@display@init`), then the `\halign`
    /// with the two templates of `\align@preamble`.
    pub(super) fn align_halign(&mut self) {
        for slot in [LINE_SKIP, BASELINE_SKIP] {
            let reg = Reg { region: Region::Glue, slot };
            let g = self.e.eqtb.glue(reg);
            self.e.eqtb.set_glue(reg, Glue { width: g.width + JOT, ..g }, false);
        }
        let lsl = Reg { region: Region::Dimen, slot: LINE_SKIP_LIMIT };
        let v = self.e.eqtb.word(lsl);
        self.e.eqtb.set_word(lsl, v + JOT, false);
        if let Some(reg) = super::super::engine_names::param("everycr") {
            let span = self.e.toks.store(&[self.doc_id(AL_ALIGN_CR)]);
            self.e.eqtb.set_toks(reg, span, false);
        }
        // `&\hfil\strut@\setboxz@h{\@lign$\m@th\displaystyle{##}$}\ifmeasuring@
        // \savefieldlength@\fi\set@field\tabskip\z@skip &\setboxz@h{\@lign$\m@th
        // \displaystyle{{}##}$}\ifmeasuring@\savefieldlength@\fi\set@field\hfil\tabskip\alignsep@`.
        let mut u = [Vec::new(), Vec::new()];
        self.tex(&format!("\\hfil\\copy{STRUT_BOX} \\setbox0\\hbox{{$\\mathsurround0pt\\displaystyle{{"), &mut u[0]);
        self.tex("\\setbox0\\hbox{$\\mathsurround0pt\\displaystyle{{}", &mut u[1]);
        let mut v = [Vec::new(), Vec::new()];
        for (k, v) in v.iter_mut().enumerate() {
            self.tex("}$}", v);
            v.push(self.doc_id(AL_FIELD));
            if k == 1 {
                self.tex("\\hfil", v);
            }
            v.push(Token::cs(END_TEMPLATE_ID));
        }
        let u = [self.e.toks.store(&u[0]), self.e.toks.store(&u[1])];
        let v = [self.e.toks.store(&v[0]), self.e.toks.store(&v[1])];
        // The `\everycr` of the preamble's `\cr`.
        let cr = self.doc_id(AL_ALIGN_CR);
        self.input.back(cr);
        self.init_align_periodic(u, v);
    }

    /// `\set@field` after a field's box: `\column@plus`, `\lineht@` its depth when deeper,
    /// `\boxz@`.
    pub(super) fn align_field(&mut self) {
        let (w, d) = self.e.eqtb.box_reg(0).map_or((0, 0), |b| {
            let b = &self.e.nodes.boxes[b as usize];
            (b.width, b.depth)
        });
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.row.fields.push(w);
            a.row.lineht = a.row.lineht.max(d);
        }
        let mut t = Vec::new();
        self.tex("\\box0 ", &mut t);
        self.back_list(&t);
    }

    /// `\\` in an `align`, `\math@cr`: `\@eqpen` (`\\*` none), then `[skip]` read without
    /// skipping spaces (`\new@ifnextchar`), the `{\ifnum0=`}\fi` brace hiding a following
    /// `&` while they are looked for; then the row's end.
    pub(super) fn align_cr(&mut self) {
        self.input.align_state += 1;
        let star = self.next_is('*');
        let skip = match self.get_token() {
            Some(t) if t == Token::char(Cat::Other, '[') => Some(self.bracket_arg()),
            Some(t) => {
                self.input.back(t);
                None
            }
            None => None,
        };
        self.input.align_state -= 1;
        let skip = match skip {
            Some(arg) => match self.glue_arg(&arg) {
                Some(g) if g.stretch == 0 && g.shrink == 0 => g.width,
                _ => return self.not_native("\\\\[...] in align that is not a length"),
            },
            None => 0,
        };
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.eqpen = if star { 10000 } else { INTER_DISPLAY_LINE_PENALTY };
        }
        self.align_row_end(skip, None);
    }

    /// `\math@cr@@@align` and `\noalign{\vskip skip}`: `\add@amps\maxfields@`, `\omit`, the
    /// tag (`\rt@altag`), `\cr`; at the end the alignment's end native follows.
    fn align_row_end(&mut self, skip: i32, last: Option<Token>) {
        let Some(a) = self.doc.as_ref().and_then(|d| d.align.as_ref()) else { return };
        let amps = a.maxfields.saturating_sub(a.row.fields.len());
        let mut t: Vec<Token> = (0..amps).map(|_| Token::char(Cat::AlignTab, '&')).collect();
        self.tex("\\omit", &mut t);
        t.push(self.doc_id(AL_TAG));
        self.tex(&format!("\\cr\\noalign{{\\vskip{skip}sp\\relax}}"), &mut t);
        t.extend(last);
        self.back_list(&t);
    }

    /// The tag entry: `\ifst@rred\nonumber\fi \if@eqnsw\global\tag@true\fi`, then with a tag
    /// `\setboxz@h{\@lign\strut@{\make@display@tag}}`, its placing left to the end.
    pub(super) fn align_tag(&mut self) {
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        if a.starred {
            a.eqnsw = false;
        }
        let mut t = Vec::new();
        if a.eqnsw {
            self.tex(&format!("\\setbox0\\hbox{{\\copy{STRUT_BOX} {{\\rt@eqstep"), &mut t);
            t.push(self.doc_id(AL_TAG_NUM));
            self.tex("}}", &mut t);
        }
        t.push(self.doc_id(AL_TAG_KEEP));
        self.back_list(&t);
    }

    /// `\print@eqnum` (`\tagform@\theequation`) and the stored `\label`.
    pub(super) fn align_tag_num(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let (n, label) = (doc.equation, doc.df_label.take());
        let mut t = Vec::new();
        self.tex(&format!("\\hbox{{\\rt@normalfont({n}\\/)}}"), &mut t);
        if let Some(key) = label {
            self.tex("\\rt@ltxlabel", &mut t);
            t.push(Token::char(Cat::BeginGroup, '{'));
            t.extend(key);
            t.push(Token::char(Cat::EndGroup, '}'));
        }
        self.back_list(&t);
    }

    /// The row's end: its tag box kept in the entry, `\ifst@rred\else\global\@eqnswtrue\fi`,
    /// `\global\lineht@\z@`.
    pub(super) fn align_tag_keep(&mut self) {
        let w = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        let tagged = a.eqnsw;
        let mut row = std::mem::take(&mut a.row);
        row.tag = if tagged { w.max(1) } else { 0 };
        a.rows.push(row);
        a.eqnsw = !a.starred;
        if tagged {
            let mut t = Vec::new();
            self.tex("\\box0 ", &mut t);
            self.back_list(&t);
        }
    }

    /// `\everycr`'s `\noalign`: the first (`\ifdt@p`) `\vskip-\lineskiplimit\vskip
    /// \normallineskiplimit`, later ones `\penalty\@eqpen`.
    pub(super) fn align_every_cr(&mut self) {
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        if std::mem::replace(&mut a.dtp, false) {
            let lsl = self.dimen_param(LINE_SKIP_LIMIT);
            self.build_glue(Glue { width: -lsl, ..Glue::default() }, false);
            self.build_glue(Glue { width: NORMAL_LINE_SKIP_LIMIT, ..Glue::default() }, false);
        } else {
            let p = a.eqpen;
            self.cur.push(Node::new(Kind::Penalty, p));
        }
        self.end_no_align();
    }

    /// Whether `\\` is `\math@cr` of an `align`.
    pub(super) fn in_align(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.align.as_ref().is_some_and(|a| a.tabs == d.tabs.len()))
    }

    /// `\end{align}` in its last row: `\math@cr` (no `*`, no `[`), then the alignment's end.
    pub(super) fn align_end(&mut self) {
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.eqpen = INTER_DISPLAY_LINE_PENALTY;
        }
        let end = self.doc_id(AL_END);
        self.align_row_end(0, Some(end));
    }

    /// The alignment's `}` (`\black@\totwidth@\egroup`): the measuring pass's numbers, the
    /// tabskip glue and the tags written into the rows, `fin_align`, `\dollardollar@end`,
    /// `\ignorespacesafterend`.
    pub(super) fn align_finish(&mut self) {
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.take()) else { return };
        if let Some(doc) = self.doc.as_mut() {
            doc.ams_display = 0;
            if doc.df_label.is_some() {
                return self.not_native("a \\label on an align row without a number");
            }
        }
        let dw = self.dimen_param(super::super::engine_noad::DISPLAY_WIDTH);
        let cols = self.align_widths(a.maxfields);
        // `\mintagsep`: `.5\fontdimen6\textfont\tw@`.
        let fam2 = match self.e.eqtb.meaning(super::super::FAM_BASE_ID + 2) {
            Meaning::Font(f) => f,
            _ => 0,
        };
        let quad = self.e.fonts.params(fam2).get(5).copied().unwrap_or(0);
        let mintagsep = super::super::engine_graphics::times(".5", quad).unwrap_or(0);
        let rows: Vec<(&[i32], i32)> = a.rows.iter().map(|r| (&r.fields[..], r.tag)).collect();
        let s = align_shifts(dw, &cols, &rows, mintagsep);
        if s.drops {
            return self.not_native("an align tag that goes below its row");
        }
        if s.totwidth > dw {
            return self.not_native("an align wider than the display");
        }
        let skip = |k: usize| Glue { width: if k == 0 { s.eqnshift } else if k % 2 == 1 { 0 } else { s.alignsep }, ..Glue::default() };
        let tags: Vec<bool> = a.rows.iter().map(|r| r.tag > 0).collect();
        // `\omit\kern-\alignsep@`, then with a tag `\place@tag`: `\kern-\tagshift@
        // \llap{\boxz@}`.
        self.align_retab(&skip, a.maxfields, |m: &mut Self, r, old| {
            let kern = |w: i32| Node { sub: 1, ..Node::new(Kind::Kern, w) };
            let mut new = vec![kern(-s.alignsep)];
            if tags.get(r).copied().unwrap_or(false) {
                new.push(kern(-s.tagshift));
                new.push(m.llap(&old));
            }
            new
        });
        let mut t = Vec::new();
        self.tex("$$\\rt@endequation ", &mut t);
        self.back_list(&t);
        self.fin_align();
    }

    /// `\llap{...}` of nodes already built: `\hbox to\z@{\hss ...}`.
    fn llap(&mut self, list: &[Node]) -> Node {
        self.push_nest(Mode::Horizontal, true, 1000);
        let g = self.e.nodes.glue_node(fil_glue(Prim::Hss), 0);
        self.cur.push(g);
        self.cur.extend_from_slice(list);
        let start = self.start as usize;
        let b: BoxRec = self.hpack(start, 0, true);
        self.pop_nest();
        Node::new(Kind::Box, self.e.nodes.add_box(b) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_columns_centre_and_split_the_room() {
        // A 345pt display and columns of 5pt and 18pt: the room left is split in two.
        let pt = 65536;
        let s = align_shifts(345 * pt, &[5 * pt, 18 * pt], &[(&[5 * pt, 18 * pt], 0)], pt);
        assert_eq!(s.eqnshift, (345 - 23) * pt / 2);
        assert_eq!(s.alignsep, s.eqnshift);
        assert_eq!(s.tagshift, 23 * pt + s.eqnshift - 345 * pt);
        assert!(!s.drops);
    }
}
