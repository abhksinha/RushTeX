//! pdfTeX's character protrusion and font expansion (pdftex.web): the per-font codes
//! `\lpcode`, `\rpcode` and `\efcode`, and `\pdffontexpand` with the expanded copies of a
//! font it makes. Codes live in a side table, allocated only for a font that sets one;
//! an expanded copy shares its base's metrics (`Font::expand` scales them on use).

use super::engine_font::{Font, Fonts};
use super::engine_node::{Kind, Node, Nodes, MARGIN_LEFT, MARGIN_RIGHT};
use super::engine_run::Machine;
use super::engine_value::Value;
use super::engine_whatsit::sized;
use crate::backend::scaled::round_xn_over_d;

/// One font's codes (pdftex.web `pdf_font_lp_base`, `rp_base`, `ef_base`).
pub struct Codes {
    pub lp: [i16; 256],
    pub rp: [i16; 256],
    pub ef: [i16; 256],
}

/// A font's expansion links (pdftex.web `pdf_font_blink`, `elink`, `stretch`, `shrink`,
/// `step`, `auto_expand`); font 0 (`null_font`) means none.
#[derive(Clone, Copy, Default, Debug)]
pub struct Expand {
    /// The base of an expanded copy.
    pub blink: u16,
    /// The next expanded copy of the same base.
    pub elink: u16,
    /// The copies at the stretch and shrink limits.
    pub stretch: u16,
    pub shrink: u16,
    pub step: u16,
    pub auto: bool,
}

/// The side table, indexed by font.
#[derive(Default)]
pub struct Micro {
    codes: Vec<Option<Box<Codes>>>,
    expand: Vec<Expand>,
}

impl std::fmt::Debug for Micro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Micro({} fonts with codes)", self.codes.iter().flatten().count())
    }
}

impl Micro {
    /// The font whose codes `f` uses: its base when it is an expanded copy.
    #[inline]
    fn owner(&self, f: u16) -> u16 {
        match self.expand.get(f as usize) {
            Some(e) if e.blink != 0 => e.blink,
            _ => f,
        }
    }

    #[inline]
    fn codes(&self, f: u16) -> Option<&Codes> {
        self.codes.get(self.owner(f) as usize).and_then(|c| c.as_deref())
    }

    /// `\lpcode` (0 by default).
    #[inline]
    pub fn lp(&self, f: u16, c: u8) -> i32 {
        self.codes(f).map_or(0, |k| k.lp[c as usize] as i32)
    }

    /// `\rpcode` (0 by default).
    #[inline]
    pub fn rp(&self, f: u16, c: u8) -> i32 {
        self.codes(f).map_or(0, |k| k.rp[c as usize] as i32)
    }

    /// `\efcode` (1000 by default).
    #[inline]
    pub fn ef(&self, f: u16, c: u8) -> i32 {
        self.codes(f).map_or(1000, |k| k.ef[c as usize] as i32)
    }

    /// The expansion links of `f`.
    #[inline]
    pub fn expand(&self, f: u16) -> Expand {
        self.expand.get(f as usize).copied().unwrap_or_default()
    }

    fn expand_mut(&mut self, f: u16) -> &mut Expand {
        if self.expand.len() <= f as usize {
            self.expand.resize(f as usize + 1, Expand::default());
        }
        &mut self.expand[f as usize]
    }

    /// pdftex.web `set_lp_code` and kin: the clamped value, the table made on first use.
    fn set(&mut self, f: u16, which: u8, c: u8, v: i32) {
        let f = self.owner(f) as usize;
        if self.codes.len() <= f {
            self.codes.resize_with(f + 1, || None);
        }
        let k = self.codes[f].get_or_insert_with(|| Box::new(Codes { lp: [0; 256], rp: [0; 256], ef: [1000; 256] }));
        match which {
            LP => k.lp[c as usize] = v.clamp(-1000, 1000) as i16,
            RP => k.rp[c as usize] = v.clamp(-1000, 1000) as i16,
            _ => k.ef[c as usize] = v.clamp(0, 1000) as i16,
        }
    }
}

const LP: u8 = 0;
const RP: u8 = 1;
const EF: u8 = 2;

fn code_kind(name: &str) -> Option<u8> {
    match name {
        "lpcode" => Some(LP),
        "rpcode" => Some(RP),
        "efcode" => Some(EF),
        _ => None,
    }
}

