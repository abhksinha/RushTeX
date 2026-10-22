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
use crate::backend::backend_ts1 as ts1;

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
    Tcrm0500,
    Tcrm0600,
    Tcrm0700,
    Tcrm0800,
    Tcrm0900,
    Tcrm1095,
    Tcrm1200,
    Tcrm1440,
    Tcrm1728,
    Tcrm2074,
    Tcrm2488,
}

impl OpticalMaster {
    pub const COUNT: usize = 31;

    /// Bits of the typewriter (cmtt) masters in a master mask.
    pub const TT_MASK: u32 = (1 << OpticalMaster::Cmtt8 as u8) | (1 << OpticalMaster::Cmtt9 as u8) | (1 << OpticalMaster::Cmtt12 as u8);

    /// Bits of the TS1 (tcrm) masters, which carry their own ToUnicode map.
    pub const TS1_MASK: u32 = ((1 << OpticalMaster::COUNT) - 1) & !((1 << OpticalMaster::Tcrm0500 as u8) - 1);

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
                Cmti7, Cmti8, Cmti9, Cmti12, Cmtt8, Cmtt9, Cmtt12, Tcrm0500, Tcrm0600, Tcrm0700, Tcrm0800, Tcrm0900, Tcrm1095, Tcrm1200, Tcrm1440, Tcrm1728, Tcrm2074, Tcrm2488,
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
        if family != FontFamily::ComputerModern {
            return None;
        }
        if face == Face::TextComp {
            // ts1cmr.fd: the EC sizes 5..24.88 (10.95pt has its own tcrm1095).
            const SIZES: [(f32, Option<OpticalMaster>); 12] = [
                (5.5, Some(Tcrm0500)), (6.5, Some(Tcrm0600)), (7.5, Some(Tcrm0700)), (8.5, Some(Tcrm0800)),
                (9.5, Some(Tcrm0900)), (10.5, None), (11.5, Some(Tcrm1095)), (13.2, Some(Tcrm1200)),
                (15.8, Some(Tcrm1440)), (19.0, Some(Tcrm1728)), (22.8, Some(Tcrm2074)), (f32::INFINITY, Some(Tcrm2488)),
            ];
            return SIZES.iter().find(|&&(below, _)| size < below).and_then(|&(_, m)| m);
        }
        if (9.5..11.0).contains(&size) {
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

    #[inline(always)]
    fn record(self) -> &'static MasterRecord {
        &MASTERS[self as usize]
    }

    /// PDF `/Widths` and layout advance widths, 1/1000 em of the master.
    #[inline]
    pub fn widths(self) -> &'static [u16; 128] {
        self.record().widths
    }

    /// fontdimens 2–5 and 7 (space, stretch, shrink, x-height, extra space), 1/1000 em.
    #[inline]
    pub fn glue(self) -> [f32; 5] {
        self.record().glue
    }

    /// Exact TFM widths and fontdimens.
    #[inline]
    pub fn fix(self) -> &'static FixMetrics {
        self.record().fix
    }

    /// Ligature/kern program; `None` for the typewriter masters, which are not shaped.
    #[inline]
    pub fn ligkern(self) -> Option<&'static LigKernTable> {
        self.record().ligkern
    }

    /// Whether the master lacks the f-ligature slots 0x0B–0x0F (cmr5 holds arrows there).
    #[inline]
    pub fn no_ff_slots(self) -> bool {
        self == OpticalMaster::Cmr5
    }

    /// The ToUnicode map the master carries itself (TS1), `None` for the shared OT1 maps.
    #[inline]
    pub fn own_tounicode(self) -> Option<&'static [u16; 128]> {
        self.record().tounicode
    }

    #[inline]
    pub fn program(self) -> &'static CmFontProgram {
        self.record().program
    }

    /// Page resource name of the master's font.
    #[inline]
    pub fn resource(self) -> &'static str {
        const NAMES: [&str; OpticalMaster::COUNT] = [
            "/FO0", "/FO1", "/FO2", "/FO3", "/FO4", "/FO5", "/FO6", "/FO7", "/FO8", "/FO9", "/FO10",
            "/FO11", "/FO12", "/FO13", "/FO14", "/FO15", "/FO16", "/FO17", "/FO18", "/FO19", "/FO20",
            "/FO21", "/FO22", "/FO23", "/FO24", "/FO25", "/FO26", "/FO27", "/FO28", "/FO29", "/FO30",
        ];
        NAMES[self as usize]
    }
}

