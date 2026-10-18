//! Computer Modern design sizes (optical masters).
//!
//! Computer Modern is drawn separately for each size: cmr12 is not cmr10 scaled by 1.2, it
//! is narrower with thinner hairlines and tighter spacing. LaTeX's font definition files
//! (`ot1cmr.fd`, `ot1lmr.fd`) select the master by size, and so does RushTeX: a text face
//! keeps its `Face`, and the master is a function of (face, size) that the width lookups,
//! the shaper and the PDF writer resolve the same way.

use crate::backend::backend_cm_fonts::{self as cm_fonts, CmFontProgram};
use crate::backend::backend_cm_fix::{self as fix, FixMetrics};
use crate::backend::backend_cm_optical as opt;
use crate::backend::backend_fonts::{Face, FontFamily, CMR5_WIDTHS, CMR7_WIDTHS, EXTRA_FACE_WIDTHS};
use crate::backend::backend_ligkern::{self as lk, LigKernTable};

/// A Computer Modern master used instead of a face's 10pt font at another size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OpticalMaster {
    Cmr5,
    Cmr6,
    Cmr7,
    Cmr8,
    Cmr9,
    Cmr12,
    Cmr17,
    Cmbx5,
    Cmbx6,
    Cmbx7,
    Cmbx8,
    Cmbx9,
    Cmbx12,
    Cmti7,
    Cmti8,
    Cmti9,
    Cmti12,
    Cmtt8,
    Cmtt9,
    Cmtt12,
}

impl OpticalMaster {
    pub const COUNT: usize = 20;

    /// Bits of the typewriter (cmtt) masters in a master mask.
    pub const TT_MASK: u32 = (1 << OpticalMaster::Cmtt8 as u8) | (1 << OpticalMaster::Cmtt9 as u8) | (1 << OpticalMaster::Cmtt12 as u8);

    #[inline(always)]
    pub fn is_typewriter(self) -> bool {
        Self::TT_MASK & (1 << self as u8) != 0
    }

    #[inline(always)]
    pub fn from_u8(v: u8) -> Self {
        const ALL: [OpticalMaster; OpticalMaster::COUNT] = {
            use OpticalMaster::*;
            [
                Cmr5, Cmr6, Cmr7, Cmr8, Cmr9, Cmr12, Cmr17, Cmbx5, Cmbx6, Cmbx7, Cmbx8, Cmbx9, Cmbx12,
                Cmti7, Cmti8, Cmti9, Cmti12, Cmtt8, Cmtt9, Cmtt12,
            ]
        };
        ALL[v as usize % Self::COUNT]
    }

    /// The master for `face` at `size` (pt), or `None` when the face's own font applies.
    /// The size ranges are those of `ot1lmr.fd`/`ot1lmtt.fd`, which agree with the discrete
    /// sizes of `ot1cmr.fd`/`ot1cmtt.fd` (10.95pt uses the 10pt master, 14.4pt the 12pt one).
    #[inline]
    pub fn select(family: FontFamily, face: Face, size: f32) -> Option<Self> {
        use OpticalMaster::*;
        if family != FontFamily::ComputerModern || (9.5..11.0).contains(&size) {
            return None;
        }
        let tier = if size < 5.5 {
            0
        } else if size < 6.5 {
            1
        } else if size < 7.5 {
            2
        } else if size < 8.5 {
            3
        } else if size < 9.5 {
            4
        } else if size < 15.0 {
            5
        } else {
            6
        };
        Some(match face {
            Face::Roman => [Cmr5, Cmr6, Cmr7, Cmr8, Cmr9, Cmr12, Cmr17][tier],
            Face::Bold => [Cmbx5, Cmbx6, Cmbx7, Cmbx8, Cmbx9, Cmbx12, Cmbx12][tier],
            Face::Italic => [Cmti7, Cmti7, Cmti7, Cmti8, Cmti9, Cmti12, Cmti12][tier],
            Face::Mono | Face::MonoBold => [Cmtt8, Cmtt8, Cmtt8, Cmtt8, Cmtt9, Cmtt12, Cmtt12][tier],
            _ => return None,
        })
    }

