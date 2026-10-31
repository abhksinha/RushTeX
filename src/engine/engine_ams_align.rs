//! amsmath's `align` and `align*` (amsmath.sty `\start@align`, `\align@`, `\math@cr`,
//! `\measure@`, `\calc@shift@align`, `\place@tag`), and `gather` and `gather*` (`\gather@`,
//! `\gmeasure@`, `\calc@shift@gather`, `\place@tag@gather`), and `multline` and
//! `multline*` (`\multline@`, `\mmeasure@`, `\rendmultline@`), in one pass; `aligned`,
//! `gathered`, and `split` in an `equation` (`\gather@split`: a one-row `gather` in a
//! `\vcenter`, the row the split's `\vcenter`).
//!
//! amsmath sets the body twice: a measuring `\halign` finds the column widths, each row's
//! field widths and tag width, from which `\eqnshift@` (the glue before the first column),
//! `\alignsep@` (between column pairs) and `\tagshift@` follow; then the real `\halign`
//! uses them. Here the body is set once. `\maxfields@` is counted from its tokens before
//! the alignment opens; the rows record their field widths, depths and tags as they are
//! built; at the alignment's end [`align_shifts`] runs amsmath's arithmetic, and
//! `align_retab` writes the glue and the tag entries into the rows already built.
//!
//! A `gather` row's shift (`\eqnshift@`, the kern before its formula) depends only on its
//! own formula and tag, but the tag is set after the formula: the rows record both, and at
//! the end [`gather_shift`] gives each row's kern, written into its first entry.
//!
//! `multline`'s measuring pass learns only the last row's width, for whether its tag fits
//! beside it; the last row is the one being set when the tag is placed.

use super::super::engine_align::NO_ALIGN_GROUP;
use super::super::engine_box::{fil_glue, Ctx, HBOX_GROUP, VBOX_GROUP, VTOP_GROUP};
use super::super::engine_noad::VCENTER_GROUP;
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_pack::{BASELINE_SKIP, LINE_SKIP, LINE_SKIP_LIMIT};
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::END_TEMPLATE_ID;
use super::{AL_ALIGN_CR, AL_END, AL_START, AL_TAG};

/// `\strutbox@`, a hidden box register.
const STRUT_BOX: u32 = 32703;
/// latex.ltx's `\jot` and `\normallineskiplimit`; the native path takes no assignment to them.
const JOT: i32 = 3 * 65536;
const NORMAL_LINE_SKIP_LIMIT: i32 = 0;
/// amsmath's `\minalignsep`.
const MIN_ALIGN_SEP: i32 = 10 * 65536;

/// latex.ltx's `\@centering`, `0pt plus 1000pt`.
const CENTERING: Glue = Glue { width: 0, stretch: 1000 * 65536, shrink: 0, stretch_order: 0, shrink_order: 0 };

/// amsmath's `\multlinegap` and `\multlinetaggap`; the native path takes no assignment
/// to them.
const MULTLINE_GAP: i32 = 10 * 65536;
const MULTLINE_TAG_GAP: i32 = 10 * 65536;

/// Which display an [`AmsAlign`] is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AmsKind {
    Align,
    /// One formula column and the tag column.
    Gather,
    /// One column of rows, each an `\hbox to\displaywidth`.
    Multline,
}

/// One `align`, `gather` or `multline` being read.
#[derive(PartialEq, Clone)]
pub(super) struct AmsAlign {
    starred: bool,
    kind: AmsKind,
    /// A `gather` made by `\gather@split` in an `equation`: in a `\vcenter`, its number
    /// already stepped by the `equation` (so a `\nonumber` steps it back).
    pub(super) split: bool,
    /// In a `multline` row: where its formula starts in the row's box, and whether the
    /// formula is still open (the last row's tag closes it, `\let\endmultline@math\relax`).
    math_start: usize,
    math_open: bool,
    /// `\maxfields@`, counted from the body and made even.
    maxfields: usize,
    /// `doc.tabs.len()` when it opened: `\\` is `\math@cr` while no `tabular` opened since.
    tabs: usize,
    /// `\if@eqnsw`, `\ifdt@p`, `\@eqpen`.
    pub(super) eqnsw: bool,
    dtp: bool,
    eqpen: i32,
    /// The space of the last `\\[...]`, for the row's `\noalign`.
    skip: i32,
    rows: Vec<Row>,
    row: Row,
}

/// One `aligned` or `gathered` being read (`\start@aligned`, `gathered`).
#[derive(PartialEq, Clone)]
pub(super) struct AmsAligned {
    kind: AlignedKind,
    /// amsmath's `\column@`: the fields begun since the alignment opened.
    column: u32,
    /// `doc.tabs.len()` when it opened: `\\` is `\math@cr` while no `tabular` opened since.
    tabs: usize,
    /// The space of the last `\\[...]`.
    skip: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AlignedKind {
    Aligned,
    Gathered,
    /// `\insplit@`: two columns, the second ending `\hfill`.
    Split,
}

/// What a row gives the measuring pass: its field widths (`\field@lengths`), the depth of
/// its deepest field (`\lineht@`) and its tag's width (`\tag@lengths`, 0 for none).
#[derive(PartialEq, Clone, Default)]
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

/// `\calc@shift@gather` (tags right, no `fleqn`): the kern before a row's formula of width
/// `w`, its tag `tag` wide (0 for none); `None` when the tag must go below the row
/// (`\shifttag@true`).
fn gather_shift(dw: i32, w: i32, tag: i32, mintagsep: i32) -> Option<i32> {
    let mut shift = dw - w;
    if tag > 0 {
        if 2 * mintagsep + w + tag > dw {
            return None;
        }
        if shift < 4 * tag {
            shift -= tag;
        }
    }
    Some((shift / 2).max(0))
}

impl Machine<'_, '_> {
    /// `\begin{align}` (`align*`): `\start@align`'s `\collect@body`, here a count of the
    /// body's fields per row for `\maxfields@`, the body put back; `\dollardollar@begin`,
    /// then [`Self::align_start`] in the display.
    pub(super) fn align_begin(&mut self, starred: bool) {
        self.ams_align_begin(starred, AmsKind::Align);
    }

