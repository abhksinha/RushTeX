//! Floats on the GDL path: latex.ltx's `\@float`, `\@xfloat`, `\end@float`, `\caption`,
//! `\@caption`, article's `\@makecaption`, and the float half of the output routine
//! (`\@specialoutput`, `\@addtocurcol`, `\@combinefloats`, `\@startcolumn`,
//! `\@addtonextcol`, `\@tryfcolumn`, `\@doclearpage`), one column.
//!
//! LaTeX keeps its float lists as macros of `\@elt\bx@A` tokens, rebuilt by `\xdef` at
//! each change, and the placement state in registers. Here a float is a slot of an
//! 18-entry arena (LaTeX's 18 insertion boxes): its box and its `\count` word (placement
//! bits 1 `h`, 2 `t`, 4 `b`, 8 `p`, 16 without `!`, plus 32 × the type), and each list is a
//! fixed queue of slot numbers. The whole state is a few cache lines and moves no tokens.

use super::super::engine_box::{Ctx, HBOX_GROUP, VBOX_GROUP};
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{BoxRec, Kind, Node};
use super::super::engine_pack::{BASELINE_SKIP, BOX_MAX_DEPTH, PAR_SKIP};
use super::super::engine_prim::Meaning;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::engine_value::MAX_DIMEN;
use super::super::engine_node::RUNNING;
use super::{Op, COLUMN_WIDTH, COLUMNSEP, COLUMNSEPRULE, R_DBLFLOATSEP, R_DBLTEXTFLOATSEP, R_DBLTOPNUMBER, CAPTION_END, CAPTION_MID, FOOTINS, MINIPAGE, NEWLINE, NORMALSIZE, R_ABOVECAPTIONSKIP, R_BELOWCAPTIONSKIP, R_BOTTOMNUMBER, R_FIGURE, R_FLOATSEP, R_FPBOT, R_FPSEP, R_FPTOP, R_INTEXTSEP, R_TEXTFLOATSEP, R_TOPNUMBER, R_TOTALNUMBER, TEMP_BOX};

/// LaTeX's insertion boxes for floats (`\bx@A` to `\bx@R`).
pub(super) const SLOTS: usize = 18;
/// The hidden box register a float is built in (`\@currbox` while it is open).
const FLOAT_BOX: u32 = 32702;

/// A float list (`\@currlist`, `\@toplist` …): slot numbers in order, `\@cons` at the end,
/// `\@next` from the front. The slots are distinct, so no list overflows.
#[derive(PartialEq, Clone, Copy, Default)]
pub(super) struct Queue {
    n: u8,
    s: [u8; SLOTS],
}

impl Queue {
    fn push(&mut self, k: u8) {
        self.s[self.n as usize] = k;
        self.n += 1;
    }

    fn pop_front(&mut self) -> Option<u8> {
        if self.n == 0 {
            return None;
        }
        let k = self.s[0];
        self.s.copy_within(1..self.n as usize, 0);
        self.n -= 1;
        Some(k)
    }

    fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn slots(&self) -> &[u8] {
        &self.s[..self.n as usize]
    }

    fn append(&mut self, o: &Queue) {
        for &k in o.slots() {
            self.push(k);
        }
    }
}

/// The float state: the arena, the lists, LaTeX's placement registers, the float open now.
#[derive(PartialEq, Clone)]
pub(super) struct Floats {
    boxes: [u32; SLOTS],
    count: [i32; SLOTS],
    free: Queue,
    cur: Queue,
    top: Queue,
    bot: Queue,
    mid: Queue,
    defer: Queue,
    /// `\@holdpg`.
    hold: Option<u32>,
    /// `\@colht`, `\@colroom`, `\@toproom`, `\@botroom`, `\@textfloatsheight`.
    pub(super) colht: i32,
    pub(super) colroom: i32,
    toproom: i32,
    botroom: i32,
    textfloatsheight: i32,
    /// `\@topnum`, `\@botnum`, `\@colnum`.
    pub(super) topnum: i32,
    botnum: i32,
    colnum: i32,
    /// `\@floatpenalty`; the open float's slot and type (`\@captype`: 1 figure, 2 table).
    penalty: i32,
    open: u8,
    pub(super) captype: u8,
    /// `\@savsk`, `\@savsf` of `\@bsphack`, and `\if@ignore` after `\@Esphack`.
    savsk: i32,
    savsf: i32,
    ignore: bool,
    /// What the float box's `\@parboxrestore` changes locally and native state keeps
    /// globally: `\if@nobreak`, `\if@noskipsec`, the lists `\\` looks through.
    saved: (bool, bool, usize),
    /// `\if@minipage` (global in LaTeX).
    pub(super) minipage: bool,
    /// `\@doclearpage`'s loop is running (else `\@startcolumn`'s).
    clearing: bool,
    /// The caption's text (`#3`).
    pub(super) caption: Vec<Token>,
    /// The open float has had its `\caption` (caption.sty's `\label` refers to it).
    pub(super) captioned: bool,
    /// subcaption: a `subfigure` or `subtable` came in this float (`\caption@setsubcaption`'s
    /// flags), and one came before its `\caption` (flag 4).
    pub(super) subbed: bool,
    pub(super) sub_above: bool,
    /// memoir's `\@memtempa` for this caption: `\vbox to\topskip{}` after the label when the
    /// caption opens its float (`\prevdepth` at most -99pt), else `\relax`.
    pub(super) memtempa: bool,
    /// The caption being made is `\captionof`'s: `\@captype` set for it alone, outside a float.
    pub(super) of: bool,
    /// float.sty's `[H]`: the open float is set here, not floated (`\@float@HH`).
    here: bool,
    /// The `[H]` box, between `\@endfloatbox` and `\float@endH`'s skips.
    here_box: u32,
    /// `\if@twocolumn`, `\if@firstcolumn`, `\@leftcolumn` (the first column made, while the
    /// second is collected), `\@dbltoplist` (only `\@topnewpage`'s box: its slot and box)
    /// and `\@dbltopnum`.
    pub(super) twocol: bool,
    first: bool,
    left: Option<u32>,
    dbltop: Option<(u8, u32)>,
    pub(super) dbltopnum: i32,
    /// float.sty: `\float@style` (an index of [`FS_NAMES`]), and each type's `\fst@<type>`
    /// (its style plus one, 0 for a type never restyled) by type bit (figure 0, table 1, the
    /// `\newfloat`s from 2).
    style: u8,
    fst: [u8; 8],
    /// `\fps@figure`, `\fps@table` (`\floatplacement`); the new types keep theirs.
    fps: [Vec<u8>; 2],
    /// The `\newfloat`s, type bit 2 on (`\ftype@<type>` 4, 8, ...).
    pub(super) types: Vec<NewFloat>,
    /// `\floatname`s given (`\fname@<type>`), by type name.
    fnames: Vec<(String, Vec<Token>)>,
    /// hyperref's `\Hy@float@currentHref`: a restyled float's anchor, made at its first
    /// caption and set in `\float@makebox`.
    href: Option<String>,
}

/// float.sty's styles, `\fs@<name>`.
pub(super) const FS_NAMES: [&str; 4] = ["plain", "plaintop", "ruled", "boxed"];
const FS_PLAIN: u8 = 0;
const FS_PLAINTOP: u8 = 1;
const FS_RULED: u8 = 2;
const FS_BOXED: u8 = 3;
/// float.sty's `\@floatcapt` (`\newsavebox`): a styled float's caption, set globally.
pub(super) const FLOAT_CAPT: u32 = 32708;

/// A float type of float.sty's `\newfloat{type}{placement}{ext}[within]`.
#[derive(PartialEq, Clone)]
pub(super) struct NewFloat {
    pub(super) name: String,
    /// `\ext@<type>`, the list file's extension (`lop`).
    pub(super) ext: String,
    /// `\fps@<type>`.
    fps: Vec<u8>,
    /// Its counter, one of `Doc::own_counters`.
    pub(super) counter: usize,
}

impl Default for Floats {
    fn default() -> Self {
        let mut free = Queue::default();
        for k in 0..SLOTS as u8 {
            free.push(k);
        }
        Floats { boxes: [0; SLOTS], count: [0; SLOTS], free, cur: Queue::default(), top: Queue::default(), bot: Queue::default(), mid: Queue::default(), defer: Queue::default(), hold: None, colht: 0, colroom: 0, toproom: 0, botroom: 0, textfloatsheight: 0, topnum: 0, botnum: 0, colnum: 0, penalty: 0, open: 0, captype: 0, savsk: 0, savsf: 0, ignore: false, saved: (false, false, 0), minipage: false, clearing: false, caption: Vec::new(), captioned: false, subbed: false, sub_above: false, memtempa: false, of: false, here: false, here_box: 0, twocol: false, first: true, left: None, dbltop: None, dbltopnum: 0, style: FS_PLAIN, fst: [0; 8], fps: [b"tbp".to_vec(), b"tbp".to_vec()], types: Vec::new(), fnames: Vec::new(), href: None }
    }
}

impl Floats {
    /// Whether any float is waiting (`\clearpage` has work).
    pub(super) fn pending(&self) -> bool {
        !(self.cur.is_empty() && self.top.is_empty() && self.bot.is_empty() && self.defer.is_empty())
    }

    /// `\@colroom`.
    pub(super) fn colroom(&self) -> i32 {
        self.colroom
    }

    /// The top and bottom lists of the column being made.
    pub(super) fn has_bottom(&self) -> bool {
        !self.bot.is_empty()
    }

    /// Whether top floats wait for the column (`\@toplist`).
    pub(super) fn has_top(&self) -> bool {
        !self.top.is_empty()
    }

    /// placeins' `\edef\@tempa{\@fb@botlist\@deferlist\@dbldeferlist}` is not empty: a float on
    /// `\@botlist` or `\@deferlist`. `None` in two columns (`\@dbldeferlist`, `\if@firstcolumn`).
    pub(super) fn held(&self) -> Option<bool> {
        (!self.twocol).then(|| !(self.bot.is_empty() && self.defer.is_empty()))
    }
}

/// article's float parameters that are macros (`\renewcommand\topfraction{.7}`), as
/// §452's fractions of the unit.
const TOP_FRACTION: (usize, i64) = (named!("topfraction"), 45875);
const BOTTOM_FRACTION: (usize, i64) = (named!("bottomfraction"), 19661);
const TEXT_FRACTION: (usize, i64) = (named!("textfraction"), 13107);
const FLOATPAGE_FRACTION: (usize, i64) = (named!("floatpagefraction"), 32768);

impl Machine<'_, '_> {
    pub(super) fn fl(&mut self) -> &mut Floats {
        &mut self.doc.as_mut().expect("a native document").floats
    }

    pub(super) fn flr(&self) -> &Floats {
        &self.doc.as_ref().expect("a native document").floats
    }

    fn ht(&self, b: u32) -> i32 {
        self.e.nodes.boxes[b as usize].height
    }

    /// `\topfraction\@colht` and kin: a fraction macro (the user's, or article's value)
    /// times `v`, as §455 scans a factor before an internal dimension.
    fn fraction_of(&mut self, f: (usize, i64), v: i32) -> i32 {
        let id = self.named_id(f.0);
        self.macro_factor(id, (false, 0, f.1)).map_or(0, |m| m(v))
    }

    /// `\@floatplacement` (in `\@opcol` and at `\begin{document}`).
    pub(super) fn float_placement(&mut self) {
        let colht = self.flr().colht;
        let (top, bot, total) = (self.rw(R_TOPNUMBER), self.rw(R_BOTTOMNUMBER), self.rw(R_TOTALNUMBER));
        let toproom = self.fraction_of(TOP_FRACTION, colht);
        let botroom = self.fraction_of(BOTTOM_FRACTION, colht);
        let f = self.fl();
        (f.topnum, f.toproom, f.botnum, f.botroom, f.colnum) = (top, toproom, bot, botroom, total);
    }

