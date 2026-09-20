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

// =========================================================================
// Compile-time /ToUnicode CMap Translation Tables (128 entries x 2 bytes = 256 bytes)
// =========================================================================

const fn make_cmr_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    map[0x00] = 0x0393; // \Gamma
    map[0x01] = 0x0394; // \Delta
    map[0x02] = 0x0398; // \Theta
    map[0x03] = 0x039B; // \Lambda
    map[0x04] = 0x039E; // \Xi
    map[0x05] = 0x03A0; // \Pi
    map[0x06] = 0x03A3; // \Sigma
    map[0x07] = 0x03A5; // \Upsilon
    map[0x08] = 0x03A6; // \Phi
    map[0x09] = 0x03A8; // \Psi
    map[0x0A] = 0x03A9; // \Omega
    map[0x0B] = 0xFB00; // ff
    map[0x0C] = 0xFB01; // fi
    map[0x0D] = 0xFB02; // fl
    map[0x0E] = 0xFB03; // ffi
    map[0x0F] = 0xFB04; // ffl
    map[0x10] = 0x0131; // \i
    map[0x11] = 0x0237; // \j
    map[0x12] = 0x0060; // `
    map[0x13] = 0x00B4; // '
    map[0x14] = 0x02C7; // caron
    map[0x15] = 0x02D8; // breve
    map[0x16] = 0x00AF; // macron
    map[0x17] = 0x02DA; // ring
    map[0x18] = 0x00B8; // cedilla
    map[0x19] = 0x00DF; // germandbls
    map[0x1A] = 0x00E6; // ae
    map[0x1B] = 0x0153; // oe
    map[0x1C] = 0x00F8; // oslash
    map[0x1D] = 0x00C6; // AE
    map[0x1E] = 0x0152; // OE
    map[0x1F] = 0x00D8; // Oslash
    let mut i = 0x20;
    while i <= 0x7E {
        map[i] = i as u16;
        i += 1;
    }
    map[0x27] = 0x2019; // '
    map[0x60] = 0x2018; // `
    map[0x7B] = 0x2013; // --
    map[0x7C] = 0x2014; // ---
    map[0x7D] = 0x201D; // ''
    map[0x7E] = 0x007E; // ~
    map[0x7F] = 0x00A8; // umlaut
    map
}

