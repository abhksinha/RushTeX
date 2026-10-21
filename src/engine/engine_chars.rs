//! The main loop for characters: tex.web §1034–§1040, in our own form.
//!
//! TeX appends a word's characters with a cursor between `cur_l` and `cur_r`, running the
//! font's lig/kern program at each step and looking one token ahead. tex.web writes it with
//! gotos into the middle of sections; here each label is a [`Step`] and the loop moves
//! between them exactly as TeX jumps. The `lig_stack` is a short vector, top last: the
//! character just read, or pseudo-ligature items (§1037) that keep the character they replace.

use super::engine_catcode::Cat;
use super::engine_font::{restart, NON_CHAR, STOP_FLAG};
use super::engine_node::{BoxRec, Kind, Node};
use super::engine_prim::{Meaning, Prim};
use super::engine_run::{Machine, Mode, X};
use crate::backend::scaled::Scaler;

/// A discretionary being built: which list comes next (0 pre-break, 1 post-break, 2 the
/// replacement), and the pre-break and post-break lists so far.
pub(super) type DiscSaved = (u8, (u32, u32), (u32, u32));

/// §269 `disc_group`.
pub(super) const DISC_GROUP: u8 = 10;

/// The labels of §1034–§1040.
#[derive(Clone, Copy)]
enum Step {
    Wrapup,
    Move,
    Move1,
    Move2,
    Lookahead,
    Lookahead1,
    LigLoop,
    LigLoop1,
    MoveLig,
}

/// A `lig_stack` entry: the character read ahead (`real`), or a pseudo-ligature item whose
/// `lig_ptr` is the character it stands for, if any.
#[derive(Clone, Copy)]
pub(super) struct Item {
    c: u8,
    real: bool,
    orig: Option<u8>,
}

impl Machine<'_, '_> {
    /// §1034 `adjust_space_factor`.
    fn adjust_space_factor(&mut self, c: u8) {
        let s = self.e.eqtb.code(super::engine_eqtb::Code::Sf, c as u32);
        self.aux = if s == 1000 {
            1000
        } else if s < 1000 {
            if s > 0 {
                s
            } else {
                self.aux
            }
        } else if self.aux < 1000 {
            1000
        } else {
            s
        };
    }

