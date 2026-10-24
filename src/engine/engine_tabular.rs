//! `tabular` on the GDL path: latex.ltx's `\tabular`, `\@tabular`, `\@array`, `\@mkpream`,
//! `\@tabularcr`, `\hline` and `\endtabular`, lowered onto the engine's alignment.
//!
//! LaTeX builds a preamble of TeX text (`\@mkpream` with `\@tabclassz` and kin, through
//! `\edef`) and runs `\halign` on it, so every entry reads its templates back as tokens.
//! Here the preamble is the same steps as data: [`mkpream`] runs `\@mkpream`'s class
//! automaton over the column specification once and gives each column its ⟨u⟩ and ⟨v⟩
//! steps ([`Cell`]). The alignment is made with a constructed preamble
//! (`init_align_with`) whose templates are one native each (`\rt@tabu`, `\rt@tabv`),
//! which run the steps of the entry's column. The rest of `\@array` and `\endtabular` are
//! builder calls.

use super::super::engine_align::NO_ALIGN_GROUP;
use super::super::engine_box::{fil_glue, Ctx, HBOX_GROUP, VBOX_GROUP, VTOP_GROUP};
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::RUNNING;
use super::super::engine_noad::VCENTER_GROUP;
use super::super::engine_pack::{BASELINE_SKIP, LINE_SKIP};
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode, X, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::END_TEMPLATE_ID;
use super::{ARSTRUT_BOX, CLINE, CLINE_CMD, HLINE, MC_BODY, MC_TAIL, MULTICOLUMN, R_ARRAYRULEWIDTH, R_DOUBLERULESEP, R_TABCOLSEP, TAB_END, TAB_PAR, TAB_SKIP, TAB_U, TAB_V};

/// One step of an entry's templates, as `\@mkpream` writes them for `tabular`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Cell {
    /// `\@arstrut`: `\unhcopy\@arstrutbox` (the first column's ⟨u⟩ starts with it).
    Strut,
    /// `\@acol`: `\hskip\tabcolsep`.
    Acol,
    /// `\@arrayrule`: `\hskip -.5\arrayrulewidth \vrule \@width\arrayrulewidth
    /// \hskip -.5\arrayrulewidth`.
    Rule,
    /// `\@classi` after a rule: `\hskip\doublerulesep`.
    DoubleSep,
    /// `\@classii` after a rule: `\hskip .5\arrayrulewidth`.
    HalfRule,
    /// The entry's own group, `{` and `}`.
    Open,
    Close,
    Hfil,
    /// `\hskip1sp`.
    OneSp,
    Ignore,
    Unskip,
}

/// A preamble: the steps of all columns, and per column where its ⟨u⟩ starts, where `#`
/// is (its ⟨v⟩ starts) and where its ⟨v⟩ ends.
#[derive(Debug, PartialEq)]
pub(super) struct Preamble {
    pub(super) ops: Vec<Cell>,
    pub(super) cols: Vec<[u16; 3]>,
}

/// An open `tabular`: its preamble, the number of open lists when it began (its `\\` is
/// the innermost while no list opened since), and the glue of a pending `\\[...]` whose
/// space is not positive (`\@yargarraycr`'s `\noalign`).
pub(super) struct Tab {
    pub(super) pre: Preamble,
    pub(super) lists: usize,
    skip: Glue,
    /// The one-column preamble of the `\multicolumn` being read.
    mc: Option<Preamble>,
}

/// One item of a column specification as `\@tfor` reads it: a character, or a braced
/// group's contents.
#[derive(Clone, Copy, PartialEq)]
enum Item<'a> {
    Ch(u8),
    Group(&'a [u8]),
}

/// The items of `s` (spaces skipped, as undelimited arguments skip them); `None` for an
/// unbalanced group.
fn items(s: &[u8]) -> Option<Vec<Item<'_>>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'{' => {
                let mut depth = 0;
                let start = i + 1;
                loop {
                    match s.get(i)? {
                        b'{' => depth += 1,
                        b'}' if depth == 1 => break,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                out.push(Item::Group(&s[start..i]));
                i += 1;
            }
            b'}' => return None,
            c => {
                out.push(Item::Ch(c));
                i += 1;
            }
        }
    }
    Some(out)
}

