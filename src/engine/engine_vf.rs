//! Virtual fonts as pdfTeX reads and typesets them (pdftex.web `do_vf`, `vf_def_font`,
//! `do_vf_packet`), in our own form.
//!
//! A font is checked for a `.vf` file the first time a page that uses it is shipped out, as
//! pdfTeX does (`pdf_font_type = new_font_type`), so fonts loaded but never typeset cost no
//! file lookup. A virtual font keeps its packets as ranges of one byte buffer; its local fonts
//! are ordinary engine fonts (found or loaded as `tfm_lookup` and `read_font_info` would).

use std::rc::Rc;

use super::engine_node::Kind;
use super::engine_run::Machine;
use crate::backend::scaled::Scaler;

// The DVI and VF opcodes (dvitype, vftovp).
const SET1: u8 = 128;
const SET_RULE: u8 = 132;
const PUT1: u8 = 133;
const PUT_RULE: u8 = 137;
const NOP: u8 = 138;
const PUSH: u8 = 141;
const POP: u8 = 142;
const RIGHT1: u8 = 143;
const W0: u8 = 147;
const X0: u8 = 152;
const DOWN1: u8 = 157;
const Y0: u8 = 161;
const Z0: u8 = 166;
const FNT_NUM_0: u8 = 171;
const FNT1: u8 = 235;
const XXX1: u8 = 239;
const LONG_CHAR: u8 = 242;
const FNT_DEF1: u8 = 243;
const PRE: u8 = 247;
const POST: u8 = 248;
const VF_ID: u8 = 202;

/// pdfTeX's `vf_max_recursion`.
const MAX_RECURSION: u8 = 10;

/// One virtual font: its local fonts (the VF's number and the engine font; the first is the
/// default) and each character's packet, a range of `code`.
pub struct Vf {
    locals: Vec<(u32, u16)>,
    /// Shared with the font's expanded copies (pdftex.web `auto_expand_vf`).
    packets: Rc<[(u32, u32)]>,
    code: Rc<[u8]>,
}

/// Which fonts are virtual: per font, 0 not yet checked, 1 real, `n + 2` virtual font `n`.
#[derive(Default)]
pub struct VfTable {
    state: Vec<u32>,
    vfs: Vec<Vf>,
}

impl VfTable {
    /// The virtual font of font `f`, if it is one.
    pub fn get(&self, f: u16) -> Option<&Vf> {
        match self.state.get(f as usize) {
            Some(&s) if s >= 2 => Some(&self.vfs[s as usize - 2]),
            _ => None,
        }
    }

    fn checked(&self, f: u16) -> bool {
        self.state.get(f as usize).is_some_and(|&s| s != 0)
    }

    fn set(&mut self, f: u16, s: u32) {
        if self.state.len() <= f as usize {
            self.state.resize(f as usize + 1, 0);
        }
        self.state[f as usize] = s;
    }
}

/// What a packet asks of the page: characters and rules at (h, v), in TeX's coordinates.
pub trait PacketOut {
    /// Character `c` of font `f` at (h, v); its width (a virtual `f` typesets its own packet).
    fn packet_char(&mut self, f: u16, c: u8, h: i32, v: i32, depth: u8) -> i32;
    fn packet_rule(&mut self, h: i32, v: i32, w: i32, ht: i32);
}

/// A big-endian `k`-byte integer of `code` at `*at`, signed or not.
fn read(code: &[u8], at: &mut usize, k: usize, signed: bool) -> i32 {
    let mut v: i64 = 0;
    for i in 0..k {
        let b = code.get(*at + i).copied().unwrap_or(0) as i64;
        v = if i == 0 && signed && b >= 128 { b - 256 } else { v * 256 + b };
    }
    *at += k;
    v as i32
}