    /// §1034–§1040: character `first` and the characters after it, with the font's
    /// ligatures and kerns; the token that ends the word is then obeyed (`reswitch`).
    // `wrapup!` resets flags that some of its call sites set again at once, as TeX's does.
    #[allow(unused_assignments)]
    pub(super) fn main_loop(&mut self, first: u8) {
        self.adjust_space_factor(first);
        let f = self.cur_font();
        if self.mode == Mode::Horizontal && !self.inner {
            self.fix_language();
        }
        let (font_bchar, label) = self.e.fonts.list[f as usize].metrics.boundary();
        let mut bchar = font_bchar;
        // §576 `font_false_bchar`: a boundary character the font also has as a real one.
        let false_bchar = if font_bchar < NON_CHAR && self.e.fonts.list[f as usize].metrics.exists(font_bchar as u8) { NON_CHAR } else { font_bchar };
        let paragraph = self.mode == Mode::Horizontal && !self.inner;
        let mut lig_stack = std::mem::take(&mut self.lig_buf);
        cur_l_next(&mut lig_stack, first);
        let mut cur_l = first as u16;
        let mut cur_r = NON_CHAR;
        let mut cur_q = self.cur.len();
        let (mut lig_present, mut lft_hit, mut rt_hit) = (false, false, false);
        let mut main_k = 0usize;
        let mut pending: Option<X> = None;
        let start = if std::mem::take(&mut self.cancel_boundary) { None } else { label };
        let mut step = match start {
            None => Step::Move2,
            Some(k) => {
                main_k = k;
                cur_r = cur_l;
                cur_l = NON_CHAR;
                Step::LigLoop1
            }
        };
        // §1035 `wrapup`: the ligature made so far, and an empty discretionary after the
        // font's hyphen character in a paragraph.
        macro_rules! wrapup {
            ($rt:expr) => {
                if cur_l < NON_CHAR {
                    let hyphen = self.e.fonts.list[f as usize].hyphen_char;
                    let ins_disc = self.cur.len() > cur_q && self.cur.last().is_some_and(|n| n.a == hyphen);
                    if lig_present {
                        // §1035 `pack_lig`.
                        let nodes = &mut self.e.nodes;
                        let at = nodes.lig_chars.len();
                        nodes.lig_chars.extend(self.cur[cur_q..].iter().map(|n| n.a as u8));
                        self.cur.truncate(cur_q);
                        let mut hits = 0;
                        if lft_hit {
                            hits = 2;
                            lft_hit = false;
                        }
                        if $rt && lig_stack.is_empty() {
                            hits += 1;
                            rt_hit = false;
                        }
                        let n = self.e.nodes.lig_node_at(f, cur_l as u8, at, hits);
                        self.cur.push(n);
                        lig_present = false;
                    }
                    if ins_disc && paragraph {
                        self.cur.push(Node { sub: 0, ..Node::new(Kind::Disc, -1) });
                    }
                }
            };
        }
        loop {
            step = match step {
                Step::Wrapup => {
                    wrapup!(rt_hit);
                    Step::Move
                }
                // §1036
                Step::Move => {
                    let Some(top) = lig_stack.last() else { break };
                    cur_q = self.cur.len();
                    cur_l = top.c as u16;
                    Step::Move1
                }
                Step::Move1 => {
                    if lig_stack.last().is_some_and(|t| !t.real) {
                        Step::MoveLig
                    } else {
                        Step::Move2
                    }
                }
                Step::Move2 => {
                    // §1036: the node appended is the character read (the `lig_stack` item),
                    // which a `=:` ligature may already have replaced as `cur_l`; it is
                    // what the ligature records as its original (§1035 `pack_lig`).
                    let read = lig_stack.pop().map_or(cur_l as u8, |t| t.c);
                    if !self.e.fonts.list[f as usize].metrics.exists(cur_l as u8) {
                        // §581 `char_warning`: a missing character is only logged.
                        return;
                    }
                    self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: read as i32, b: 0 });
                    Step::Lookahead
                }
                // §1038: the next token: another character, or the end of the word.
                Step::Lookahead => match self.get_x_token(false) {
                    None => {
                        cur_r = bchar;
                        Step::LigLoop
                    }
                    Some(x) => match self.char_of(x) {
                        Some(c) => {
                            cur_l_next(&mut lig_stack, c);
                            Step::Lookahead1
                        }
                        None => {
                            if x.m == Meaning::Prim(Prim::NoBoundary) {
                                bchar = NON_CHAR;
                            }
                            cur_r = bchar;
                            pending = Some(x);
                            Step::LigLoop
                        }
                    },
                },
                Step::Lookahead1 => {
                    let c = lig_stack.last().map_or(0, |t| t.c);
                    self.adjust_space_factor(c);
                    cur_r = c as u16;
                    if cur_r == false_bchar {
                        // §1038: no spurious ligatures with a boundary character.
                        cur_r = NON_CHAR;
                    }
                    Step::LigLoop
                }
                // §1039: the program of `cur_l`, if it has one and there is a `cur_r`.
                Step::LigLoop => match self.e.fonts.list[f as usize].metrics.lig_kern_start(cur_l as u8) {
                    Some(k) if cur_r != NON_CHAR => {
                        main_k = k;
                        Step::LigLoop1
                    }
                    _ => Step::Wrapup,
                },
                Step::LigLoop1 => {
                    let font = &self.e.fonts.list[f as usize];
                    let (prog, kerns) = font.metrics.lig_kern();
                    let j = prog[main_k];
                    let (skip, next, op, rem) = ((j >> 24) as u8, (j >> 16 & 0xFF) as u16, (j >> 8 & 0xFF) as u8, (j & 0xFF) as u8);
                    if next == cur_r && skip <= STOP_FLAG {
                        // §1040: a kern, or a ligature of one of the eight kinds.
                        if op >= STOP_FLAG {
                            let k = kerns.get(restart(j) - 256 * STOP_FLAG as usize).copied().unwrap_or(0);
                            let w = Scaler::new(font.size).scale(k);
                            wrapup!(rt_hit);
                            self.cur.push(Node::new(Kind::Kern, w));
                            Step::Move
                        } else {
                            if cur_l == NON_CHAR {
                                lft_hit = true;
                            } else if lig_stack.is_empty() {
                                rt_hit = true;
                            }
                            match op {
                                // `=:|`, `=:|>`
                                1 | 5 => {
                                    cur_l = rem as u16;
                                    lig_present = true;
                                }
                                // `|=:`, `|=:>`
                                2 | 6 => {
                                    cur_r = rem as u16;
                                    match lig_stack.last_mut() {
                                        None => {
                                            lig_stack.push(Item { c: rem, real: false, orig: None });
                                            bchar = NON_CHAR;
                                        }
                                        Some(top) if top.real => *top = Item { c: rem, real: false, orig: Some(top.c) },
                                        Some(top) => top.c = rem,
                                    }
                                }
                                // `|=:|`
                                3 => {
                                    cur_r = rem as u16;
                                    lig_stack.push(Item { c: rem, real: false, orig: None });
                                }
                                // `|=:|>`, `|=:|>>`
                                7 | 11 => {
                                    wrapup!(false);
                                    cur_q = self.cur.len();
                                    cur_l = rem as u16;
                                    lig_present = true;
                                }
                                // `=:`
                                _ => {
                                    cur_l = rem as u16;
                                    lig_present = true;
                                }
                            }
                            if !matches!(op, 1 | 2 | 3 | 5 | 6 | 7 | 11) {
                                if lig_stack.is_empty() {
                                    Step::Wrapup
                                } else {
                                    Step::Move1
                                }
                            } else if op > 4 && op != 7 {
                                Step::Wrapup
                            } else if cur_l < NON_CHAR {
                                Step::LigLoop
                            } else {
                                main_k = label.unwrap_or(0);
                                Step::LigLoop1
                            }
                        }
                    } else if skip == 0 {
                        main_k += 1;
                        Step::LigLoop1
                    } else if skip >= STOP_FLAG {
                        Step::Wrapup
                    } else {
                        main_k += skip as usize + 1;
                        Step::LigLoop1
                    }
                }
                // §1037: past a pseudo-ligature item, keeping the character it stood for.
                Step::MoveLig => {
                    let item = lig_stack.pop().expect("a lig_stack item");
                    if let Some(c) = item.orig {
                        self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
                    }
                    lig_present = true;
                    match lig_stack.last() {
                        None if item.orig.is_some() => Step::Lookahead,
                        None => {
                            cur_r = bchar;
                            Step::LigLoop
                        }
                        Some(top) => {
                            cur_r = top.c as u16;
                            Step::LigLoop
                        }
                    }
                }
            };
        }
        self.lig_buf = lig_stack;
        // `reswitch`: the token that ended the word.
        if let Some(x) = pending {
            self.command(x, 0);
        }
    }

    /// The character a token adds to a word (§1038): a letter or other character, a
    /// `\chardef` character, or `\char` with its number.
    fn char_of(&mut self, x: X) -> Option<u8> {
        match x.m {
            Meaning::Char(c) if matches!(c.cat(), Some(Cat::Letter | Cat::Other)) => u8::try_from(c.chr() as u32).ok(),
            Meaning::CharDef(n) => u8::try_from(n).ok(),
            Meaning::Prim(Prim::Char) => Some(self.scan_char_num()),
            _ => None,
        }
    }

    /// §434 `scan_char_num`: a character code, 0 after an error.
    pub(super) fn scan_char_num(&mut self) -> u8 {
        let v = self.scan_int();
        u8::try_from(v).unwrap_or_else(|_| {
            self.error(format!("Bad character code ({v})"));
            0
        })
    }

    /// §1123 `make_accent`: the accent character over the next character, with kerns that
    /// centre it (allowing for the slants of both fonts), raised for a taller letter.
    pub(super) fn make_accent(&mut self) {
        let a_char = self.scan_char_num();
        let af = self.cur_font();
        let font = &self.e.fonts.list[af as usize];
        let Some([a, _, _, _]) = font.dims(a_char) else { return };
        let (x, s) = (font.params[4], font.params[0] as f64 / 65536.0);
        self.do_assignments();
        // §1124: the character to accent.
        let f = self.cur_font();
        let next = self.get_x_token(false);
        let c = match next {
            Some(n) => match n.m {
                Meaning::Char(t) if matches!(t.cat(), Some(Cat::Letter | Cat::Other)) => u8::try_from(t.chr() as u32).ok(),
                Meaning::CharDef(c) => u8::try_from(c).ok(),
                Meaning::Prim(Prim::Char) => Some(self.scan_char_num()),
                _ => {
                    self.input.back(n.t);
                    None
                }
            },
            None => None,
        };
        let font = &self.e.fonts.list[f as usize];
        let q = c.and_then(|c| font.dims(c).map(|d| (c, d)));
        let accent = Node { kind: Kind::Char, sub: 0, font: af, a: a_char as i32, b: 0 };
        match q {
            Some((c, [w, h, _, _])) => {
                // §1125
                let t = font.params[0] as f64 / 65536.0;
                let p = if h != x {
                    let start = self.cur.len();
                    self.cur.push(accent);
                    let b = self.hpack(start, 0, false);
                    let b = self.e.nodes.add_box(BoxRec { shift: x - h, ..b });
                    Node::new(Kind::Box, b as i32)
                } else {
                    accent
                };
                let delta = super::engine_ship::zround((w - a) as f64 / 2.0 + h as f64 * t - x as f64 * s);
                self.cur.push(Node { sub: 2, ..Node::new(Kind::Kern, delta) });
                self.cur.push(p);
                self.cur.push(Node { sub: 2, ..Node::new(Kind::Kern, -a - delta) });
                self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
            }
            None => self.cur.push(accent),
        }
        self.aux = 1000;
    }

    /// §1117 `append_discretionary`: `\-` is a discretionary with the font's hyphen; for
    /// `\discretionary` its three lists follow.
    pub(super) fn append_discretionary(&mut self, hyphen: bool) {
        self.cur.push(Node { sub: 0, ..Node::new(Kind::Disc, -1) });
        if hyphen {
            let f = self.cur_font();
            let font = &self.e.fonts.list[f as usize];
            let c = font.hyphen_char;
            if let Ok(c) = u8::try_from(c) {
                if font.metrics.exists(c) {
                    let pre = self.e.nodes.store(&[Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 }]);
                    self.e.nodes.discs.push([pre, (0, 0)]);
                    let last = self.cur.len() - 1;
                    self.cur[last].a = self.e.nodes.discs.len() as i32 - 1;
                }
            }
            return;
        }
        self.disc_saved.push((0, (0, 0), (0, 0)));
        self.open_disc_list();
    }

    fn open_disc_list(&mut self) {
        self.begin_group(DISC_GROUP);
        self.scan_left_brace();
        self.push_nest(Mode::Horizontal, true, 1000);
    }

    /// §1119 `build_discretionary`: the `}` of one of a discretionary's lists.
    pub(super) fn build_discretionary(&mut self) {
        self.end_group(DISC_GROUP);
        // §1121: only characters, kerns, boxes, rules and ligatures belong.
        // The list ends at the first node that does not (§1121).
        let start = self.start as usize;
        let mut list: Vec<Node> = self.cur.drain(start..).collect();
        if let Some(k) = list.iter().position(|n| !matches!(n.kind, Kind::Char | Kind::Kern | Kind::Box | Kind::Rule | Kind::Lig)) {
            self.error("Improper discretionary list");
            list.truncate(k);
        }
        self.pop_nest();
        let Some(&(stage, pre, post)) = self.disc_saved.last() else { return };
        let span = self.e.nodes.store(&list);
        match stage {
            0 | 1 => {
                let k = self.disc_saved.len() - 1;
                self.disc_saved[k] = if stage == 0 { (1, span, post) } else { (2, pre, span) };
                self.open_disc_list();
            }
            _ => {
                // §1120: the replacement text follows the discretionary.
                self.disc_saved.pop();
                let mut n = list.len();
                let d = self.cur.len() - 1;
                if n > 0 && self.mode == Mode::Math {
                    self.error("Illegal math \\discretionary");
                    n = 0;
                } else {
                    self.cur.extend_from_slice(&list);
                }
                let a = if pre.0 == pre.1 && post.0 == post.1 {
                    -1
                } else {
                    self.e.nodes.discs.push([pre, post]);
                    self.e.nodes.discs.len() as i32 - 1
                };
                self.cur[d] = Node { sub: n.min(255) as u8, ..Node::new(Kind::Disc, a) };
            }
        }
    }

    /// §1045 `\noboundary`: before a character it cancels the left boundary.
    pub(super) fn no_boundary(&mut self) {
        let Some(x) = self.get_x_token(false) else { return };
        if matches!(x.m, Meaning::Char(c) if matches!(c.cat(), Some(Cat::Letter | Cat::Other))) || matches!(x.m, Meaning::CharDef(_) | Meaning::Prim(Prim::Char)) {
            self.cancel_boundary = true;
        }
        self.command(x, 0);
    }
}

/// §1038 `main_loop_lookahead+1`: the character read ahead goes on the `lig_stack`.
fn cur_l_next(lig_stack: &mut Vec<Item>, c: u8) {
    lig_stack.clear();
    lig_stack.push(Item { c, real: true, orig: None });
}