impl Machine<'_, '_> {
    /// The protrusion and expansion commands among the long-tail primitives; `false` for
    /// any other.
    pub(super) fn micro_command(&mut self, name: &str) -> bool {
        if let Some(which) = code_kind(name) {
            // pdftex.web §1431 (`assign_font_int`): font, character, `=`, value; global.
            let f = self.scan_font_ident();
            let c = self.scan_char_num();
            self.scan_optional_equals();
            let v = self.scan_int();
            self.e.fonts.micro.set(f, which, c, v);
            self.after_assignment();
            return true;
        }
        if name == "pdffontexpand" {
            self.read_expand_font();
            return true;
        }
        false
    }

    /// `\the\lpcode\font`c` and kin.
    pub(super) fn micro_value(&mut self, name: &str) -> Option<Value> {
        let which = code_kind(name)?;
        let f = self.scan_font_ident();
        let c = self.scan_char_num();
        let m = &self.e.fonts.micro;
        Some(Value::Int(match which {
            LP => m.lp(f, c),
            RP => m.rp(f, c),
            _ => m.ef(f, c),
        }))
    }

    /// pdftex.web `read_expand_font`: `\pdffontexpand` font stretch shrink step
    /// [`autoexpand`].
    fn read_expand_font(&mut self) {
        let f = self.scan_font_ident();
        if f == 0 {
            self.error("pdfTeX error (font expansion): invalid font identifier");
        }
        if self.e.fonts.micro.expand(f).blink != 0 {
            self.error("pdfTeX error (font expansion): \\pdffontexpand cannot be used this way (the base font has been expanded)");
        }
        self.scan_optional_equals();
        let stretch = self.scan_int().clamp(0, 1000);
        let shrink = self.scan_int().clamp(0, 500);
        let step = self.scan_int().clamp(0, 100);
        if step == 0 {
            return self.error("pdfTeX error (font expansion): invalid step");
        }
        let stretch = stretch - stretch % step;
        let shrink = shrink - shrink % step;
        if stretch == 0 && shrink == 0 {
            self.error("pdfTeX error (font expansion): invalid limit(s)");
        }
        let auto = self.scan_keyword("autoexpand");
        if auto {
            self.skip_one_space();
        }
        if self.e.fonts.list[f as usize].expand != 0 {
            return self.error("pdfTeX error (font expansion): this font has been expanded by another font so it cannot be used now");
        }
        let x = self.e.fonts.micro.expand(f);
        if x.step != 0 {
            // Already expanded: the parameters must agree.
            let ratio = |k: u16| if k == 0 { 0 } else { self.e.fonts.list[k as usize].expand as i32 };
            let (have_stretch, have_shrink) = (ratio(x.stretch), -ratio(x.shrink));
            if x.step as i32 != step {
                self.error("pdfTeX error (font expansion): font has been expanded with different expansion step");
            }
            if have_stretch != stretch {
                self.error("pdfTeX error (font expansion): font has been expanded with different stretch limit");
            }
            if have_shrink != shrink {
                self.error("pdfTeX error (font expansion): font has been expanded with different shrink limit");
            }
            if x.auto != auto {
                self.error("pdfTeX error (font expansion): font has been expanded with different auto expansion value");
            }
            return;
        }
        self.set_expand_params(f, auto, stretch, shrink, step);
    }

    /// pdftex.web `set_expand_params`: font `f` expands by up to `stretch` and `shrink`
    /// thousandths in steps of `step`, through copies made now.
    pub(super) fn set_expand_params(&mut self, f: u16, auto: bool, stretch: i32, shrink: i32, step: i32) {
        let m = self.e.fonts.micro.expand_mut(f);
        m.step = step as u16;
        m.auto = auto;
        if stretch > 0 {
            let k = self.get_expand_font(f, stretch);
            self.e.fonts.micro.expand_mut(f).stretch = k;
        }
        if shrink > 0 {
            let k = self.get_expand_font(f, -shrink);
            self.e.fonts.micro.expand_mut(f).shrink = k;
        }
    }