/// `\@expast`: each `*{n}{cols}` replaced by `n` copies of `cols`, the first star first and
/// the result read again (`\@xexpast`), until none is left.
fn expast(spec: &[u8]) -> Option<Vec<u8>> {
    let mut s = spec.to_vec();
    for _ in 0..64 {
        let Some(star) = top_level_star(&s) else { return Some(s) };
        let rest = &s[star + 1..];
        let it = items(rest)?;
        let (count, body) = (it.first()?, it.get(1)?);
        let digits = match count {
            Item::Ch(c) => std::slice::from_ref(c),
            Item::Group(g) => g,
        };
        let digits = std::str::from_utf8(digits).ok()?.trim();
        let n: usize = digits.parse().ok().filter(|n| *n <= 256)?;
        let body: Vec<u8> = match body {
            Item::Ch(c) => vec![*c],
            Item::Group(g) => g.to_vec(),
        };
        // The bytes after the body: skip the two items in the text itself.
        let after = skip_items(rest, 2)?;
        let mut next = s[..star].to_vec();
        for _ in 0..n {
            next.extend_from_slice(&body);
        }
        next.extend_from_slice(&rest[after..]);
        if next.len() > 4096 {
            return None;
        }
        s = next;
    }
    None
}

/// The position of the first `*` outside braces.
fn top_level_star(s: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    for (k, &c) in s.iter().enumerate() {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'*' if depth == 0 => return Some(k),
            _ => {}
        }
    }
    None
}

/// The offset in `s` just past its first `n` items.
fn skip_items(s: &[u8], n: usize) -> Option<usize> {
    let mut i = 0;
    for _ in 0..n {
        while s.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
            i += 1;
        }
        if *s.get(i)? == b'{' {
            let mut depth = 0;
            loop {
                match s.get(i)? {
                    b'{' => depth += 1,
                    b'}' if depth == 1 => break,
                    b'}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
        }
        i += 1;
    }
    Some(i)
}

/// `\@mkpream` for `tabular` (`\@tabclassz`, `\@classi`, `\@classii`, `\@tabclassiv`): the
/// columns of `spec`, or `None` for what the native path does not take: `p` columns, an
/// `@{...}` that is not empty, an unknown character (`\@preamerr`), or no column.
pub(super) fn mkpream(spec: &[u8]) -> Option<Preamble> {
    const AMP: u8 = 0;
    const SHARP: u8 = 1;
    let spec = expast(spec)?;
    // The preamble as a step list with `&` and `#` marks (`None`), `\@firstamptrue`,
    // `\@lastchclass6`.
    let mut list: Vec<Result<Cell, u8>> = vec![Ok(Cell::Strut)];
    let mut first_amp = true;
    let mut last = 6u8;
    // `\@addamp`.
    fn add_amp(list: &mut Vec<Result<Cell, u8>>, first: &mut bool) {
        if !std::mem::replace(first, false) {
            list.push(Err(AMP));
        }
    }
    for item in items(&spec)? {
        // `\@testpach`: the class of the item, and for a column its alignment.
        let ch = match item {
            Item::Ch(c) => Some(c),
            Item::Group([c]) => Some(*c),
            Item::Group(_) => None,
        };
        let (class, num) = if last == 2 {
            (4, 0)
        } else if last == 3 {
            (5, 0)
        } else {
            match ch? {
                b'c' => (0, 0),
                b'l' => (0, 1),
                b'r' => (0, 2),
                b'|' => (1, 0),
                b'@' => (2, 0),
                _ => return None,
            }
        };
        match class {
            0 => {
                match last {
                    0 | 5 => {
                        list.push(Ok(Cell::Acol));
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(Cell::Acol));
                    }
                    1 => {
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(Cell::Acol));
                    }
                    4 => add_amp(&mut list, &mut first_amp),
                    6 => {
                        first_amp = false;
                        list.push(Ok(Cell::Acol));
                    }
                    _ => {}
                }
                // `{\hfil\hskip1sp\ignorespaces\@sharp\unskip\hfil}` for `c`; `l` has no
                // `\hfil` before, `r` none after.
                list.push(Ok(Cell::Open));
                if num != 1 {
                    list.push(Ok(Cell::Hfil));
                }
                list.extend([Ok(Cell::OneSp), Ok(Cell::Ignore), Err(SHARP), Ok(Cell::Unskip)]);
                if num != 2 {
                    list.push(Ok(Cell::Hfil));
                }
                list.push(Ok(Cell::Close));
            }
            1 => match last {
                0 | 5 => list.extend([Ok(Cell::Acol), Ok(Cell::Rule)]),
                1 => list.extend([Ok(Cell::DoubleSep), Ok(Cell::Rule)]),
                4 | 6 => list.push(Ok(Cell::Rule)),
                _ => {}
            },
            2 => {
                if last == 1 {
                    list.push(Ok(Cell::HalfRule));
                }
            }
            // `@{...}`: only an empty one adds nothing to the preamble.
            4 if item == Item::Group(&[]) => {}
            _ => return None,
        }
        last = class;
    }
    match last {
        0 | 5 => list.push(Ok(Cell::Acol)),
        1 | 4 => {}
        _ => return None,
    }
    // Split at the marks: each column one `#`.
    let mut pre = Preamble { ops: Vec::with_capacity(list.len()), cols: Vec::new() };
    let mut col = [0u16, u16::MAX, 0];
    for step in list {
        let at = pre.ops.len() as u16;
        match step {
            Ok(c) => pre.ops.push(c),
            Err(SHARP) if col[1] == u16::MAX => col[1] = at,
            Err(AMP) if col[1] != u16::MAX => {
                col[2] = at;
                pre.cols.push(col);
                col = [at, u16::MAX, 0];
            }
            Err(_) => return None,
        }
    }
    if col[1] == u16::MAX {
        return None;
    }
    col[2] = pre.ops.len() as u16;
    pre.cols.push(col);
    Some(pre)
}

