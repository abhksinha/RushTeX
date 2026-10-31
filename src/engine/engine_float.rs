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
use super::{Op, COLUMN_WIDTH, COLUMNSEP, COLUMNSEPRULE, DEFAULT_COLOR, R_DBLFLOATSEP, R_DBLTEXTFLOATSEP, R_DBLTOPNUMBER, CAPTION_END, CAPTION_MID, FOOTINS, MINIPAGE, NEWLINE, NORMALSIZE, R_ABOVECAPTIONSKIP, R_BELOWCAPTIONSKIP, R_BOTTOMNUMBER, R_FIGURE, R_FLOATSEP, R_FPBOT, R_FPSEP, R_FPTOP, R_INTEXTSEP, R_TEXTFLOATSEP, R_TOPNUMBER, R_TOTALNUMBER, TEMP_BOX};

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
    caption: Vec<Token>,
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
}

impl Default for Floats {
    fn default() -> Self {
        let mut free = Queue::default();
        for k in 0..SLOTS as u8 {
            free.push(k);
        }
        Floats { boxes: [0; SLOTS], count: [0; SLOTS], free, cur: Queue::default(), top: Queue::default(), bot: Queue::default(), mid: Queue::default(), defer: Queue::default(), hold: None, colht: 0, colroom: 0, toproom: 0, botroom: 0, textfloatsheight: 0, topnum: 0, botnum: 0, colnum: 0, penalty: 0, open: 0, captype: 0, savsk: 0, savsf: 0, ignore: false, saved: (false, false, 0), minipage: false, clearing: false, caption: Vec::new(), here: false, here_box: 0, twocol: false, first: true, left: None, dbltop: None, dbltopnum: 0 }
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

    fn flr(&self) -> &Floats {
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
        let mut fps: Vec<u8> = Vec::new();
        if self.next_is('[') {
            let arg = self.bracket_arg();
            for t in arg {
                match u8::try_from(t.chr() as u32) {
                    Ok(c) if t.cs_id().is_none() => fps.push(c),
                    _ => return self.not_native("a float placement of commands"),
                }
            }
            fps.retain(|c| *c != b' ');
        }
        // float.sty's `\@xfloat#1[`: `[H]` is `\@float@HH`, the box set in place.
        if fps == b"H" && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"float")) {
            return self.float_here_open(ftype);
        }
        // `\@fpsadddefault`: an empty placement or a lone `!` takes `\fps@<type>` (`tbp`).
        if fps.is_empty() || fps == b"!" {
            fps.extend_from_slice(b"tbp");
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
        f.penalty = if hmode { -10002 } else { -10003 };
        // `\global\setbox\@currbox \color@vbox \normalcolor \vbox\bgroup`.
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_BOX, global: true }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            // `\normalcolor`: `\let\current@color\default@color \set@color`.
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.color_stack(Some(super::DEFAULT_COLOR));
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
        self.open_box(VBOX_GROUP, Ctx::Set { n: FLOAT_BOX, global: false }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.color_stack(Some(super::DEFAULT_COLOR));
        }
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        let w = self.e.eqtb.word(COLUMN_WIDTH);
        self.set_dimen_named(param!("hsize"), w);
        self.par_box_restore();
        self.normal_font();
        self.size_command(NORMALSIZE);
        self.set_minipage();
    }

    /// `\end{figure}` and kin: `\@endfloatbox`, then `\end@float`, then `\end`'s
    /// `\endgroup`.
    pub(super) fn float_close(&mut self) {
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
        let r = R_FIGURE + t as usize - 1;
        let n = self.rw(r) + 1;
        self.set_rw(r, n, true);
        // hyperref: `\H@refstepcounter` (no anchor here) and `\hyper@makecurrent{figure}`;
        // the anchor goes in the caption text (`caption_tokens`).
        // hyperref names the anchor by `\theHfigure` (`float_href`), not `\thefigure`.
        let prefix = self.doc.as_ref().map_or(String::new(), |d| d.the_prefix[t as usize - 1].clone());
        if self.hyper() {
            self.store_current_label(&format!("{prefix}{n}"));
            let name = self.float_href(t, n);
            self.set_current_href(&name);
        } else {
            self.set_current_label(&format!("{prefix}{n}"));
        }
        if self.next_is('[') {
            self.bracket_arg();
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        self.fl().caption = text;
        self.run_ops(vec![Op::Par, Op::Call(|m| m.caption_body())]);
    }

    /// The tokens of `#1: #2` (`\fnum@<type>`: `\figurename\nobreakspace\thefigure`).
    fn caption_tokens(&mut self, out: &mut Vec<Token>) {
        let t = self.flr().captype;
        let name = if t == 1 { (named!("figurename"), "Figure") } else { (named!("tablename"), "Table") };
        let id = self.named_id(name.0);
        if matches!(self.e.eqtb.meaning(id), Meaning::Macro(_)) {
            out.push(Token::cs(id));
        } else {
            out.extend(name.1.chars().map(|c| Token::char(super::Cat::Letter, c)));
        }
        out.push(self.named_cs(named!("nobreakspace")));
        let n = self.rw(R_FIGURE + t as usize - 1);
        let prefix = self.doc.as_ref().map_or(String::new(), |d| d.the_prefix[t as usize - 1].clone());
        out.extend(prefix.chars().map(|c| Token::char(if c.is_ascii_alphabetic() { super::Cat::Letter } else { super::Cat::Other }, c)));
        out.extend(n.to_string().chars().map(|c| Token::char(super::Cat::Other, c)));
        out.extend([Token::char(super::Cat::Other, ':'), Token::char(super::Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
        // hyperref's `\@caption`: `\Hy@raisedlink{\hyper@@anchor{figure.N}{\relax}}` first.
        if self.hyper() {
            let name = self.float_href(t, n);
            let t = self.text_raised_link(name);
            out.extend(t);
        }
        out.extend_from_slice(&self.flr().caption);
    }

    /// The rest of `\@caption`: the `\addcontentsline` write, `\begingroup \@parboxrestore
    /// \if@minipage\@setminipage\fi \normalsize`, and article's `\@makecaption` up to
    /// `\sbox\@tempboxa{#1: #2}`.
    fn caption_body(&mut self) {
        self.aux_write(0);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.par_box_restore();
        if self.flr().minipage {
            self.set_minipage();
        }
        self.size_command(NORMALSIZE);
        let g = self.rg(R_ABOVECAPTIONSKIP);
        self.build_glue(g, false);
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
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(TEMP_BOX).unwrap_or(0);
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
        self.caption_end();
    }

    /// `\vskip\belowcaptionskip`, `\@caption`'s `\par`, `\endgroup`.
    pub(super) fn caption_end(&mut self) {
        let g = self.rg(R_BELOWCAPTIONSKIP);
        self.build_glue(g, false);
        self.par();
        self.end_group(SEMI_SIMPLE_GROUP);
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
            self.color_stack(Some(DEFAULT_COLOR));
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
            self.color_stack(Some(super::DEFAULT_COLOR));
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
        if self.mode != Mode::Vertical {
            return self.run_ops(vec![Op::Par, Op::Call(|m| m.top_new_page_close())]);
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
        doc.special = Some(false);
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