    /// pdftex.web `get_expand_font`: the copy of `f` expanded by `e` thousandths (a
    /// nonzero multiple of its step), made on first use.
    pub(super) fn get_expand_font(&mut self, f: u16, e: i32) -> u16 {
        let mut k = self.e.fonts.micro.expand(f).elink;
        while k != 0 {
            if self.e.fonts.list[k as usize].expand as i32 == e {
                return k;
            }
            k = self.e.fonts.micro.expand(k).elink;
        }
        let k = self.load_expand_font(f, e);
        let elink = self.e.fonts.micro.expand(f).elink;
        self.e.fonts.micro.expand_mut(k).elink = elink;
        self.e.fonts.micro.expand_mut(f).elink = k;
        k
    }

    /// pdftex.web `load_expand_font` with `auto_expand_font` and `copy_expand_params`: a
    /// font `name+e` (or `name-e`) of the same size, found if loaded, else the base's copy
    /// scaled by `1000 + e` over 1000. A non-automatic expansion would read that TFM file,
    /// which the engine's font loader does not do; the copy stands in for it.
    fn load_expand_font(&mut self, f: u16, e: i32) -> u16 {
        let base = &self.e.fonts.list[f as usize];
        let name = format!("{}{}{e}", base.name, if e > 0 { "+" } else { "" });
        let size = base.size;
        let k = match self.e.fonts.find(&name, size) {
            Some(k) => k,
            None => self.e.fonts.push_expanded(f, std::borrow::Cow::Owned(name), e as i16),
        };
        // `copy_expand_params`.
        let (step, auto) = {
            let b = self.e.fonts.micro.expand(f);
            (b.step, b.auto)
        };
        self.e.fonts.list[k as usize].expand = e as i16;
        let m = self.e.fonts.micro.expand_mut(k);
        m.blink = f;
        m.step = step;
        m.auto = auto;
        k
    }

    /// pdftex.web `fix_expand_value`: `e` limited to `f`'s range and rounded to its step.
    pub(super) fn fix_expand_value(&self, f: u16, e: i32) -> i32 {
        if e == 0 {
            return 0;
        }
        let x = self.e.fonts.micro.expand(f);
        let (neg, mut e, max) = if e < 0 {
            (true, -e, if x.shrink == 0 { 0 } else { -(self.e.fonts.list[x.shrink as usize].expand as i32) })
        } else {
            (false, e, if x.stretch == 0 { 0 } else { self.e.fonts.list[x.stretch as usize].expand as i32 })
        };
        let step = x.step.max(1) as i32;
        if e > max {
            e = max;
        } else if e % step > 0 {
            e = step * round_xn_over_d(e, 1, step);
        }
        if neg {
            -e
        } else {
            e
        }
    }

    /// pdftex.web `expand_font`: `f` expanded by `e` thousandths, any value in its range.
    pub(super) fn expand_font(&mut self, f: u16, e: i32) -> u16 {
        let e = self.fix_expand_value(f, e);
        if e == 0 {
            return f;
        }
        self.get_expand_font(f, e)
    }
}

/// pdftex.web `cp_skipable`: what character protrusion looks through at a margin. A glue
/// node counts when its value is zero (pdfTeX asks for the shared `zero_glue`, which every
/// zero parameter and register holds).
pub(super) fn cp_skipable(nodes: &Nodes, n: &Node) -> bool {
    match n.kind {
        Kind::Ins | Kind::Mark | Kind::Adjust | Kind::Penalty => true,
        Kind::Whatsit => !sized(n.sub),
        Kind::Disc => n.a == -1 && n.sub == 0,
        Kind::Math => n.a == 0,
        Kind::Kern => !n.is_margin_kern() && (n.a == 0 || n.sub == 0 || n.sub == 3),
        // `glue_ptr = zero_glue`: a zero glue that is not a new specification (`font` 1
        // marks an explicit `\hskip 0pt`).
        Kind::Glue => n.font == 0 && nodes.glue(n) == Default::default(),
        Kind::Box => {
            let b = &nodes.boxes[n.a as usize];
            !b.vertical && b.width == 0 && b.height == 0 && b.depth == 0 && b.list.0 == b.list.1
        }
        _ => false,
    }
}

/// Where a margin search ends: the node found, or every node skipped.
enum Found {
    Node(Node),
    Skipped,
}

/// A non-empty horizontal box's list.
fn hlist<'n>(nodes: &'n Nodes, n: &Node) -> Option<&'n [Node]> {
    if n.kind != Kind::Box {
        return None;
    }
    let b = &nodes.boxes[n.a as usize];
    (!b.vertical && b.list.0 != b.list.1).then(|| nodes.get(b.list))
}