/// Everything the pipeline needs about one master, in one row: adding a design size is
/// adding a variant and its row.
struct MasterRecord {
    /// PDF `/Widths` and layout advance widths, 1/1000 em of the master.
    widths: &'static [u16; 128],
    /// fontdimens 2–5 and 7 (space, stretch, shrink, x-height, extra space), 1/1000 em.
    glue: [f32; 5],
    /// Exact TFM widths and fontdimens.
    fix: &'static FixMetrics,
    /// Ligature/kern program; `None` for the typewriter masters, which are not shaped.
    ligkern: Option<&'static LigKernTable>,
    program: &'static CmFontProgram,
    /// Its own ToUnicode map (TS1); `None` for the shared OT1 maps.
    tounicode: Option<&'static [u16; 128]>,
}

static MASTERS: [MasterRecord; OpticalMaster::COUNT] = [
    /* Cmr5 */ MasterRecord { widths: &CMR5_WIDTHS.0, glue: [472.229, 208.338, 138.891, 430.554, 138.891], fix: &fix::CMR5_FIX, ligkern: Some(&lk::CMR5), program: &cm_fonts::CMR5_PROGRAM, tounicode: None },
    /* Cmr6 */ MasterRecord { widths: &opt::CMR6_WIDTHS.0, glue: opt::CMR6_GLUE, fix: &fix::CMR6_FIX, ligkern: Some(&opt::CMR6), program: &opt::CMR6_PROGRAM, tounicode: None },
    /* Cmr7 */ MasterRecord { widths: &CMR7_WIDTHS.0, glue: [384.923, 184.525, 123.016, 430.555, 123.016], fix: &fix::CMR7_FIX, ligkern: Some(&lk::CMR7), program: &cm_fonts::CMR7_PROGRAM, tounicode: None },
    /* Cmr8 */ MasterRecord { widths: &opt::CMR8_WIDTHS.0, glue: opt::CMR8_GLUE, fix: &fix::CMR8_FIX, ligkern: Some(&opt::CMR8), program: &opt::CMR8_PROGRAM, tounicode: None },
    /* Cmr9 */ MasterRecord { widths: &opt::CMR9_WIDTHS.0, glue: opt::CMR9_GLUE, fix: &fix::CMR9_FIX, ligkern: Some(&opt::CMR9), program: &opt::CMR9_PROGRAM, tounicode: None },
    /* Cmr12 */ MasterRecord { widths: &opt::CMR12_WIDTHS.0, glue: opt::CMR12_GLUE, fix: &fix::CMR12_FIX, ligkern: Some(&opt::CMR12), program: &opt::CMR12_PROGRAM, tounicode: None },
    /* Cmr17 */ MasterRecord { widths: &opt::CMR17_WIDTHS.0, glue: opt::CMR17_GLUE, fix: &fix::CMR17_FIX, ligkern: Some(&opt::CMR17), program: &opt::CMR17_PROGRAM, tounicode: None },
    /* Cmbx5 */ MasterRecord { widths: &opt::CMBX5_WIDTHS.0, glue: opt::CMBX5_GLUE, fix: &fix::CMBX5_FIX, ligkern: Some(&opt::CMBX5), program: &opt::CMBX5_PROGRAM, tounicode: None },
    /* Cmbx6 */ MasterRecord { widths: &opt::CMBX6_WIDTHS.0, glue: opt::CMBX6_GLUE, fix: &fix::CMBX6_FIX, ligkern: Some(&opt::CMBX6), program: &opt::CMBX6_PROGRAM, tounicode: None },
    /* Cmbx7 */ MasterRecord { widths: &opt::CMBX7_WIDTHS.0, glue: opt::CMBX7_GLUE, fix: &fix::CMBX7_FIX, ligkern: Some(&opt::CMBX7), program: &opt::CMBX7_PROGRAM, tounicode: None },
    /* Cmbx8 */ MasterRecord { widths: &opt::CMBX8_WIDTHS.0, glue: opt::CMBX8_GLUE, fix: &fix::CMBX8_FIX, ligkern: Some(&opt::CMBX8), program: &opt::CMBX8_PROGRAM, tounicode: None },
    /* Cmbx9 */ MasterRecord { widths: &opt::CMBX9_WIDTHS.0, glue: opt::CMBX9_GLUE, fix: &fix::CMBX9_FIX, ligkern: Some(&opt::CMBX9), program: &opt::CMBX9_PROGRAM, tounicode: None },
    /* Cmbx12 */ MasterRecord { widths: &opt::CMBX12_WIDTHS.0, glue: opt::CMBX12_GLUE, fix: &fix::CMBX12_FIX, ligkern: Some(&opt::CMBX12), program: &opt::CMBX12_PROGRAM, tounicode: None },
    /* Cmti7 */ MasterRecord { widths: &opt::CMTI7_WIDTHS.0, glue: opt::CMTI7_GLUE, fix: &fix::CMTI7_FIX, ligkern: Some(&opt::CMTI7), program: &opt::CMTI7_PROGRAM, tounicode: None },
    /* Cmti8 */ MasterRecord { widths: &opt::CMTI8_WIDTHS.0, glue: opt::CMTI8_GLUE, fix: &fix::CMTI8_FIX, ligkern: Some(&opt::CMTI8), program: &opt::CMTI8_PROGRAM, tounicode: None },
    /* Cmti9 */ MasterRecord { widths: &opt::CMTI9_WIDTHS.0, glue: opt::CMTI9_GLUE, fix: &fix::CMTI9_FIX, ligkern: Some(&opt::CMTI9), program: &opt::CMTI9_PROGRAM, tounicode: None },
    /* Cmti12 */ MasterRecord { widths: &opt::CMTI12_WIDTHS.0, glue: opt::CMTI12_GLUE, fix: &fix::CMTI12_FIX, ligkern: Some(&opt::CMTI12), program: &opt::CMTI12_PROGRAM, tounicode: None },
    /* Cmtt8 */ MasterRecord { widths: &EXTRA_FACE_WIDTHS[Face::MathMono8 as usize - 18].0, glue: [531.258, 0.0, 0.0, 430.555, 531.258], fix: &fix::CMTT8_FIX, ligkern: None, program: &cm_fonts::CMTT8_PROGRAM, tounicode: None },
    /* Cmtt9 */ MasterRecord { widths: &opt::CMTT9_WIDTHS.0, glue: opt::CMTT9_GLUE, fix: &fix::CMTT9_FIX, ligkern: None, program: &opt::CMTT9_PROGRAM, tounicode: None },
    /* Cmtt12 */ MasterRecord { widths: &opt::CMTT12_WIDTHS.0, glue: opt::CMTT12_GLUE, fix: &fix::CMTT12_FIX, ligkern: None, program: &opt::CMTT12_PROGRAM, tounicode: None },
    /* Tcrm0500 */ MasterRecord { widths: &ts1::TCRM0500_WIDTHS.0, glue: [472.107, 208.282, 138.855, 430.450, 138.855], fix: &ts1::TCRM0500_FIX, ligkern: None, program: &ts1::TCRM0500_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm0600 */ MasterRecord { widths: &ts1::TCRM0600_WIDTHS.0, glue: [416.565, 194.397, 129.598, 430.450, 129.598], fix: &ts1::TCRM0600_FIX, ligkern: None, program: &ts1::TCRM0600_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm0700 */ MasterRecord { widths: &ts1::TCRM0700_WIDTHS.0, glue: [384.827, 184.479, 122.986, 430.450, 122.986], fix: &ts1::TCRM0700_FIX, ligkern: None, program: &ts1::TCRM0700_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm0800 */ MasterRecord { widths: &ts1::TCRM0800_WIDTHS.0, glue: [354.080, 177.040, 118.027, 430.450, 118.027], fix: &ts1::TCRM0800_FIX, ligkern: None, program: &ts1::TCRM0800_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm0900 */ MasterRecord { widths: &ts1::TCRM0900_WIDTHS.0, glue: [342.509, 171.254, 114.170, 430.450, 114.170], fix: &ts1::TCRM0900_FIX, ligkern: None, program: &ts1::TCRM0900_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm1095 */ MasterRecord { widths: &ts1::TCRM1095_WIDTHS.0, glue: [331.557, 165.606, 110.404, 430.449, 110.404], fix: &ts1::TCRM1095_FIX, ligkern: None, program: &ts1::TCRM1095_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm1200 */ MasterRecord { widths: &ts1::TCRM1200_WIDTHS.0, glue: [326.309, 163.155, 108.769, 430.450, 108.769], fix: &ts1::TCRM1200_FIX, ligkern: None, program: &ts1::TCRM1200_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm1440 */ MasterRecord { widths: &ts1::TCRM1440_WIDTHS.0, glue: [317.133, 158.806, 105.870, 430.397, 105.870], fix: &ts1::TCRM1440_FIX, ligkern: None, program: &ts1::TCRM1440_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm1728 */ MasterRecord { widths: &ts1::TCRM1728_WIDTHS.0, glue: [313.066, 156.694, 104.463, 430.387, 104.463], fix: &ts1::TCRM1728_FIX, ligkern: None, program: &ts1::TCRM1728_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm2074 */ MasterRecord { widths: &ts1::TCRM2074_WIDTHS.0, glue: [308.242, 154.331, 102.887, 430.367, 102.887], fix: &ts1::TCRM2074_FIX, ligkern: None, program: &ts1::TCRM2074_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
    /* Tcrm2488 */ MasterRecord { widths: &ts1::TCRM2488_WIDTHS.0, glue: [304.235, 152.367, 101.578, 430.350, 101.578], fix: &ts1::TCRM2488_FIX, ligkern: None, program: &ts1::TCRM2488_PROGRAM, tounicode: Some(&ts1::TS1_TOUNICODE) },
];

/// The embedded program of the font file `name` (`cmr10`, `tcrm1095`), from the base faces
/// and the design-size masters; `None` for a font that is not embedded.
pub fn program_by_name(name: &str) -> Option<&'static CmFontProgram> {
    use cm_fonts::*;
    static BASE: [&CmFontProgram; 38] = [
        &CMR10_PROGRAM, &CMMI10_PROGRAM, &CMSY10_PROGRAM, &CMEX10_PROGRAM, &CMBX10_PROGRAM, &CMTI10_PROGRAM, &CMR5_PROGRAM,
        &MSAM10_PROGRAM, &MSBM10_PROGRAM, &CMR7_PROGRAM, &CMMI7_PROGRAM, &CMMI5_PROGRAM, &CMSY7_PROGRAM, &CMMI6_PROGRAM,
        &CMMI8_PROGRAM, &CMMI9_PROGRAM, &CMMI12_PROGRAM, &CMSY6_PROGRAM, &CMSY8_PROGRAM, &CMSY9_PROGRAM, &CMSY5_PROGRAM,
        &CMTT10_PROGRAM, &CMTT8_PROGRAM, &CMSS10_PROGRAM, &CMSS8_PROGRAM, &EUFM10_PROGRAM, &EUFM7_PROGRAM, &EUFM5_PROGRAM,
        &CMMIB10_PROGRAM, &CMMIB7_PROGRAM, &CMMIB5_PROGRAM, &CMBSY10_PROGRAM, &CMBSY7_PROGRAM, &CMBSY5_PROGRAM,
        &CMEX7_PROGRAM, &CMCSC10_PROGRAM, &CMSL10_PROGRAM, &CMSSBX10_PROGRAM,
    ];
    let same = |p: &CmFontProgram| p.name.eq_ignore_ascii_case(name);
    BASE.iter().copied().find(|p| same(p)).or_else(|| (0..OpticalMaster::COUNT as u8).map(|i| OpticalMaster::from_u8(i).program()).find(|p| same(p)))
}

/// The ToUnicode map of the font file `name`, by its encoding: OT1 text, typewriter, OML,
/// OMS, OMX, the AMS symbol fonts, or a TS1 master's own.
pub fn tounicode_by_name(name: &str) -> Option<&'static [u16; 128]> {
    use cm_fonts::*;
    let n = name.to_ascii_lowercase();
    let n = n.trim_end_matches(|c: char| c.is_ascii_digit());
    Some(match n {
        "cmtt" => &CMTT_TO_UNICODE,
        "cmmi" | "cmmib" => &CMMI_TO_UNICODE,
        "cmsy" | "cmbsy" => &CMSY_TO_UNICODE,
        "cmex" => &CMEX_TO_UNICODE,
        "msam" => &MSAM_TO_UNICODE,
        "msbm" => &MSBM_TO_UNICODE,
        "cmr" | "cmbx" | "cmti" | "cmsl" | "cmss" | "cmssbx" | "cmcsc" | "cmbxti" | "cmssi" => &CMR_TO_UNICODE,
        _ => return (0..OpticalMaster::COUNT as u8).map(OpticalMaster::from_u8).find(|m| m.program().name.eq_ignore_ascii_case(name)).and_then(|m| m.own_tounicode()),
    })
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
