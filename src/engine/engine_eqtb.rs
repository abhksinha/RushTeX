//! The engine's equivalents: integer, dimension and glue quantities, both TeX's parameters and
//! the numbered registers, with grouping. Semantics follow tex.web §220–§283 (`eq_define`,
//! `eq_word_define`, `geq_word_define`, `eq_save`, `unsave`); the representation is ours:
//! one flat typed array per kind, grown only as far as a document reaches, group levels kept
//! beside the values, and one save stack of compact entries.

/// The number of integer, dimension, glue and token-list parameters (tex.web §236, §247,
/// §224, §230, with e-TeX's and pdfTeX's), from the generated table.
pub use super::engine_tables::{DIMEN_PARAMS, GLUE_PARAMS, INT_PARAMS, TOKS_PARAMS};
use super::engine_token::Span;

/// The deepest grouping allowed (TeX stops at 255 save levels; e-TeX at 65535).
const MAX_LEVEL: u16 = u16::MAX - 1;
/// §221 `level_one`: the level of a global or never-grouped value.
const LEVEL_ONE: u16 = 1;

use super::engine_catcode::{Cat, CatTable};
use super::engine_prim::Meaning;

/// The kind of quantity a slot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Region {
    Int = 0,
    Dimen = 1,
    Glue = 2,
    /// Token lists: `\toks` registers and `\everypar` and its kin (§230).
    Toks = 3,
    /// `\muskip` registers (§230 `mu_skip_base`); the three mu glue parameters stay with
    /// the glue parameters, as in TeX, and [`Reg::is_mu`] tells them apart.
    Mu = 4,
}

/// A quantity: a parameter (slots below the region's parameter count) or a register.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reg {
    pub region: Region,
    pub slot: u32,
}

impl Reg {
    /// Register `n` (`\count n`, `\dimen n`, `\skip n`) of `region`.
    pub const fn register(region: Region, n: u32) -> Reg {
        let base = match region {
            Region::Int => INT_PARAMS,
            Region::Dimen => DIMEN_PARAMS,
            Region::Glue => GLUE_PARAMS,
            Region::Toks => TOKS_PARAMS,
            Region::Mu => 0,
        };
        Reg { region, slot: base + n }
    }

    /// Whether the quantity is mu glue: a `\muskip` register or `\thinmuskip`,
    /// `\medmuskip`, `\thickmuskip` (§224's glue parameters 15–17).
    pub const fn is_mu(self) -> bool {
        matches!(self.region, Region::Mu) || (matches!(self.region, Region::Glue) && self.slot >= 15 && self.slot <= 17)
    }
}

/// Glue in scaled points (§150): width, stretch and shrink, and their orders
/// (0 finite, 1 fil, 2 fill, 3 filll).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Glue {
    pub width: i32,
    pub stretch: i32,
    pub shrink: i32,
    pub stretch_order: u8,
    pub shrink_order: u8,
}

/// One kind's values and their group levels (a level of 0 means "never assigned", read as
/// `level_one`).
#[derive(Debug, Default)]
struct Table<T> {
    vals: Vec<T>,
    levels: Vec<u16>,
}

impl<T: Copy + Default + super::engine_snapshot::Codec> Table<T> {
    fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        w.all(&self.vals);
        w.all(&self.levels);
    }

    fn undump(r: &mut super::engine_snapshot::Reader) -> Option<Self> {
        let t = Table { vals: r.all()?, levels: r.all()? };
        (t.vals.len() == t.levels.len()).then_some(t)
    }
}

impl<T: Copy + Default> Table<T> {
    #[inline]
    fn get(&self, slot: u32) -> T {
        self.vals.get(slot as usize).copied().unwrap_or_default()
    }

    #[inline]
    fn level(&self, slot: u32) -> u16 {
        self.levels.get(slot as usize).copied().unwrap_or(0).max(LEVEL_ONE)
    }

    fn put(&mut self, slot: u32, v: T, level: u16) {
        let i = slot as usize;
        if i >= self.vals.len() {
            self.vals.resize(i + 1, T::default());
            self.levels.resize(i + 1, 0);
        }
        self.vals[i] = v;
        self.levels[i] = level;
    }
}