    /// `\begin{gather}` (`gather*`): `\start@gather`'s `\collect@body`, the body put back
    /// (a `&` in it is not native); then as `align`.
    pub(super) fn gather_begin(&mut self, starred: bool) {
        self.ams_align_begin(starred, AmsKind::Gather);
    }

    /// `\begin{multline}` (`multline*`): `\start@multline`'s `\collect@body`, the body put
    /// back after the first row's `\multline@indent`; `\shoveleft`, `\shoveright` and `&`
    /// are not native.
    pub(super) fn multline_begin(&mut self, starred: bool) {
        self.ams_align_begin(starred, AmsKind::Multline);
    }

    fn ams_align_begin(&mut self, starred: bool, kind: AmsKind) {
        if self.mode == Mode::Math {
            return self.not_native("an align inside math");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        self.ams_display(3);
        let name = match (kind, starred) {
            (AmsKind::Align, false) => "align",
            (AmsKind::Align, true) => "align*",
            (AmsKind::Gather, false) => "gather",
            (AmsKind::Gather, true) => "gather*",
            (AmsKind::Multline, false) => "multline",
            (AmsKind::Multline, true) => "multline*",
        };
        let (begin, end, nl) = (self.named_id(named!("begin")), self.named_id(named!("end")), self.named_id(named!("\\")));
        let mut body = Vec::new();
        let (mut depth, mut fields, mut maxfields, mut crs) = (0i32, 1usize, 0usize, 0usize);
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
                None if t.cat() == Some(Cat::AlignTab) && depth == 0 && inner == 0 => {
                    if kind != AmsKind::Align {
                        return self.not_native("& in gather or multline");
                    }
                    fields += 1
                }
                Some(c) if c == nl && depth == 0 && inner == 0 => {
                    maxfields = maxfields.max(fields);
                    fields = 1;
                    crs += 1;
                }
                Some(c) if self.doc.as_ref().is_some_and(|d| d.par_id == c) => return self.not_native("a paragraph in align"),
                Some(c) if self.hides_tab(c, 8) => return self.not_native("a macro with & in align"),
                Some(c) if kind == AmsKind::Multline && matches!(self.e.cs.name(c), "shoveleft" | "shoveright") => return self.not_native("\\shoveleft or \\shoveright"),
                _ => {}
            }
            body.push(t);
        }
        maxfields = maxfields.max(fields);
        maxfields += maxfields % 2;
        let tabs = self.doc.as_ref().map_or(0, |d| d.tabs.len());
        // `\mmeasure@` sets the body in an `\halign` under the display's `\everycr`: its
        // `\crcr` takes the first `\ifdt@p`, so the real rows start with `\penalty\@eqpen`,
        // as the body's last `\\` left it (`\\*` or not, 10000), or as it was.
        let pen = self.display_pen();
        let (dtp, eqpen) = match self.doc.as_ref() {
            Some(d) if kind == AmsKind::Multline => (false, if crs > 0 { pen } else { d.eqpen }),
            _ => (true, pen),
        };
        if kind == AmsKind::Multline && crs > 0 {
            if let Some(d) = self.doc.as_mut() {
                d.eqpen = pen;
            }
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.align = Some(Box::new(AmsAlign { starred, kind, split: false, math_start: 0, math_open: false, maxfields, tabs, eqnsw: !starred, dtp, eqpen, skip: 0, rows: Vec::new(), row: Row::default() }));
        }
        let dollar = Token::char(Cat::MathShift, '$');
        let mut t = vec![dollar, dollar, self.doc_id(AL_START)];
        if kind == AmsKind::Multline {
            t.push(self.doc_id(doc!("rt@mlindent")));
        }
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
        if !self.in_split_display() {
            self.reset_strut_box();
        }
        match self.ams_kind() {
            Some(AmsKind::Gather) => self.gather_halign(),
            Some(AmsKind::Multline) => self.multline_halign(),
            _ => self.align_halign(),
        }
    }

    /// `\openup\jot` and `\everycr` the rows' `\noalign` (`\@display@init`), then the
    /// `\halign` with the two templates of `\align@preamble`.
    pub(super) fn align_halign(&mut self) {
        let (u, v) = self.ams_display_init();
        // The `\everycr` of the preamble's `\cr`.
        let cr = self.doc_id(AL_ALIGN_CR);
        self.input.back(cr);
        self.init_align_periodic(u, v, [Glue::default(); 3]);
    }