impl Vf {
    /// pdfTeX's `do_vf_packet` for character `c` of the font, whose size `size` scales the
    /// packet's movements, with the reference point at (`h`, `v`).
    pub fn typeset(&self, size: i32, c: u8, h: i32, v: i32, depth: u8, out: &mut dyn PacketOut) {
        if depth >= MAX_RECURSION {
            return;
        }
        let Some(&(start, len)) = self.packets.get(c as usize) else { return };
        if len == u32::MAX {
            return;
        }
        let code = &self.code[start as usize..(start + len) as usize];
        let scaler = Scaler::new(size);
        let scaled = |at: &mut usize, k: usize| scaler.scale(read(code, at, k, true));
        let mut f = self.locals.first().map_or(0, |l| l.1);
        let (mut cur_h, mut cur_v) = (h, v);
        let (mut w, mut x, mut y, mut z) = (0, 0, 0, 0);
        let mut stack: Vec<[i32; 6]> = Vec::new();
        let mut at = 0usize;
        while at < code.len() {
            let cmd = code[at];
            at += 1;
            let (ch, moves) = match cmd {
                0..=127 => (cmd, true),
                SET1..=131 => (read(code, &mut at, (cmd - SET1 + 1) as usize, false) as u8, true),
                PUT1..=136 => (read(code, &mut at, (cmd - PUT1 + 1) as usize, false) as u8, false),
                SET_RULE | PUT_RULE => {
                    let ht = scaled(&mut at, 4);
                    let wd = scaled(&mut at, 4);
                    if wd > 0 && ht > 0 {
                        out.packet_rule(cur_h, cur_v, wd, ht);
                        if cmd == SET_RULE {
                            cur_h += wd;
                        }
                    }
                    continue;
                }
                NOP => continue,
                PUSH => {
                    stack.push([cur_h, cur_v, w, x, y, z]);
                    continue;
                }
                POP => {
                    if let Some(s) = stack.pop() {
                        [cur_h, cur_v, w, x, y, z] = s;
                    }
                    continue;
                }
                RIGHT1..=146 => {
                    cur_h += scaled(&mut at, (cmd - RIGHT1 + 1) as usize);
                    continue;
                }
                W0..=151 => {
                    if cmd > W0 {
                        w = scaled(&mut at, (cmd - W0) as usize);
                    }
                    cur_h += w;
                    continue;
                }
                X0..=156 => {
                    if cmd > X0 {
                        x = scaled(&mut at, (cmd - X0) as usize);
                    }
                    cur_h += x;
                    continue;
                }
                DOWN1..=160 => {
                    cur_v += scaled(&mut at, (cmd - DOWN1 + 1) as usize);
                    continue;
                }
                Y0..=165 => {
                    if cmd > Y0 {
                        y = scaled(&mut at, (cmd - Y0) as usize);
                    }
                    cur_v += y;
                    continue;
                }
                Z0..=170 => {
                    if cmd > Z0 {
                        z = scaled(&mut at, (cmd - Z0) as usize);
                    }
                    cur_v += z;
                    continue;
                }
                FNT_NUM_0..=234 | FNT1 => {
                    let k = if cmd == FNT1 { read(code, &mut at, 1, false) as u32 } else { (cmd - FNT_NUM_0) as u32 };
                    if let Some(l) = self.locals.iter().find(|l| l.0 == k) {
                        f = l.1;
                    }
                    continue;
                }
                // A special in a packet; the engine writes none yet.
                XXX1..=LONG_CHAR => {
                    let n = read(code, &mut at, (cmd - XXX1 + 1) as usize, false);
                    at += n.max(0) as usize;
                    continue;
                }
                _ => return,
            };
            let wd = out.packet_char(f, ch, cur_h, cur_v, depth + 1);
            if moves {
                cur_h += wd;
            }
        }
    }
}

impl Machine<'_, '_> {
    /// pdfTeX's `do_vf` for every font of box `b` not yet checked, before the box is shipped.
    pub(super) fn check_virtual_fonts(&mut self, b: u32) {
        let mut todo = Vec::new();
        self.page_fonts(b, &mut todo);
        while let Some(f) = todo.pop() {
            if !self.vf.checked(f) {
                self.load_vf(f, &mut todo);
            }
        }
    }

    /// The fonts of the characters in box `b`, nested boxes and leaders included, that have
    /// not been checked yet.
    fn page_fonts(&self, b: u32, todo: &mut Vec<u16>) {
        let nodes = &self.e.nodes;
        let list = nodes.boxes[b as usize].list;
        for n in nodes.get(list) {
            match n.kind {
                Kind::Char | Kind::Lig | Kind::Word if !self.vf.checked(n.font) && !todo.contains(&n.font) => todo.push(n.font),
                Kind::Box => self.page_fonts(n.a as u32, todo),
                Kind::Glue if (100..110).contains(&n.sub) => self.page_fonts(n.b as u32, todo),
                _ => {}
            }
        }
    }