/// The character code tables besides catcodes (§230): `\lccode`, `\uccode`, `\sfcode`,
/// `\mathcode`, `\delcode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Code {
    Lc = 0,
    Uc = 1,
    Sf = 2,
    Math = 3,
    Del = 4,
}

impl Code {
    /// §232's IniTeX value of the code of `c`: letters map to their lowercase and uppercase,
    /// uppercase letters have space factor 999, digits and letters are variable-family
    /// math characters, and only `.` has a delimiter code (0).
    pub fn initex(self, c: u32) -> i32 {
        let ch = char::from_u32(c).unwrap_or('\0');
        let upper = ch.is_ascii_uppercase();
        let letter = ch.is_ascii_alphabetic();
        match self {
            Code::Lc if letter => ch.to_ascii_lowercase() as i32,
            Code::Uc if letter => ch.to_ascii_uppercase() as i32,
            Code::Lc | Code::Uc => 0,
            Code::Sf if upper => 999,
            Code::Sf => 1000,
            Code::Math if ch.is_ascii_digit() => c as i32 + 0x7000,
            Code::Math if letter => c as i32 + 0x7100,
            Code::Math => c as i32,
            Code::Del if ch == '.' => 0,
            Code::Del => -1,
        }
    }

    /// §1232: the largest value `\<code>code` takes (`\delcode` takes any 24 bits).
    pub fn max(self) -> i32 {
        match self {
            Code::Lc | Code::Uc => 255,
            Code::Sf => 0x7FFF,
            Code::Math => 0x8000,
            Code::Del => 0xFF_FFFF,
        }
    }
}

/// A save-stack entry (§268): an old value to restore at the end of its group, or the
/// boundary that opens a group.
#[derive(Clone, Copy, Debug)]
enum Saved {
    Boundary,
    Word { region: Region, level: u16, slot: u32, val: i32 },
    Glue { mu: bool, level: u16, slot: u32, glue: Glue },
    Cat { level: u16, ch: u8, cat: u8 },
    Meaning { level: u16, id: u32, m: Meaning },
    Toks { level: u16, slot: u32, span: Span },
    Code { kind: Code, level: u16, ch: u32, val: i32 },
    Box { level: u16, n: u32, b: u32 },
    Shape { level: u16, v: u32 },
}

/// The equivalents and the save stack.
#[derive(Debug)]
pub struct Eqtb {
    ints: Table<i32>,
    dimens: Table<i32>,
    glues: Table<Glue>,
    mus: Table<Glue>,
    toks: Table<Span>,
    /// The other code tables, indexed by character; a never-assigned entry reads as IniTeX's
    /// value, so an untouched table costs nothing.
    codes: [Table<i32>; 5],
    /// §230 `cat_code_base`: the catcode table and each entry's group level.
    cats: CatTable,
    cat_levels: [u16; 256],
    /// §230 `box_base`: box register `n` holds box index + 1, or 0 when void.
    boxes: Table<u32>,
    /// §230 `par_shape_loc`: `\parshape` as an index + 1 into `shapes` (0 for none), and its
    /// level; every shape ever set is kept, so restoring one is an index.
    par_shape: u32,
    par_shape_level: u16,
    pub shapes: Vec<Vec<[i32; 2]>>,
    /// §222 `eq_type`/`equiv` of each control sequence, by id.
    meanings: Table<Meaning>,
    save: Vec<Saved>,
    level: u16,
}

impl Default for Eqtb {
    fn default() -> Self {
        use super::engine_names as n;
        let mut e = Eqtb { ints: Table::default(), dimens: Table::default(), glues: Table::default(), mus: Table::default(), toks: Table::default(), codes: Default::default(), cats: CatTable::latex(), cat_levels: [0; 256], boxes: Table::default(), par_shape: 0, par_shape_level: LEVEL_ONE, shapes: Vec::new(), meanings: Table::default(), save: Vec::new(), level: LEVEL_ONE };
        // §240: the integer parameters IniTeX does not start at zero.
        for (slot, v) in [(n::MAG, 1000), (n::TOLERANCE, 10000), (n::MAX_DEAD_CYCLES, 25), (n::HANG_AFTER, 1), (n::ESCAPE_CHAR, b'\\' as i32), (n::END_LINE_CHAR, 13)] {
            e.ints.put(slot, v, LEVEL_ONE);
        }
        e
    }
}

