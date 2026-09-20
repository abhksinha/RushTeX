//! Embedded Computer Modern Type 1 font streams and descriptors.
//!
//! Pre-compressed Type 1 font programs (CMR10, CMMI10, CMSY10, CMEX10)
//! providing 100% pixel-level visual and geometric identity with Donald Knuth's
//! TeX and legacy pdfLaTeX outputs.

/// Type 1 font stream descriptor with exact PFB payload lengths and PostScript FontBBox.
#[derive(Clone, Copy, Debug)]
pub struct CmFontProgram {
    pub name: &'static str,
    pub length1: u32,
    pub length2: u32,
    pub length3: u32,
    pub flags: u32,
    pub bbox: [i32; 4],
    pub ascent: i32,
    pub descent: i32,
    pub cap_height: i32,
    pub italic_angle: i32,
    pub stem_v: i32,
    pub compressed_stream: &'static [u8],
}

pub static CMR10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR10",
    length1: 4287,
    length2: 30900,
    length3: 545,
    flags: 32,
    bbox: [-40, -250, 1009, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 69,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr10.flate"),
};

pub static CMMI10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI10",
    length1: 4323,
    length2: 31411,
    length3: 545,
    flags: 4,
    bbox: [-32, -250, 1048, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi10.flate"),
};

pub static CMSY10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY10",
    length1: 4973,
    length2: 27031,
    length3: 545,
    flags: 4,
    bbox: [-29, -960, 1116, 775],
    ascent: 750,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 40,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy10.flate"),
};

pub static CMEX10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMEX10",
    length1: 5979,
    length2: 23707,
    length3: 545,
    flags: 4,
    bbox: [-24, -2960, 1454, 772],
    ascent: 40,
    descent: -600,
    cap_height: 0,
    italic_angle: 0,
    stem_v: 47,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmex10.flate"),
};

pub static CMBX10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX10",
    length1: 4293,
    length2: 29953,
    length3: 545,
    flags: 32,
    bbox: [-56, -250, 1164, 750],
    ascent: 694,
    descent: -194,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 115,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx10.flate"),
};

pub static CMTI10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI10",
    length1: 4302,
    length2: 33077,
    length3: 545,
    flags: 32,
    bbox: [-35, -250, 1124, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti10.flate"),
};

pub static CMR7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR7",
    length1: 4279,
    length2: 27918,
    length3: 545,
    flags: 32,
    bbox: [-27, -250, 1122, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 69,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr7.flate"),
};

pub static CMR5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR5",
    length1: 4342,
    length2: 26902,
    length3: 545,
    flags: 32,
    bbox: [-10, -250, 1304, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 69,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr5.flate"),
};

pub static CMMI7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI7",
    length1: 4314,
    length2: 31402,
    length3: 545,
    flags: 4,
    bbox: [-1, -250, 1171, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi7.flate"),
};

pub static CMMI5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI5",
    length1: 4314,
    length2: 33033,
    length3: 545,
    flags: 4,
    bbox: [37, -250, 1349, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi5.flate"),
};

pub static CMSY7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY7",
    length1: 4965,
    length2: 27186,
    length3: 545,
    flags: 4,
    bbox: [-15, -951, 1251, 782],
    ascent: 750,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 40,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy7.flate"),
};

pub static CMSY5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY5",
    length1: 4964,
    length2: 27386,
    length3: 545,
    flags: 4,
    bbox: [21, -944, 1448, 791],
    ascent: 750,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 40,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy5.flate"),
};

pub static MSAM10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "MSAM10",
    length1: 5469,
    length2: 25730,
    length3: 545,
    flags: 4,
    bbox: [8, -463, 1331, 1003],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 40,
    compressed_stream: include_bytes!("../../assets/cm_fonts/msam10.flate"),
};

pub static MSBM10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "MSBM10",
    length1: 5185,
    length2: 28944,
    length3: 545,
    flags: 4,
    bbox: [-55, -420, 2343, 920],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 40,
    compressed_stream: include_bytes!("../../assets/cm_fonts/msbm10.flate"),
};