    /// `\begin{document}`'s `\@colht\textheight \@colroom\textheight \@floatplacement`.
    pub(super) fn floats_begin(&mut self, height: i32) {
        let f = self.fl();
        (f.colht, f.colroom) = (height, height);
        self.float_placement();
    }

    /// `\@parboxrestore` (`\@arrayparboxrestore` with `\sloppy`, `\let\\\@normalcr`): the
    /// paragraph shape of a box's text.
    pub(super) fn par_box_restore(&mut self) {
        if let Some((id, m, nl)) = self.doc.as_ref().map(|d| (d.par_id, d.par_prim, d.newline_id)) {
            self.e.eqtb.set_meaning(id, m, false);
            self.e.eqtb.set_meaning(nl, Meaning::Doc(NEWLINE as u16), false);
        }
        self.set_everypar(0);
        let w = self.dimen_param(super::super::engine_pack::HSIZE);
        self.set_rw(super::R_LINEWIDTH, w, false);
        self.set_rw(super::R_TOTALLEFTMARGIN, 0, false);
        self.set_dimen_named(param!("parindent"), 0);
        for r in [param!("parskip"), param!("leftskip"), param!("rightskip")] {
            self.set_glue_named(r, Glue::default());
        }
        self.set_rg(super::R_ATRIGHTSKIP, Glue::default());
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        let one = self.dimen_of("1pt");
        self.set_glue_named(param!("lineskip"), Glue { width: one, ..Glue::default() });
        self.set_dimen_named(param!("lineskiplimit"), 0);
        let normal = self.e.eqtb.glue(super::NORMAL_BASELINE_SKIP);
        self.e.eqtb.set_glue(Reg { region: Region::Glue, slot: BASELINE_SKIP }, normal, false);
        // `\sloppy`, in the font around the box.
        self.set_int_named(param!("tolerance"), 9999);
        let e = self.dimen_of("3em");
        self.set_dimen_named(param!("emergencystretch"), e);
        let half = self.dimen_of(".5pt");
        self.set_dimen_named(param!("hfuzz"), half);
        self.set_dimen_named(param!("vfuzz"), half);
    }

    /// `\@setminipage`: `\@minipagetrue \everypar{\@minipagefalse\everypar{}}`.
    pub(super) fn set_minipage(&mut self) {
        self.fl().minipage = true;
        self.set_everypar(MINIPAGE);
    }