/// The group nesting went past [`MAX_LEVEL`].
#[derive(Debug, PartialEq, Eq)]
pub struct GroupOverflow;

impl Eqtb {
    /// The integer (`Int`) or scaled-point (`Dimen`) value of `reg`; a glue's width.
    #[inline]
    pub fn word(&self, reg: Reg) -> i32 {
        match reg.region {
            Region::Int => self.ints.get(reg.slot),
            Region::Dimen => self.dimens.get(reg.slot),
            Region::Glue => self.glues.get(reg.slot).width,
            Region::Mu => self.mus.get(reg.slot).width,
            Region::Toks => 0,
        }
    }

    /// The glue of `reg`; a word quantity as fixed glue of that width.
    #[inline]
    pub fn glue(&self, reg: Reg) -> Glue {
        match reg.region {
            Region::Glue => self.glues.get(reg.slot),
            Region::Mu => self.mus.get(reg.slot),
            _ => Glue { width: self.word(reg), ..Glue::default() },
        }
    }

    /// The current group level (1 outside every group).
    pub fn level(&self) -> u16 {
        self.level
    }

    /// §283 `eq_word_define` / `geq_word_define`: assigns an integer or dimension. A local
    /// assignment saves the old value once per group; a global one marks the slot so the
    /// end of the group keeps it.
    pub fn set_word(&mut self, reg: Reg, v: i32, global: bool) {
        let level = self.level;
        let table = match reg.region {
            Region::Int => &mut self.ints,
            Region::Dimen => &mut self.dimens,
            Region::Glue | Region::Mu => {
                let g = Glue { width: v, ..Glue::default() };
                return self.set_glue(reg, g, global);
            }
            Region::Toks => return,
        };
        if global {
            table.put(reg.slot, v, LEVEL_ONE);
            return;
        }
        let old = table.level(reg.slot);
        if old != level {
            self.save.push(Saved::Word { region: reg.region, level: old, slot: reg.slot, val: table.get(reg.slot) });
        }
        table.put(reg.slot, v, level);
    }

    /// §277 `eq_define` / §279 `geq_define` for glue.
    pub fn set_glue(&mut self, reg: Reg, g: Glue, global: bool) {
        let mu = match reg.region {
            Region::Glue => false,
            Region::Mu => true,
            _ => return self.set_word(reg, g.width, global),
        };
        let level = self.level;
        let table = if mu { &mut self.mus } else { &mut self.glues };
        if global {
            table.put(reg.slot, g, LEVEL_ONE);
            return;
        }
        let old = table.level(reg.slot);
        if old != level {
            let glue = table.get(reg.slot);
            self.save.push(Saved::Glue { mu, level: old, slot: reg.slot, glue });
        }
        let table = if mu { &mut self.mus } else { &mut self.glues };
        table.put(reg.slot, g, level);
    }

    /// The token list of a `Toks` quantity (empty when never assigned).
    #[inline]
    pub fn toks(&self, reg: Reg) -> Span {
        self.toks.get(reg.slot)
    }

    /// §1226 `\toks`/`\everypar` assignment: grouped like the other equivalents.
    pub fn set_toks(&mut self, reg: Reg, span: Span, global: bool) {
        if global {
            self.toks.put(reg.slot, span, LEVEL_ONE);
            return;
        }
        let old = self.toks.level(reg.slot);
        if old != self.level {
            self.save.push(Saved::Toks { level: old, slot: reg.slot, span: self.toks.get(reg.slot) });
        }
        self.toks.put(reg.slot, span, self.level);
    }

    /// `\lccode` and its kin of character `c`.
    #[inline]
    pub fn code(&self, kind: Code, c: u32) -> i32 {
        let t = &self.codes[kind as usize];
        match t.levels.get(c as usize) {
            Some(&l) if l != 0 => t.vals[c as usize],
            _ => kind.initex(c),
        }
    }