/// pdftex.web `find_protchar_left` and `find_protchar_right`: the first node from the
/// left (or the right) that is not skipped, looking inside horizontal boxes.
fn protchar(nodes: &Nodes, list: &[Node], right: bool) -> Found {
    let step = |n: &Node| match hlist(nodes, n) {
        Some(inner) => protchar(nodes, inner, right),
        None if cp_skipable(nodes, n) => Found::Skipped,
        None => Found::Node(*n),
    };
    let found = if right { list.iter().rev().find_map(|n| match step(n) { Found::Node(x) => Some(x), Found::Skipped => None }) } else { list.iter().find_map(|n| match step(n) { Found::Node(x) => Some(x), Found::Skipped => None }) };
    found.map_or(Found::Skipped, Found::Node)
}

/// pdftex.web `char_pw`: how far the character (or ligature) `n` protrudes into the
/// margin, `\\lpcode` or `\\rpcode` thousandths of its font's quad.
pub(super) fn char_pw(fonts: &Fonts, n: Option<Node>, right: bool) -> i32 {
    let Some(n) = n.filter(|n| matches!(n.kind, Kind::Char | Kind::Lig)) else { return 0 };
    let c = if right { fonts.micro.rp(n.font, n.a as u8) } else { fonts.micro.lp(n.font, n.a as u8) };
    if c == 0 {
        return 0;
    }
    let quad = fonts.params(n.font).get(5).copied().unwrap_or(0);
    round_xn_over_d(quad, c, 1000)
}

/// The protruding node at the right end of `line` (pdftex.web `find_protchar_right`).
pub(super) fn protchar_right(nodes: &Nodes, line: &[Node]) -> Option<Node> {
    match protchar(nodes, line, true) {
        Found::Node(n) => Some(n),
        Found::Skipped => None,
    }
}

fn empty_hbox(nodes: &Nodes, n: &Node) -> bool {
    n.kind == Kind::Box && {
        let b = &nodes.boxes[n.a as usize];
        !b.vertical && b.width == 0 && b.height == 0 && b.depth == 0 && b.list.0 == b.list.1
    }
}

/// The protruding node at the left end of `line` (pdftex.web `find_protchar_left`): a
/// first empty box (`\\parindent` 0pt) is passed over; else, with `discard`, the
/// discardable nodes after a break are (glue, kerns, penalties, math nodes).
pub(super) fn protchar_left(nodes: &Nodes, line: &[Node], discard: bool) -> Option<Node> {
    let mut i = 0;
    if line.len() > 1 && empty_hbox(nodes, &line[0]) {
        i = 1;
    } else if discard {
        while i + 1 < line.len() && matches!(line[i].kind, Kind::Glue | Kind::Kern | Kind::Penalty | Kind::Math) {
            i += 1;
        }
    }
    match protchar(nodes, &line[i.min(line.len())..], false) {
        Found::Node(n) => Some(n),
        Found::Skipped => None,
    }
}

impl Machine<'_, '_> {
    /// pdftex.web `new_margin_kern`: `-w` at the margin, remembering the character.
    pub(super) fn margin_kern(w: i32, ch: Node, right: bool) -> Node {
        Node { kind: Kind::Kern, sub: if right { MARGIN_RIGHT } else { MARGIN_LEFT }, font: ch.font, a: -w, b: ch.a }
    }

    /// `\leftmarginkern` and `\rightmarginkern` of box `list`: the margin kern at that
    /// end, past skipped nodes and `\leftskip` (`\rightskip`), if there is one.
    pub(super) fn margin_kern_of(&self, list: &[Node], right: bool) -> Option<i32> {
        let nodes = &self.e.nodes;
        let skip_code = if right { 9 } else { 8 };
        let pass = |n: &&Node| cp_skipable(nodes, n) || (n.kind == Kind::Glue && n.sub == skip_code);
        let n = if right { list.iter().rev().find(|n| !pass(n)) } else { list.iter().find(|n| !pass(n)) };
        match n {
            Some(n) if n.is_margin_kern() && (n.sub == MARGIN_RIGHT) == right => Some(n.a),
            _ => None,
        }
    }
}