    /// PDF `/Widths` and layout advance widths, 1/1000 em of the master.
    #[inline]
    pub fn widths(self) -> &'static [u16; 128] {
        use OpticalMaster::*;
        match self {
            Cmr5 => &CMR5_WIDTHS.0,
            Cmr6 => &opt::CMR6_WIDTHS.0,
            Cmr7 => &CMR7_WIDTHS.0,
            Cmr8 => &opt::CMR8_WIDTHS.0,
            Cmr9 => &opt::CMR9_WIDTHS.0,
            Cmr12 => &opt::CMR12_WIDTHS.0,
            Cmr17 => &opt::CMR17_WIDTHS.0,
            Cmbx5 => &opt::CMBX5_WIDTHS.0,
            Cmbx6 => &opt::CMBX6_WIDTHS.0,
            Cmbx7 => &opt::CMBX7_WIDTHS.0,
            Cmbx8 => &opt::CMBX8_WIDTHS.0,
            Cmbx9 => &opt::CMBX9_WIDTHS.0,
            Cmbx12 => &opt::CMBX12_WIDTHS.0,
            Cmti7 => &opt::CMTI7_WIDTHS.0,
            Cmti8 => &opt::CMTI8_WIDTHS.0,
            Cmti9 => &opt::CMTI9_WIDTHS.0,
            Cmti12 => &opt::CMTI12_WIDTHS.0,
            Cmtt8 => &EXTRA_FACE_WIDTHS[Face::MathMono8 as usize - 18].0,
            Cmtt9 => &opt::CMTT9_WIDTHS.0,
            Cmtt12 => &opt::CMTT12_WIDTHS.0,
        }
    }

    /// fontdimens 2–5 and 7 (space, stretch, shrink, x-height, extra space), 1/1000 em.
    #[inline]
    pub fn glue(self) -> [f32; 5] {
        use OpticalMaster::*;
        match self {
            Cmr5 => [472.229, 208.338, 138.891, 430.554, 138.891],
            Cmr6 => opt::CMR6_GLUE,
            Cmr7 => [384.923, 184.525, 123.016, 430.555, 123.016],
            Cmr8 => opt::CMR8_GLUE,
            Cmr9 => opt::CMR9_GLUE,
            Cmr12 => opt::CMR12_GLUE,
            Cmr17 => opt::CMR17_GLUE,
            Cmbx5 => opt::CMBX5_GLUE,
            Cmbx6 => opt::CMBX6_GLUE,
            Cmbx7 => opt::CMBX7_GLUE,
            Cmbx8 => opt::CMBX8_GLUE,
            Cmbx9 => opt::CMBX9_GLUE,
            Cmbx12 => opt::CMBX12_GLUE,
            Cmti7 => opt::CMTI7_GLUE,
            Cmti8 => opt::CMTI8_GLUE,
            Cmti9 => opt::CMTI9_GLUE,
            Cmti12 => opt::CMTI12_GLUE,
            Cmtt8 => [531.258, 0.0, 0.0, 430.555, 531.258],
            Cmtt9 => opt::CMTT9_GLUE,
            Cmtt12 => opt::CMTT12_GLUE,
        }
    }

    /// Exact TFM widths and fontdimens.
    #[inline]
    pub fn fix(self) -> &'static FixMetrics {
        use OpticalMaster::*;
        match self {
            Cmr5 => &fix::CMR5_FIX,
            Cmr6 => &fix::CMR6_FIX,
            Cmr7 => &fix::CMR7_FIX,
            Cmr8 => &fix::CMR8_FIX,
            Cmr9 => &fix::CMR9_FIX,
            Cmr12 => &fix::CMR12_FIX,
            Cmr17 => &fix::CMR17_FIX,
            Cmbx5 => &fix::CMBX5_FIX,
            Cmbx6 => &fix::CMBX6_FIX,
            Cmbx7 => &fix::CMBX7_FIX,
            Cmbx8 => &fix::CMBX8_FIX,
            Cmbx9 => &fix::CMBX9_FIX,
            Cmbx12 => &fix::CMBX12_FIX,
            Cmti7 => &fix::CMTI7_FIX,
            Cmti8 => &fix::CMTI8_FIX,
            Cmti9 => &fix::CMTI9_FIX,
            Cmti12 => &fix::CMTI12_FIX,
            Cmtt8 => &fix::CMTT8_FIX,
            Cmtt9 => &fix::CMTT9_FIX,
            Cmtt12 => &fix::CMTT12_FIX,
        }
    }

    /// Ligature/kern program; `None` for the typewriter masters, which are not shaped.
    #[inline]
    pub fn ligkern(self) -> Option<&'static LigKernTable> {
        use OpticalMaster::*;
        Some(match self {
            Cmr5 => &lk::CMR5,
            Cmr6 => &opt::CMR6,
            Cmr7 => &lk::CMR7,
            Cmr8 => &opt::CMR8,
            Cmr9 => &opt::CMR9,
            Cmr12 => &opt::CMR12,
            Cmr17 => &opt::CMR17,
            Cmbx5 => &opt::CMBX5,
            Cmbx6 => &opt::CMBX6,
            Cmbx7 => &opt::CMBX7,
            Cmbx8 => &opt::CMBX8,
            Cmbx9 => &opt::CMBX9,
            Cmbx12 => &opt::CMBX12,
            Cmti7 => &opt::CMTI7,
            Cmti8 => &opt::CMTI8,
            Cmti9 => &opt::CMTI9,
            Cmti12 => &opt::CMTI12,
            Cmtt8 | Cmtt9 | Cmtt12 => return None,
        })
    }

    /// Whether the master lacks the f-ligature slots 0x0B–0x0F (cmr5 holds arrows there).
    #[inline]
    pub fn no_ff_slots(self) -> bool {
        self == OpticalMaster::Cmr5
    }

    #[inline]
    pub fn program(self) -> &'static CmFontProgram {
        use OpticalMaster::*;
        match self {
            Cmr5 => &cm_fonts::CMR5_PROGRAM,
            Cmr6 => &opt::CMR6_PROGRAM,
            Cmr7 => &cm_fonts::CMR7_PROGRAM,
            Cmr8 => &opt::CMR8_PROGRAM,
            Cmr9 => &opt::CMR9_PROGRAM,
            Cmr12 => &opt::CMR12_PROGRAM,
            Cmr17 => &opt::CMR17_PROGRAM,
            Cmbx5 => &opt::CMBX5_PROGRAM,
            Cmbx6 => &opt::CMBX6_PROGRAM,
            Cmbx7 => &opt::CMBX7_PROGRAM,
            Cmbx8 => &opt::CMBX8_PROGRAM,
            Cmbx9 => &opt::CMBX9_PROGRAM,
            Cmbx12 => &opt::CMBX12_PROGRAM,
            Cmti7 => &opt::CMTI7_PROGRAM,
            Cmti8 => &opt::CMTI8_PROGRAM,
            Cmti9 => &opt::CMTI9_PROGRAM,
            Cmti12 => &opt::CMTI12_PROGRAM,
            Cmtt8 => &cm_fonts::CMTT8_PROGRAM,
            Cmtt9 => &opt::CMTT9_PROGRAM,
            Cmtt12 => &opt::CMTT12_PROGRAM,
        }
    }

    /// Page resource name of the master's font.
    #[inline]
    pub fn resource(self) -> &'static str {
        const NAMES: [&str; OpticalMaster::COUNT] = [
            "/FO0", "/FO1", "/FO2", "/FO3", "/FO4", "/FO5", "/FO6", "/FO7", "/FO8", "/FO9", "/FO10",
            "/FO11", "/FO12", "/FO13", "/FO14", "/FO15", "/FO16", "/FO17", "/FO18", "/FO19",
        ];
        NAMES[self as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_latex_design_sizes() {
        let cm = FontFamily::ComputerModern;
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 10.0), None);
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 10.95), None);
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 12.0), Some(OpticalMaster::Cmr12));
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 14.4), Some(OpticalMaster::Cmr12));
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 17.28), Some(OpticalMaster::Cmr17));
        assert_eq!(OpticalMaster::select(cm, Face::Bold, 14.4), Some(OpticalMaster::Cmbx12));
        assert_eq!(OpticalMaster::select(cm, Face::Roman, 9.0), Some(OpticalMaster::Cmr9));
        assert_eq!(OpticalMaster::select(cm, Face::Italic, 6.0), Some(OpticalMaster::Cmti7));
        assert_eq!(OpticalMaster::select(cm, Face::SmallCaps, 14.4), None);
        assert_eq!(OpticalMaster::select(FontFamily::Times, Face::Roman, 14.4), None);
        for v in 0..OpticalMaster::COUNT as u8 {
            assert_eq!(OpticalMaster::from_u8(v) as u8, v);
        }
        // cmr12 is narrower than cmr10 relative to its size.
        assert!(OpticalMaster::Cmr12.widths()[b'a' as usize] < crate::backend::fonts::CMR10_WIDTHS.0[b'a' as usize]);
    }
}