    /// Reads `<name>.vf` for font `f`: real when there is none. Local fonts go on `todo`.
    fn load_vf(&mut self, f: u16, todo: &mut Vec<u16>) {
        self.vf.set(f, 1);
        if self.auto_expand_vf(f, todo) {
            return;
        }
        let name = self.e.fonts.list[f as usize].name.clone();
        let Some(bytes) = self.read_file.as_ref().and_then(|read| read(&format!("{name}.vf"))) else { return };
        if bytes.len() < 3 || bytes[0] != PRE || bytes[1] != VF_ID {
            self.error(format!("Bad VF file {name}.vf"));
            return;
        }
        let size = self.e.fonts.list[f as usize].size;
        let scaler = Scaler::new(size);
        // The preamble: comment, checksum, design size.
        let mut at = 3 + bytes[2] as usize + 8;
        let mut locals = Vec::new();
        // `vf_def_font`: checksum, scaled size, design size, area and name.
        while let Some(&cmd @ FNT_DEF1..=246) = bytes.get(at) {
            at += 1;
            let k = read(&bytes, &mut at, (cmd - FNT_DEF1 + 1) as usize, false) as u32;
            at += 4;
            let fs = scaler.scale(read(&bytes, &mut at, 4, true));
            at += 4;
            let (a, l) = (bytes.get(at).copied().unwrap_or(0) as usize, bytes.get(at + 1).copied().unwrap_or(0) as usize);
            at += 2 + a;
            let local: String = bytes.get(at..at + l).unwrap_or_default().iter().map(|&c| char::from(c)).collect();
            at += l;
            let id = self.find_font(&local, fs, None);
            // `vf_def_font`: the local fonts of an expandable virtual font expand with it.
            let x = self.e.fonts.micro.expand(f);
            if id != 0 && x.step != 0 {
                let ratio = |k: u16| if k == 0 { 0 } else { self.e.fonts.list[k as usize].expand as i32 };
                let (stretch, shrink) = (ratio(x.stretch), -ratio(x.shrink));
                self.set_expand_params(id, x.auto, stretch, shrink, x.step as i32);
            }
            if id != 0 && !self.vf.checked(id) {
                todo.push(id);
            }
            locals.push((k, id));
        }
        let mut packets = vec![(0u32, u32::MAX); 256];
        let mut code = Vec::new();
        while let Some(&cmd) = bytes.get(at) {
            at += 1;
            let (len, cc) = match cmd {
                0..=241 => {
                    let cc = read(&bytes, &mut at, 1, false);
                    at += 3;
                    (cmd as usize, cc)
                }
                LONG_CHAR => {
                    let len = read(&bytes, &mut at, 4, false) as usize;
                    let cc = read(&bytes, &mut at, 4, false);
                    at += 4;
                    (len, cc)
                }
                POST => break,
                _ => {
                    self.error(format!("Bad VF file {name}.vf"));
                    return;
                }
            };
            let Some(dvi) = bytes.get(at..at + len) else { break };
            if let Ok(c) = u8::try_from(cc) {
                packets[c as usize] = (code.len() as u32, len as u32);
                code.extend_from_slice(dvi);
            }
            at += len;
        }
        self.vf.vfs.push(Vf { locals, packets: packets.into(), code: code.into() });
        let n = self.vf.vfs.len() as u32 + 1;
        self.vf.set(f, n);
    }
    /// pdftex.web `auto_expand_vf`: an expanded copy of a virtual font is virtual too, with
    /// the base's packets and each local font replaced by its copy expanded as much.
    fn auto_expand_vf(&mut self, f: u16, todo: &mut Vec<u16>) -> bool {
        let (e, bf) = (self.e.fonts.list[f as usize].expand as i32, self.e.fonts.micro.expand(f).blink);
        if e == 0 || bf == 0 {
            return false;
        }
        if !self.vf.checked(bf) {
            self.load_vf(bf, todo);
        }
        let Some(base) = self.vf.get(bf) else { return false };
        let (packets, code) = (base.packets.clone(), base.code.clone());
        let mut locals = base.locals.clone();
        for l in &mut locals {
            l.1 = self.get_expand_font(l.1, e);
            if !self.vf.checked(l.1) {
                todo.push(l.1);
            }
        }
        self.vf.vfs.push(Vf { locals, packets, code });
        let n = self.vf.vfs.len() as u32 + 1;
        self.vf.set(f, n);
        true
    }
}