/// The characters of a column specification read as an argument; `None` when it holds a
/// control sequence.
fn spec_bytes(spec: &[Token]) -> Option<Vec<u8>> {
    spec.iter().map(|t| if t.cs_id().is_some() { None } else { u8::try_from(t.chr() as u32).ok() }).collect()
}

/// `.5\arrayrulewidth` as TeX scans it (§455: `xn_over_d(v, 32768, 65536)`, rounding toward
/// zero).
fn half(w: i32) -> i32 {
    (w as i64 * 32768 / 65536) as i32
}

impl Machine<'_, '_> {
    /// `\begin{tabular}`'s `\tabular` after `\begin`'s group: `\leavevmode \hbox \bgroup $`,
    /// then (after `\everymath`) [`Self::tabular_array`].
    pub(super) fn tabular_open(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("tabular in math");
        }
        if self.leave_vmode() {
            return;
        }
        let pos = if self.next_is('[') { self.bracket_arg().first().map(|t| t.chr()) } else { None };
        let mut spec = Vec::new();
        self.read_arg(&mut spec);
        let Some(pre) = spec_bytes(&spec).and_then(|s| mkpream(&s)) else { return self.not_native("tabular preamble") };
        let Some(doc) = self.doc.as_mut() else { return };
        let lists = doc.lists.len();
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), mc: None });
        let group = match pos {
            Some('t') => VTOP_GROUP,
            Some('b') => VBOX_GROUP,
            _ => VCENTER_GROUP,
        };
        doc.tab_group = group;
        self.open_box(HBOX_GROUP, Ctx::Shift(0), false, 0, false);
        // `$`: `\everymath` runs before the rest of `\@tabular`.
        if let Some(doc) = self.doc.as_mut() {
            doc.steps.push(vec![super::Op::Call(|m| m.tabular_array())]);
        }
        let step = self.doc_id(super::STEP);
        self.input.back(step);
        self.enter_text_math();
    }

    /// `\m@th`, then `\@array`: the `\vtop`, `\vbox` or `\vcenter`, `\@arstrutbox`, no
    /// `\lineskip` or `\baselineskip`, and `\ialign` with the preamble.
    fn tabular_array(&mut self) {
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: MATH_SURROUND }, 0, false);
        let group = self.doc.as_ref().map_or(VCENTER_GROUP, |d| d.tab_group);
        self.open_box(group, Ctx::Shift(0), false, 0, false);
        // `\setbox\@arstrutbox\hbox{\vrule \@height\arraystretch\ht\strutbox
        // \@depth\arraystretch\dp\strutbox \@width\z@}`.
        let Some(stretch) = self.array_stretch() else { return self.not_native("an \\arraystretch that is not a number") };
        let (ht, dp) = (stretch(self.strut_height()), stretch(self.strut_depth()));
        self.push_nest(Mode::Horizontal, true, 1000);
        self.build_rule([0, ht, dp]);
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.box_end(Ctx::Set { n: ARSTRUT_BOX, global: false }, Some(idx));
        for slot in [LINE_SKIP, BASELINE_SKIP] {
            self.e.eqtb.set_glue(Reg { region: Region::Glue, slot }, Glue::default(), false);
        }
        // `\def\par{\ifnum\currentgrouptype=6 \else\@@par\fi}`, and `\let\\\@tabularcr`
        // (`\\`'s own native, which finds the tabular innermost).
        if let Some((par, nl)) = self.doc.as_ref().map(|d| (d.par_id, d.newline_id)) {
            self.e.eqtb.set_meaning(par, Meaning::Doc(TAB_PAR as u16), false);
            self.e.eqtb.set_meaning(nl, Meaning::Doc(super::NEWLINE as u16), false);
        }
        let n = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(0, |t| t.pre.cols.len());
        let u = [self.doc_id(TAB_U)];
        let v = [self.doc_id(TAB_V), Token::cs(END_TEMPLATE_ID)];
        let (u, v) = (self.e.toks.store(&u), self.e.toks.store(&v));
        self.init_align_with(n, u, v);
    }

    /// `\arraystretch` as the factor of `\arraystretch\ht\strutbox`: 1 unless the document
    /// redefined it, else its digits read as §448–§455 read a decimal before an internal
    /// dimension (`nx_plus_y(n, v, xn_over_d(v, f, 65536))`, the sign last); `None` for a
    /// body that is not a plain decimal.
    fn array_stretch(&self) -> Option<impl Fn(i32) -> i32> {
        let doc = self.doc.as_ref()?;
        let (mut neg, mut int, mut frac) = (false, 1i64, 0i64);
        if let Meaning::Macro(m) = self.e.eqtb.meaning(doc.stretch_id) {
            let text = self.e.toks.get(m.text);
            let body = text.iter().position(|t| *t == Token::END_MATCH).filter(|&k| k == 0).map(|k| &text[k + 1..])?;
            let bytes: Vec<u8> = body.iter().map(|t| if t.cs_id().is_some() { None } else { u8::try_from(t.chr() as u32).ok() }).collect::<Option<_>>()?;
            let s = std::str::from_utf8(&bytes).ok()?.trim_matches(' ');
            let mut s = s.trim_start_matches(|c: char| {
                if c == '-' {
                    neg = !neg;
                }
                c == '-' || c == '+' || c == ' '
            });
            let digits = s.bytes().take_while(u8::is_ascii_digit).count();
            int = if digits == 0 { 0 } else { s[..digits].parse().ok()? };
            s = &s[digits..];
            if let Some(d) = s.strip_prefix(['.', ',']) {
                if !d.bytes().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                // §452 `round_decimals` of at most 17 digits.
                let mut a = 0i64;
                for c in d.bytes().take(17).rev() {
                    a = (a + (c - b'0') as i64 * 131072) / 10;
                }
                frac = (a + 1) / 2;
            } else if !s.is_empty() || digits == 0 {
                return None;
            }
        }
        Some(move |v: i32| {
            let r = int * v as i64 + v as i64 * frac / 65536;
            (if neg { -r } else { r }) as i32
        })
    }

    /// `\rt@tabu` and `\rt@tabv`: the ⟨u⟩ or ⟨v⟩ steps of the entry's column.
    pub(super) fn tabular_template(&mut self, v: bool) {
        let k = self.align_column();
        self.cell_steps(false, k, v);
    }

    /// The ⟨u⟩ (`v` false) or ⟨v⟩ steps of column `k` of the tabular's preamble, or of the
    /// `\multicolumn`'s (`mc`). The steps are read in place, one byte each.
    fn cell_steps(&mut self, mc: bool, k: usize, v: bool) {
        fn pre<'m>(m: &'m Machine<'_, '_>, mc: bool) -> Option<&'m Preamble> {
            m.doc.as_ref().and_then(|d| d.tabs.last()).and_then(|t| if mc { t.mc.as_ref() } else { Some(&t.pre) })
        }
        let Some([a, b]) = pre(self, mc).and_then(|p| p.cols.get(k)).map(|c| if v { [c[1], c[2]] } else { [c[0], c[1]] }) else { return };
        for i in a as usize..b as usize {
            let Some(op) = pre(self, mc).map(|p| p.ops[i]) else { return };
            self.cell_step(op);
        }
    }

    pub(in crate::engine) fn is_multicolumn(&self, m: Meaning) -> bool {
        m == Meaning::Doc(MULTICOLUMN as u16) && self.in_tabular()
    }

    /// `\multicolumn{n}{spec}{text}` as an entry's first token, after its `\omit`:
    /// `\@multispan`'s `n - 1` times `\span\omit`, then (`\rt@mcbody`) `\begingroup
    /// \@arstrut \@preamble` with `text` at the `#`, and (`\rt@mctail`) the rest of the
    /// preamble, `\hbox{}\endgroup\ignorespaces`.
    pub(in crate::engine) fn multicolumn(&mut self) {
        let (mut n, mut spec, mut text) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut n);
        self.read_arg(&mut spec);
        self.read_arg(&mut text);
        let n = spec_bytes(&n).and_then(|b| std::str::from_utf8(&b).ok()?.trim().parse::<usize>().ok()).filter(|n| (1..=256).contains(n));
        let pre = spec_bytes(&spec).and_then(|s| mkpream(&s)).filter(|p| p.cols.len() == 1);
        let (Some(n), Some(pre)) = (n, pre) else { return self.not_native("a \\multicolumn the native path does not take") };
        let Some(doc) = self.doc.as_mut() else { return };
        if let Some(t) = doc.tabs.last_mut() {
            t.mc = Some(pre);
        }
        let (span, omit) = (Token::cs(doc.span_id), Token::cs(doc.omit_id));
        let mut list = Vec::with_capacity(2 * n + text.len() + 2);
        for _ in 1..n {
            list.extend([span, omit]);
        }
        list.push(self.doc_id(MC_BODY));
        list.extend_from_slice(&text);
        list.push(self.doc_id(MC_TAIL));
        self.back_list(&list);
    }

    /// `\rt@mcbody` and `\rt@mctail`.
    pub(super) fn multicolumn_part(&mut self, tail: bool) {
        if !tail {
            self.begin_group(SEMI_SIMPLE_GROUP);
            return self.cell_steps(true, 0, false);
        }
        self.cell_steps(true, 0, true);
        self.build_hbox(|_| {});
        self.end_group(SEMI_SIMPLE_GROUP);
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.mc = None;
        }
        self.ignore_spaces();
    }

    /// `\cline{i-j}` (`\@cline`) at a row's start: an omitted entry spanning columns 1 to
    /// `i - 1` when `i > 1`, then one spanning `i` to `j` holding `\leaders\hrule
    /// \@height\arrayrulewidth\hfill`, `\cr`, and `\noalign{\vskip-\arrayrulewidth}`.
    fn cline(&mut self) -> Option<bool> {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let range = spec_bytes(&arg).and_then(|b| {
            let s = String::from_utf8(b).ok()?;
            let (i, j) = s.split_once('-')?;
            Some((i.trim().parse::<usize>().ok()?, j.trim().parse::<usize>().ok()?))
        });
        let Some((i, j)) = range.filter(|&(i, j)| i >= 1 && j >= i && j <= 256) else {
            self.not_native("a \\cline the native path does not take");
            return Some(false);
        };
        let w = self.rw(R_ARRAYRULEWIDTH);
        let doc = self.doc.as_mut()?;
        let (span, omit, cr) = (Token::cs(doc.span_id), Token::cs(doc.omit_id), Token::cs(doc.cr_id));
        if let Some(t) = doc.tabs.last_mut() {
            t.skip = Glue { width: -w, ..Glue::default() };
        }
        let mut list = Vec::with_capacity(2 * j + 4);
        list.push(omit);
        if i > 1 {
            for _ in 2..i {
                list.extend([span, omit]);
            }
            list.extend([Token::char(super::super::engine_catcode::Cat::AlignTab, '&'), omit]);
        }
        for _ in i..j {
            list.extend([span, omit]);
        }
        list.extend([self.doc_id(CLINE), cr, self.doc_id(TAB_SKIP)]);
        self.back_list(&list);
        Some(true)
    }

    /// `\cline`'s `\leaders\hrule\@height\arrayrulewidth\hfill` (rule leaders are glue of
    /// subtype 110 holding the rule's index, as `scan_box` makes them).
    pub(super) fn cline_rule(&mut self) {
        let w = self.rw(R_ARRAYRULEWIDTH);
        let r = self.e.nodes.rule_node([RUNNING, w, 0]);
        let mut n = self.e.nodes.glue_node(fil_glue(Prim::Hfill), 110);
        n.b = r.a;
        self.cur.push(n);
    }

    fn cell_step(&mut self, op: Cell) {
        let hskip = |m: &mut Self, w: i32| m.build_glue(Glue { width: w, ..Glue::default() }, true);
        match op {
            Cell::Strut => {
                if let Some(b) = self.e.eqtb.box_reg(ARSTRUT_BOX) {
                    let list = self.e.nodes.boxes[b as usize].list;
                    let mut cur = std::mem::take(&mut self.cur);
                    self.e.nodes.copy_into(list, &mut cur);
                    self.cur = cur;
                }
            }
            Cell::Acol => hskip(self, self.rw(R_TABCOLSEP)),
            Cell::Rule => {
                let w = self.rw(R_ARRAYRULEWIDTH);
                hskip(self, -half(w));
                self.build_rule([w, RUNNING, RUNNING]);
                hskip(self, -half(w));
            }
            Cell::DoubleSep => hskip(self, self.rw(R_DOUBLERULESEP)),
            Cell::HalfRule => hskip(self, half(self.rw(R_ARRAYRULEWIDTH))),
            Cell::Open => self.begin_group(SIMPLE_GROUP),
            Cell::Close => self.end_group(SIMPLE_GROUP),
            Cell::Hfil => {
                let n = self.e.nodes.glue_node(fil_glue(Prim::Hfil), 0);
                self.cur.push(n);
            }
            Cell::OneSp => hskip(self, 1),
            Cell::Ignore => self.ignore_spaces(),
            Cell::Unskip => self.unskip(),
        }
    }

    /// Whether `\\` is the innermost `tabular`'s `\@tabularcr`.
    pub(super) fn in_tabular(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.tabs.last().is_some_and(|t| t.lists == d.lists.len()))
    }

    /// `\@tabularcr`: `{\ifnum0=`}\fi\@ifstar\@xtabularcr\@xtabularcr`, the brace hiding a
    /// following `&` from the alignment while `*` and `[` are looked for; then `\cr`, after
    /// `\@xargarraycr`'s strut for a positive space, or before `\@yargarraycr`'s
    /// `\noalign{\@vspace@calcify{#1}}` for another.
    pub(super) fn tabular_cr(&mut self) {
        self.input.align_state += 1;
        let _ = self.next_is('*');
        let arg = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        self.input.align_state -= 1;
        let cr = Token::cs(self.doc.as_ref().map_or(0, |d| d.cr_id));
        let Some(arg) = arg else { return self.input.back(cr) };
        // `\ifdim #1>\z@`.
        let relax = self.doc_id_named("relax");
        let mut list = arg.clone();
        list.push(relax);
        self.back_list(&list);
        let d = self.scan_dimen(false).0;
        if self.get_token() != Some(relax) {
            return self.not_native("\\\\[...] that is not a dimension");
        }
        if d > 0 {
            // `\unskip`, then `\@tempdima #1 \advance\@tempdima \dp\@arstrutbox \vrule
            // \@height\z@ \@depth\@tempdima \@width\z@`.
            self.unskip();
            let dp = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].depth);
            self.build_rule([0, 0, d + dp]);
            return self.input.back(cr);
        }
        let Some(g) = self.glue_arg(&arg) else { return self.not_native("\\\\[...] that is not glue") };
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.skip = g;
        }
        let list = [cr, self.doc_id(TAB_SKIP)];
        self.back_list(&list);
    }

    /// What the next entry's first token (`x`, §785) expands to in LaTeX when it is a native:
    /// `Some(true)` to look again, `Some(false)` when done, `None` for a row's first token.
    pub(in crate::engine) fn align_peek_native(&mut self, x: X) -> Option<bool> {
        let doc = self.doc.as_ref()?;
        doc.tabs.last()?;
        if x.t == Token::cs(doc.par_id) {
            // `\def\par{\ifnum\currentgrouptype=6 \else\@@par\fi}`: between rows, nothing.
            return Some(true);
        }
        let Meaning::Doc(i) = x.m else { return None };
        match i as usize {
            super::END => {
                self.end_env();
                Some(true)
            }
            HLINE => {
                self.hline();
                Some(false)
            }
            CLINE_CMD => self.cline(),
            TAB_SKIP => {
                let g = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(Glue::default(), |t| t.skip);
                self.begin_group(NO_ALIGN_GROUP);
                self.normal_paragraph();
                self.build_glue(g, false);
                self.end_no_align();
                Some(false)
            }
            TAB_END => {
                self.tabular_close();
                Some(false)
            }
            _ => None,
        }
    }

    /// `\hline`: `\noalign{\ifnum0=`}\fi\hrule \@height\arrayrulewidth \futurelet
    /// \reserved@a\@xhline`, and for a second `\hline` next `\vskip\doublerulesep
    /// \vskip-\arrayrulewidth`, then the `\noalign`'s `}`.
    fn hline(&mut self) {
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let w = self.rw(R_ARRAYRULEWIDTH);
        self.build_rule([RUNNING, w, 0]);
        if let Some(t) = self.get_token() {
            let again = self.meaning(t) == Meaning::Doc(HLINE as u16);
            self.input.back(t);
            if again {
                let sep = self.rw(R_DOUBLERULESEP);
                self.build_glue(Glue { width: sep, ..Glue::default() }, true);
                self.build_glue(Glue { width: -w, ..Glue::default() }, true);
            }
        }
        self.end_no_align();
    }

    /// `\end{tabular}`: `\endtabular` (`\crcr\egroup\egroup $\egroup`), then `\end`'s
    /// `\endgroup`, the `\crcr` ending an entry left open.
    pub(super) fn tabular_end(&mut self) {
        let crcr = Token::cs(self.doc.as_ref().map_or(0, |d| d.crcr_id));
        let list = [crcr, self.doc_id(TAB_END)];
        self.back_list(&list);
    }

    /// After the alignment's last row: its `}`, the box's, the formula's `$`, the `\hbox`'s,
    /// and `\end`'s `\endgroup`.
    fn tabular_close(&mut self) {
        self.fin_align();
        self.package();
        self.after_math();
        self.package();
        self.end_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.tabs.pop();
        }
    }
}