const fn make_cmmi_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    map[0x00] = 0x0393; // \Gamma
    map[0x01] = 0x0394; // \Delta
    map[0x02] = 0x0398; // \Theta
    map[0x03] = 0x039B; // \Lambda
    map[0x04] = 0x039E; // \Xi
    map[0x05] = 0x03A0; // \Pi
    map[0x06] = 0x03A3; // \Sigma
    map[0x07] = 0x03A5; // \Upsilon
    map[0x08] = 0x03A6; // \Phi
    map[0x09] = 0x03A8; // \Psi
    map[0x0A] = 0x03A9; // \Omega
    map[0x0B] = 0x03B1; // \alpha
    map[0x0C] = 0x03B2; // \beta
    map[0x0D] = 0x03B3; // \gamma
    map[0x0E] = 0x03B4; // \delta
    map[0x0F] = 0x03F5; // \epsilon
    map[0x10] = 0x03B6; // \zeta
    map[0x11] = 0x03B7; // \eta
    map[0x12] = 0x03B8; // \theta
    map[0x13] = 0x03B9; // \iota
    map[0x14] = 0x03BA; // \kappa
    map[0x15] = 0x03BB; // \lambda
    map[0x16] = 0x03BC; // \mu
    map[0x17] = 0x03BD; // \nu
    map[0x18] = 0x03BE; // \xi
    map[0x19] = 0x03C0; // \pi
    map[0x1A] = 0x03C1; // \rho
    map[0x1B] = 0x03C3; // \sigma
    map[0x1C] = 0x03C4; // \tau
    map[0x1D] = 0x03C5; // \upsilon
    map[0x1E] = 0x03D5; // \phi
    map[0x1F] = 0x03C7; // \chi
    map[0x20] = 0x03C8; // \psi
    map[0x21] = 0x03C9; // \omega
    map[0x22] = 0x03B5; // \varepsilon
    map[0x23] = 0x03D1; // \vartheta
    map[0x24] = 0x03D6; // \varpi
    map[0x25] = 0x03F1; // \varrho
    map[0x26] = 0x03C2; // \varsigma
    map[0x27] = 0x03C6; // \varphi
    map[0x28] = 0x2190; // \leftarrow
    map[0x29] = 0x21C0; // \rightharpoonup
    map[0x2A] = 0x21BC; // \leftharpoonup
    map[0x2B] = 0x21C1; // \rightharpoondown
    map[0x2C] = 0x21BD; // \leftharpoondown
    map[0x2D] = 0x25C1; // \triangleleft
    map[0x2E] = 0x25B7; // \triangleright
    map[0x2F] = 0x002F; // /
    let mut d = 0;
    while d < 10 {
        map[0x30 + d] = 0x0030 + d as u16;
        d += 1;
    }
    map[0x3A] = 0x002E; // .
    map[0x3B] = 0x002C; // ,
    map[0x3C] = 0x003C; // <
    map[0x3D] = 0x002F; // /
    map[0x3E] = 0x003E; // >
    map[0x3F] = 0x22C6; // \star
    map[0x40] = 0x2202; // \partial
    let mut c = 0;
    while c < 26 {
        map[0x41 + c] = 0x0041 + c as u16;
        map[0x61 + c] = 0x0061 + c as u16;
        c += 1;
    }
    map[0x5B] = 0x266D; // \flat
    map[0x5C] = 0x266E; // \natural
    map[0x5D] = 0x266F; // \sharp
    map[0x5E] = 0x2323; // \smile
    map[0x5F] = 0x2322; // \frown
    map[0x60] = 0x2113; // \ell
    map[0x7B] = 0x2118; // \wp
    map[0x7C] = 0x0131; // \imath
    map[0x7D] = 0x0237; // \jmath
    map[0x7E] = 0x22C8; // \bowtie
    map[0x7F] = 0x002E; // .
    map
}