/// pdftex.web `get_kern`: the kern font `f`'s program puts between `lc` and `rc` (0 for
/// none), scaled to the font, expanded copies included.
fn get_kern(font: &Font, info: &[i32], lc: u8, rc: u8) -> i32 {
    use super::engine_font::{restart, STOP_FLAG};
    let Some(mut k) = font.metrics.lig_kern_start(lc) else { return 0 };
    let (prog, _) = font.metrics.lig_kern();
    while let Some(j) = prog.try_get(k) {
        let (skip, next, op) = ((j >> 24) as u8, (j >> 16 & 0xFF) as u8, (j >> 8 & 0xFF) as u8);
        if next == rc && skip <= STOP_FLAG && op >= STOP_FLAG {
            return font.expanded(font.kern(restart(j) - 256 * STOP_FLAG as usize, info));
        }
        if skip >= STOP_FLAG {
            return 0;
        }
        k += skip as usize + 1;
    }
    0
}

/// The character a node shows (a ligature's own), for expansion.
#[inline]
fn shown_char(n: &Node) -> Option<(u16, u8)> {
    matches!(n.kind, Kind::Char | Kind::Lig).then(|| (n.font, n.a as u8))
}

impl Machine<'_, '_> {
    fn char_width(&self, f: u16, c: u8) -> i32 {
        self.e.fonts.dims(f, c).map_or(0, |d| d[0])
    }

    /// pdftex.web `char_stretch` (`shrink` false) and `char_shrink`: what the character
    /// gains at its font's expansion limit, scaled by its `\efcode`.
    pub(super) fn char_flex(&self, f: u16, c: u8, shrink: bool) -> i32 {
        let x = self.e.fonts.micro.expand(f);
        let k = if shrink { x.shrink } else { x.stretch };
        let ef = self.e.fonts.micro.ef(f, c);
        if k == 0 || ef <= 0 {
            return 0;
        }
        let dw = if shrink { self.char_width(f, c) - self.char_width(k, c) } else { self.char_width(k, c) - self.char_width(f, c) };
        if dw > 0 {
            round_xn_over_d(dw, ef, 1000)
        } else {
            0
        }
    }

    /// pdftex.web `kern_stretch` and `kern_shrink`: what a font kern between characters
    /// `l` and `r` of one font gains at the font's limit.
    pub(super) fn kern_flex(&self, l: Node, kern: i32, r: Node, shrink: bool) -> i32 {
        let (Some((fl, cl)), Some((fr, cr))) = (shown_char(&l), shown_char(&r)) else { return 0 };
        let x = self.e.fonts.micro.expand(fl);
        let k = if shrink { x.shrink } else { x.stretch };
        if fl != fr || k == 0 {
            return 0;
        }
        let d = get_kern(&self.e.fonts.list[k as usize], &self.e.fonts.info, cl, cr);
        round_xn_over_d(if shrink { kern - d } else { d - kern }, self.e.fonts.micro.ef(fl, cl), 1000)
    }

    /// pdftex.web `do_subst_font` for one character of font `f`: the copy expanded by
    /// `ratio` thousandths of the font's limit, times the character's `\efcode`.
    fn subst_font(&mut self, f: u16, c: u8, ratio: i32) -> u16 {
        let ef = self.e.fonts.micro.ef(f, c);
        if ef == 0 {
            return f;
        }
        let x = self.e.fonts.micro.expand(f);
        let ext = crate::backend::backend_pdftex::ext_xn_over_d;
        let e = if x.stretch != 0 && ratio > 0 {
            ext((ratio * ef) as i64, self.e.fonts.list[x.stretch as usize].expand as i64, 1_000_000) as i32
        } else if x.shrink != 0 && ratio < 0 {
            ext((ratio * ef) as i64, -(self.e.fonts.list[x.shrink as usize].expand as i64), 1_000_000) as i32
        } else {
            return f;
        };
        self.expand_font(f, e)
    }

    /// The characters of a discretionary's texts in the copies `ratio` asks for, changed in
    /// place: the line owns the discretionary and its texts (model M).
    fn subst_disc(&mut self, n: &mut Node, ratio: i32) {
        if n.a < 0 {
            return;
        }
        for (s, e) in self.e.nodes.discs[n.a as usize] {
            for i in s as usize..e as usize {
                let m = self.e.nodes.list[i];
                if let Some((f, c)) = shown_char(&m) {
                    self.e.nodes.list[i].font = self.subst_font(f, c, ratio);
                }
            }
        }
    }