    /// §1232 `def_code` for the code tables other than catcodes.
    pub fn set_code(&mut self, kind: Code, c: u32, v: i32, global: bool) {
        let level = if global { LEVEL_ONE } else { self.level };
        let old_level = self.codes[kind as usize].levels.get(c as usize).copied().unwrap_or(0);
        if !global && old_level.max(LEVEL_ONE) != self.level {
            let val = self.code(kind, c);
            self.save.push(Saved::Code { kind, level: old_level, ch: c, val });
        }
        self.codes[kind as usize].put(c, v, level);
    }

    /// IniTeX's catcodes (§232), for building a format; a new `Eqtb` starts in LaTeX's.
    pub fn use_initex_catcodes(&mut self) {
        self.cats = CatTable::initex();
    }

    /// The current catcode table, for the tokenizer.
    #[inline]
    pub fn cats(&self) -> &CatTable {
        &self.cats
    }

    #[inline]
    pub fn catcode(&self, c: char) -> Cat {
        self.cats.get(c)
    }

    /// `\catcode` (§1232 `define(p, data, ...)`): grouped like any other equivalent.
    pub fn set_catcode(&mut self, ch: u8, cat: Cat, global: bool) {
        let i = ch as usize;
        if global {
            self.cats.0[i] = cat as u8;
            self.cat_levels[i] = LEVEL_ONE;
            return;
        }
        let old = self.cat_levels[i].max(LEVEL_ONE);
        if old != self.level {
            self.save.push(Saved::Cat { level: old, ch, cat: self.cats.0[i] });
        }
        self.cats.0[i] = cat as u8;
        self.cat_levels[i] = self.level;
    }

    /// §1248 `\parshape`: the (indent, length) pairs in force, empty for none.
    pub fn par_shape(&self) -> &[[i32; 2]] {
        match self.par_shape {
            0 => &[],
            v => &self.shapes[v as usize - 1],
        }
    }

    pub fn set_par_shape(&mut self, lines: Vec<[i32; 2]>, global: bool) {
        let v = if lines.is_empty() {
            0
        } else {
            self.shapes.push(lines);
            self.shapes.len() as u32
        };
        if !global && self.par_shape_level != self.level {
            self.save.push(Saved::Shape { level: self.par_shape_level, v: self.par_shape });
        }
        self.par_shape = v;
        self.par_shape_level = if global { LEVEL_ONE } else { self.level };
    }

    /// Box register `n`: the index of its box, or `None` when void.
    #[inline]
    pub fn box_reg(&self, n: u32) -> Option<u32> {
        self.boxes.get(n).checked_sub(1)
    }

    /// §1077 `\setbox`: grouped like any equivalent.
    pub fn set_box(&mut self, n: u32, b: Option<u32>, global: bool) {
        let v = b.map_or(0, |b| b + 1);
        if global {
            return self.boxes.put(n, v, LEVEL_ONE);
        }
        let old = self.boxes.level(n);
        if old != self.level {
            self.save.push(Saved::Box { level: old, n, b: self.boxes.get(n) });
        }
        self.boxes.put(n, v, self.level);
    }

    /// The register's box replaced at its own level, unsaved: `\box n` voiding it (§1079)
    /// and `\wd n=` changing it (§1247) act on the box itself, not as assignments.
    pub fn replace_box(&mut self, n: u32, b: Option<u32>) {
        let level = self.boxes.level(n);
        self.boxes.put(n, b.map_or(0, |b| b + 1), level);
    }

    /// Every box the registers and the save stack hold, as an index (for `engine_gc`).
    pub fn for_each_box(&mut self, mut f: impl FnMut(&mut u32)) {
        let saved = self.save.iter_mut().filter_map(|s| if let Saved::Box { b, .. } = s { Some(b) } else { None });
        for v in self.boxes.vals.iter_mut().chain(saved) {
            if *v > 0 {
                let mut b = *v - 1;
                f(&mut b);
                *v = b + 1;
            }
        }
    }

    /// Whether no group is open (§1304: a format is dumped only outside every group).
    pub fn at_top(&self) -> bool {
        self.level == LEVEL_ONE && self.save.is_empty()
    }