    /// `\gather@`'s `\halign to\displaywidth` with `\tabskip\z@skip`: the formula column
    /// (`\strut@\setboxz@h{$\m@th\displaystyle{##}$}\calc@shift@gather\set@gather@field
    /// \tabskip\@centering`) and the tag column, which every row enters with `\omit`.
    fn gather_halign(&mut self) {
        let (u, v) = self.ams_display_init();
        let cr = self.doc_id(AL_ALIGN_CR);
        self.input.back(cr);
        let dw = self.dimen_param(super::super::engine_noad::DISPLAY_WIDTH);
        self.init_align_cols(&[(u[0], v[0]), (u[1], v[1])], vec![Glue::default(), CENTERING, Glue::default()], true, dw);
    }

    /// `\multline@`'s `\halign` with `\tabskip\z@skip` and one template, `\hbox to
    /// \totwidth@{\hfil\strut@$\m@th\displaystyle{}##\endmultline@math\hfil}`, `\totwidth@`
    /// the display's width.
    fn multline_halign(&mut self) {
        self.ams_spread();
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let (au, av) = (self.doc_id(doc!("rt@alu")), self.doc_id(doc!("rt@alv")));
        let u = self.e.toks.store(&[au, open, close]);
        let v = self.e.toks.store(&[av, Token::cs(END_TEMPLATE_ID)]);
        let cr = self.doc_id(AL_ALIGN_CR);
        self.input.back(cr);
        self.init_align_cols(&[(u, v)], vec![Glue::default(); 2], false, 0);
    }

    /// The first row's `\multline@indent` (tags right): `\hfilneg\hskip\multlinegap`, in its
    /// formula.
    pub(super) fn multline_indent(&mut self) {
        self.build_glue(fil_glue(Prim::HfilNeg), false);
        self.build_glue(Glue { width: MULTLINE_GAP, ..Glue::default() }, false);
    }

    /// `\end{multline}` in its last row, `\rendmultline@`. With a number (`\iftag@`, here
    /// `\@eqnswtrue`): the formula closed, `\hskip\multlinetaggap\make@display@tag` when the
    /// tag fits beside the row's formula (`\mmeasure@`'s width of the last row), otherwise
    /// not native. Without: `\hskip\multlinegap` in the formula. Then `\hfilneg\math@cr`.
    pub(super) fn multline_end(&mut self) {
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        let (tagged, first, start) = (a.eqnsw && !a.starred, a.rows.is_empty(), a.math_start);
        if tagged {
            a.math_open = false;
            self.after_math();
            // `\mmeasure@` sets the formula without the first row's indent.
            let w = self.measure_h(&self.cur[start..]).0 - if first { MULTLINE_GAP } else { 0 };
            self.build_glue(Glue { width: MULTLINE_TAG_GAP, ..Glue::default() }, false);
            let at = self.cur.len();
            self.begin_group(SIMPLE_GROUP);
            self.tag_equation_step();
            self.align_tag_num();
            self.end_group(SIMPLE_GROUP);
            let tag = self.cur[at..].iter().find(|n| n.kind == Kind::Box).map_or(0, |n| self.e.nodes.boxes[n.a as usize].width);
            let dw = self.dimen_param(super::super::engine_noad::DISPLAY_WIDTH);
            if w + tag + MULTLINE_TAG_GAP > dw {
                return self.not_native("a multline tag that goes below its row");
            }
        } else {
            if self.doc.as_ref().is_some_and(|d| d.df_label.is_some()) {
                return self.not_native("a \\label in a multline without a number");
            }
            self.build_glue(Glue { width: MULTLINE_GAP, ..Glue::default() }, false);
        }
        self.build_glue(fil_glue(Prim::HfilNeg), false);
        self.align_end();
    }

    fn ams_kind(&self) -> Option<AmsKind> {
        self.doc.as_ref().and_then(|d| d.align.as_ref()).map(|a| a.kind)
    }