    /// pdfTeX's font expansion of a finished line (`hpack` with `cal_expand_ratio`, then
    /// with `subst_ex_font`): the line from `start` in the build buffer, to be packed to
    /// `w`, gets its characters in fonts expanded by the ratio its glue would need, as far
    /// as finite glue alone would set it. The caller packs it afterwards.
    pub(super) fn expand_line(&mut self, start: usize, w: i32) {
        let (x, _, _, stretch, shrink) = self.measure_h(&self.cur[start..]);
        let excess = w - x;
        if excess == 0 {
            return;
        }
        // Mode `cal_expand_ratio`: the font stretch and shrink of the line.
        let (mut fs, mut fsh) = (0i32, 0i32);
        let mut prev_char: Option<usize> = None;
        for i in start..self.cur.len() {
            let n = self.cur[i];
            match n.kind {
                Kind::Char | Kind::Lig => {
                    prev_char = Some(i);
                    fs += self.char_flex(n.font, n.a as u8, false);
                    fsh += self.char_flex(n.font, n.a as u8, true);
                }
                Kind::Kern if n.sub == 0 => {
                    if let (Some(l), Some(&r)) = (prev_char.filter(|&l| l + 1 == i), self.cur.get(i + 1)) {
                        let l = self.cur[l];
                        fs += self.kern_flex(l, n.a, r, false);
                        fsh += self.kern_flex(l, n.a, r, true);
                    }
                }
                // A margin kern's character at the limits: an automatic copy shares the
                // base's quad, so its width does not change; the copies are made, though,
                // as pdfTeX makes them (font numbers follow).
                Kind::Kern if n.is_margin_kern() => {
                    self.subst_font(n.font, n.b as u8, 1000);
                    self.subst_font(n.font, n.b as u8, -1000);
                }
                _ => {}
            }
        }
        let ratio = if excess > 0 {
            if super::engine_pack::order(&stretch) == 0 && fs > 0 {
                crate::backend::backend_pdftex::divide_scaled(excess as i64, fs as i64, 3).0 as i32
            } else {
                0
            }
        } else if super::engine_pack::order(&shrink) == 0 && fsh > 0 {
            crate::backend::backend_pdftex::divide_scaled(excess as i64, fsh as i64, 3).0 as i32
        } else {
            0
        };
        if ratio == 0 {
            return;
        }
        let ratio = ratio.clamp(-1000, 1000);
        // Mode `subst_ex_font`. A font kern changes only between characters still in one
        // font that has limits, which a substituted character never is: kerns stay.
        for i in start..self.cur.len() {
            let mut n = self.cur[i];
            match n.kind {
                Kind::Char | Kind::Lig => n.font = self.subst_font(n.font, n.a as u8, ratio),
                Kind::Disc => self.subst_disc(&mut n, ratio),
                Kind::Kern if n.is_margin_kern() => n.font = self.subst_font(n.font, n.b as u8, ratio),
                _ => continue,
            }
            self.cur[i] = n;
        }
    }
}

impl Machine<'_, '_> {
    /// pdftex.web `check_expand_pars` for a paragraph: the step and the stretch and shrink
    /// limits (thousandths) of its first character in a font set up for expansion, which
    /// every other such font must share. `None` when no character can expand.
    pub(super) fn expand_pars(&self, par: &[Node]) -> Option<[i32; 3]> {
        let micro = &self.e.fonts.micro;
        let f = par.iter().filter(|n| matches!(n.kind, Kind::Char | Kind::Lig)).map(|n| n.font).find(|&f| {
            let x = micro.expand(f);
            x.step != 0 && (x.stretch != 0 || x.shrink != 0)
        })?;
        let x = micro.expand(f);
        let limit = |k: u16| if k == 0 { -1 } else { self.e.fonts.list[k as usize].expand as i32 };
        Some([x.step as i32, limit(x.stretch), if x.shrink == 0 { -1 } else { -limit(x.shrink) }])
    }
}

/// `rushtex-gen microtype`'s table: per text font (TFM name and size in sp) what microtype's
/// `\pickup@font` hook gives it, `-` or its `\pdffontexpand` stretch, shrink, step and
/// autoexpand, then its codes other than the defaults (`lp65=38`); then `sub KEY SIZE`, a
/// substituted selection that would give its font other codes.
static MICROTYPE: &str = include_str!("../../assets/tables/microtype.txt");