    /// Every token list the equivalents hold: macro texts, token registers and parameters,
    /// and the values saved for restoring.
    pub fn for_each_span(&mut self, mut f: impl FnMut(&mut Span)) {
        for m in &mut self.meanings.vals {
            if let Meaning::Macro(mac) = m {
                f(&mut mac.text);
            }
        }
        for s in &mut self.toks.vals {
            f(s);
        }
        for s in &mut self.save {
            match s {
                Saved::Toks { span, .. } => f(span),
                Saved::Meaning { m: Meaning::Macro(mac), .. } => f(&mut mac.text),
                _ => {}
            }
        }
    }

    /// The equivalents of a format (dumped at top level, so no save stack).
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        self.ints.dump(w);
        self.dimens.dump(w);
        self.glues.dump(w);
        self.mus.dump(w);
        self.toks.dump(w);
        for t in &self.codes {
            t.dump(w);
        }
        w.bytes.extend_from_slice(&self.cats.0);
        w.all(&self.cat_levels);
        self.boxes.dump(w);
        w.u32(self.par_shape);
        w.count(self.shapes.len());
        for s in &self.shapes {
            w.all(s);
        }
        self.meanings.dump(w);
    }

    /// Reads [`Eqtb::dump`]'s form; every token list must lie within the `toks` tokens read.
    pub(super) fn undump(r: &mut super::engine_snapshot::Reader, toks: u32) -> Option<Eqtb> {
        let (ints, dimens, glues, mus, toks_t) = (Table::undump(r)?, Table::undump(r)?, Table::undump(r)?, Table::undump(r)?, Table::<Span>::undump(r)?);
        let codes = [Table::undump(r)?, Table::undump(r)?, Table::undump(r)?, Table::undump(r)?, Table::undump(r)?];
        let cats = CatTable(r.take::<256>()?);
        let cat_levels: [u16; 256] = r.all::<u16>()?.try_into().ok()?;
        let boxes = Table::<u32>::undump(r)?;
        let par_shape = r.u32()?;
        let n = r.count()?;
        let shapes = (0..n).map(|_| r.all::<[i32; 2]>()).collect::<Option<Vec<_>>>()?;
        if par_shape as usize > shapes.len() {
            return None;
        }
        let meanings = Table::<Meaning>::undump(r)?;
        let inside = |s: &Span| s.end <= toks;
        let spans_ok = toks_t.vals.iter().all(inside) && meanings.vals.iter().all(|m| !matches!(m, Meaning::Macro(mac) if !inside(&mac.text)));
        spans_ok.then_some(Eqtb { ints, dimens, glues, mus, toks: toks_t, codes, cats, cat_levels, boxes, par_shape, par_shape_level: LEVEL_ONE, shapes, meanings, save: Vec::new(), level: LEVEL_ONE })
    }

    /// What control sequence `id` means; `Unset` when never assigned.
    #[inline]
    pub fn meaning(&self, id: u32) -> Meaning {
        self.meanings.get(id)
    }

    /// §277 `eq_define` / §279 `geq_define` of a control sequence.
    pub fn set_meaning(&mut self, id: u32, m: Meaning, global: bool) {
        if global {
            self.meanings.put(id, m, LEVEL_ONE);
            return;
        }
        let old = self.meanings.level(id);
        if old != self.level {
            self.save.push(Saved::Meaning { level: old, id, m: self.meanings.get(id) });
        }
        self.meanings.put(id, m, self.level);
    }

    /// §274 `new_save_level`: opens a group.
    pub fn begin_group(&mut self) -> Result<(), GroupOverflow> {
        if self.level >= MAX_LEVEL {
            return Err(GroupOverflow);
        }
        self.save.push(Saved::Boundary);
        self.level += 1;
        Ok(())
    }

    /// §281 `unsave`: closes the innermost group, restoring what it changed locally. A value
    /// assigned globally inside the group (now at `level_one`) is kept (§283). Outside any
    /// group it does nothing.
    pub fn end_group(&mut self) {
        if self.level == LEVEL_ONE {
            return;
        }
        while let Some(s) = self.save.pop() {
            match s {
                Saved::Boundary => break,
                Saved::Word { region, level, slot, val } => {
                    let table = if region == Region::Int { &mut self.ints } else { &mut self.dimens };
                    if table.level(slot) != LEVEL_ONE {
                        table.put(slot, val, level);
                    }
                }
                Saved::Glue { mu, level, slot, glue } => {
                    let table = if mu { &mut self.mus } else { &mut self.glues };
                    if table.level(slot) != LEVEL_ONE {
                        table.put(slot, glue, level);
                    }
                }
                Saved::Toks { level, slot, span } => {
                    if self.toks.level(slot) != LEVEL_ONE {
                        self.toks.put(slot, span, level);
                    }
                }
                Saved::Shape { level, v } => {
                    if self.par_shape_level != LEVEL_ONE {
                        self.par_shape = v;
                        self.par_shape_level = level;
                    }
                }
                Saved::Box { level, n, b } => {
                    if self.boxes.level(n) != LEVEL_ONE {
                        self.boxes.put(n, b, level);
                    }
                }
                Saved::Code { kind, level, ch, val } => {
                    let t = &mut self.codes[kind as usize];
                    if t.level(ch) != LEVEL_ONE {
                        t.put(ch, val, level);
                    }
                }
                Saved::Meaning { level, id, m } => {
                    if self.meanings.level(id) != LEVEL_ONE {
                        self.meanings.put(id, m, level);
                    }
                }
                Saved::Cat { level, ch, cat } => {
                    let i = ch as usize;
                    if self.cat_levels[i].max(LEVEL_ONE) != LEVEL_ONE {
                        self.cats.0[i] = cat;
                        self.cat_levels[i] = level;
                    }
                }
            }
        }
        self.level -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Reg = Reg::register(Region::Dimen, 11);

    #[test]
    fn local_assignment_ends_with_its_group() {
        let mut e = Eqtb::default();
        e.set_word(D, 100, false);
        e.begin_group().unwrap();
        e.set_word(D, 200, false);
        e.set_word(D, 300, false);
        assert_eq!(e.word(D), 300);
        e.end_group();
        assert_eq!(e.word(D), 100);
    }

    #[test]
    fn global_assignment_survives_the_group() {
        let mut e = Eqtb::default();
        e.begin_group().unwrap();
        e.set_word(D, 200, false);
        e.begin_group().unwrap();
        e.set_word(D, 300, true);
        e.end_group();
        e.end_group();
        assert_eq!(e.word(D), 300);
    }

    #[test]
    fn glue_is_grouped_and_saved_once_per_level() {
        let g = Reg::register(Region::Glue, 11);
        let mut e = Eqtb::default();
        let a = Glue { width: 65536, stretch: 65536, stretch_order: 1, ..Glue::default() };
        e.begin_group().unwrap();
        e.set_glue(g, a, false);
        e.set_glue(g, Glue::default(), false);
        assert_eq!(e.save.len(), 2); // the boundary and one saved value
        e.end_group();
        assert_eq!(e.glue(g), Glue::default());
        assert!(e.save.is_empty());
    }

    #[test]
    fn initex_parameters() {
        let e = Eqtb::default();
        assert_eq!(e.word(super::super::engine_names::param("tolerance").unwrap()), 10000);
        assert_eq!(e.word(super::super::engine_names::param("mag").unwrap()), 1000);
        assert_eq!(e.word(super::super::engine_names::param("parindent").unwrap()), 0);
    }

    #[test]
    fn catcodes_are_grouped() {
        let mut e = Eqtb::default();
        assert_eq!(e.catcode('@'), Cat::Other);
        e.begin_group().unwrap();
        e.set_catcode(b'@', Cat::Letter, false);
        e.set_catcode(b'|', Cat::Active, true);
        assert_eq!(e.catcode('@'), Cat::Letter);
        e.end_group();
        assert_eq!(e.catcode('@'), Cat::Other);
        assert_eq!(e.catcode('|'), Cat::Active);
    }

    #[test]
    fn unused_registers_cost_nothing() {
        let e = Eqtb::default();
        assert_eq!(e.word(Reg::register(Region::Int, 30000)), 0);
        assert!(e.dimens.vals.is_empty());
    }
}