    /// `\reset@strutbox@`.
    fn reset_strut_box(&mut self) {
        let (ht, dp) = (self.strut_height(), self.strut_depth());
        self.open_box(HBOX_GROUP, Ctx::Set { n: STRUT_BOX, global: true }, false, 0, false);
        self.open_box(VBOX_GROUP, Ctx::Shift(NORMAL_LINE_SKIP_LIMIT / 2), false, 0, false);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -NORMAL_LINE_SKIP_LIMIT) });
        self.build_hbox(|m| m.build_rule([0, ht, dp]));
        self.package();
        self.package();
    }

    /// `\openup\jot` and `\everycr` the rows' `\noalign` (`\@display@init`).
    fn ams_spread(&mut self) {
        if !self.in_split_display() {
            self.open_up_jot();
        }
        {
            let reg = param!("everycr");
            let span = self.e.toks.store(&[self.doc_id(AL_ALIGN_CR)]);
            self.e.eqtb.set_toks(reg, span, false);
        }
    }

    /// `\openup\jot`.
    fn open_up_jot(&mut self) {
        // The `\normalbaselineskip` hyperref raises its targets by, before the change.
        let bs = self.e.eqtb.glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }).width;
        self.e.eqtb.set_word(super::NORMAL_BASELINE, bs, false);
        for slot in [LINE_SKIP, BASELINE_SKIP] {
            let reg = Reg { region: Region::Glue, slot };
            let g = self.e.eqtb.glue(reg);
            self.e.eqtb.set_glue(reg, Glue { width: g.width + JOT, ..g }, false);
        }
        let lsl = Reg { region: Region::Dimen, slot: LINE_SKIP_LIMIT };
        let v = self.e.eqtb.word(lsl);
        self.e.eqtb.set_word(lsl, v + JOT, false);
    }

    /// [`Self::ams_spread`], and the field templates, as `align`'s.
    fn ams_display_init(&mut self) -> ([super::super::engine_token::Span; 2], [super::super::engine_token::Span; 2]) {
        self.ams_spread();
        // `&\hfil\strut@\setboxz@h{\@lign$\m@th\displaystyle{##}$}\ifmeasuring@
        // \savefieldlength@\fi\set@field\tabskip\z@skip &\setboxz@h{\@lign$\m@th
        // \displaystyle{{}##}$}\ifmeasuring@\savefieldlength@\fi\set@field\hfil\tabskip\alignsep@`:
        // each template one native around the formula's braces.
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let (au, av) = (self.doc_id(doc!("rt@alu")), self.doc_id(doc!("rt@alv")));
        let u = [self.e.toks.store(&[au, open]), self.e.toks.store(&[au, open, open, close])];
        let v = self.e.toks.store(&[close, av, Token::cs(END_TEMPLATE_ID)]);
        (u, [v, v])
    }

    fn in_gather(&self) -> bool {
        self.ams_kind() == Some(AmsKind::Gather)
    }

    /// A field's template before its formula: in the first of a pair `\hfil\strut@`, then
    /// `\setboxz@h{\@lign$\m@th\displaystyle`.
    pub(super) fn align_u(&mut self) {
        if self.ams_kind() == Some(AmsKind::Multline) {
            let dw = self.dimen_param(super::super::engine_noad::DISPLAY_WIDTH);
            self.open_box(HBOX_GROUP, Ctx::Shift(0), true, dw, false);
            self.build_glue(fil_glue(Prim::Hfil), false);
            self.build_copy_register(STRUT_BOX);
            let at = self.cur.len();
            if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
                (a.math_start, a.math_open) = (at, true);
            }
            self.enter_text_math();
            self.math_surround_zero();
            return self.build_style(0);
        }
        if self.in_gather() {
            self.build_copy_register(STRUT_BOX);
        } else if self.align_column().is_multiple_of(2) {
            self.build_glue(fil_glue(Prim::Hfil), false);
            self.build_copy_register(STRUT_BOX);
        }
        self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
        self.enter_text_math();
        self.math_surround_zero();
        self.build_style(0);
    }

    /// After the formula: `$}`, `\set@field`, and in the second of a pair `\hfil`.
    pub(super) fn align_v(&mut self) {
        if self.ams_kind() == Some(AmsKind::Multline) {
            // `\endmultline@math\hfil}`.
            if self.doc.as_mut().and_then(|d| d.align.as_mut()).is_some_and(|a| std::mem::replace(&mut a.math_open, false)) {
                self.after_math();
            }
            self.build_glue(fil_glue(Prim::Hfil), false);
            return self.package();
        }
        self.after_math();
        self.package();
        if self.in_gather() {
            // `\set@gather@field`: `\kern\eqnshift@`, its width written at the end,
            // `\boxz@\hfil`.
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
            self.align_field();
            return self.build_glue(fil_glue(Prim::Hfil), false);
        }
        self.align_field();
        if self.align_column() % 2 == 1 {
            self.build_glue(fil_glue(Prim::Hfil), false);
        }
    }

    /// `\set@field` after a field's box: `\column@plus`, `\lineht@` its depth when deeper,
    /// `\boxz@`.
    fn align_field(&mut self) {
        let (w, d) = self.e.eqtb.box_reg(0).map_or((0, 0), |b| {
            let b = &self.e.nodes.boxes[b as usize];
            (b.width, b.depth)
        });
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.row.fields.push(w);
            a.row.lineht = a.row.lineht.max(d);
        }
        self.build_box_register(0);
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
        let pen = if star { 10000 } else { self.display_pen() };
        if let Some(d) = self.doc.as_mut() {
            d.eqpen = pen;
        }
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.eqpen = pen;
        }
        self.align_row_end(skip, None);
    }

    /// `\math@cr@@@align` and `\noalign{\vskip skip}`: `\add@amps\maxfields@`, `\omit`, the
    /// tag (`\rt@altag`), `\cr`; at the end the alignment's end native follows.
    fn align_row_end(&mut self, skip: i32, last: Option<Token>) {
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        if a.kind == AmsKind::Multline {
            // `\restore@math@cr`: `\math@cr@@@` is `\cr`.
            a.rows.push(Row::default());
            a.skip = skip;
            let Some(cr) = self.doc.as_ref().map(|d| d.cr_id) else { return };
            let mut t = vec![Token::cs(cr), self.doc_id(doc!("rt@algap"))];
            t.extend(last);
            return self.back_list(&t);
        }
        // `\math@cr@@@gather`'s `&`, the tag column's template done by the tag native.
        let amps = if a.kind == AmsKind::Gather { 1 } else { a.maxfields.saturating_sub(a.row.fields.len()) };
        let mut t: Vec<Token> = (0..amps).map(|_| Token::char(Cat::AlignTab, '&')).collect();
        let Some((omit, cr)) = self.doc.as_ref().map(|d| (d.omit_id, d.cr_id)) else { return };
        t.extend([Token::cs(omit), self.doc_id(AL_TAG), Token::cs(cr), self.doc_id(doc!("rt@algap"))]);
        t.extend(last);
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.skip = skip;
        }
        self.back_list(&t);
    }

    /// The row's `\noalign{\vskip skip}`.
    pub(super) fn align_gap(&mut self) {
        let skip = self.doc.as_ref().and_then(|d| d.align.as_ref()).map_or(0, |a| a.skip);
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        self.build_glue(Glue { width: skip, ..Glue::default() }, true);
        self.end_no_align();
    }

    /// The tag entry: `\ifst@rred\nonumber\fi \if@eqnsw\global\tag@true\fi`, then with a tag
    /// `\setboxz@h{\@lign\strut@{\make@display@tag}}`, its placing left to the end. In a
    /// `gather` the box is made with no tag too, and placed at once (`\place@tag@gather`).
    /// A `\tag` (`\tag@in@align`) left the row unnumbered with `\tag@true`: its `\df@tag`.
    pub(super) fn align_tag(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        let given = doc.df_tag.is_some();
        let Some(a) = doc.align.as_mut() else { return };
        if a.starred {
            a.eqnsw = false;
        }
        if a.kind == AmsKind::Gather {
            let (tagged, stepped) = (a.eqnsw, a.split);
            if !tagged && !given && self.doc.as_ref().is_some_and(|d| d.df_label.is_some()) {
                return self.not_native("a \\label on a gather row without a number");
            }
            self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
            self.build_copy_register(STRUT_BOX);
            if tagged {
                if stepped && self.hyper() {
                    return self.not_native("hyperref: a gather row whose number a split stepped");
                }
                self.begin_group(SIMPLE_GROUP);
                if !stepped {
                    self.tag_equation_step();
                }
                self.align_tag_num();
                self.end_group(SIMPLE_GROUP);
            } else if given {
                self.align_df_tag();
            }
            self.package();
            return self.gather_tag_keep();
        }
        let tagged = a.eqnsw || given;
        if tagged {
            self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
            self.build_copy_register(STRUT_BOX);
            if given {
                self.align_df_tag();
            } else {
                self.begin_group(SIMPLE_GROUP);
                self.tag_equation_step();
                self.align_tag_num();
                self.end_group(SIMPLE_GROUP);
            }
            self.package();
        }
        self.align_tag_keep(tagged);
    }

    /// `\make@display@tag` after a `\tag`: `\df@tag` (the tag's box, `\@currentlabel` its
    /// text, in the box's group) and the stored `\label`.
    fn align_df_tag(&mut self) {
        self.begin_group(SIMPLE_GROUP);
        if let Some(text) = self.doc.as_ref().and_then(|d| d.df_tag.as_ref()).map(|t| t.0.clone()) {
            self.set_current_label(&text);
        }
        self.df_tag();
        if let Some(key) = self.doc.as_mut().and_then(|d| d.df_label.take()) {
            self.label_key(&key);
        }
        self.end_group(SIMPLE_GROUP);
    }

    /// `\print@eqnum` (`\tagform@\theequation`: `\hbox{\m@th\normalfont(n\/)}`) and the
    /// stored `\label`.
    fn align_tag_num(&mut self) {
        let n = self.equation_text();
        let Some(doc) = self.doc.as_mut() else { return };
        let label = doc.df_label.take();
        self.build_hbox(|m| {
            m.begin_group(SIMPLE_GROUP);
            m.normal_font();
            m.equation_tag(&n);
            m.end_group(SIMPLE_GROUP);
        });
        if let Some(key) = label {
            self.label_key(&key);
        }
    }

    /// The row's end: its tag box kept in the entry, `\ifst@rred\else\global\@eqnswtrue\fi`,
    /// `\global\lineht@\z@`.
    fn align_tag_keep(&mut self, tagged: bool) {
        let w = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        let mut row = std::mem::take(&mut a.row);
        row.tag = if tagged { w.max(1) } else { 0 };
        a.rows.push(row);
        a.eqnsw = !a.starred;
        if tagged {
            self.build_box_register(0);
        }
    }

    /// A `gather` row's end: its tag width (0 for none) kept, the tag entry `\llap{\boxz@}`
    /// (the tags right and no formula past the display, or the document has gone to the
    /// engine at the end), `\ifst@rred\else\global\@eqnswtrue\fi`.
    fn gather_tag_keep(&mut self) {
        let w = self.e.eqtb.box_reg(0).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) else { return };
        let mut row = std::mem::take(&mut a.row);
        row.tag = w;
        a.rows.push(row);
        a.eqnsw = !a.starred;
        self.build_box_register(0);
        if let Some(b) = self.cur.pop() {
            let l = self.llap(&[b]);
            self.cur.push(l);
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
    /// In an `align` or `gather` whose rows take a `\tag` (not a `split`'s, not `multline`).
    pub(super) fn in_tag_rows(&self) -> bool {
        self.in_align() && self.doc.as_ref().and_then(|d| d.align.as_ref()).is_some_and(|a| !a.split && a.kind != AmsKind::Multline)
    }

    pub(super) fn in_align(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.align.as_ref().is_some_and(|a| a.tabs == d.tabs.len()))
    }

    /// `\end{align}` in its last row: `\math@cr` (no `*`, no `[`), then the alignment's end.
    pub(super) fn align_end(&mut self) {
        let pen = self.display_pen();
        if let Some(d) = self.doc.as_mut() {
            d.eqpen = pen;
        }
        if let Some(a) = self.doc.as_mut().and_then(|d| d.align.as_mut()) {
            a.eqpen = pen;
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
        // `\mintagsep`: `.5\fontdimen6\textfont\tw@`.
        let fam2 = match self.e.eqtb.meaning(super::super::FAM_BASE_ID + 2) {
            Meaning::Font(f) => f,
            _ => 0,
        };
        let quad = self.e.fonts.params(fam2).get(5).copied().unwrap_or(0);
        let mintagsep = super::super::engine_graphics::times(".5", quad).unwrap_or(0);
        match a.kind {
            AmsKind::Gather => return self.gather_finish(&a, dw, mintagsep),
            AmsKind::Multline => {
                let dollar = Token::char(Cat::MathShift, '$');
                let t = [dollar, dollar, self.doc_id(doc!("rt@endequation"))];
                self.back_list(&t);
                return self.fin_align();
            }
            AmsKind::Align => {}
        }
        let cols = self.align_widths(a.maxfields);
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
        let dollar = Token::char(Cat::MathShift, '$');
        let t = [dollar, dollar, self.doc_id(doc!("rt@endequation"))];
        self.back_list(&t);
        self.fin_align();
    }

    /// `gather`'s end: `\gmeasure@`'s `\totwidth@` (the widest formula), past the display
    /// not native (`\black@` and the tag's kern); each row's `\calc@shift@gather` kern
    /// written into its formula entry; `fin_align`, `\dollardollar@end`.
    fn gather_finish(&mut self, a: &AmsAlign, dw: i32, mintagsep: i32) {
        let width = |r: &Row| r.fields.first().copied().unwrap_or(0);
        if a.rows.iter().any(|r| width(r) > dw) {
            return self.not_native("a gather wider than the display");
        }
        let Some(shifts) = a.rows.iter().map(|r| gather_shift(dw, width(r), r.tag, mintagsep)).collect::<Option<Vec<i32>>>() else {
            return self.not_native("a gather tag that goes below its row");
        };
        let skip = |k: usize| if k == 1 { CENTERING } else { Glue::default() };
        self.align_retab(&skip, 0, |_: &mut Self, r, mut old| {
            if let Some(k) = old.iter_mut().find(|n| n.kind == Kind::Kern) {
                k.a = shifts.get(r).copied().unwrap_or(0);
            }
            old
        });
        let dollar = Token::char(Cat::MathShift, '$');
        let t = [dollar, dollar, self.doc_id(doc!("rt@endequation"))];
        self.back_list(&t);
        self.fin_align();
        // `\gather@split`'s `\vcenter`.
        if a.split {
            self.package();
        }
    }

    /// `\begin{aligned}` and `\begin{gathered}` (`[t]`, `[b]` or `[c]`) in a formula, inside
    /// `\begin`'s group: `\alignedspace@left` (amsmath's default `alignedleftspaceyesifneg`:
    /// `\null`, and `\,` after negative glue or a negative kern), `\ams@start@box`'s
    /// `\vtop`, `\vbox` or `\vcenter`, `\spread@equation`, and the `\ialign`: for
    /// `aligned` `\start@aligned`'s two periodic templates, `\tabskip\alignsep@` (10pt,
    /// `\minalignsep`) after each pair; for `gathered` one, `\hfil\strut@$\m@th
    /// \displaystyle##$\hfil`. Inside an `align`, `gather`, `multline`, `eqnarray` or another
    /// `aligned` it is not native.
    pub(super) fn aligned_begin(&mut self, gather: bool) {
        if self.mode != Mode::Math {
            return self.not_native("aligned outside a formula");
        }
        if self.doc.as_ref().is_some_and(|d| d.align.is_some() || d.eqnarray.is_some() || d.aligned.is_some()) {
            return self.not_native("aligned inside another alignment");
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        let pos = if self.next_is('[') { self.bracket_arg().iter().map(|t| t.chr()).collect::<String>() } else { String::new() };
        let group = match pos.trim() {
            "t" => VTOP_GROUP,
            "b" => VBOX_GROUP,
            "c" | "" => VCENTER_GROUP,
            _ => return self.not_native("aligned with a position not t, b or c"),
        };
        let negative = match self.tail().filter(|_| self.cur.len() > self.start as usize) {
            Some(n) if n.kind == Kind::Glue => self.e.nodes.glue(&n).width < 0,
            Some(n) if n.kind == Kind::Kern => n.a < 0,
            _ => false,
        };
        self.build_hbox(|_| {});
        if negative {
            let g = self.thin_mu_skip();
            self.build_mskip(g);
        }
        let tabs = self.doc.as_ref().map_or(0, |d| d.tabs.len());
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
            let kind = if gather { AlignedKind::Gathered } else { AlignedKind::Aligned };
            doc.aligned = Some(AmsAligned { kind, column: 0, tabs, skip: 0 });
        }
        self.open_box(group, Ctx::Shift(0), false, 0, false);
        self.reset_strut_box();
        self.open_up_jot();
        // `\ialign`: `\everycr{}\tabskip\z@skip`.
        {
            let reg = param!("everycr");
            let empty = self.e.toks.store(&[]);
            self.e.eqtb.set_toks(reg, empty, false);
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        if gather {
            let u = self.e.toks.store(&[self.doc_id(doc!("rt@gdu"))]);
            let v = self.e.toks.store(&[self.doc_id(doc!("rt@gdv")), Token::cs(END_TEMPLATE_ID)]);
            return self.init_align_cols(&[(u, v)], vec![Glue::default(); 2], false, 0);
        }
        let (au, av) = (self.doc_id(doc!("rt@adu")), self.doc_id(doc!("rt@adv")));
        let u = [self.e.toks.store(&[au, open]), self.e.toks.store(&[au, open, open, close])];
        let v = self.e.toks.store(&[close, av, Token::cs(END_TEMPLATE_ID)]);
        let sep = Glue { width: MIN_ALIGN_SEP, ..Glue::default() };
        self.init_align_periodic(u, [v, v], [Glue::default(), Glue::default(), sep]);
    }

    /// Whether `\\` is `\math@cr` of an `aligned` or `gathered`.
    pub(super) fn in_aligned(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.aligned.as_ref().is_some_and(|a| a.tabs == d.tabs.len()))
    }

    /// An `aligned` field before its formula: `\column@plus`, in the first of a pair
    /// `\hfil\strut@`, then `$\m@th\displaystyle`.
    pub(super) fn aligned_u(&mut self) {
        if let Some(a) = self.doc.as_mut().and_then(|d| d.aligned.as_mut()) {
            a.column += 1;
        }
        if self.align_column().is_multiple_of(2) {
            self.build_glue(fil_glue(Prim::Hfil), false);
            self.build_copy_register(STRUT_BOX);
        }
        self.enter_text_math();
        self.math_surround_zero();
        self.build_style(0);
    }

    /// After the formula: `$`, and in the second of a pair `\hfil` (`split`'s `\hfill`).
    pub(super) fn aligned_v(&mut self) {
        self.after_math();
        if self.align_column() % 2 == 1 {
            let split = self.doc.as_ref().and_then(|d| d.aligned.as_ref()).is_some_and(|a| a.kind == AlignedKind::Split);
            self.build_glue(fil_glue(if split { Prim::Hfill } else { Prim::Hfil }), false);
        }
    }

    /// `gathered`'s template before its formula: `\hfil\strut@$\m@th\displaystyle`.
    pub(super) fn gathered_u(&mut self) {
        self.build_glue(fil_glue(Prim::Hfil), false);
        self.build_copy_register(STRUT_BOX);
        self.enter_text_math();
        self.math_surround_zero();
        self.build_style(0);
    }

    /// After the formula: `$\hfil`.
    pub(super) fn gathered_v(&mut self) {
        self.after_math();
        self.build_glue(fil_glue(Prim::Hfil), false);
    }

    /// `\\` in an `aligned` or `gathered`, `\math@cr` as in `align` (the global `\@eqpen`
    /// set), then `\math@cr@@@`: in `aligned` `\math@cr@@@aligned`, which after an even
    /// count of fields (`\column@`, never reset between rows) adds `&\kern-\alignsep@`;
    /// `\cr`, and `\noalign{\vskip skip}`.
    pub(super) fn aligned_cr(&mut self) {
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
                _ => return self.not_native("\\\\[...] in aligned that is not a length"),
            },
            None => 0,
        };
        let pen = if star { 10000 } else { self.display_pen() };
        let Some(doc) = self.doc.as_mut() else { return };
        doc.eqpen = pen;
        let (cr, Some(a)) = (doc.cr_id, doc.aligned.as_mut()) else { return };
        a.skip = skip;
        let extra = a.kind == AlignedKind::Aligned && a.column.is_multiple_of(2);
        let mut t = Vec::new();
        if extra {
            t.extend([Token::char(Cat::AlignTab, '&'), self.doc_id(doc!("rt@adkern"))]);
        }
        t.extend([Token::cs(cr), self.doc_id(doc!("rt@adgap"))]);
        self.back_list(&t);
    }

    /// `\kern-\alignsep@` in the field `\math@cr@@@aligned` adds.
    pub(super) fn aligned_kern(&mut self) {
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -MIN_ALIGN_SEP) });
    }

    /// The row's `\noalign{\vskip skip}`.
    pub(super) fn aligned_gap(&mut self) {
        let skip = self.doc.as_ref().and_then(|d| d.aligned.as_ref()).map_or(0, |a| a.skip);
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        self.build_glue(Glue { width: skip, ..Glue::default() }, true);
        self.end_no_align();
    }

    /// `\end{aligned}`: `\crcr\egroup`, the box's `\egroup`, and `\end`'s `\endgroup`.
    pub(super) fn aligned_end(&mut self) {
        let Some(crcr) = self.doc.as_ref().map(|d| d.crcr_id) else { return };
        let t = [Token::cs(crcr), self.doc_id(doc!("rt@adend")), self.named_cs(named!("endgroup"))];
        self.back_list(&t);
    }

    /// The alignment's `}` and the box's; for `split` its `\vbox`'s and then `\rendsplit@`'s
    /// `\vcenter{\boxz@}` (`centertags`, amsmath's default).
    pub(super) fn aligned_finish(&mut self) {
        let split = self.doc.as_mut().and_then(|d| d.aligned.take()).is_some_and(|a| a.kind == AlignedKind::Split);
        self.fin_align();
        self.package();
        if split {
            self.package();
        }
    }

    /// `\begin{split}` directly in an amsmath `equation` or `equation*`, the display's own
    /// group: `\gather@split`. The body is read to `\end{split}` (a field past the second, a
    /// `\tag`, `\nonumber`, `\notag` or `\intertext` there is not native) and put back as
    /// the one row of `\gather@`, in a `\vcenter`; `\end{equation}` is then
    /// `\endmathdisplay@a`'s `\math@cr\black@\totwidth@\egroup\egroup`, the row's number the
    /// `equation`'s (its `\incr@eqnum` already done).
    pub(super) fn split_begin(&mut self) {
        let kind = self.doc.as_ref().map_or(0, |d| d.ams_display);
        let alone = self.doc.as_ref().is_some_and(|d| d.align.is_none() && d.aligned.is_none() && d.eqnarray.is_none());
        if self.mode != Mode::Math || !matches!(kind, 1 | 2) || !alone || self.groups.last() != Some(&super::super::engine_noad::MATH_SHIFT_GROUP) {
            return self.not_native("split outside an equation");
        }
        let (begin, end, nl) = (self.named_id(named!("begin")), self.named_id(named!("end")), self.named_id(named!("\\")));
        let mut body = Vec::new();
        let (mut depth, mut fields, mut inner) = (0i32, 1usize, 0usize);
        loop {
            let Some(t) = self.get_token() else { return self.not_native("a split without its end") };
            if t == Token::cs(end) || t == Token::cs(begin) {
                let mut arg = Vec::new();
                self.read_arg(&mut arg);
                let env: String = arg.iter().map(|t| t.chr()).collect();
                if t == Token::cs(end) && env == "split" && inner == 0 {
                    break;
                }
                if !matches!(env.as_str(), "array" | "cases" | "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix") {
                    return self.not_native("an environment inside split");
                }
                if t == Token::cs(end) {
                    let Some(n) = inner.checked_sub(1) else { return self.not_native("an \\end inside split without its \\begin") };
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
                None if t.cat() == Some(Cat::AlignTab) && depth == 0 && inner == 0 => {
                    fields += 1;
                    if fields > 2 {
                        return self.not_native("a third field in a split row");
                    }
                }
                Some(c) if c == nl && depth == 0 && inner == 0 => fields = 1,
                Some(c) if self.doc.as_ref().is_some_and(|d| d.par_id == c) => return self.not_native("a paragraph in split"),
                Some(c) if self.hides_tab(c, 8) => return self.not_native("a macro with & in split"),
                Some(c) if matches!(self.e.cs.name(c), "tag" | "nonumber" | "notag" | "intertext") => return self.not_native("\\tag, \\nonumber or \\intertext in split"),
                _ => {}
            }
            body.push(t);
        }
        let numbered = kind == 1;
        let pen = self.display_pen();
        let tabs = self.doc.as_ref().map_or(0, |d| d.tabs.len());
        // `\gather@`'s `\let\label\label@in@display`, in the row's formula too.
        if let Some(doc) = self.doc.as_mut() {
            doc.ams_display = 3;
        }
        // `\gather@split`'s `\spread@equation`, in the display's group (its `\openup` is in
        // force when the display's box joins the page); `\gather@`'s own is then empty.
        self.reset_strut_box();
        self.open_up_jot();
        if let Some(doc) = self.doc.as_mut() {
            doc.align = Some(Box::new(AmsAlign { starred: !numbered, kind: AmsKind::Gather, split: true, math_start: 0, math_open: false, maxfields: 2, tabs, eqnsw: numbered, dtp: true, eqpen: pen, skip: 0, rows: Vec::new(), row: Row::default() }));
        }
        self.open_box(VCENTER_GROUP, Ctx::Shift(0), false, 0, false);
        let mut t = vec![self.doc_id(AL_START), self.doc_id(doc!("rt@splitopen"))];
        t.extend(body);
        t.extend([Token::cs(end), Token::char(Cat::BeginGroup, '{')]);
        t.extend("split".chars().map(|c| Token::char(Cat::Letter, c)));
        t.push(Token::char(Cat::EndGroup, '}'));
        self.back_list(&t);
    }

    /// The row's `\begin{split}`, `\insplit@`: its group, the `\vcenter` and `\vbox`, and the
    /// `\ialign` of two columns, `\hfil\strut@$\m@th\displaystyle{##}$` and `$\m@th
    /// \displaystyle{{}##}$\hfill`.
    pub(super) fn split_open(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let tabs = self.doc.as_ref().map_or(0, |d| d.tabs.len());
        if let Some(doc) = self.doc.as_mut() {
            doc.aligned = Some(AmsAligned { kind: AlignedKind::Split, column: 0, tabs, skip: 0 });
        }
        self.open_box(VCENTER_GROUP, Ctx::Shift(0), false, 0, false);
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        {
            let reg = param!("everycr");
            let empty = self.e.toks.store(&[]);
            self.e.eqtb.set_toks(reg, empty, false);
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let (au, av) = (self.doc_id(doc!("rt@adu")), self.doc_id(doc!("rt@adv")));
        let u = [self.e.toks.store(&[au, open]), self.e.toks.store(&[au, open, open, close])];
        let v = self.e.toks.store(&[close, av, Token::cs(END_TEMPLATE_ID)]);
        self.init_align_cols(&[(u[0], v), (u[1], v)], vec![Glue::default(); 3], false, 0);
    }

    /// Whether the display is the `gather` of a `split`.
    pub(super) fn in_split_display(&self) -> bool {
        self.doc.as_ref().and_then(|d| d.align.as_ref()).is_some_and(|a| a.split)
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

    #[test]
    fn gather_rows_centre_and_make_room_for_a_wide_tag() {
        let pt = 65536;
        // No tag: centred.
        assert_eq!(gather_shift(345 * pt, 45 * pt, 0, pt), Some(150 * pt));
        // A tag narrow against the room: still centred.
        assert_eq!(gather_shift(345 * pt, 45 * pt, 20 * pt, pt), Some(150 * pt));
        // Room under four tag widths: the tag's width taken off before halving.
        assert_eq!(gather_shift(345 * pt, 245 * pt, 30 * pt, pt), Some(35 * pt));
        // No room for formula, tag and twice the separation: the tag drops.
        assert_eq!(gather_shift(345 * pt, 320 * pt, 30 * pt, pt), None);
    }
}