/// §247 `math_surround_code`.
const MATH_SURROUND: u32 = 1;

#[cfg(test)]
mod tests {
    use super::Cell::*;
    use super::*;

    fn cols(spec: &str) -> Option<Vec<(Vec<Cell>, Vec<Cell>)>> {
        let p = mkpream(spec.as_bytes())?;
        Some(p.cols.iter().map(|c| (p.ops[c[0] as usize..c[1] as usize].to_vec(), p.ops[c[1] as usize..c[2] as usize].to_vec())).collect())
    }

    #[test]
    fn mkpream_follows_the_class_automaton() {
        // `c`: `\@arstrut\hskip\tabcolsep{\hfil\hskip1sp\ignorespaces # \unskip\hfil}\hskip\tabcolsep`.
        assert_eq!(cols("c"), Some(vec![(vec![Strut, Acol, Open, Hfil, OneSp, Ignore], vec![Unskip, Hfil, Close, Acol])]));
        // `|l|r|`: rules, and a column after a rule has its own `\@acol`.
        assert_eq!(
            cols("|l|r|"),
            Some(vec![(vec![Strut, Rule, Acol, Open, OneSp, Ignore], vec![Unskip, Hfil, Close, Acol, Rule]), (vec![Acol, Open, Hfil, OneSp, Ignore], vec![Unskip, Close, Acol, Rule])])
        );
        // `||`: `\doublerulesep` between; `@{}`: no `\tabcolsep` on that side.
        assert_eq!(cols("@{}c||c@{}").map(|c| c.len()), Some(2));
        assert_eq!(cols("@{}c@{}"), Some(vec![(vec![Strut, Open, Hfil, OneSp, Ignore], vec![Unskip, Hfil, Close])]));
        assert!(cols("c||c").unwrap()[0].1.contains(&DoubleSep));
        // `*{3}{c}` is `ccc`; `*{2}{|c}|` is `|c|c|`.
        assert_eq!(cols("*{3}{c}"), cols("ccc"));
        assert_eq!(cols("*{2}{|c}|"), cols("|c|c|"));
        assert_eq!(cols("{c} c"), cols("cc"));
        // Not native: `p{...}`, a non-empty `@{...}`, an unknown column, no column.
        assert_eq!(cols("p{2cm}"), None);
        assert_eq!(cols("@{:}c"), None);
        assert_eq!(cols("x"), None);
        assert_eq!(cols("|"), None);
    }
}