/// The table's font lines by (TFM name, size), and its substituted selections by (key,
/// size).
struct MtTable {
    fonts: std::collections::HashMap<(&'static str, i32), &'static str>,
    subs: std::collections::HashSet<(&'static str, i32)>,
}

/// The table, read once.
fn microtype_table() -> &'static MtTable {
    static T: std::sync::OnceLock<MtTable> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let (mut fonts, mut subs) = (std::collections::HashMap::new(), std::collections::HashSet::new());
        for l in MICROTYPE.lines().skip(1) {
            let sub = l.strip_prefix("sub ");
            let mut w = sub.unwrap_or(l).splitn(3, ' ');
            let (Some(name), Some(Ok(size))) = (w.next(), w.next().map(str::parse)) else { continue };
            match sub {
                Some(_) => {
                    subs.insert((name, size));
                }
                None => {
                    fonts.insert((name, size), w.next().unwrap_or("-"));
                }
            }
        }
        MtTable { fonts, subs }
    })
}

/// `\pdfadjustspacing` and `\pdfprotrudechars` as `\MT@setup@` sets them.
pub(super) fn microtype_ints() -> (i32, i32) {
    let mut w = MICROTYPE.lines().next().unwrap_or_default().split(' ').skip(1).map(|v| v.parse().unwrap_or(0));
    (w.next().unwrap_or(0), w.next().unwrap_or(0))
}

impl Machine<'_, '_> {
    /// microtype's `\MT@setupfont` for text font `f`, once: its codes and its
    /// `\pdffontexpand` from the table; a font the table lacks goes to the engine.
    pub(super) fn microtype_setup(&mut self, key: &str, f: u16) {
        // microtype's sets (`alltext`): the text encodings; others it leaves alone.
        if !matches!(key.split('/').next(), Some("OT1" | "T1" | "TS1")) {
            return;
        }
        let size = self.e.fonts.list[f as usize].size;
        if microtype_table().subs.contains(&(key, size)) {
            return self.not_native("microtype: a substituted font shape with codes of its own");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.mt_fonts.contains(&f) {
            return;
        }
        doc.mt_fonts.push(f);
        let font = &self.e.fonts.list[f as usize];
        let Some(&line) = microtype_table().fonts.get(&(&*font.name, font.size)) else {
            return self.not_native("microtype: a font without its codes");
        };
        let mut w = line.split(' ');
        let expand = match w.next() {
            Some("-") | None => None,
            Some(stretch) => {
                let mut n = || w.next().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                Some((stretch.parse::<i32>().unwrap_or(0), n(), n(), n() != 0))
            }
        };
        for code in w {
            let (which, rest) = code.split_at(2);
            let Some((c, v)) = rest.split_once('=') else { continue };
            let (Ok(c), Ok(v)) = (c.parse::<u8>(), v.parse::<i32>()) else { continue };
            let which = match which {
                "lp" => LP,
                "rp" => RP,
                _ => EF,
            };
            self.e.fonts.micro.set(f, which, c, v);
        }
        if let Some((stretch, shrink, step, auto)) = expand {
            self.set_expand_params(f, auto, stretch, shrink, step);
        }
    }
}

impl Machine<'_, '_> {
    /// microtype's `\MT@get@prot` of character `c` in the current font: the margin kern of a
    /// line of `c` alone (`\leftmarginkern` or `\rightmarginkern`), which `\leftprotrusion`
    /// and `\rightprotrusion` add as a kern when it is not zero; 0 without microtype.
    pub(super) fn protrusion_of(&self, c: u8, right: bool) -> i32 {
        if !self.microtype() || self.e.eqtb.word(super::engine_names::param_const("pdfprotrudechars")) <= 0 {
            return 0;
        }
        let n = Node { kind: Kind::Char, sub: 0, font: self.cur_font(), a: c as i32, b: 0 };
        -char_pw(&self.e.fonts, Some(n), right)
    }

    /// [`Self::protrusion_of`] as an explicit kern in the current list.
    pub(super) fn protrusion_kern(&mut self, c: u8, right: bool) {
        let w = self.protrusion_of(c, right);
        if w != 0 {
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
        }
    }
}