const fn make_cmsy_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    map[0x00] = 0x2212; // -
    map[0x01] = 0x22C5; // \cdot
    map[0x02] = 0x00D7; // \times
    map[0x03] = 0x2217; // \ast
    map[0x04] = 0x00F7; // \div
    map[0x05] = 0x22C4; // \diamond
    map[0x06] = 0x00B1; // \pm
    map[0x07] = 0x2213; // \mp
    map[0x08] = 0x2295; // \oplus
    map[0x09] = 0x2296; // \ominus
    map[0x0A] = 0x2297; // \otimes
    map[0x0B] = 0x2298; // \oslash
    map[0x0C] = 0x2299; // \odot
    map[0x0D] = 0x25EF; // \bigcirc
    map[0x0E] = 0x2218; // \circ
    map[0x0F] = 0x2219; // \bullet
    map[0x10] = 0x224D; // \asymp
    map[0x11] = 0x2261; // \equiv
    map[0x12] = 0x2286; // \subseteq
    map[0x13] = 0x2287; // \supseteq
    map[0x14] = 0x2264; // \le
    map[0x15] = 0x2265; // \ge
    map[0x16] = 0x227C; // \preceq
    map[0x17] = 0x227D; // \succeq
    map[0x18] = 0x223C; // \sim
    map[0x19] = 0x2248; // \approx
    map[0x1A] = 0x2282; // \subset
    map[0x1B] = 0x2283; // \supset
    map[0x1C] = 0x226A; // \ll
    map[0x1D] = 0x226B; // \gg
    map[0x1E] = 0x227A; // \prec
    map[0x1F] = 0x227B; // \succ
    map[0x20] = 0x2190; // \leftarrow
    map[0x21] = 0x2192; // \rightarrow
    map[0x22] = 0x2191; // \uparrow
    map[0x23] = 0x2193; // \downarrow
    map[0x24] = 0x2194; // \leftrightarrow
    map[0x25] = 0x2197; // \nearrow
    map[0x26] = 0x2198; // \searrow
    map[0x27] = 0x2243; // \simeq
    map[0x28] = 0x21D0; // \Leftarrow
    map[0x29] = 0x21D2; // \Rightarrow
    map[0x2A] = 0x21D1; // \Uparrow
    map[0x2B] = 0x21D3; // \Downarrow
    map[0x2C] = 0x21D4; // \Leftrightarrow
    map[0x2D] = 0x2196; // \nwarrow
    map[0x2E] = 0x2199; // \swarrow
    map[0x2F] = 0x221D; // \propto
    map[0x30] = 0x2032; // \prime
    map[0x31] = 0x221E; // \infty
    map[0x32] = 0x2208; // \in
    map[0x33] = 0x220B; // \ni
    map[0x34] = 0x25B3; // \triangle
    map[0x35] = 0x25B3; // \bigtriangleup
    map[0x36] = 0x25BD; // \bigtriangledown
    map[0x37] = 0x002F; // /
    map[0x38] = 0x2200; // \forall
    map[0x39] = 0x2203; // \exists
    map[0x3A] = 0x00AC; // \neg
    map[0x3B] = 0x2205; // \emptyset
    map[0x3C] = 0x211C; // \Re
    map[0x3D] = 0x2111; // \Im
    map[0x3E] = 0x22A4; // \top
    map[0x3F] = 0x22A5; // \bot
    map[0x40] = 0x2135; // \aleph
    let mut c = 0;
    while c < 26 {
        map[0x41 + c] = 0x0041 + c as u16;
        c += 1;
    }
    map[0x5B] = 0x222A; // \cup
    map[0x5C] = 0x2229; // \cap
    map[0x5D] = 0x228E; // \uplus
    map[0x5E] = 0x2227; // \wedge
    map[0x5F] = 0x2228; // \vee
    map[0x60] = 0x22A2; // \vdash
    map[0x61] = 0x22A3; // \dashv
    map[0x62] = 0x230A; // \lfloor
    map[0x63] = 0x230B; // \rfloor
    map[0x64] = 0x2308; // \lceil
    map[0x65] = 0x2309; // \rceil
    map[0x66] = 0x007B; // {
    map[0x67] = 0x007D; // }
    map[0x68] = 0x27E8; // \langle
    map[0x69] = 0x27E9; // \rangle
    map[0x6A] = 0x007C; // |
    map[0x6B] = 0x2225; // \|
    map[0x6C] = 0x2195; // \updownarrow
    map[0x6D] = 0x21D5; // \Updownarrow
    map[0x6E] = 0x005C; // \backslash
    map[0x6F] = 0x2240; // \wr
    map[0x70] = 0x221A; // \surd
    map[0x71] = 0x2A3F; // \amalg
    map[0x72] = 0x2207; // \nabla
    map[0x73] = 0x222B; // \int
    map[0x74] = 0x2294; // \sqcup
    map[0x75] = 0x2293; // \sqcap
    map[0x76] = 0x2291; // \sqsubseteq
    map[0x77] = 0x2292; // \sqsupseteq
    map[0x78] = 0x00A7; // \S
    map[0x79] = 0x2020; // \dagger
    map[0x7A] = 0x2021; // \ddagger
    map[0x7B] = 0x00B6; // \P
    map[0x7C] = 0x2663; // \clubsuit
    map[0x7D] = 0x2662; // \diamondsuit
    map[0x7E] = 0x2661; // \heartsuit
    map[0x7F] = 0x2660; // \spadesuit
    map
}