    /// `\begin{figure}` and kin (`\@float`, `\@xfloat`) after `\begin`'s group: the
    /// placement, `\@bsphack`, the slot, and the float's `\vbox` begun.
    pub(super) fn float_open(&mut self, ftype: u8) {
        if self.mode == Mode::Math || self.inner {
            return self.not_native("a float in a box or a formula");
        }
        // caption.sty's `\@float@setevery` for a restyled type (`engine_caption`).
        if let Some(s) = self.styled(ftype).filter(|_| self.caption_pkg()) {
            self.caption_float_every(s == FS_RULED, s == FS_BOXED);
        }
        let mut fps: Vec<u8> = Vec::new();
        // `\@float`: `\@ifnextchar[`, else `[\fps@<type>]`.
        let deflt = self.type_fps(ftype);
        if self.next_is('[') {
            let arg = self.bracket_arg();
            for t in arg {
                match u8::try_from(t.chr() as u32) {
                    Ok(c) if t.cs_id().is_none() => fps.push(c),
                    _ => return self.not_native("a float placement of commands"),
                }
            }
            fps.retain(|c| *c != b' ');
        } else {
            fps.clone_from(&deflt);
        }
        // float.sty's `\@xfloat#1[`: `[H]` is `\@float@HH`, the box set in place.
        if fps == b"H" && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"float")) {
            return self.float_here_open(ftype);
        }
        // `\@fpsadddefault`: an empty placement or a lone `!` takes `\fps@<type>` (`tbp`).
        if fps.is_empty() || fps == b"!" {
            fps.extend_from_slice(&deflt);
        }
        let hmode = self.mode == Mode::Horizontal;
        let (savsk, savsf) = if hmode { (self.last_skip().width, self.aux) } else { (0, 0) };
        let Some(k) = self.fl().free.pop_front() else { return self.not_native("too many unprocessed floats") };
        let mut a = 16i32;
        for c in fps {
            let bit = |a: &mut i32, b: i32| {
                if (*a / b) % 2 == 0 {
                    *a += b;
                }
            };
            match c {
                b'h' => bit(&mut a, 1),
                b't' => bit(&mut a, 2),
                b'b' => bit(&mut a, 4),
                b'p' => bit(&mut a, 8),
                b'!' if a > 15 => a -= 16,
                b'!' => {}
                _ => return self.not_native("an unknown float placement"),
            }
        }
        let doc = self.doc.as_mut().expect("a native document");
        let saved = (doc.nobreak, doc.noskipsec, doc.cr_base);
        (doc.nobreak, doc.noskipsec, doc.cr_base) = (false, false, doc.lists.len());
        let f = &mut doc.floats;
        f.count[k as usize] = a + 32 * ftype as i32;
        (f.open, f.captype, f.savsk, f.savsf, f.saved) = (k, ftype, savsk, savsf, saved);
        (f.captioned, f.subbed, f.sub_above) = (false, false, false);
        f.penalty = if hmode { -10002 } else { -10003 };
        // `\global\setbox\@currbox \color@vbox \normalcolor \vbox\bgroup`.
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_BOX, global: true }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            // `\normalcolor`: `\let\current@color\default@color \set@color`.
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.push_default_color();
        }
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        // `\hsize\columnwidth \@parboxrestore \@floatboxreset` (`\reset@font \normalsize
        // \@setminipage`).
        let w = self.e.eqtb.word(COLUMN_WIDTH);
        self.set_dimen_named(param!("hsize"), w);
        self.par_box_restore();
        self.normal_font();
        self.size_command(NORMALSIZE);
        self.set_minipage();
        // setspace.sty's `\@xfloat`: `\def\baselinestretch{\setspace@singlespace}\normalsize`.
        if self.setspace() {
            self.setspace_single();
            self.size_command(NORMALSIZE);
        }
        // caption.sty's `\caption@xfloat`: `\caption@setanchor`, with hyperref `hypcap`'s
        // anchor (`engine_caption`); a restyled type's is frozen (`\caption@freezeHref`),
        // made in `\float@makebox`.
        if self.caption_pkg() && self.hyper() && self.styled(ftype).is_none() {
            self.caption_hypcap();
        }
        self.caption_typed(ftype);
    }

    /// float.sty's `\@float@HH#1[H]` in vertical mode, for an unstyled type: `\@captype`,
    /// `\setbox\@currbox\color@vbox\normalcolor\vbox\bgroup\hsize\columnwidth\@parboxrestore
    /// \@floatboxreset\@setnobreak` (no slot, no `\@floatpenalty`, a local box).
    fn float_here_open(&mut self, ftype: u8) {
        if self.mode == Mode::Math || self.inner {
            return self.not_native("a float in a box or a formula");
        }
        let doc = self.doc.as_mut().expect("a native document");
        let saved = (doc.nobreak, doc.noskipsec, doc.cr_base);
        (doc.nobreak, doc.noskipsec, doc.cr_base) = (false, false, doc.lists.len());
        let f = &mut doc.floats;
        (f.captype, f.saved, f.here) = (ftype, saved, true);
        (f.captioned, f.subbed, f.sub_above) = (false, false, false);
        // `\setbox\@floatcapt=\vbox{}`: an empty caption, not a void one, until `\caption`.
        let start = self.cur.len();
        let empty = self.vpack(start, 0, false, MAX_DIMEN);
        let empty = self.e.nodes.add_box(BoxRec { vertical: true, ..empty });
        self.e.eqtb.set_box(FLOAT_CAPT, Some(empty), false);
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_BOX, global: false }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.push_default_color();
        }
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        let w = self.e.eqtb.word(COLUMN_WIDTH);
        self.set_dimen_named(param!("hsize"), w);
        self.par_box_restore();
        self.normal_font();
        self.size_command(NORMALSIZE);
        self.set_minipage();
        // caption.sty's `\caption@xfloat` (its `\@xfloat`, set at `\begin{document}` over
        // float.sty's) runs `\@float@HH` and then `\caption@settype` (`\caption@clrflags`)
        // and `\caption@setanchor` as for any float: `\label` is `\caption@xlabel`, the
        // sub-caption flags start clear, and with hyperref `hypcap` puts its anchor here.
        if self.caption_pkg() && self.hyper() && self.styled(ftype).is_none() {
            self.caption_hypcap();
        }
        self.caption_typed(ftype);
    }

    /// `\end{figure}` and kin: `\@endfloatbox`, then `\end@float`, then `\end`'s
    /// `\endgroup`.
    pub(super) fn float_close(&mut self) {
        // subfig's `\end@float` before `\@endfloatbox` (`engine_caption`).
        if !self.subfloat_float_end() {
            return;
        }
        self.run_ops(vec![Op::Par, Op::Call(|m| m.float_box_end())]);
    }

    fn float_box_end(&mut self) {
        // `\vskip\z@skip \@minipagefalse \outer@nobreak \egroup \color@endbox`.
        self.build_glue(Glue::default(), false);
        self.fl().minipage = false;
        self.package();
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(FLOAT_BOX).unwrap_or(0);
        self.e.eqtb.replace_box(FLOAT_BOX, None);
        // float.sty's `\float@end` and `\float@endH`: a restyled type's box made again,
        // `\float@makebox\columnwidth`.
        let b = match self.styled(self.flr().captype) {
            Some(s) => self.float_makebox(b, s),
            None => b,
        };
        self.caption_typed_end();
        let height = self.doc.as_ref().map_or(0, |d| d.class.dimen("textheight"));
        let doc = self.doc.as_mut().expect("a native document");
        (doc.nobreak, doc.noskipsec, doc.cr_base) = doc.floats.saved;
        let f = &mut doc.floats;
        f.captype = 0;
        // float.sty's `\float@endH`: `\vskip\intextsep \box\@currbox \vskip\intextsep\relax`.
        // In a paragraph the `\vskip` ends it first (§1094).
        if std::mem::take(&mut f.here) {
            f.here_box = b;
            let mut ops = if self.mode == Mode::Horizontal { vec![self.cur_par()] } else { Vec::new() };
            ops.push(Op::Call(|m| {
                let g = m.rg(R_INTEXTSEP);
                m.build_glue(g, false);
                let b = m.flr().here_box;
                m.append_to_vlist(b);
                m.build_glue(g, false);
                m.float_env_end();
            }));
            return self.run_ops(ops);
        }
        let k = f.open;
        f.boxes[k as usize] = b;
        let p = f.penalty;
        if p >= 0 {
            return self.float_env_end();
        }
        // `\@largefloatcheck`.
        if self.ht(b) > height {
            self.e.nodes.boxes[b as usize].height = height;
        }
        self.fl().cur.push(k);
        if p < -10002 {
            // `\penalty -\@Miv \@tempdima\prevdepth \vbox{} \prevdepth\@tempdima
            // \penalty\@floatpenalty`.
            self.run_ops(vec![Op::Penalty(-10004), Op::Call(|m| m.empty_vbox(true)), Op::Penalty(p), Op::Call(|m| m.float_env_end())]);
        } else {
            // `\vadjust{\penalty -\@Miv \vbox{}\penalty\@floatpenalty}\@Esphack`.
            self.build_adjust(|m| {
                m.cur.push(Node::new(Kind::Penalty, -10004));
                m.empty_vbox(false);
                m.cur.push(Node::new(Kind::Penalty, p));
            });
            let (savsk, savsf) = (self.flr().savsk, self.flr().savsf);
            self.aux = savsf;
            if savsk > 0 {
                self.cur.push(Node::new(Kind::Penalty, 10000));
                self.build_glue(Glue::default(), false);
                self.fl().ignore = true;
                self.ignore_spaces();
            }
            self.float_env_end();
        }
    }

    /// `\vbox{}` in a vertical list; `keep` keeps `\prevdepth` (`\@tempdima\prevdepth`).
    pub(super) fn empty_vbox(&mut self, keep: bool) {
        let saved = self.aux;
        let start = self.cur.len();
        let b = self.vpack(start, 0, false, MAX_DIMEN);
        let b = self.e.nodes.add_box(BoxRec { vertical: true, ..b });
        self.append_to_vlist(b);
        if keep {
            self.aux = saved;
        }
    }

    /// `\end`'s `\endgroup`, then `\if@ignore \@ignorefalse \ignorespaces \fi`.
    fn float_env_end(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        if std::mem::take(&mut self.fl().ignore) {
            self.ignore_spaces();
        }
    }

    /// hyperref's anchor of caption `n` of float type `t`: `figure.\theHfigure`, which is
    /// `\the\value{figure}`, or under report (`\@addtoreset{figure}{chapter}`, latex.ltx)
    /// `\theHchapter.\the\value{figure}`.
    pub(super) fn float_href(&self, t: u8, n: i32) -> String {
        // `\counterwithin{figure}{section}`: hyperref's `\@addtoreset` makes `\theHfigure`
        // `\theHsection.\the\value{figure}`; made before hyperref, `\thefigure` is read (with
        // `\@arabic` `\number`): the heading's number either way, outside the appendix.
        if let Some(d) = self.doc.as_ref().filter(|d| !d.appendix) {
            if let Some(l) = d.fig_within[t as usize - 1] {
                return format!("{}.{}.{n}", ["figure", "table"][t as usize - 1], super::heading_number(&d.counters, l as usize, false));
            }
        }
        let within = self.doc.as_ref().filter(|d| d.report).map_or(String::new(), |d| format!(".{}", super::hyperref::the_h_top(d.counters[0], d.appendix)));
        format!("{}{within}.{n}", ["figure", "table"][t as usize - 1])
    }

    /// `\caption`: `\refstepcounter\@captype`, `\@dblarg{\@caption\@captype}`, and
    /// `\@caption`'s `\par`.
    pub(super) fn caption(&mut self) {
        let t = self.doc.as_ref().map_or(0, |d| d.floats.captype);
        if t == 0 {
            return self.not_native("\\caption outside a float");
        }
        // float.sty's `\@float@c@<type>`: a restyled type's caption is `\float@caption`.
        if self.styled(t).is_some() {
            return self.float_caption(t);
        }
        // threeparttable's `\TPT@caption`: in the table's box it is not native.
        let tpt = self.tpt_phase();
        if tpt == super::threeparttable::IN_TABLE {
            return self.not_native("a \\caption in a threeparttable's table");
        }
        // caption's `\TPT@caption` (caption.sty:1432): not native yet.
        if tpt != 0 && self.caption_pkg() {
            return self.not_native("caption: a \\caption in a threeparttable");
        }
        if self.caption_pkg() {
            if let Some(what) = self.caption_pkg_refused() {
                return self.not_native(what);
            }
            if self.next_is('*') {
                return self.not_native("caption: \\caption*");
            }
            // subcaption's `\caption@subcaption` in a `subfigure` (`engine_caption`).
            if self.caption_in_sub() {
                return self.subcaption();
            }
            if self.flr().captioned && self.flr().subbed {
                return self.not_native("subcaption: a second \\caption in a float with sub-captions");
            }
            self.fl().captioned = true;
        }
        let r = R_FIGURE + t as usize - 1;
        // caption.sty's `\caption@refstepcounter`: after a `subfigure` the counter has
        // stepped there (`\caption@@refcounter`: `\refstepcounter` without its step).
        let stepped = self.caption_pkg() && self.flr().subbed;
        let n = self.rw(r) + !stepped as i32;
        if !stepped {
            self.set_rw(r, n, true);
            self.caption_sub_reset(t);
        }
        // hyperref: `\H@refstepcounter` (no anchor here) and `\hyper@makecurrent{figure}`;
        // the anchor goes in the caption text (`caption_tokens`).
        // hyperref names the anchor by `\theHfigure` (`float_href`), not `\thefigure`.
        // `\thefigure`: the class's or the document's (`float_the`).
        let Some(text) = self.float_the(t as usize - 1, n) else { return self.not_native("a \\thefigure or \\thetable the native job cannot read") };
        // The list of figures' entry (`caption_body`) is `predict`'s, which reads a body's
        // redefinition with no regard to groups: its number must be this one.
        let listed = self.doc.as_ref().and_then(|d| d.contents.as_ref().filter(|c| c.iter().any(|e| e.level >= super::LOF)).and_then(|c| c.get(d.toc_seen)));
        if !self.caption_in_sub() && listed.is_some_and(|e| e.number != text) {
            return self.not_native("a \\thefigure or \\thetable the pre-scan read differently");
        }
        if self.hyper() {
            self.store_current_label(&text);
            // caption.sty: `\caption@makecurrentHref` restores `hypcap`'s anchor.
            let name = if self.caption_pkg() { self.caption_hc() } else { self.float_href(t, n) };
            self.set_current_href(&name);
        } else {
            self.set_current_label(&text);
        }
        self.cref_step(if t == 1 { "figure" } else { "table" });
        if self.next_is('[') {
            self.bracket_arg();
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if tpt != 0 {
            text = self.tpt_caption_text(text);
            // Before the table it waits for the measure (`engine_threeparttable`).
            if tpt == super::threeparttable::BEFORE {
                return self.tpt_hold_caption(text);
            }
        }
        self.fl().caption = text;
        self.run_ops(vec![Op::Par, Op::Call(|m| m.caption_body())]);
    }

    /// The tokens of `#1: #2` (`\fnum@<type>`: `\figurename\nobreakspace\thefigure`).
    pub(super) fn caption_tokens(&mut self, out: &mut Vec<Token>) {
        let t = self.flr().captype;
        // french3.ldf (`\FBprocess@options`, `\AtBeginDocument`): `\fnum@<type>` is
        // `{\ifFBfrench\FBfigtabshape\fi\fnum@<type>ORI}` (`\scshape`), and article's
        // `\@makecaption` is `\FB@makecaption` with `\FBCaption@Separator` `\CaptionSeparator`,
        // `\space\textendash\space`.
        let french = self.babel_french();
        if french {
            out.push(Token::char(super::Cat::BeginGroup, '{'));
            out.extend(self.tex_tokens("\\scshape"));
        }
        self.fnum_kernel(t, out);
        if french {
            out.push(Token::char(super::Cat::EndGroup, '}'));
            out.push(Token::char(super::Cat::Space, ' '));
            out.extend(self.tex_tokens("\\textendash"));
            out.extend([Token::char(super::Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
        } else {
            out.extend([Token::char(super::Cat::Other, ':'), Token::char(super::Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
        }
        // hyperref's `\@caption`: `\Hy@raisedlink{\hyper@@anchor{figure.N}{\relax}}` first;
        // under caption.sty `hypcap` made it at the float's start (`\caption@makeanchor` is
        // `\@firstofone`).
        if self.hyper() && !self.caption_pkg() {
            let n = self.rw(R_FIGURE + t as usize - 1);
            let name = self.float_href(t, n);
            let t = self.text_raised_link(name);
            out.extend(t);
        }
        out.extend_from_slice(&self.flr().caption);
    }

    /// A caption's `\addcontentsline{\ext@<type>}`: with a list of figures or tables the
    /// write is a contents entry (`LOF`, `LOT`), checked against `predict`'s; else an inert
    /// `.aux` write.
    fn caption_contents(&mut self) {
        let t = self.flr().captype as usize;
        // A `\newfloat` type's list (`ftype` 4, 8, ..) is `LOF + 2 + k`, `k` its place.
        let level = if t <= 2 { super::LOF - 1 + t } else { super::LOF + t.ilog2() as usize };
        match self.doc.as_mut().filter(|d| d.contents.as_ref().is_some_and(|c| c.iter().any(|e| e.level >= super::LOF))) {
            Some(doc) => {
                let saved = std::mem::replace(&mut doc.level, level);
                self.contents_write();
                if let Some(doc) = self.doc.as_mut() {
                    doc.level = saved;
                }
            }
            None => self.aux_write(0),
        }
    }

    /// The kernel's `\fnum@figure` (`\figurename\nobreakspace\thefigure`) and `\fnum@table`.
    /// A `\newfloat` type's is float.sty's `\fname@<type>{} \the<type>`, under caption.sty
    /// too (its default label format, `original`, caption3.sty:744).
    fn fnum_kernel(&mut self, t: u8, out: &mut Vec<Token>) {
        if t > 2 {
            let name = self.flr().types[t.ilog2() as usize - 2].name.clone();
            match self.flr().fnames.iter().find(|n| n.0 == name) {
                Some(n) => out.extend_from_slice(&n.1),
                None => out.extend(name.chars().map(|c| Token::char(super::Cat::Letter, c))),
            }
            out.extend([Token::char(super::Cat::BeginGroup, '{'), Token::char(super::Cat::EndGroup, '}'), Token::char(super::Cat::Space, ' ')]);
            out.push(Token::cs(self.e.cs.intern(&format!("the{name}"))));
            return;
        }
        let name = if t == 1 { (named!("figurename"), "Figure") } else { (named!("tablename"), "Table") };
        let id = self.named_id(name.0);
        if matches!(self.e.eqtb.meaning(id), Meaning::Macro(_)) {
            out.push(Token::cs(id));
        } else {
            out.extend(name.1.chars().map(|c| Token::char(super::Cat::Letter, c)));
        }
        out.push(self.named_cs(named!("nobreakspace")));
        let n = self.rw(R_FIGURE + t as usize - 1);
        // `caption` read the text before (`float_the`).
        let text = self.float_the(t as usize - 1, n).unwrap_or_default();
        out.extend(text.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { super::Cat::Letter } else if c == ' ' { super::Cat::Space } else { super::Cat::Other }, c)));
    }

    /// The rest of `\@caption`: the `\addcontentsline` write, `\begingroup \@parboxrestore
    /// \if@minipage\@setminipage\fi \normalsize`, and article's `\@makecaption` up to
    /// `\sbox\@tempboxa{#1: #2}`.
    pub(super) fn caption_body(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.koma) {
            return self.koma_caption_body();
        }
        let sub = self.caption_in_sub();
        // subcaption's `list=0`: no entry.
        if !sub {
            self.caption_contents();
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.par_box_restore();
        if self.flr().minipage {
            self.set_minipage();
        }
        self.size_command(NORMALSIZE);
        // caption.sty's `\caption@makecaption` (`engine_caption`).
        if self.caption_pkg() {
            return self.caption_open();
        }
        // memoir's `\@makecaption` (memoir.cls:5964): `\ifdim\prevdepth>-99\p@ \vskip
        // \abovecaptionskip \else \def\@memtempa{\vbox to\topskip{}}\fi`.
        if self.memoir() && self.mode != Mode::Vertical {
            return self.not_native("memoir: a caption in a paragraph");
        }
        let first = self.memoir() && self.aux <= -99 * 65536;
        self.fl().memtempa = first;
        if !first {
            let g = self.rg(R_ABOVECAPTIONSKIP);
            self.build_glue(g, false);
        }
        self.open_box(HBOX_GROUP, Ctx::Set { n: TEMP_BOX, global: false }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            let c = self.current_color();
            self.color_stack(Some(&c));
        }
        let mut list = Vec::new();
        self.caption_tokens(&mut list);
        list.push(self.doc_id(CAPTION_MID));
        self.back_list(&list);
    }

    /// `\@makecaption` after the `\sbox`: the caption centred in `\hsize`, or set again as a
    /// paragraph when it is wider.
    pub(super) fn caption_mid(&mut self) {
        if self.caption_pkg() {
            return self.caption_measured();
        }
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(TEMP_BOX).unwrap_or(0);
        // memoir's `\@@makecaption`: the measured `{#1: }#2` narrower than `\linewidth` is
        // set `\centering`, else as it is; both as a paragraph, `{#1: }{#2\par}`
        // (`\@contnfont`, `\@contdelim` `: `, `\@conttfont`, `\@contcshortstyle`,
        // `\@contpre`, `\@contpost` and `\@contfinal` are memoir's defaults: nothing).
        if self.memoir() {
            let narrow = self.e.nodes.boxes[b as usize].width < self.rw(super::R_LINEWIDTH);
            let mut list = Vec::new();
            if narrow {
                list.push(self.doc_id(doc!("centering")));
            } else if self.flr().memtempa {
                return self.not_native("memoir: a wide caption first in its float");
            }
            let at = list.len();
            self.caption_tokens(&mut list);
            if self.flr().memtempa {
                // `{#1\@contdelim}\@memtempa{#2..}`: after the label's `: `.
                let ign = self.named_cs(named!("ignorespaces"));
                if let Some(k) = list[at..].iter().position(|t| *t == ign) {
                    let vb = self.tex_tokens("\\vbox to\\topskip{}");
                    list.splice(at + k..at + k, vb);
                }
            }
            let par = self.doc.as_ref().map_or(0, |d| d.par_id);
            list.extend([Token::cs(par), self.doc_id(CAPTION_END)]);
            return self.back_list(&list);
        }
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        if self.e.nodes.boxes[b as usize].width > hsize {
            let mut list = Vec::new();
            self.caption_tokens(&mut list);
            let par = self.doc.as_ref().map_or(0, |d| d.par_id);
            list.extend([Token::cs(par), self.doc_id(CAPTION_END)]);
            return self.back_list(&list);
        }
        // `\global\@minipagefalse \hb@xt@\hsize{\hfil\box\@tempboxa\hfil}`.
        self.fl().minipage = false;
        self.centred_temp_box(b, hsize);
        self.caption_end();
    }

    /// `\hb@xt@\hsize{\hfil\box\@tempboxa\hfil}` of box `b`, `\@tempboxa`, in a vertical list.
    fn centred_temp_box(&mut self, b: u32, hsize: i32) {
        self.e.eqtb.replace_box(TEMP_BOX, None);
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        let start = self.cur.len();
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        self.cur.push(Node::new(Kind::Box, b as i32));
        let n = self.e.nodes.glue_node(fil, 0);
        self.cur.push(n);
        let h = self.hpack(start, hsize, true);
        let h = self.e.nodes.add_box(h);
        self.append_to_vlist(h);
    }

    /// `\vskip\belowcaptionskip`, `\@caption`'s `\par`, `\endgroup`.
    pub(super) fn caption_end(&mut self) {
        let g = self.rg(R_BELOWCAPTIONSKIP);
        self.build_glue(g, false);
        self.par();
        self.end_group(SEMI_SIMPLE_GROUP);
        self.captionof_end();
    }

    // The output routine.

    fn output_penalty(&self) -> i32 {
        self.e.eqtb.word(param!("outputpenalty"))
    }

    /// `\@specialoutput` (`\outputpenalty` below `-\@M`).
    pub(super) fn special_output(&mut self, p: i32) {
        if p > -10002 {
            return self.doc_clear_page();
        }
        // Box 0 is a box like any other: a void `\box255` is `None`.
        let b = self.e.eqtb.box_reg(255);
        self.e.eqtb.replace_box(255, None);
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let start = self.cur.len();
        if p < -10003 {
            // `\global\setbox\@holdpg\vbox{\unvbox\@cclv}`.
            if let Some(b) = b {
                self.unvbox(b);
            }
            let h = self.vpack(start, 0, false, d);
            let h = self.e.nodes.add_box(BoxRec { vertical: true, ..h });
            self.fl().hold = Some(h);
            return self.output_tail();
        }
        // `\global\setbox\@holdpg\vbox{\unvbox\@holdpg \unvbox\@cclv \setbox\@tempboxa
        // \lastbox \unskip}`, then `\unvbox\@holdpg`.
        if let Some(h) = self.fl().hold.take() {
            self.unvbox(h);
        }
        if let Some(b) = b {
            self.unvbox(b);
        }
        if self.cur.len() > start && self.cur.last().is_some_and(|n| n.kind == Kind::Box) {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
        if self.cur.len() > start && self.cur.last().is_some_and(|n| n.kind == Kind::Glue) {
            if let Some(n) = self.cur.pop() {
                self.e.nodes.flush_node(n);
            }
        }
        let h = self.vpack(start, 0, false, d);
        let (mut pageht, pagedp) = (h.height, h.depth);
        self.cur.extend_from_slice(self.e.nodes.get(h.list));
        self.e.nodes.free_block(h.list);
        let Some(k) = self.fl().cur.pop_front() else { return self.not_native("an output with no float") };
        pageht += pagedp;
        if let Some(f) = self.e.eqtb.box_reg(FOOTINS) {
            let r = self.e.nodes.boxes[f as usize];
            pageht += r.height + r.depth + self.e.eqtb.glue(Reg::register(Region::Glue, FOOTINS)).width;
            // `\@reinserts`: `\insert\footins{\unvbox\footins}`.
            let list = self.e.nodes.get(r.list).to_vec();
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(f);
            self.e.eqtb.replace_box(FOOTINS, None);
            self.build_insert(FOOTINS as u8, &list);
        }
        let mut ops = Vec::new();
        self.add_to_cur_col(k, pageht, &mut ops);
        if self.output_penalty() < 0 {
            if self.doc.as_ref().is_some_and(|d| d.nobreak) {
                ops.push(Op::Penalty(10000));
            } else {
                let p = self.int_named(param!("interlinepenalty"));
                self.add_penalty(p, &mut ops);
            }
        }
        ops.push(Op::Call(|m| m.output_tail()));
        self.run_ops(ops);
    }

    /// `\unvbox` of box `b`, which dies.
    fn unvbox(&mut self, b: u32) {
        let r = self.e.nodes.boxes[b as usize];
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
    }

    /// An integer parameter by name.
    pub(super) fn int_named(&self, r: Reg) -> i32 {
        self.e.eqtb.word(r)
    }

    /// `\@setfloattypecounts`: (`\@currtype`, `\@fpstype`).
    fn type_counts(&self, k: u8) -> (i32, i32) {
        let c = self.flr().count[k as usize];
        (c / 32 * 32, c - c / 32 * 32)
    }

    /// `\@bitor t list`: a float of `list` whose `\count` over `t` is odd.
    fn bitor(&self, t: i32, lists: &[&Queue]) -> bool {
        let f = self.flr();
        t != 0 && lists.iter().any(|l| l.slots().iter().any(|&e| (f.count[e as usize] / t) % 2 == 1))
    }

    /// `\@testwrongwidth`: the box's depth is not `\f@depth` (zero in one column).
    fn wrong_width(&self, k: u8) -> bool {
        self.e.nodes.boxes[self.flr().boxes[k as usize] as usize].depth != 0
    }

    fn float_ht(&self, k: u8) -> i32 {
        self.ht(self.flr().boxes[k as usize])
    }

    /// `\@flsettextmin`.
    fn text_min(&mut self, fpstype: i32) -> i32 {
        if fpstype < 16 {
            0
        } else {
            let colht = self.flr().colht;
            self.fraction_of(TEXT_FRACTION, colht)
        }
    }

    /// `\@addtocurcol`: the float goes here (`h`), at the top or bottom, or waits; what
    /// `h` places goes in `ops`.
    fn add_to_cur_col(&mut self, k: u8, pageht: i32, ops: &mut Vec<Op>) {
        let (currtype, fpstype) = self.type_counts(k);
        let mut insert = false;
        if fpstype != 8 && fpstype != 24 {
            let textmin = self.text_min(fpstype) + self.flr().textfloatsheight;
            let ht = self.float_ht(k);
            let mut req = pageht.max(textmin) + ht;
            let saved = req;
            if self.flr().colroom > req {
                let orig = self.flr().colnum;
                let colnum = if fpstype < 16 && orig == 0 { 1 } else { orig };
                self.fl().colnum = colnum;
                if colnum > 0 && !(self.bitor(currtype, &[&self.flr().defer]) || self.wrong_width(k)) {
                    if self.bitor(currtype, &[&self.flr().bot]) {
                        insert = self.add_to_bot(k, saved, fpstype);
                    } else {
                        if self.flr().count[k as usize] % 2 == 1 {
                            let sep = self.rg(R_INTEXTSEP).width;
                            req += sep;
                            if self.flr().colroom > req {
                                let f = self.fl();
                                f.colnum -= 1;
                                f.textfloatsheight += ht + 2 * sep;
                                f.mid.push(k);
                                let doc = self.doc.as_mut().expect("a native document");
                                if doc.nobreak {
                                    doc.nobreak = false;
                                    ops.push(Op::Penalty(10000));
                                    self.set_everypar(0);
                                } else {
                                    let p = self.int_named(param!("interlinepenalty"));
                                    self.add_penalty(p, ops);
                                }
                                let g = self.rg(R_INTEXTSEP);
                                let p = self.int_named(param!("interlinepenalty"));
                                ops.push(Op::Vskip(g));
                                ops.push(Op::Call(|m| m.mid_box()));
                                ops.push(Op::Penalty(p));
                                ops.push(Op::Vskip(g));
                                if self.output_penalty() < -10002 {
                                    ops.push(Op::Vskip(super::neg(self.glue_param(PAR_SKIP))));
                                }
                                {
            let r = param!("outputpenalty");
                                    self.e.eqtb.set_word(r, 0, false);
                                }
                                insert = true;
                            }
                        }
                        if !insert {
                            insert = self.add_to_top_or_bot(k, saved, fpstype);
                        }
                    }
                }
                if !insert {
                    self.fl().colnum = orig;
                }
            }
        }
        if !insert {
            // `\@resethfps`: a lone `h` becomes `ht`.
            if fpstype == 1 || fpstype == 17 {
                self.fl().count[k as usize] += 2;
            }
            self.fl().defer.push(k);
        }
    }

    /// `\box\@currbox` of the float `\@addtocurcol` placed here: the last of `\@midlist`.
    fn mid_box(&mut self) {
        let f = self.fl();
        let Some(&k) = f.mid.slots().last() else { return };
        let b = std::mem::take(&mut f.boxes[k as usize]);
        self.append_to_vlist(b);
    }

    /// `\@flcheckspace` then `\@flupdates` for the top (`top`) or bottom list.
    fn try_place(&mut self, k: u8, saved: i32, fpstype: i32, top: bool) -> bool {
        let f = self.flr();
        let (num, room, empty) = if top { (f.topnum, f.toproom, f.top.is_empty()) } else { (f.botnum, f.botroom, f.bot.is_empty()) };
        // `\@flsetnum`.
        let num = if fpstype < 16 && num == 0 { 1 } else { num };
        if num <= 0 {
            return false;
        }
        let sep = if empty { self.rg(R_TEXTFLOATSEP) } else { self.rg(R_FLOATSEP) }.width;
        let ht = self.float_ht(k);
        let req = saved + sep;
        if !(self.flr().colroom > req && (room > ht || fpstype < 16)) {
            return false;
        }
        if top && self.bitor(self.type_counts(k).0, &[&self.flr().mid, &self.flr().bot]) {
            return false;
        }
        let d = -ht - sep;
        let f = self.fl();
        f.colnum -= 1;
        f.colroom += d;
        if top {
            (f.topnum, f.toproom) = (num - 1, f.toproom + d);
            f.top.push(k);
        } else {
            (f.botnum, f.botroom) = (num - 1, f.botroom + d);
            f.bot.push(k);
        }
        true
    }

    /// `\@addtobot`.
    fn add_to_bot(&mut self, k: u8, saved: i32, fpstype: i32) -> bool {
        (self.flr().count[k as usize] / 4) % 2 == 1 && self.try_place(k, saved, fpstype, false)
    }

    /// `\@addtotoporbot`.
    fn add_to_top_or_bot(&mut self, k: u8, saved: i32, fpstype: i32) -> bool {
        ((self.flr().count[k as usize] / 2) % 2 == 1 && self.try_place(k, saved, fpstype, true)) || self.add_to_bot(k, saved, fpstype)
    }

    /// `\@addtonextcol`, in its group: a deferred float into the new column's top or
    /// bottom, or back on `\@deferlist`.
    fn add_to_next_col(&mut self, k: u8) {
        let (currtype, fpstype) = self.type_counts(k);
        let mut insert = false;
        if fpstype != 8 && fpstype != 24 {
            let req = self.float_ht(k) + self.text_min(fpstype);
            if self.flr().colroom > req {
                let orig = self.flr().colnum;
                let colnum = if fpstype < 16 && orig == 0 { 1 } else { orig };
                self.fl().colnum = colnum;
                if colnum > 0 && !(self.bitor(currtype, &[&self.flr().defer]) || self.wrong_width(k)) {
                    insert = self.add_to_top_or_bot(k, req, fpstype);
                }
                if !insert {
                    self.fl().colnum = orig;
                }
            }
        }
        if !insert {
            self.fl().defer.push(k);
        }
    }

    /// `\@startcolumn`: a float page from `\@deferlist`, or its floats tried for the new
    /// column. Whether a float column was made (`\if@fcolmade`).
    fn start_column(&mut self) -> bool {
        let colht = self.flr().colht;
        self.fl().colroom = colht;
        let fpmin = self.fraction_of(FLOATPAGE_FRACTION, colht);
        if self.try_fcolumn(true, fpmin) {
            return true;
        }
        let old = std::mem::take(&mut self.fl().defer);
        for &k in old.slots() {
            self.add_to_next_col(k);
        }
        false
    }

    /// `\@tryfcolumn\@deferlist` (`\@makefcolumn` without `testfp`, with `\@fpmin`
    /// `-\maxdimen`): a float page of the first floats that fill more than `fpmin`, as
    /// the page's column.
    fn try_fcolumn(&mut self, testfp: bool, fpmin: i32) -> bool {
        let colht = self.flr().colht;
        let sep = self.rg(R_FPSEP);
        let mut trylist = self.flr().defer;
        if trylist.is_empty() {
            return false;
        }
        let mut failed = Queue::default();
        let testp = |m: &Machine, e: u8| testfp && (m.flr().count[e as usize] / 8) % 2 == 0;
        let mut made = None;
        while let Some(k) = trylist.pop_front() {
            let t = self.type_counts(k).0;
            if self.bitor(t, &[&failed]) || testp(self, k) || self.wrong_width(k) || self.float_ht(k) > colht {
                failed.push(k);
                continue;
            }
            // `\@ytryfc`.
            let (mut succeed, mut fail) = (Queue::default(), Queue::default());
            succeed.push(k);
            let mut a = self.float_ht(k);
            for &e in trylist.slots() {
                let t = self.type_counts(e).0;
                let b = a + self.float_ht(e) + sep.width;
                if self.bitor(t, &[&failed, &fail]) || testp(self, e) || self.wrong_width(e) || b > colht {
                    fail.push(e);
                } else {
                    succeed.push(e);
                    a = b;
                }
            }
            if a > fpmin {
                made = Some((succeed, fail));
                break;
            }
            failed.push(k);
        }
        let Some((succeed, fail)) = made else { return false };
        // `\@vtryfc`: `\vbox to\@colht{\vskip\@fptop \vskip-\@fpsep \@elt…\vskip\@fpbot}`,
        // each `\@elt` `\vskip\@fpsep \box #1`.
        let start = self.cur.len();
        self.vskip(self.rg(R_FPTOP));
        self.vskip(super::neg(sep));
        for &e in succeed.slots() {
            self.vskip(sep);
            let b = std::mem::take(&mut self.fl().boxes[e as usize]);
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
        self.vskip(self.rg(R_FPBOT));
        let d = self.dimen_param(BOX_MAX_DEPTH);
        let col = self.vpack(start, colht, true, d);
        let col = self.e.nodes.add_box(BoxRec { vertical: true, ..col });
        let f = self.fl();
        f.defer = failed;
        f.defer.append(&fail);
        f.free.append(&succeed);
        if let Some(doc) = self.doc.as_mut() {
            doc.column = Some(col);
        }
        true
    }

    /// After `\@opcol` (`\@outputpage`): `\@floatplacement`, then the next float page of
    /// `\@doclearpage` or `\@startcolumn`, else the end of the output routine.
    pub(super) fn after_opcol(&mut self) {
        self.fl().textfloatsheight = 0;
        self.float_placement();
        let made = if self.flr().clearing { self.try_fcolumn(false, -MAX_DIMEN) } else { self.start_column() };
        if made {
            return self.opcol();
        }
        if std::mem::take(&mut self.fl().clearing) {
            if self.flr().twocol {
                return self.clear_two_columns();
            }
            if !self.flr().defer.is_empty() {
                return self.not_native("floats left after \\clearpage");
            }
        }
        self.output_tail();
    }

    /// `\@doclearpage` with no footnotes: the page's material dropped, every float on
    /// float pages.
    fn doc_clear_page(&mut self) {
        if self.e.eqtb.box_reg(FOOTINS).is_some() {
            return self.not_native("\\clearpage with footnotes waiting");
        }
        // `\setbox\@tempboxa\vsplit\@cclv to\z@ \unvbox\@tempboxa \setbox\@tempboxa\box\@cclv`:
        // what comes before the first breakpoint (`\clearpage`'s `\write`) stays, the rest goes.
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            let start = self.cur.len();
            self.unvbox(b);
            let keep = self.cur[start..].iter().take_while(|n| n.kind == Kind::Whatsit).count();
            if self.cur.get(start + keep).is_some_and(|n| n.kind != Kind::Glue) {
                return self.not_native("\\clearpage with material before its first breakpoint");
            }
            for n in self.cur.split_off(start + keep) {
                self.e.nodes.flush_node(n);
            }
        }
        let colht = self.flr().colht;
        let f = self.fl();
        if !f.cur.is_empty() {
            return self.not_native("floats lost");
        }
        let mut defer = f.top;
        defer.append(&f.bot);
        defer.append(&f.defer);
        (f.defer, f.top, f.bot, f.colroom) = (defer, Queue::default(), Queue::default(), colht);
        f.clearing = true;
        if self.try_fcolumn(false, -MAX_DIMEN) {
            return self.opcol();
        }
        self.fl().clearing = false;
        if self.flr().twocol {
            return self.clear_two_columns();
        }
        self.output_tail();
    }

    /// The end of `\output`: `\vsize` for the next page from `\@colroom` (`\@emptycol` when
    /// floats leave no room), or `\maxdimen` while a page is held.
    pub(super) fn output_tail(&mut self) {
        // `\multi@column@out`'s end.
        if self.doc.as_ref().is_some_and(|d| d.multicol.is_some()) {
            return self.multicols_tail();
        }
        let r = param!("vsize");
        if self.output_penalty() <= -10004 {
            return self.e.eqtb.set_word(r, MAX_DIMEN, true);
        }
        let (colroom, height) = (self.flr().colroom, self.doc.as_ref().map_or(0, |d| d.class.dimen("textheight")));
        let b = self.glue_param(BASELINE_SKIP).width;
        if colroom < b + (b as i64 * 32768 / 65536) as i32 && colroom < height {
            // `\@emptycol`: `\vbox{}\penalty -\@M`.
            self.empty_vbox(false);
            self.cur.push(Node::new(Kind::Penalty, -10000));
            return;
        }
        self.e.eqtb.set_word(r, colroom, true);
    }

    /// `\@makecol`'s start: `\xdef\@freelist{\@freelist\@midlist}`.
    pub(super) fn free_mid(&mut self) {
        let f = self.fl();
        let mid = std::mem::take(&mut f.mid);
        f.free.append(&mid);
    }

    /// `\@outputbox@attachfloats`: `\@cflt` and `\@cflb` around the column's material at
    /// `cur[start..]`; the outputbox's depth after.
    pub(super) fn attach_floats(&mut self, start: usize, mut depth: i32) -> i32 {
        let fs = self.rg(R_FLOATSEP);
        let tfs = self.rg(R_TEXTFLOATSEP);
        if !self.flr().top.is_empty() {
            // `\@cflt`: `\vbox{\boxmaxdepth\maxdepth \unvbox\@tempboxa \vskip-\floatsep
            // \topfigrule \vskip\textfloatsep \unvbox\@outputbox}`.
            let top = std::mem::take(&mut self.fl().top);
            let mut nodes = Vec::with_capacity(2 * top.n as usize + 2);
            for &k in top.slots() {
                let b = std::mem::take(&mut self.fl().boxes[k as usize]);
                nodes.push(Node::new(Kind::Box, b as i32));
                nodes.push(self.e.nodes.glue_node(fs, 0));
            }
            nodes.push(self.e.nodes.glue_node(super::neg(fs), 0));
            nodes.push(self.e.nodes.glue_node(tfs, 0));
            self.cur.splice(start..start, nodes);
            self.fl().free.append(&top);
            let d = self.dimen_param(super::super::engine_page::MAX_DEPTH);
            depth = self.repack(start, d);
        }
        if !self.flr().bot.is_empty() {
            // `\@outputbox@append{}`, then `\@cflb`: `\vbox{\unvbox\@outputbox
            // \vskip\textfloatsep \botfigrule \unvbox\@tempboxa \vskip-\floatsep}`.
            let max = self.doc.as_ref().map_or(0, |d| d.class.dimen("@maxdepth"));
            self.repack(start, max);
            let bot = std::mem::take(&mut self.fl().bot);
            self.vskip(tfs);
            for &k in bot.slots() {
                let b = std::mem::take(&mut self.fl().boxes[k as usize]);
                self.cur.push(Node::new(Kind::Box, b as i32));
                self.vskip(fs);
            }
            self.vskip(super::neg(fs));
            self.fl().free.append(&bot);
            let d = self.dimen_param(BOX_MAX_DEPTH);
            depth = self.repack(start, d);
        }
        depth
    }

    /// `\setbox\@outputbox\vbox{...}` of the material at `cur[start..]`, unpacked again:
    /// its depth.
    pub(super) fn repack(&mut self, start: usize, d: i32) -> i32 {
        let a = self.vpack(start, 0, false, d);
        self.cur.extend_from_slice(self.e.nodes.get(a.list));
        self.e.nodes.free_block(a.list);
        a.depth
    }

    // Two columns.

    /// `\@opcol` after `\@makecol` or a float column (in `doc.column`): `\@outputpage` in
    /// one column, `\@outputdblcol` in two.
    pub(super) fn opcol(&mut self) {
        if !self.flr().twocol {
            return self.start_foot();
        }
        let Some(column) = self.doc.as_mut().and_then(|d| d.column.take()) else { return };
        // `\@outputdblcol`'s first branch: `\global\@firstcolumnfalse \global\setbox
        // \@leftcolumn\copy\@outputbox`; then the rest of `\@opcol` and `\@startcolumn`.
        if std::mem::replace(&mut self.fl().first, false) {
            self.fl().left = Some(column);
            return self.after_opcol();
        }
        self.fl().first = true;
        let Some(left) = self.fl().left.take() else { return self.not_native("a second column with no first") };
        // `\setbox\@outputbox\vbox{\hb@xt@\textwidth{\hb@xt@\columnwidth{\box\@leftcolumn
        // \hss}\hfil{\normalcolor\vrule\@width\columnseprule}\hfil\hb@xt@\columnwidth{\box
        // \@outputbox\hss}}}`.
        let cw = self.e.eqtb.word(COLUMN_WIDTH);
        let width = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let rule = self.e.eqtb.word(COLUMNSEPRULE);
        let hss = Glue { stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1, ..Glue::default() };
        let hfil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        let colored = self.colored();
        let start = self.cur.len();
        let mut lr = [0u32; 2];
        for (k, b) in [left, column].into_iter().enumerate() {
            let s = self.cur.len();
            self.cur.push(Node::new(Kind::Box, b as i32));
            let g = self.e.nodes.glue_node(hss, 0);
            self.cur.push(g);
            let h = self.hpack(s, cw, true);
            lr[k] = self.e.nodes.add_box(h);
        }
        self.cur.push(Node::new(Kind::Box, lr[0] as i32));
        let g = self.e.nodes.glue_node(hfil, 0);
        self.cur.push(g);
        if colored {
            self.push_default_color();
        }
        let r = self.e.nodes.rule_node([rule, RUNNING, RUNNING]);
        self.cur.push(r);
        if colored {
            self.color_stack(None);
        }
        let g = self.e.nodes.glue_node(hfil, 0);
        self.cur.push(g);
        self.cur.push(Node::new(Kind::Box, lr[1] as i32));
        let row = self.hpack(start, width, true);
        let row = self.e.nodes.add_box(row);
        let d = self.dimen_param(BOX_MAX_DEPTH);
        // `\@combinedblfloats`: `\vbox to\textheight{\unvbox\@tempboxa \vskip-\dblfloatsep
        // \vskip\dbltextfloatsep \unvbox\@outputbox}`, `\@tempboxa` each box of `\@dbltoplist`
        // and `\vskip\dblfloatsep` (`\dblfigrule` only when `\@dbltopnum>\m@ne`, and
        // `\@topnewpage` made it `\m@ne`).
        let column = if let Some((k, top)) = self.fl().dbltop.take() {
            let (fs, tfs) = (self.rg(R_DBLFLOATSEP), self.rg(R_DBLTEXTFLOATSEP));
            self.cur.push(Node::new(Kind::Box, top as i32));
            let n = self.e.nodes.glue_node(fs, 0);
            self.cur.push(n);
            let n = self.e.nodes.glue_node(super::neg(fs), 0);
            self.cur.push(n);
            let n = self.e.nodes.glue_node(tfs, 0);
            self.cur.push(n);
            self.cur.push(Node::new(Kind::Box, row as i32));
            self.fl().free.push(k);
            let height = self.doc.as_ref().map_or(0, |d| d.class.dimen("textheight"));
            self.vpack(start, height, true, d)
        } else {
            self.cur.push(Node::new(Kind::Box, row as i32));
            self.vpack(start, 0, false, d)
        };
        let column = self.e.nodes.add_box(BoxRec { vertical: true, ..column });
        if let Some(doc) = self.doc.as_mut() {
            doc.column = Some(column);
        }
        self.start_foot();
    }

    /// The end of `\@outputpage` (`\global\@colht\textheight`), then `\@outputdblcol`'s
    /// `\@dblfloatplacement` (`\global\@dbltopnum\c@dbltopnumber`) and `\@startdblcolumn`,
    /// which with no double-column float changes nothing: `\@tryfcolumn` finds every
    /// single-column float of the wrong width (`\f@depth` 1sp), and `\@addtodblcol` puts each
    /// back on `\@deferlist` in order.
    pub(super) fn page_done(&mut self) {
        if !self.flr().twocol {
            return;
        }
        let height = self.doc.as_ref().map_or(0, |d| d.class.dimen("textheight"));
        let top = self.rw(R_DBLTOPNUMBER);
        let f = self.fl();
        f.colht = height;
        f.dbltopnum = top;
    }

    /// `\@doclearpage` after its float columns, in two columns: in the first column
    /// `\global\@colht\textheight` and `\@dblfloatplacement\@makefcolumn\@deferlist` (no
    /// double-column float, so no page); in the second `\vbox{}\clearpage`, which ends the
    /// column (an empty one) and clears again.
    fn clear_two_columns(&mut self) {
        if !self.flr().defer.is_empty() {
            return self.not_native("floats left after \\clearpage");
        }
        if self.flr().first {
            if self.flr().dbltop.is_some() {
                return self.not_native("a two-column top at \\clearpage");
            }
            self.page_done();
            return self.output_tail();
        }
        // `\vbox{}`, then `\clearpage`: `\ifnum\@dbltopnum=\m@ne \ifdim\pagetotal<\topskip
        // \hbox{}`, `\newpage` (`\if@nobreak\@nobreakfalse\fi \par \vfil \penalty-\@M`),
        // `\@@write\m@ne{}\vbox{}\penalty-\@Mi`.
        let topskip = self.e.eqtb.glue(param!("topskip")).width;
        if self.flr().dbltopnum == -1 && self.page.so_far[1] < topskip {
            return self.not_native("\\clearpage on an empty column under a two-column top");
        }
        self.empty_vbox(false);
        if let Some(doc) = self.doc.as_mut() {
            doc.nobreak = false;
        }
        let n = self.e.nodes.glue_node(Glue { stretch: 65536, stretch_order: 1, ..Glue::default() }, 0);
        self.cur.push(n);
        self.cur.push(Node::new(Kind::Penalty, -10000));
        self.cur.push(Node { kind: Kind::Whatsit, sub: super::super::engine_whatsit::WRITE, font: super::super::engine_whatsit::NULL_STREAM, a: 0, b: 0 });
        self.empty_vbox(false);
        self.cur.push(Node::new(Kind::Penalty, -10001));
        self.output_tail();
    }

    /// `\begin{document}` in two columns: `\columnwidth` is `(\textwidth-\columnsep)/2`
    /// (`\divide` truncates), and `\hsize` and `\linewidth` follow; `\@firstcolumntrue`, and
    /// `\@dblfloatplacement`'s `\global\@dbltopnum\c@dbltopnumber`. The width.
    pub(super) fn two_columns_begin(&mut self, width: i32) -> i32 {
        let cw = (width - self.e.eqtb.word(COLUMNSEP)) / 2;
        self.e.eqtb.set_word(COLUMN_WIDTH, cw, true);
        let top = self.rw(R_DBLTOPNUMBER);
        let f = self.fl();
        (f.twocol, f.first, f.dbltopnum) = (true, true, top);
        cw
    }

    /// A document's `\twocolumn[text]` in two columns (the class's `twocolumn`): its
    /// `\clearpage`, then `\@topnewpage[text]` (`top_new_page_open`, the text, its end
    /// without a page style). Without `[text]` (`\@floatplacement` only) or in one column
    /// (a switch of the column count) it declines.
    fn twocolumn(&mut self) {
        if !self.two_columns() || !self.next_is('[') {
            return self.not_native("\\twocolumn without a top or in one column");
        }
        let text = self.bracket_arg();
        let mut t = vec![self.doc_id(super::NEW_PAGE), self.doc_id(super::CLEAR_PAGE_TAIL), self.doc_id(doc!("rt@topnewpage"))];
        t.extend(text);
        if let Some(d) = self.doc.as_mut() {
            d.steps.push(vec![Op::Call(|m| m.top_new_page_end(false))]);
        }
        t.push(self.doc_id(super::STEP));
        self.back_list(&t);
    }

    /// `\begin{@twocolumnfalse}` (`\@twocolumnfalse`, local to `\begin`'s group): what
    /// asks `\if@twocolumn` inside (`\maketitle`, `abstract`) takes one column.
    pub(super) fn twocolumn_false_begin(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let Some(d) = self.doc.as_mut() else { return };
        d.endpe = false;
        let was = std::mem::replace(&mut d.floats.twocol, false);
        d.pkg.twocol_saved.push(was);
    }

    /// `\end{@twocolumnfalse}`: `\end@twocolumnfalse` is `\relax`; `\end`'s `\endgroup`
    /// restores the switch.
    pub(super) fn twocolumn_false_end(&mut self) {
        let Some(d) = self.doc.as_mut() else { return };
        let Some(was) = d.pkg.twocol_saved.pop() else { return self.not_native("\\end{@twocolumnfalse} without its begin") };
        d.floats.twocol = was;
        self.end_group(SEMI_SIMPLE_GROUP);
    }

    /// `\twocolumn[` after its `\clearpage` (article's `\maketitle` in two columns): the
    /// globals of `\twocolumn`, then `\@topnewpage`'s `\@next\@currbox\@freelist` and
    /// `\global\setbox\@currbox\color@vbox\normalcolor\vbox{\hsize\textwidth\@parboxrestore`.
    pub(super) fn top_new_page_open(&mut self) {
        if self.mode != Mode::Vertical || self.inner {
            return self.not_native("\\twocolumn[...] in a box or a paragraph");
        }
        let width = self.doc.as_ref().map_or(0, |d| d.class.dimen("textwidth"));
        let cw = (width - self.e.eqtb.word(COLUMNSEP)) / 2;
        self.e.eqtb.set_word(COLUMN_WIDTH, cw, true);
        self.e.eqtb.set_word(param!("hsize"), cw, true);
        self.set_rw(super::R_LINEWIDTH, cw, true);
        self.fl().first = true;
        let Some(k) = self.fl().free.pop_front() else { return self.not_native("too many unprocessed floats") };
        let doc = self.doc.as_mut().expect("a native document");
        let saved = (doc.nobreak, doc.noskipsec, doc.cr_base);
        (doc.nobreak, doc.noskipsec, doc.cr_base) = (false, false, doc.lists.len());
        let f = &mut doc.floats;
        (f.open, f.saved) = (k, saved);
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_BOX, global: true }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.push_default_color();
        }
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        self.set_dimen_named(param!("hsize"), width);
        self.par_box_restore();
    }

    /// `\@topnewpage`'s end after the title's material: `\vskip-\dbltextfloatsep}\color@endbox`,
    /// the box no taller than `\textheight`, `\global\advance\@colht-(\ht+\dbltextfloatsep)`,
    /// the box as `\@dbltoplist`, `\global\@dbltopnum\m@ne`, then `\global\vsize\@colht
    /// \global\@colroom\@colht\@floatplacement` (`\@emptycol` below `2.5\baselineskip` is
    /// refused). And article's `\thispagestyle{plain}`.
    pub(super) fn top_new_page_close(&mut self) {
        self.top_new_page_end(true);
    }

    /// [`Self::top_new_page_close`], with article's `\thispagestyle{plain}` (`title`) or
    /// without (a document's own `\twocolumn[...]`).
    fn top_new_page_end(&mut self, title: bool) {
        if self.mode != Mode::Vertical {
            let then: super::Native = if title { |m| m.top_new_page_end(true) } else { |m| m.top_new_page_end(false) };
            return self.run_ops(vec![Op::Par, Op::Call(then)]);
        }
        let tfs = self.rg(R_DBLTEXTFLOATSEP);
        self.build_glue(super::neg(tfs), false);
        self.package();
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(FLOAT_BOX).unwrap_or(0);
        self.e.eqtb.replace_box(FLOAT_BOX, None);
        let height = self.doc.as_ref().map_or(0, |d| d.class.dimen("textheight"));
        if self.ht(b) > height {
            self.e.nodes.boxes[b as usize].height = height;
        }
        let ht = self.ht(b);
        let baseline = self.glue_param(BASELINE_SKIP).width;
        let doc = self.doc.as_mut().expect("a native document");
        (doc.nobreak, doc.noskipsec, doc.cr_base) = doc.floats.saved;
        if title {
            doc.special = Some(false);
        }
        let f = &mut doc.floats;
        if f.dbltop.is_some() {
            return self.not_native("Float(s) lost");
        }
        f.colht -= ht + tfs.width;
        f.dbltop = Some((f.open, b));
        f.dbltopnum = -1;
        let colht = f.colht;
        if colht < 2 * baseline + (baseline as i64 * 32768 / 65536) as i32 {
            return self.not_native("\\twocolumn[...] too tall");
        }
        self.e.eqtb.set_word(param!("vsize"), colht, true);
        self.fl().colroom = colht;
        self.float_placement();
    }
}

// float.sty's float types and styles: `\newfloat`, `\floatstyle`, `\restylefloat`,
// `\floatname`, `\floatplacement`, `\float@caption` and `\float@makebox`.
impl Machine<'_, '_> {
    /// A restyled type's style (float.sty's `\fst@<type>` is not `\relax`): an index of
    /// [`FS_NAMES`].
    pub(super) fn styled(&self, t: u8) -> Option<u8> {
        let f = &self.doc.as_ref()?.floats;
        let s = *f.fst.get(t.checked_ilog2()? as usize)?;
        s.checked_sub(1)
    }

    /// `\fps@<type>`.
    fn type_fps(&self, t: u8) -> Vec<u8> {
        let f = self.flr();
        match t {
            1 | 2 => f.fps[t as usize - 1].clone(),
            _ => f.types.get(t.ilog2() as usize - 2).map_or_else(|| b"tbp".to_vec(), |n| n.fps.clone()),
        }
    }

    /// The `\newfloat` type of environment `env` (`type` or `type*`): its `\ftype@<type>`.
    pub(super) fn new_float_type(&self, env: &str) -> Option<u8> {
        let name = env.strip_suffix('*').unwrap_or(env);
        let k = self.doc.as_ref()?.floats.types.iter().position(|t| t.name == name)?;
        Some(4u8 << k)
    }

    /// A type named in a float.sty command: figure, table or a `\newfloat`.
    fn float_type_named(&self, name: &str) -> Option<u8> {
        match name {
            "figure" => Some(1),
            "table" => Some(2),
            _ if !name.ends_with('*') => self.new_float_type(name),
            _ => None,
        }
    }

    /// float.sty's commands define macros locally (`\def`, `\@namedef`): taken in the
    /// preamble outside any group, where that lasts.
    fn float_preamble(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| !d.body) && self.e.eqtb.level() == 1
    }

    /// `\floatstyle{style}`: `\def\float@style{#1}` of a style float.sty defines.
    fn float_style(&mut self) {
        let mut a = Vec::new();
        self.read_arg(&mut a);
        match Self::plain_text(&a).and_then(|s| FS_NAMES.iter().position(|n| *n == s)) {
            Some(k) if self.float_preamble() => self.fl().style = k as u8,
            _ => self.not_native("float: \\floatstyle in the body or of another style"),
        }
    }

    /// `\restylefloat{type}`: `\fst@<type>` the current style; `\begin{type}` runs it,
    /// `\caption` is `\float@caption` and `\end{type}` `\float@end`.
    fn restyle_float(&mut self) {
        if self.next_is('*') {
            return self.not_native("float: \\restylefloat*");
        }
        let mut a = Vec::new();
        self.read_arg(&mut a);
        let t = Self::plain_text(&a).and_then(|s| self.float_type_named(&s));
        match t {
            Some(t) if self.float_preamble() => {
                let f = self.fl();
                f.fst[t.ilog2() as usize] = f.style + 1;
            }
            _ => self.not_native("float: \\restylefloat in the body or of another type"),
        }
    }

    /// `\floatname{type}{text}`: `\@namedef{fname@<type>}{text}`, the text kept as tokens.
    fn float_name(&mut self) {
        let (mut a, mut text) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut text);
        let Some(name) = Self::plain_text(&a).filter(|s| !s.is_empty() && self.float_preamble()) else { return self.not_native("float: \\floatname in the body or of a type with commands") };
        let f = self.fl();
        match f.fnames.iter_mut().find(|n| n.0 == name) {
            Some(n) => n.1 = text,
            None => f.fnames.push((name, text)),
        }
    }

    /// float.sty's `\listof{type}{title}` (`list_of_float`).
    fn list_of_cmd(&mut self) {
        let (mut a, mut title) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut title);
        let Some(ty) = Self::plain_text(&a) else { return self.not_native("float: \\listof of a type with commands") };
        self.list_of_float(&ty, title);
    }

    /// `\floatplacement{type}{placement}`: `\@namedef{fps@<type>}{#2}`.
    fn float_placement_cmd(&mut self) {
        let (mut a, mut p) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut p);
        let t = Self::plain_text(&a).and_then(|s| self.float_type_named(&s));
        let p = Self::plain_text(&p).filter(|p| !p.is_empty() && p.bytes().all(|c| b"htbpH!".contains(&c)));
        match (t, p) {
            (Some(t), Some(p)) if self.float_preamble() => {
                let f = self.fl();
                match t {
                    1 | 2 => f.fps[t as usize - 1] = p.into_bytes(),
                    _ => f.types[t.ilog2() as usize - 2].fps = p.into_bytes(),
                }
            }
            _ => self.not_native("float: \\floatplacement in the body or of another type"),
        }
    }

    /// `\newfloat{type}{placement}{ext}[within]` in the preamble: `\ext@<type>`,
    /// `\fps@<type>`, `\fname@<type>` (the type's name unless a `\floatname` came first),
    /// `\ftype@<type>` (4, 8, ... after figure and table), `\restylefloat{type}`,
    /// `\fnum@<type>` (`\fname@<type>{} \the<type>`), and its counter: `\newcounter{type}`
    /// (`\the<type>` `\arabic{type}`), or `\newcounter{type}[within]` with `\the<type>`
    /// `\the<within>.\arabic{type}` (`\float@newx`).
    pub(super) fn new_float(&mut self) {
        let (mut a, mut p, mut x) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut p);
        self.read_arg(&mut x);
        let (Some(name), Some(place), Some(ext)) = (Self::plain_text(&a), Self::plain_text(&p), Self::plain_text(&x)) else { return self.not_native("float: \\newfloat with commands") };
        let fresh = !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphabetic()) && self.float_type_named(&name).is_none() && self.own_counter_named(&name).is_none() && !super::counter::KERNEL_COUNTERS.contains(&name.as_str());
        if !fresh || !self.float_preamble() || self.flr().types.len() >= 6 || place.is_empty() || !place.bytes().all(|c| b"htbpH!".contains(&c)) || ext.is_empty() || !ext.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return self.not_native("float: \\newfloat of that type, in the body, or past six types");
        }
        let within = if self.next_is('[') {
            let w = self.bracket_arg();
            match self.within_counter(&w) {
                Some(c) => Some((c, w)),
                None => return self.not_native("float: \\newfloat within a counter not the document's or a heading's"),
            }
        } else {
            None
        };
        let k = match within {
            None => self.make_own_counter(&a, None, None),
            Some((Ok(o), _)) => self.make_own_counter(&a, Some(o), None),
            Some((Err(l), _)) => self.make_own_counter(&a, None, Some(l)),
        };
        if let Some((_, w)) = within {
            // `\edef\the#1{\noexpand\the#2.\noexpand\arabic{#1}}`.
            let wname: String = w.iter().map(|t| t.chr()).collect();
            let the_w = self.e.cs.intern(&format!("the{wname}"));
            let mut t = vec![Token::END_MATCH, Token::cs(the_w), Token::char(super::Cat::Other, '.'), self.doc_id(doc!("arabic")), Token::char(super::Cat::BeginGroup, '{')];
            t.extend(a.iter().copied());
            t.push(Token::char(super::Cat::EndGroup, '}'));
            let text = self.e.toks.store(&t);
            let id = self.e.cs.intern(&format!("the{name}"));
            self.e.eqtb.set_meaning(id, Meaning::Macro(super::super::engine_prim::Macro { text, flags: 0 }), false);
        }
        let f = self.fl();
        f.types.push(NewFloat { name, ext, fps: place.into_bytes(), counter: k });
        f.fst[f.types.len() + 1] = f.style + 1;
    }

    /// float.sty's `\caption` of a restyled type: `\refstepcounter\@captype`, then
    /// `\@dblarg{\float@caption\@captype}`: `\addcontentsline`, and `\global\setbox
    /// \@floatcapt\vbox\bgroup\@parboxrestore\normalsize\@fs@capt{\fnum@<type>}{\ignorespaces
    /// #3}` (`\floatc@ruled` `{\@fs@cfont #1} #2\par`; `\floatc@plain` measures `{\@fs@cfont
    /// #1:} #2` first). No `\par` before it, unlike the kernel's `\@caption`.
    fn float_caption(&mut self, t: u8) {
        if self.flr().of || self.tpt_phase() != 0 || self.memoir() || self.doc.as_ref().is_some_and(|d| d.koma) {
            return self.not_native("float: a restyled caption with a threeparttable, KOMA or memoir");
        }
        let cap = self.caption_pkg();
        if cap {
            if let Some(what) = self.caption_pkg_refused() {
                return self.not_native(what);
            }
            if self.next_is('*') {
                return self.not_native("caption: \\caption*");
            }
            if self.caption_in_sub() || self.flr().subbed {
                return self.not_native("caption: a restyled float with sub-captions");
            }
            // Its anchor is not `hypcap`'s `figure.caption.N`, which the references read.
            if t <= 2 && self.hyper() {
                return self.not_native("caption: a restyled figure or table with hyperref");
            }
            self.fl().captioned = true;
        }
        let hyper = self.hyper();
        if hyper && self.doc.as_ref().is_some_and(|d| d.appendix) {
            return self.not_native("hyperref: a restyled float's anchor in the appendix");
        }
        // hyperref's `\caption`: `\H@refstepcounter\@captype` (the kernel's, no anchor), then
        // `\Hy@float@caption`: `\hyper@makecurrent{\@captype}` into `\Hy@float@currentHref`
        // the first time in the float (the anchor goes in `\float@makebox`). caption.sty's
        // the same through `\caption@refstepcounter` and `\caption@prepareanchor` (its
        // frozen `hypcap` anchor, `\caption@currentHref`, set by its `\float@makebox` too).
        let (label, name, href) = if t <= 2 {
            let r = R_FIGURE + t as usize - 1;
            let n = self.rw(r) + 1;
            self.set_rw(r, n, true);
            let prefix = self.doc.as_ref().map_or(String::new(), |d| d.the_prefix[t as usize - 1].clone());
            (format!("{prefix}{n}"), (if t == 1 { "figure" } else { "table" }).to_string(), self.float_href(t, n))
        } else {
            let k = self.flr().types[t.ilog2() as usize - 2].counter;
            self.step_own(k, false);
            let Some(label) = self.own_label(k) else { return self.not_native("\\refstepcounter of a counter whose \\the the native job cannot read") };
            let name = self.flr().types[t.ilog2() as usize - 2].name.clone();
            (label, name, self.own_href(k))
        };
        if hyper {
            self.store_current_label(&label);
            self.cref_step(&name);
            let href = match cap {
                true => self.fl().href.insert(href).clone(),
                false => self.fl().href.get_or_insert(href).clone(),
            };
            self.set_current_href(&href);
        } else {
            self.set_current_label(&label);
            self.cref_step(&name);
        }
        if self.next_is('[') {
            self.bracket_arg();
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        // caption.sty's `\caption@@caption`: `\par`, `\caption@beginex` (the contents line),
        // then the box (`engine_caption`).
        if cap {
            self.fl().caption = text;
            return self.run_ops(vec![
                Op::Par,
                Op::Call(|m| {
                    m.caption_contents();
                    m.caption_floatcapt();
                }),
            ]);
        }
        self.caption_contents();
        // The end of `\float@caption`'s first line (float.sty:131, no `%`) is a space of the
        // macro's text: interword glue in a paragraph.
        if self.mode == Mode::Horizontal {
            self.append_space();
        }
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_CAPT, global: true }, false, 0, false);
        self.par_box_restore();
        self.size_command(NORMALSIZE);
        self.fl().caption = text;
        let mut list = Vec::new();
        if self.styled(t) == Some(FS_RULED) {
            self.float_capt_tokens(t, &mut list);
            let par = self.doc.as_ref().map_or(0, |d| d.par_id);
            list.extend([Token::cs(par), self.doc_id(doc!("rt@fcaptend"))]);
        } else {
            self.open_box(HBOX_GROUP, Ctx::Set { n: TEMP_BOX, global: false }, false, 0, false);
            self.float_capt_tokens(t, &mut list);
            list.push(self.doc_id(doc!("rt@fcaptmid")));
        }
        self.back_list(&list);
    }

    /// `{\@fs@cfont \fnum@<type>} \ignorespaces #3` (`\floatc@ruled`), with a `:` after the
    /// number in `\floatc@plain`; `\@fs@cfont` is `\bfseries` (ruled, boxed) or `\rmfamily`.
    fn float_capt_tokens(&mut self, t: u8, out: &mut Vec<Token>) {
        let s = self.styled(t).unwrap_or(FS_PLAIN);
        out.push(Token::char(super::Cat::BeginGroup, '{'));
        out.push(self.doc_id(if s >= FS_RULED { doc!("bfseries") } else { doc!("rmfamily") }));
        self.fnum_kernel(t, out);
        if s != FS_RULED {
            out.push(Token::char(super::Cat::Other, ':'));
        }
        out.extend([Token::char(super::Cat::EndGroup, '}'), Token::char(super::Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
        out.extend_from_slice(&self.flr().caption);
    }

    /// `\floatc@plain` after the measure: `\hbox to\hsize{\hfil\box\@tempboxa\hfil}`, or the
    /// caption set again as a paragraph when it is wider.
    fn float_capt_mid(&mut self) {
        self.package();
        let b = self.e.eqtb.box_reg(TEMP_BOX).unwrap_or(0);
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        if self.e.nodes.boxes[b as usize].width > hsize {
            let t = self.flr().captype;
            let mut list = Vec::new();
            self.float_capt_tokens(t, &mut list);
            let par = self.doc.as_ref().map_or(0, |d| d.par_id);
            list.extend([Token::cs(par), self.doc_id(doc!("rt@fcaptend"))]);
            return self.back_list(&list);
        }
        self.centred_temp_box(b, hsize);
        self.float_capt_end();
    }

    /// `\@ifnextchar[{\float@ccon}{\egroup}`: the spaces after the caption gobbled, the
    /// caption's box ended into `\@floatcapt`.
    fn float_capt_end(&mut self) {
        if self.next_is('[') {
            return self.not_native("float: a caption's continuation (\\float@ccon)");
        }
        self.package();
    }

    /// `\float@makebox{\columnwidth}` of a restyled type's box `b`: `\vbox{\hsize=#1
    /// \@parboxrestore \@fs@pre`, the caption (`\unvbox\@floatcapt`, unless void) above or
    /// below the box with `\@fs@mid` between, `\par\@fs@post\vskip\z@}`. Nothing in it makes a
    /// paragraph, so the parameters it sets act only on `\fs@boxed`'s `\hsize`.
    fn float_makebox(&mut self, b: u32, style: u8) -> u32 {
        const PT2: i32 = 131072;
        let capt = self.e.eqtb.box_reg(FLOAT_CAPT);
        if capt.is_some() {
            self.e.eqtb.replace_box(FLOAT_CAPT, None);
        }
        let kern = |v: i32| Node { sub: 1, ..Node::new(Kind::Kern, v) };
        let start = self.cur.len();
        // hyperref's `\float@makebox`: `\hsize=#1\relax\hyper@@anchor{\Hy@float@currentHref}
        // {\relax}` first in the box, a vertical list empty so far (`\lastnodetype` -1): the
        // dest and `\nobreak`.
        // caption.sty's (over float.sty's own `\float@makebox`, not hyperref's): `#1\relax
        // \caption@defrostHref`, its `hypcap` anchor (`\caption@startanchor`).
        let cap = self.caption_pkg();
        if let Some(name) = self.fl().href.take() {
            if cap {
                self.caption_start_anchor(name);
            } else {
                self.append_dest(name);
                self.cur.push(Node::new(Kind::Penalty, 10000));
            }
        }
        // caption.sty:885: `\fs@plaintop`'s `\@fs@mid` `\vspace\abovecaptionskip`, `\fs@boxed`'s
        // `\kern\abovecaptionskip`.
        let above = self.rg(R_ABOVECAPTIONSKIP).width;
        match style {
            // `\fs@ruled`: `\hrule height.8pt depth0pt \kern2pt`, the caption, `\kern2pt\hrule
            // \kern2pt`, the box, `\kern2pt\hrule`.
            FS_RULED => {
                let r = self.e.nodes.rule_node([RUNNING, 52429, 0]);
                self.cur.extend([r, kern(PT2)]);
                if let Some(c) = capt {
                    self.unvbox(c);
                    let r = self.e.nodes.rule_node([RUNNING, 26214, 0]);
                    self.cur.extend([kern(PT2), r, kern(PT2)]);
                }
                self.unvbox(b);
                let r = self.e.nodes.rule_node([RUNNING, 26214, 0]);
                self.cur.extend([kern(PT2), r]);
            }
            // `\fs@boxed`: the box framed (`\@fs@pre`), `\kern2pt`, the caption below.
            FS_BOXED => {
                let framed = self.fs_boxed(b);
                self.unvbox(framed);
                if let Some(c) = capt {
                    self.cur.push(kern(if cap { above } else { PT2 }));
                    self.unvbox(c);
                }
            }
            // `\fs@plaintop`: the caption, `\vspace\belowcaptionskip`, the box.
            FS_PLAINTOP => {
                if let Some(c) = capt {
                    self.unvbox(c);
                    self.makebox_vspace(if cap { R_ABOVECAPTIONSKIP } else { R_BELOWCAPTIONSKIP });
                }
                self.unvbox(b);
            }
            // `\fs@plain`: the box, `\vspace\abovecaptionskip`, the caption.
            _ => {
                self.unvbox(b);
                if let Some(c) = capt {
                    self.makebox_vspace(R_ABOVECAPTIONSKIP);
                    self.unvbox(c);
                }
            }
        }
        self.build_glue(Glue::default(), true);
        let v = self.vpack(start, 0, false, MAX_DIMEN);
        self.e.nodes.add_box(BoxRec { vertical: true, ..v })
    }

    /// `\vspace{skip}` in vertical mode (`\@vspace`): `\vskip` of the skip
    /// (`\@vspace@calcify`), then `\vskip\z@skip`.
    fn makebox_vspace(&mut self, r: usize) {
        let g = self.rg(r);
        self.build_glue(g, false);
        self.build_glue(Glue::default(), false);
    }

    /// `\fs@boxed`'s `\@fs@pre`: `\setbox\@currbox\vbox{\hbadness10000 \moveleft3.4pt\vbox{
    /// \advance\hsize by6.8pt \hrule \hbox to\hsize{\vrule\kern3pt \vbox{\kern3pt\box\@currbox
    /// \kern3pt}\kern3pt\vrule}\hrule}}`. Each box comes first or after a rule, so no
    /// interline glue.
    fn fs_boxed(&mut self, b: u32) -> u32 {
        const PT3: i32 = 196608;
        let kern = |v: i32| Node { sub: 1, ..Node::new(Kind::Kern, v) };
        let vbox = |m: &mut Self, start: usize, shift: i32| {
            let v = m.vpack(start, 0, false, MAX_DIMEN);
            m.e.nodes.add_box(BoxRec { vertical: true, shift, ..v })
        };
        let s = self.cur.len();
        self.cur.extend([kern(PT3), Node::new(Kind::Box, b as i32), kern(PT3)]);
        let inner = vbox(self, s, 0);
        let s = self.cur.len();
        let w = self.e.eqtb.word(COLUMN_WIDTH) + 445645;
        let vr = |m: &mut Self| m.e.nodes.rule_node([26214, RUNNING, RUNNING]);
        let r = vr(self);
        self.cur.extend([r, kern(PT3), Node::new(Kind::Box, inner as i32), kern(PT3)]);
        let r = vr(self);
        self.cur.push(r);
        let h = self.hpack(s, w, true);
        let h = self.e.nodes.add_box(h);
        let s = self.cur.len();
        let hr = |m: &mut Self| m.e.nodes.rule_node([RUNNING, 26214, 0]);
        let r = hr(self);
        self.cur.extend([r, Node::new(Kind::Box, h as i32)]);
        let r = hr(self);
        self.cur.push(r);
        let framed = vbox(self, s, -222822);
        let s = self.cur.len();
        self.cur.push(Node::new(Kind::Box, framed as i32));
        vbox(self, s, 0)
    }
}

/// Two columns': article's `\twocolumn[\@maketitle]` natives, in `Meaning::Doc` order after the kernel's (`engine_doc::DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@topnewpage", |m| m.top_new_page_open()),
    ("twocolumn", |m| m.twocolumn()),
    ("rt@topnewpageend", |m| m.top_new_page_close()),
    ("rt@notopfloats", |m| m.fl().topnum = 0),
    // float.sty's commands (`PACKAGE_NATIVES`) and the two halves of `\float@caption`.
    ("rt@newfloat", |m| m.new_float()),
    ("rt@floatstyle", |m| m.float_style()),
    ("rt@restylefloat", |m| m.restyle_float()),
    ("rt@floatname", |m| m.float_name()),
    ("rt@floatplacement", |m| m.float_placement_cmd()),
    ("rt@fcaptmid", |m| m.float_capt_mid()),
    ("rt@fcaptend", |m| m.float_capt_end()),
    ("rt@listof", |m| m.list_of_cmd()),
    // algorithm.sty's `\listofalgorithms`: `\listof{algorithm}{\listalgorithmname}`.
    ("rt@listofalgorithms", |m| {
        let title = vec![Token::cs(m.e.cs.intern("listalgorithmname"))];
        m.list_of_float("algorithm", title);
    }),
];
