//! Names of the engine's quantities: TeX's built-in parameters (a static table, nothing
//! built at startup) and the registers a document names with `\newcount`, `\newdimen`,
//! `\newskip` and `\newlength`.

use std::collections::HashMap;

use super::engine_eqtb::{Reg, Region};

pub use super::engine_tables::PARAMS;

/// tex.web §236 `mag`, §240's initial values: the parameters IniTeX does not start at zero.
pub const MAG: u32 = 17;
pub const TOLERANCE: u32 = 1;
pub const MAX_DEAD_CYCLES: u32 = 40;
pub const HANG_AFTER: u32 = 41;
pub const ESCAPE_CHAR: u32 = 45;
pub const END_LINE_CHAR: u32 = 48;

/// The parameter named `name` (without the backslash).
pub fn param(name: &str) -> Option<Reg> {
    let i = PARAMS.binary_search_by(|p| p.0.cmp(name)).ok()?;
    let (_, region, index) = PARAMS[i];
    Some(Reg { region, slot: index as u32 })
}

/// The registers a document has named, and the next free register of each kind.
#[derive(Debug)]
pub struct Names {
    user: HashMap<Box<str>, Reg>,
    /// Next register number per region (`Int`, `Dimen`, `Glue`): LaTeX's `ltalloc`
    /// hands them out upward from 11; 10 and below are scratch registers.
    next: [u32; 4],
}

impl Default for Names {
    fn default() -> Self {
        Self { user: HashMap::new(), next: [11; 4] }
    }
}

impl Names {
    /// The quantity `name` means: a named register, else a parameter.
    pub fn lookup(&self, name: &str) -> Option<Reg> {
        self.user.get(name).copied().or_else(|| param(name))
    }

    /// A register the document named itself (not a parameter).
    pub fn user(&self, name: &str) -> Option<Reg> {
        self.user.get(name).copied()
    }

    /// `\newcount` and its kin: `name` becomes the next free register of `region`.
    pub fn allocate(&mut self, name: &str, region: Region) -> Reg {
        let next = &mut self.next[region as usize];
        let reg = Reg::register(region, *next);
        *next += 1;
        self.user.insert(name.into(), reg);
        reg
    }

    /// `\countdef`, `\dimendef`, `\skipdef`: `name` is register `n` of `region`.
    pub fn bind(&mut self, name: &str, reg: Reg) {
        self.user.insert(name.into(), reg);
    }

    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
    }

    #[cfg(feature = "oracle")]
    pub(super) fn dump(&self, w: &mut super::engine_snapshot::Writer) {
        use super::engine_snapshot::Codec;
        // Sorted, so the same state always gives the same bytes.
        let mut user: Vec<_> = self.user.iter().collect();
        user.sort_unstable_by(|a, b| a.0.cmp(b.0));
        w.count(user.len());
        for (name, reg) in user {
            w.str(name);
            reg.put(w);
        }
        w.all(&self.next);
    }

    #[cfg(feature = "oracle")]
    pub(super) fn undump(r: &mut super::engine_snapshot::Reader) -> Option<Names> {
        use super::engine_snapshot::Codec;
        let n = r.count()?;
        let mut user = HashMap::with_capacity(n);
        for _ in 0..n {
            user.insert(r.str()?.into_boxed_str(), Reg::take(r)?);
        }
        Some(Names { user, next: r.all::<u32>()?.try_into().ok()? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::engine_eqtb::{DIMEN_PARAMS, GLUE_PARAMS, INT_PARAMS, TOKS_PARAMS};

    #[test]
    fn table_is_sorted_and_complete() {
        assert!(PARAMS.windows(2).all(|w| w[0].0 < w[1].0));
        for (region, count) in [(Region::Int, INT_PARAMS), (Region::Dimen, DIMEN_PARAMS), (Region::Glue, GLUE_PARAMS), (Region::Toks, TOKS_PARAMS)] {
            let mut seen: Vec<u16> = PARAMS.iter().filter(|p| p.1 == region).map(|p| p.2).collect();
            seen.sort_unstable();
            assert_eq!(seen, (0..count as u16).collect::<Vec<_>>());
        }
    }

    #[test]
    fn initial_value_slots_name_their_parameters() {
        for (name, slot) in [("mag", MAG), ("tolerance", TOLERANCE), ("maxdeadcycles", MAX_DEAD_CYCLES), ("hangafter", HANG_AFTER), ("escapechar", ESCAPE_CHAR), ("endlinechar", END_LINE_CHAR)] {
            assert_eq!(param(name), Some(Reg { region: Region::Int, slot }), "{name}");
        }
    }

    #[test]
    fn user_registers_shadow_nothing_and_count_up() {
        let mut n = Names::default();
        let a = n.allocate("foo", Region::Dimen);
        let b = n.allocate("bar", Region::Dimen);
        assert_eq!(b.slot, a.slot + 1);
        assert_eq!(n.lookup("foo"), Some(a));
        assert_eq!(n.lookup("parindent"), param("parindent"));
        assert_eq!(n.lookup("nosuch"), None);
    }
}