const fn make_cmex_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    map[0x00] = 0x0028; // (
    map[0x01] = 0x0029; // )
    map[0x02] = 0x005B; // [
    map[0x03] = 0x005D; // ]
    map[0x04] = 0x230A; // \lfloor
    map[0x05] = 0x230B; // \rfloor
    map[0x06] = 0x2308; // \lceil
    map[0x07] = 0x2309; // \rceil
    map[0x08] = 0x007B; // {
    map[0x09] = 0x007D; // }
    map[0x0A] = 0x27E8; // \langle
    map[0x0B] = 0x27E9; // \rangle
    map[0x0C] = 0x002F; // /
    map[0x0D] = 0x005C; // \backslash
    map[0x46] = 0x2A06; // \bigsqcup text
    map[0x47] = 0x2A06; // \bigsqcup display
    map[0x48] = 0x222E; // \ointop text
    map[0x49] = 0x222E; // \ointop display
    map[0x4A] = 0x2A00; // \bigodot text
    map[0x4B] = 0x2A00; // \bigodot display
    map[0x4C] = 0x2A01; // \bigoplus text
    map[0x4D] = 0x2A01; // \bigoplus display
    map[0x4E] = 0x2A02; // \bigotimes text
    map[0x4F] = 0x2A02; // \bigotimes display
    map[0x50] = 0x2211; // \sum text
    map[0x58] = 0x2211; // \sum display
    map[0x51] = 0x220F; // \prod text
    map[0x59] = 0x220F; // \prod display
    map[0x52] = 0x222B; // \int text
    map[0x5A] = 0x222B; // \int display
    map[0x53] = 0x22C3; // \bigcup text
    map[0x5B] = 0x22C3; // \bigcup display
    map[0x54] = 0x22C2; // \bigcap text
    map[0x5C] = 0x22C2; // \bigcap display
    map[0x55] = 0x228E; // \biguplus text
    map[0x5D] = 0x228E; // \biguplus display
    map[0x56] = 0x22C1; // \bigwedge text
    map[0x5E] = 0x22C1; // \bigwedge display
    map[0x57] = 0x22C0; // \bigvee text
    map[0x5F] = 0x22C0; // \bigvee display
    map[0x60] = 0x2210; // \coprod text
    map[0x61] = 0x2210; // \coprod display
    map[0x62] = 0x0302; // \widehat variant 0
    map[0x63] = 0x0302; // \widehat variant 1
    map[0x64] = 0x0302; // \widehat variant 2
    map[0x65] = 0x0303; // \widetilde variant 0
    map[0x66] = 0x0303; // \widetilde variant 1
    map[0x67] = 0x0303; // \widetilde variant 2
    map[0x70] = 0x221A; // radical variant 0
    map[0x71] = 0x221A; // radical variant 1
    map[0x72] = 0x221A; // radical variant 2
    map[0x73] = 0x221A; // radical variant 3
    map[0x74] = 0x221A; // radical bot
    map[0x75] = 0x221A; // radical rep
    map[0x76] = 0x221A; // radical top
    map[0x7A] = 0x23DF; // \braceld (bottom curly bracket)
    map[0x7B] = 0x23DF; // \bracerd (bottom curly bracket)
    map[0x7C] = 0x23DE; // \bracelu (top curly bracket)
    map[0x7D] = 0x23DE; // \braceru (top curly bracket)
    map
}

const fn make_msam_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    map[0x03] = 0x2264; // \leqslant
    map[0x04] = 0x2265; // \geqslant
    map[0x43] = 0x2235; // \because
    map[0x44] = 0x2234; // \therefore
    map[0x46] = 0x25A1; // \square
    map[0x4D] = 0x25B3; // \vartriangle
    map
}

const fn make_msbm_to_unicode() -> [u16; 128] {
    let mut map = [0u16; 128];
    let mut c = 0;
    while c < 26 {
        map[0x41 + c] = 0x0041 + c as u16;
        c += 1;
    }
    map
}

pub static CMR_TO_UNICODE: [u16; 128] = make_cmr_to_unicode();
pub static CMMI_TO_UNICODE: [u16; 128] = make_cmmi_to_unicode();
pub static CMSY_TO_UNICODE: [u16; 128] = make_cmsy_to_unicode();
pub static CMEX_TO_UNICODE: [u16; 128] = make_cmex_to_unicode();
pub static MSAM_TO_UNICODE: [u16; 128] = make_msam_to_unicode();
pub static MSBM_TO_UNICODE: [u16; 128] = make_msbm_to_unicode();


