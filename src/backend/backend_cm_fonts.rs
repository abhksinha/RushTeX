//! Embedded Computer Modern CFF (Type 1C) font streams and descriptors.
//!
//! The optical sizes (cmr7, cmmi7/5, cmsy7/5, cmex7) and the math-alphabet fonts (cmss10/8,
//! cmtt10/8, eufm10/7/5, cmmib10/7/5, cmbsy10/7/5) are converted from the TeX Live Type 1
//! `.pfb` files to bare CFF + zlib (`assets/cm_fonts/*.flate`); see `gen_cm_assets.py`.
//!
//! Pre-compressed CFF font programs (CMR10, CMMI10, CMSY10, CMEX10, etc.)
//! providing 100% pixel-level visual and geometric identity with Donald Knuth's
//! TeX and legacy pdfLaTeX outputs in an optimized Compact Font Format.

/// CFF font stream descriptor with PostScript FontBBox and differences encoding.
#[derive(Clone, Copy, Debug)]
pub struct CmFontProgram {
    pub name: &'static str,
    pub flags: u32,
    pub bbox: [i32; 4],
    pub ascent: i32,
    pub descent: i32,
    pub cap_height: i32,
    pub italic_angle: i32,
    pub stem_v: i32,
    pub diff_encoding: &'static str,
    pub compressed_stream: &'static [u8],
}

pub static OT1_DIFF: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis /suppress 160 /space /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /sfthyphen /nbspace /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /dieresis";
pub static OMX_DIFF: &str = "0 /parenleftbig /parenrightbig /bracketleftbig /bracketrightbig /floorleftbig /floorrightbig /ceilingleftbig /ceilingrightbig /braceleftbig /bracerightbig /angbracketleftbig /angbracketrightbig /vextendsingle /vextenddouble /slashbig /backslashbig /parenleftBig /parenrightBig /parenleftbigg /parenrightbigg /bracketleftbigg /bracketrightbigg /floorleftbigg /floorrightbigg /ceilingleftbigg /ceilingrightbigg /braceleftbigg /bracerightbigg /angbracketleftbigg /angbracketrightbigg /slashbigg /backslashbigg /parenleftBigg /parenrightBigg /bracketleftBigg /bracketrightBigg /floorleftBigg /floorrightBigg /ceilingleftBigg /ceilingrightBigg /braceleftBigg /bracerightBigg /angbracketleftBigg /angbracketrightBigg /slashBigg /backslashBigg /slashBig /backslashBig /parenlefttp /parenrighttp /bracketlefttp /bracketrighttp /bracketleftbt /bracketrightbt /bracketleftex /bracketrightex /bracelefttp /bracerighttp /braceleftbt /bracerightbt /braceleftmid /bracerightmid /braceex /arrowvertex /parenleftbt /parenrightbt /parenleftex /parenrightex /angbracketleftBig /angbracketrightBig /unionsqtext /unionsqdisplay /contintegraltext /contintegraldisplay /circledottext /circledotdisplay /circleplustext /circleplusdisplay /circlemultiplytext /circlemultiplydisplay /summationtext /producttext /integraltext /uniontext /intersectiontext /unionmultitext /logicalandtext /logicalortext /summationdisplay /productdisplay /integraldisplay /uniondisplay /intersectiondisplay /unionmultidisplay /logicalanddisplay /logicalordisplay /coproducttext /coproductdisplay /hatwide /hatwider /hatwidest /tildewide /tildewider /tildewidest /bracketleftBig /bracketrightBig /floorleftBig /floorrightBig /ceilingleftBig /ceilingrightBig /braceleftBig /bracerightBig /radicalbig /radicalBig /radicalbigg /radicalBigg /radicalbt /radicalvertex /radicaltp /arrowvertexdbl /arrowtp /arrowbt /bracehtipdownleft /bracehtipdownright /bracehtipupleft /bracehtipupright /arrowdbltp /arrowdblbt /parenleftBigg 159 /radical /space /parenleftbig /parenrightbig /bracketleftbig /bracketrightbig /floorleftbig /floorrightbig /ceilingleftbig /ceilingrightbig /braceleftbig /bracerightbig 173 /angbracketleftbig /angbracketrightbig /vextendsingle /vextenddouble /slashbig /backslashbig /parenleftBig /parenrightBig /parenleftbigg /parenrightbigg /bracketleftbigg /bracketrightbigg /floorleftbigg /floorrightbigg /ceilingleftbigg /ceilingrightbigg /braceleftbigg /bracerightbigg /angbracketleftbigg /angbracketrightbigg /slashbigg /backslashbigg /parenleftBigg /arrowdblbt 208 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega";
pub static OML_DIFF: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /alpha /beta /gamma /delta /epsilon1 /zeta /eta /theta /iota /kappa /lambda /mu /nu /xi /pi /rho /sigma /tau /upsilon /phi /chi /psi /omega /epsilon /theta1 /pi1 /rho1 /sigma1 /phi1 /arrowlefttophalf /arrowleftbothalf /arrowrighttophalf /arrowrightbothalf /arrowhookleft /arrowhookright /triangleright /triangleleft /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /period /comma /less /slash /greater /star /partialdiff /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /flat /natural /sharp /slurbelow /slurabove /lscript /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /dotlessi /dotlessj /weierstrass /vector /tie /psi 160 /space /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi 173 /Omega /alpha /beta /gamma /delta /epsilon1 /zeta /eta /theta /iota /kappa /lambda /mu /nu /xi /pi /rho /sigma /tau /upsilon /phi /chi /psi /tie";
pub static CMR5_DIFF: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /less /equal /greater /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis /suppress 160 /space /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /sfthyphen /nbspace /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /dieresis";
pub static OMS_DIFF: &str = "0 /minus /periodcentered /multiply /asteriskmath /divide /diamondmath /plusminus /minusplus /circleplus /circleminus /circlemultiply /circledivide /circledot /circlecopyrt /openbullet /bullet /equivasymptotic /equivalence /reflexsubset /reflexsuperset /lessequal /greaterequal /precedesequal /followsequal /similar /approxequal /propersubset /propersuperset /lessmuch /greatermuch /precedes /follows /arrowleft /arrowright /arrowup /arrowdown /arrowboth /arrownortheast /arrowsoutheast /similarequal /arrowdblleft /arrowdblright /arrowdblup /arrowdbldown /arrowdblboth /arrownorthwest /arrowsouthwest /proportional /prime /infinity /element /owner /triangle /triangleinv /negationslash /mapsto /universal /existential /logicalnot /emptyset /Rfractur /Ifractur /latticetop /perpendicular /aleph /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /union /intersection /unionmulti /logicaland /logicalor /turnstileleft /turnstileright /floorleft /floorright /ceilingleft /ceilingright /braceleft /braceright /angbracketleft /angbracketright /bar /bardbl /arrowbothv /arrowdblbothv /backslash /wreathproduct /radical /coproduct /nabla /integral /unionsq /intersectionsq /subsetsqequal /supersetsqequal /section /dagger /daggerdbl /paragraph /club /diamond /heart /spade /arrowleft 160 /space /minus /periodcentered /multiply /asteriskmath /divide /diamondmath /plusminus /minusplus /circleplus /circleminus 173 /circlemultiply /circledivide /circledot /circlecopyrt /openbullet /bullet /equivasymptotic /equivalence /reflexsubset /reflexsuperset /lessequal /greaterequal /precedesequal /followsequal /similar /approxequal /propersubset /propersuperset /lessmuch /greatermuch /precedes /follows /arrowleft /spade";
pub static CMTI_DIFF: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /sterling /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis /suppress 160 /space /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /sfthyphen /nbspace /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /dieresis";
pub static MSAM_DIFF: &str = "0 /squaredot /squareplus /squaremultiply /square /squaresolid /squaresmallsolid /diamond /diamondsolid /clockwise /anticlockwise /harpoonleftright /harpoonrightleft /squareminus /forces /forcesbar /satisfies /dblarrowheadright /dblarrowheadleft /dblarrowleft /dblarrowright /dblarrowup /dblarrowdwn /harpoonupright /harpoondownright /harpoonupleft /harpoondownleft /arrowtailright /arrowtailleft /arrowparrleftright /arrowparrrightleft /shiftleft /shiftright /squiggleright /squiggleleftright /curlyleft /curlyright /circleequal /followsorequal /greaterorsimilar /greaterorapproxeql /multimap /therefore /because /equalsdots /defines /precedesorequal /lessorsimilar /lessorapproxeql /equalorless /equalorgreater /equalorprecedes /equalorfollows /precedesorcurly /lessdblequal /lessorequalslant /lessorgreater /primereverse /axisshort /equaldotrightleft /equaldotleftright /followsorcurly /greaterdblequal /greaterorequalslant /greaterorless /squareimage /squareoriginal /triangleright /triangleleft /trianglerightequal /triangleleftequal /star /between /triangledownsld /trianglerightsld /triangleleftsld /arrowaxisright /arrowaxisleft /triangle /trianglesolid /triangleinv /ringinequal /lessequalgreater /greaterlessequal /lessdbleqlgreater /greaterdbleqlless /Yen /arrowtripleright /arrowtripleleft /check /orunderscore /nand /perpcorrespond /angle /measuredangle /sphericalangle /proportional /smile /frown /subsetdbl /supersetdbl /uniondbl /intersectiondbl /uprise /downfall /multiopenleft /multiopenright /subsetdblequal /supersetdblequal /difference /geomequivalent /muchless /muchgreater /rightanglenw /rightanglene /circleR /circleS /fork /dotplus /revsimilar /revasymptequal /rightanglesw /rightanglese /maltesecross /complement /intercal /circlering /circleasterisk /circleminus /squiggleright 160 /space /squaredot /squareplus /squaremultiply /square /squaresolid /squaresmallsolid /diamond /diamondsolid /clockwise /anticlockwise 173 /harpoonleftright /harpoonrightleft /squareminus /forces /forcesbar /satisfies /dblarrowheadright /dblarrowheadleft /dblarrowleft /dblarrowright /dblarrowup /dblarrowdwn /harpoonupright /harpoondownright /harpoonupleft /harpoondownleft /arrowtailright /arrowtailleft /arrowparrleftright /arrowparrrightleft /shiftleft /shiftright /squiggleright /circleminus";
pub static MSBM_DIFF: &str = "0 /lessornotequal /greaterornotequal /notlessequal /notgreaterequal /notless /notgreater /notprecedes /notfollows /lessornotdbleql /greaterornotdbleql /notlessorslnteql /notgreaterorslnteql /lessnotequal /greaternotequal /notprecedesoreql /notfollowsoreql /precedeornoteqvlnt /followornoteqvlnt /lessornotsimilar /greaterornotsimilar /notlessdblequal /notgreaterdblequal /precedenotslnteql /follownotslnteql /precedenotdbleqv /follownotdbleqv /lessnotdblequal /greaternotdblequal /notsimilar /notapproxequal /upslope /downslope /notsubsetoreql /notsupersetoreql /notsubsetordbleql /notsupersetordbleql /subsetornotdbleql /supersetornotdbleql /subsetornoteql /supersetornoteql /subsetnoteql /supersetnoteql /notsubseteql /notsuperseteql /notparallel /notbar /notshortbar /notshortparallel /notturnstile /notforces /notsatisfies /notforcesextra /nottriangeqlright /nottriangeqlleft /nottriangleleft /nottriangleright /notarrowleft /notarrowright /notdblarrowleft /notdblarrowright /notdblarrowboth /notarrowboth /dividemultiply /emptyset /notexistential /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /hatwide /hatwider /tildewide /tildewider 96 /Finv /Gmir 102 /Omegainv /eth /equalorsimilar /beth /gimel /daleth /lessdot /greaterdot /multicloseleft /multicloseright /barshort /parallelshort /integerdivide /similar /approxequal /approxorequal /followsorequal /precedesorequal /archleftdown /archrightdown /Digamma /kappa /k /planckover2pi /planckover2pi1 /epsiloninv /notsubsetoreql 160 /space /lessornotequal /greaterornotequal /notlessequal /notgreaterequal /notless /notgreater /notprecedes /notfollows /lessornotdbleql /greaterornotdbleql 173 /notlessorslnteql /notgreaterorslnteql /lessnotequal /greaternotequal /notprecedesoreql /notfollowsoreql /precedeornoteqvlnt /followornoteqvlnt /lessornotsimilar /greaterornotsimilar /notlessdblequal /notgreaterdblequal /precedenotslnteql /follownotslnteql /precedenotdbleqv /follownotdbleqv /lessnotdblequal /greaternotdblequal /notsimilar /notapproxequal /upslope /downslope /notsubsetoreql /epsiloninv";

pub static CMR10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR10",
    flags: 32,
    bbox: [-40, -250, 1009, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 69,
    diff_encoding: OT1_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr10.flate"),
};

pub static CMMI10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI10",
    flags: 4,
    bbox: [-32, -250, 1048, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    diff_encoding: OML_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi10.flate"),
};

pub static CMSY10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY10",
    flags: 4,
    bbox: [-29, -960, 1116, 775],
    ascent: 750,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 40,
    diff_encoding: OMS_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy10.flate"),
};

pub static CMEX10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMEX10",
    flags: 4,
    bbox: [-24, -2960, 1454, 772],
    ascent: 40,
    descent: -600,
    cap_height: 0,
    italic_angle: 0,
    stem_v: 47,
    diff_encoding: OMX_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmex10.flate"),
};

pub static CMBX10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX10",
    flags: 32,
    bbox: [-56, -250, 1164, 750],
    ascent: 694,
    descent: -194,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 115,
    diff_encoding: OT1_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx10.flate"),
};

pub static CMTI10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI10",
    flags: 32,
    bbox: [-35, -250, 1124, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 72,
    diff_encoding: CMTI_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti10.flate"),
};

pub static CMR5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR5",
    flags: 32,
    bbox: [-10, -250, 1304, 750],
    ascent: 694,
    descent: -194,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 69,
    diff_encoding: CMR5_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr5.flate"),
};

pub static MSAM10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "MSAM10",
    flags: 4,
    bbox: [8, -463, 1331, 1003],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 40,
    diff_encoding: MSAM_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/msam10.flate"),
};

pub static MSBM10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "MSBM10",
    flags: 4,
    bbox: [-55, -420, 2343, 920],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 40,
    diff_encoding: MSBM_DIFF,
    compressed_stream: include_bytes!("../../assets/cm_fonts/msbm10.flate"),
};

// ---- GENERATED by scratchpad fontgen/emit_rust.py (Type 1C programs from TeX Live .pfb) ----

pub static CMR7_DIFF_ENC: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis";
pub static CMMI7_DIFF_ENC: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /alpha /beta /gamma /delta /epsilon1 /zeta /eta /theta /iota /kappa /lambda /mu /nu /xi /pi /rho /sigma /tau /upsilon /phi /chi /psi /omega /epsilon /theta1 /pi1 /rho1 /sigma1 /phi1 /arrowlefttophalf /arrowleftbothalf /arrowrighttophalf /arrowrightbothalf /arrowhookleft /arrowhookright /triangleright /triangleleft /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /period /comma /less /slash /greater /star /partialdiff /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /flat /natural /sharp /slurbelow /slurabove /lscript /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /dotlessi /dotlessj /weierstrass /vector /tie";
pub static CMSY7_DIFF_ENC: &str = "0 /minus /periodcentered /multiply /asteriskmath /divide /diamondmath /plusminus /minusplus /circleplus /circleminus /circlemultiply /circledivide /circledot /circlecopyrt /openbullet /bullet /equivasymptotic /equivalence /reflexsubset /reflexsuperset /lessequal /greaterequal /precedesequal /followsequal /similar /approxequal /propersubset /propersuperset /lessmuch /greatermuch /precedes /follows /arrowleft /arrowright /arrowup /arrowdown /arrowboth /arrownortheast /arrowsoutheast /similarequal /arrowdblleft /arrowdblright /arrowdblup /arrowdbldown /arrowdblboth /arrownorthwest /arrowsouthwest /proportional /prime /infinity /element /owner /triangle /triangleinv /negationslash /mapsto /universal /existential /logicalnot /emptyset /Rfractur /Ifractur /latticetop /perpendicular /aleph /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /union /intersection /unionmulti /logicaland /logicalor /turnstileleft /turnstileright /floorleft /floorright /ceilingleft /ceilingright /braceleft /braceright /angbracketleft /angbracketright /bar /bardbl /arrowbothv /arrowdblbothv /backslash /wreathproduct /radical /coproduct /nabla /integral /unionsq /intersectionsq /subsetsqequal /supersetsqequal /section /dagger /daggerdbl /paragraph /club /diamond /heart /spade";
pub static CMTT10_DIFF_ENC: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /visiblespace /exclam /quotedbl /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /less /equal /greater /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /backslash /bracketright /asciicircum /underscore /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /braceleft /bar /braceright /asciitilde /dieresis";
pub static EUFM10_DIFF_ENC: &str = "0 /dnos /dalt /fnos /falt /galt /kalt /talt /ualt 18 /quoteleft /quoteright 32 /space /exclam 38 /ampersand /quotesingle /parenleft /parenright /asterisk /plus /comma /minus /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon 61 /equal 63 /question 65 /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft 93 /bracketright /asciicircum 97 /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z 125 /quotedbl 127 /onealt";
pub static CMEX7_DIFF_ENC: &str = "0 /parenleftbig /parenrightbig /bracketleftbig /bracketrightbig /floorleftbig /floorrightbig /ceilingleftbig /ceilingrightbig /braceleftbig /bracerightbig /angbracketleftbig /angbracketrightbig /vextendsingle /vextenddouble /slashbig /backslashbig /parenleftBig /parenrightBig /parenleftbigg /parenrightbigg /bracketleftbigg /bracketrightbigg /floorleftbigg /floorrightbigg /ceilingleftbigg /ceilingrightbigg /braceleftbigg /bracerightbigg /angbracketleftbigg /angbracketrightbigg /slashbigg /backslashbigg /parenleftBigg /parenrightBigg /bracketleftBigg /bracketrightBigg /floorleftBigg /floorrightBigg /ceilingleftBigg /ceilingrightBigg /braceleftBigg /bracerightBigg /angbracketleftBigg /angbracketrightBigg /slashBigg /backslashBigg /slashBig /backslashBig /parenlefttp /parenrighttp /bracketlefttp /bracketrighttp /bracketleftbt /bracketrightbt /bracketleftex /bracketrightex /bracelefttp /bracerighttp /braceleftbt /bracerightbt /braceleftmid /bracerightmid /braceex /arrowvertex /parenleftbt /parenrightbt /parenleftex /parenrightex /angbracketleftBig /angbracketrightBig /unionsqtext /unionsqdisplay /contintegraltext /contintegraldisplay /circledottext /circledotdisplay /circleplustext /circleplusdisplay /circlemultiplytext /circlemultiplydisplay /summationtext /producttext /integraltext /uniontext /intersectiontext /unionmultitext /logicalandtext /logicalortext /summationdisplay /productdisplay /integraldisplay /uniondisplay /intersectiondisplay /unionmultidisplay /logicalanddisplay /logicalordisplay /coproducttext /coproductdisplay /hatwide /hatwider /hatwidest /tildewide /tildewider /tildewidest /bracketleftBig /bracketrightBig /floorleftBig /floorrightBig /ceilingleftBig /ceilingrightBig /braceleftBig /bracerightBig /radicalbig /radicalBig /radicalbigg /radicalBigg /radicalbt /radicalvertex /radicaltp /arrowvertexdbl /arrowtp /arrowbt /bracehtipdownleft /bracehtipdownright /bracehtipupleft /bracehtipupright /arrowdbltp /arrowdblbt";

pub static CMR7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR7",
    flags: 32,
    bbox: [-27, -250, 1122, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 79,
    diff_encoding: CMR7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr7.flate"),
};

pub static CMMI7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI7",
    flags: 4,
    bbox: [-1, -250, 1171, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 81,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi7.flate"),
};

pub static CMMI5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI5",
    flags: 4,
    bbox: [37, -250, 1349, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 90,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi5.flate"),
};

pub static CMSY7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY7",
    flags: 4,
    bbox: [-15, -951, 1251, 782],
    ascent: 750,
    descent: -951,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 49,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy7.flate"),
};

pub static CMMI6_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI6",
    flags: 4,
    bbox: [11, -250, 1241, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 85,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi6.flate"),
};

pub static CMMI8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI8",
    flags: 4,
    bbox: [-24, -250, 1110, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 78,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi8.flate"),
};

pub static CMMI9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI9",
    flags: 4,
    bbox: [-29, -250, 1075, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 74,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi9.flate"),
};

pub static CMMI12_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMI12",
    flags: 4,
    bbox: [-31, -250, 1026, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 65,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmi12.flate"),
};

pub static CMSY6_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY6",
    flags: 4,
    bbox: [-4, -948, 1329, 786],
    ascent: 750,
    descent: -948,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 52,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy6.flate"),
};

pub static CMSY8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY8",
    flags: 4,
    bbox: [-30, -955, 1185, 779],
    ascent: 750,
    descent: -955,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 46,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy8.flate"),
};

pub static CMSY9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY9",
    flags: 4,
    bbox: [-29, -958, 1146, 777],
    ascent: 750,
    descent: -958,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 43,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy9.flate"),
};

pub static CMSY5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSY5",
    flags: 4,
    bbox: [21, -944, 1448, 791],
    ascent: 750,
    descent: -944,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 56,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsy5.flate"),
};

pub static CMTT10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTT10",
    flags: 32,
    bbox: [-4, -233, 537, 696],
    ascent: 694,
    descent: -222,
    cap_height: 611,
    italic_angle: 0,
    stem_v: 69,
    diff_encoding: CMTT10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmtt10.flate"),
};

pub static CMTT8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTT8",
    flags: 32,
    bbox: [-5, -232, 545, 699],
    ascent: 694,
    descent: -222,
    cap_height: 611,
    italic_angle: 0,
    stem_v: 76,
    diff_encoding: CMTT10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmtt8.flate"),
};

pub static CMSS10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSS10",
    flags: 32,
    bbox: [-61, -250, 999, 759],
    ascent: 750,
    descent: -250,
    cap_height: 694,
    italic_angle: 0,
    stem_v: 78,
    diff_encoding: CMR7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmss10.flate"),
};

pub static CMSS8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSS8",
    flags: 32,
    bbox: [-65, -250, 1062, 761],
    ascent: 750,
    descent: -250,
    cap_height: 694,
    italic_angle: 0,
    stem_v: 87,
    diff_encoding: CMR7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmss8.flate"),
};

pub static EUFM10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "EUFM10",
    flags: 4,
    bbox: [-28, -257, 1055, 741],
    ascent: 741,
    descent: -250,
    cap_height: 691,
    italic_angle: 0,
    stem_v: 76,
    diff_encoding: EUFM10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/eufm10.flate"),
};

pub static EUFM7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "EUFM7",
    flags: 4,
    bbox: [-1, -257, 1158, 741],
    ascent: 741,
    descent: -250,
    cap_height: 692,
    italic_angle: 0,
    stem_v: 82,
    diff_encoding: EUFM10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/eufm7.flate"),
};

pub static EUFM5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "EUFM5",
    flags: 4,
    bbox: [63, -257, 1363, 741],
    ascent: 741,
    descent: -251,
    cap_height: 695,
    italic_angle: 0,
    stem_v: 91,
    diff_encoding: EUFM10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/eufm5.flate"),
};

pub static CMMIB10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMIB10",
    flags: 4,
    bbox: [-15, -250, 1216, 750],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 113,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmib10.flate"),
};

pub static CMMIB7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMIB7",
    flags: 4,
    bbox: [-7, -250, 1341, 750],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 63,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmib7.flate"),
};

pub static CMMIB5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMMIB5",
    flags: 4,
    bbox: [14, -250, 1537, 751],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 83,
    diff_encoding: CMMI7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmmib5.flate"),
};

pub static CMBSY10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBSY10",
    flags: 4,
    bbox: [-26, -940, 1332, 825],
    ascent: 750,
    descent: -940,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 61,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbsy10.flate"),
};

pub static CMBSY7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBSY7",
    flags: 4,
    bbox: [-21, -927, 1489, 837],
    ascent: 750,
    descent: -927,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 73,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbsy7.flate"),
};

pub static CMBSY5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBSY5",
    flags: 4,
    bbox: [-13, -914, 1699, 854],
    ascent: 787,
    descent: -914,
    cap_height: 686,
    italic_angle: -14,
    stem_v: 86,
    diff_encoding: CMSY7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbsy5.flate"),
};

pub static CMEX7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMEX7",
    flags: 4,
    bbox: [-12, -2951, 1627, 770],
    ascent: 750,
    descent: -2951,
    cap_height: 750,
    italic_angle: 0,
    stem_v: 80,
    diff_encoding: CMEX7_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmex7.flate"),
};

pub static CMCSC10_DIFF_ENC: &str = "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /less /equal /greater /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis";


pub static CMCSC10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMCSC10",
    flags: 32,
    bbox: [14, -250, 1077, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 72,
    diff_encoding: CMCSC10_DIFF_ENC,
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmcsc10.flate"),
};


pub static CMSL10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSL10",
    flags: 32,
    bbox: [-62, -250, 1123, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -9,
    stem_v: 79,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmsl10.flate"),
};


pub static CMSSBX10_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMSSBX10",
    flags: 32,
    bbox: [-71, -250, 1099, 780],
    ascent: 750,
    descent: -250,
    cap_height: 694,
    italic_angle: 0,
    stem_v: 136,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmssbx10.flate"),
};

// ---- END GENERATED ----

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
    // OT1 positions that are not ASCII (the font's encoding vector, `OT1_DIFF`).
    map[0x22] = 0x201D; // quotedblright ('')
    map[0x27] = 0x2019; // quoteright
    map[0x3C] = 0x00A1; // exclamdown
    map[0x3E] = 0x00BF; // questiondown
    map[0x5C] = 0x201C; // quotedblleft (``)
    map[0x5E] = 0x02C6; // circumflex
    map[0x5F] = 0x02D9; // dotaccent
    map[0x60] = 0x2018; // quoteleft
    map[0x7B] = 0x2013; // endash (--)
    map[0x7C] = 0x2014; // emdash (---)
    map[0x7D] = 0x02DD; // hungarumlaut
    map[0x7E] = 0x02DC; // tilde
    map[0x7F] = 0x00A8; // dieresis
    map
}

/// ToUnicode of the OT1 typewriter fonts (cmtt): ASCII except for these positions.
const fn make_cmtt_to_unicode() -> [u16; 128] {
    let mut map = make_cmr_to_unicode();
    map[0x0B] = 0x2191; // arrowup
    map[0x0C] = 0x2193; // arrowdown
    map[0x0D] = 0x0027; // quotesingle
    map[0x0E] = 0x00A1; // exclamdown
    map[0x0F] = 0x00BF; // questiondown
    map[0x20] = 0x2423; // visible space
    let mut i = 0x21;
    while i <= 0x7E {
        map[i] = i as u16;
        i += 1;
    }
    map[0x27] = 0x2019; // quoteright
    map[0x60] = 0x2018; // quoteleft
    map[0x7F] = 0x00A8; // dieresis
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
    // OML: dotless i and j, \wp, then the \vec accent.
    map[0x7B] = 0x0131; // \imath
    map[0x7C] = 0x0237; // \jmath
    map[0x7D] = 0x2118; // \wp
    map[0x7E] = 0x20D7; // \vec
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
    map[0x34] = 0x25B3; // \bigtriangleup
    map[0x35] = 0x25BD; // \bigtriangledown
    map[0x36] = 0x0338; // \not (negationslash)
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
    map[0x0C] = 0x007C; // |
    map[0x0D] = 0x2016; // \|
    map[0x0E] = 0x002F; // /
    map[0x0F] = 0x005C; // \backslash
    map[0x2E] = 0x002F; // / Big
    map[0x2F] = 0x005C; // \backslash Big
    map[0x1E] = 0x002F; // / bigg
    map[0x1F] = 0x005C; // \backslash bigg
    map[0x2C] = 0x002F; // / Bigg
    map[0x2D] = 0x005C; // \backslash Bigg
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
    map[0x62] = 0x02C6; // \widehat variant 0
    map[0x63] = 0x02C6; // \widehat variant 1
    map[0x64] = 0x02C6; // \widehat variant 2
    map[0x65] = 0x02DC; // \widetilde variant 0
    map[0x66] = 0x02DC; // \widetilde variant 1
    map[0x67] = 0x02DC; // \widetilde variant 2
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
pub static CMTT_TO_UNICODE: [u16; 128] = make_cmtt_to_unicode();
pub static CMMI_TO_UNICODE: [u16; 128] = make_cmmi_to_unicode();
pub static CMSY_TO_UNICODE: [u16; 128] = make_cmsy_to_unicode();
pub static CMEX_TO_UNICODE: [u16; 128] = make_cmex_to_unicode();
pub static MSAM_TO_UNICODE: [u16; 128] = make_msam_to_unicode();
pub static MSBM_TO_UNICODE: [u16; 128] = make_msbm_to_unicode();

// The design-size masters LaTeX's font definition files select (`ot1cmr.fd`): cmr, cmbx,
// cmti and cmtt at the sizes the base faces above do not cover.

pub static CMR6_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR6",
    flags: 32,
    bbox: [-20, -250, 1193, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 83,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr6.flate"),
};

pub static CMR8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR8",
    flags: 32,
    bbox: [-36, -250, 1070, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 76,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr8.flate"),
};

pub static CMR9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR9",
    flags: 32,
    bbox: [-39, -250, 1036, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 74,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr9.flate"),
};

pub static CMR12_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR12",
    flags: 32,
    bbox: [-34, -251, 988, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 65,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr12.flate"),
};

pub static CMR17_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMR17",
    flags: 32,
    bbox: [-33, -250, 945, 749],
    ascent: 749,
    descent: -249,
    cap_height: 683,
    italic_angle: 0,
    stem_v: 53,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmr17.flate"),
};

pub static CMBX5_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX5",
    flags: 32,
    bbox: [-40, -250, 1483, 756],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 133,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx5.flate"),
};

pub static CMBX6_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX6",
    flags: 32,
    bbox: [-49, -250, 1367, 753],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 130,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx6.flate"),
};

pub static CMBX7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX7",
    flags: 32,
    bbox: [-55, -250, 1289, 751],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 127,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx7.flate"),
};

pub static CMBX8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX8",
    flags: 32,
    bbox: [-59, -250, 1235, 750],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 122,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx8.flate"),
};

pub static CMBX9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX9",
    flags: 32,
    bbox: [-58, -250, 1195, 750],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 117,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx9.flate"),
};

pub static CMBX12_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMBX12",
    flags: 32,
    bbox: [-53, -251, 1139, 750],
    ascent: 750,
    descent: -250,
    cap_height: 686,
    italic_angle: 0,
    stem_v: 109,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmbx12.flate"),
};

pub static CMTI7_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI7",
    flags: 96,
    bbox: [-27, -250, 1268, 750],
    ascent: 750,
    descent: -250,
    cap_height: 678,
    italic_angle: -14,
    stem_v: 76,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /sterling /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti7.flate"),
};

pub static CMTI8_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI8",
    flags: 96,
    bbox: [-35, -250, 1190, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 73,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /sterling /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti8.flate"),
};

pub static CMTI9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI9",
    flags: 96,
    bbox: [-35, -250, 1148, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 70,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /sterling /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti9.flate"),
};

pub static CMTI12_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTI12",
    flags: 96,
    bbox: [-36, -251, 1103, 750],
    ascent: 750,
    descent: -250,
    cap_height: 683,
    italic_angle: -14,
    stem_v: 63,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /ff /fi /fl /ffi /ffl /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /suppress /exclam /quotedblright /numbersign /sterling /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /exclamdown /equal /questiondown /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /quotedblleft /bracketright /circumflex /dotaccent /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /endash /emdash /hungarumlaut /tilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmti12.flate"),
};

pub static CMTT9_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTT9",
    flags: 33,
    bbox: [-6, -233, 542, 698],
    ascent: 694,
    descent: -222,
    cap_height: 611,
    italic_angle: 0,
    stem_v: 74,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /visiblespace /exclam /quotedbl /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /less /equal /greater /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /backslash /bracketright /asciicircum /underscore /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /braceleft /bar /braceright /asciitilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmtt9.flate"),
};

pub static CMTT12_PROGRAM: CmFontProgram = CmFontProgram {
    name: "CMTT12",
    flags: 33,
    bbox: [-1, -234, 524, 695],
    ascent: 694,
    descent: -222,
    cap_height: 611,
    italic_angle: 0,
    stem_v: 65,
    diff_encoding: "0 /Gamma /Delta /Theta /Lambda /Xi /Pi /Sigma /Upsilon /Phi /Psi /Omega /arrowup /arrowdown /quotesingle /exclamdown /questiondown /dotlessi /dotlessj /grave /acute /caron /breve /macron /ring /cedilla /germandbls /ae /oe /oslash /AE /OE /Oslash /visiblespace /exclam /quotedbl /numbersign /dollar /percent /ampersand /quoteright /parenleft /parenright /asterisk /plus /comma /hyphen /period /slash /zero /one /two /three /four /five /six /seven /eight /nine /colon /semicolon /less /equal /greater /question /at /A /B /C /D /E /F /G /H /I /J /K /L /M /N /O /P /Q /R /S /T /U /V /W /X /Y /Z /bracketleft /backslash /bracketright /asciicircum /underscore /quoteleft /a /b /c /d /e /f /g /h /i /j /k /l /m /n /o /p /q /r /s /t /u /v /w /x /y /z /braceleft /bar /braceright /asciitilde /dieresis",
    compressed_stream: include_bytes!("../../assets/cm_fonts/cmtt12.flate"),
};

// The TS1 text-symbol masters (`ts1cmr.fd`, tcrm), which carry their own ToUnicode map.

pub static TCRM0500_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM0500",
    flags: 32,
    bbox: [-211, -338, 1855, 946],
    ascent: 946,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm0500.flate"),
};

pub static TCRM0600_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM0600",
    flags: 32,
    bbox: [-210, -320, 1719, 944],
    ascent: 944,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm0600.flate"),
};

pub static TCRM0700_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM0700",
    flags: 32,
    bbox: [-203, -320, 1628, 942],
    ascent: 942,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm0700.flate"),
};

pub static TCRM0800_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM0800",
    flags: 32,
    bbox: [-203, -320, 1554, 938],
    ascent: 938,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm0800.flate"),
};

pub static TCRM0900_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM0900",
    flags: 32,
    bbox: [-196, -322, 1502, 937],
    ascent: 937,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm0900.flate"),
};

pub static TCRM1095_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM1095",
    flags: 32,
    bbox: [-188, -320, 1445, 942],
    ascent: 942,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm1095.flate"),
};

pub static TCRM1200_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM1200",
    flags: 32,
    bbox: [-185, -320, 1420, 942],
    ascent: 942,
    descent: -250,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm1200.flate"),
};

pub static TCRM1440_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM1440",
    flags: 32,
    bbox: [-178, -319, 1370, 944],
    ascent: 944,
    descent: -249,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm1440.flate"),
};

pub static TCRM1728_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM1728",
    flags: 32,
    bbox: [-174, -318, 1347, 949],
    ascent: 947,
    descent: -249,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm1728.flate"),
};

pub static TCRM2074_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM2074",
    flags: 32,
    bbox: [-170, -318, 1322, 952],
    ascent: 947,
    descent: -249,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm2074.flate"),
};

pub static TCRM2488_PROGRAM: CmFontProgram = CmFontProgram {
    name: "SFRM2488",
    flags: 32,
    bbox: [-167, -317, 1298, 954],
    ascent: 947,
    descent: -249,
    cap_height: 947,
    italic_angle: 0,
    stem_v: 50,
    diff_encoding: "0 /Grave /Acute /Circumflex /Tilde /Dieresis /Hungarumlaut /Ring /Caron /Breve /Macron /Dotaccent /cedilla /ogonek /quotesinglbase.ts1 /quotedblbase.ts1 /twelveudash /threequartersemdash /afii61664.cap /arrowleft /arrowright /tieaccentlowercase /tieaccentcapital /tieaccentlowercase.new /tieaccentcapital.new /afii61664.asc /uni2422 /dollar /quotesingle /asteriskmath /comma /hyphendbl /period /fraction /zerooldstyle /oneoldstyle /twooldstyle /threeoldstyle /fouroldstyle /fiveoldstyle /sixoldstyle /sevenoldstyle /eightoldstyle /nineoldstyle /angleleft /minus /angleright /uni2127 /circle /Omega /uni301A /uni301B /arrowup /arrowdown /grave.ts1 /born /divorced /died /leaf /married /musicalnote /tildelow /hyphendbl.alt /breve.ts1 /caron.ts1 /hungarumlaut.ts1 /dblgrave.ts1 /dagger /daggerdbl /uni2016 /perthousand /bullet /uni2103 /dollaroldstyle /centoldstyle /florin /colonmonetary /uni20A9 /uni20A6 /guarani /peso /lira /recipe /uni203D /gnaborretni /dong /trademark /uni2031 /paragraph.alt /uni0E3F /afii61352 /discount /estimated /openbullet /uni2120 /uni2045 /uni2046 /cent /sterling /currency /yen /brokenbar /section /dieresis.ts1 /copyright /ordfeminine /copyleft /logicalnot /uni2117 /registered /macron.ts1 /degree /plusminus /twosuperior /threesuperior /acute.ts1 /mu /paragraph /periodcentered /uni203B /onesuperior /ordmasculine /radical /onequarter /onehalf /threequarters /Euro /multiply /divide",
    compressed_stream: include_bytes!("../../assets/cm_fonts/tcrm2488.flate"),
};

/// TS1 ToUnicode by code.
pub static TS1_TOUNICODE: [u16; 128] = [0xF6CE, 0xF6C9, 0x0000, 0x0000, 0xF6CB, 0xF6CF, 0x0000, 0xF6CA, 0x0000, 0xF6D0, 0x0000, 0x00B8, 0x02DB, 0x201A, 0x201E, 0x0000, 0xF6DE, 0x200C, 0x2190, 0x2192, 0x0000, 0x0000, 0x0000, 0x0000, 0x200C, 0x2422, 0x0024, 0x0027, 0x2217, 0x002C, 0x0000, 0x002E, 0x2044, 0xF730, 0xF731, 0xF732, 0xF733, 0xF734, 0xF735, 0xF736, 0xF737, 0xF738, 0xF739, 0x2329, 0x2212, 0x232A, 0x2127, 0x25CB, 0x2126, 0x301A, 0x301B, 0x2191, 0x2193, 0x0060, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x266A, 0x0000, 0x0000, 0x02D8, 0x02C7, 0x02DD, 0xF6D6, 0x2020, 0x2021, 0x2016, 0x2030, 0x2022, 0x2103, 0xF724, 0xF7A2, 0x0192, 0x20A1, 0x20A9, 0x20A6, 0x0000, 0x0000, 0x20A4, 0x0000, 0x203D, 0x0000, 0x20AB, 0x2122, 0x2031, 0x00B6, 0x0E3F, 0x2116, 0x0000, 0x212E, 0x25E6, 0x2120, 0x2045, 0x2046, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x0000, 0x00AC, 0x2117, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x203B, 0x00B9, 0x00BA, 0x221A, 0x00BC, 0x00BD, 0x00BE, 0x20AC, 0x00D7, 0x00F7];

/// The embedded program of the font file `name` (`cmr10`, `tcrm1095`): a base face or a
/// design-size master; `None` for a font that is not embedded.
pub fn program_by_name(name: &str) -> Option<&'static CmFontProgram> {
    static ALL: [&CmFontProgram; 66] = [
        &CMR10_PROGRAM, &CMMI10_PROGRAM, &CMSY10_PROGRAM, &CMEX10_PROGRAM, &CMBX10_PROGRAM, &CMTI10_PROGRAM, &CMR5_PROGRAM,
        &MSAM10_PROGRAM, &MSBM10_PROGRAM, &CMR7_PROGRAM, &CMMI7_PROGRAM, &CMMI5_PROGRAM, &CMSY7_PROGRAM, &CMMI6_PROGRAM,
        &CMMI8_PROGRAM, &CMMI9_PROGRAM, &CMMI12_PROGRAM, &CMSY6_PROGRAM, &CMSY8_PROGRAM, &CMSY9_PROGRAM, &CMSY5_PROGRAM,
        &CMTT10_PROGRAM, &CMTT8_PROGRAM, &CMSS10_PROGRAM, &CMSS8_PROGRAM, &EUFM10_PROGRAM, &EUFM7_PROGRAM, &EUFM5_PROGRAM,
        &CMMIB10_PROGRAM, &CMMIB7_PROGRAM, &CMMIB5_PROGRAM, &CMBSY10_PROGRAM, &CMBSY7_PROGRAM, &CMBSY5_PROGRAM,
        &CMEX7_PROGRAM, &CMCSC10_PROGRAM, &CMSL10_PROGRAM, &CMSSBX10_PROGRAM,
        &CMR6_PROGRAM, &CMR8_PROGRAM, &CMR9_PROGRAM, &CMR12_PROGRAM, &CMR17_PROGRAM, &CMBX5_PROGRAM, &CMBX6_PROGRAM,
        &CMBX7_PROGRAM, &CMBX8_PROGRAM, &CMBX9_PROGRAM, &CMBX12_PROGRAM, &CMTI7_PROGRAM, &CMTI8_PROGRAM, &CMTI9_PROGRAM,
        &CMTI12_PROGRAM, &CMTT9_PROGRAM, &CMTT12_PROGRAM,
        &TCRM0500_PROGRAM, &TCRM0600_PROGRAM, &TCRM0700_PROGRAM, &TCRM0800_PROGRAM, &TCRM0900_PROGRAM, &TCRM1095_PROGRAM,
        &TCRM1200_PROGRAM, &TCRM1440_PROGRAM, &TCRM1728_PROGRAM, &TCRM2074_PROGRAM, &TCRM2488_PROGRAM,
    ];
    ALL.iter().copied().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// The ToUnicode map of the font file `name`, by its encoding: OT1 text, typewriter, OML,
/// OMS, OMX, the AMS symbol fonts, or the TS1 masters' own.
pub fn tounicode_by_name(name: &str) -> Option<&'static [u16; 128]> {
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
        _ => return program_by_name(name).filter(|p| p.name.starts_with("SFRM")).map(|_| &TS1_TOUNICODE),
    })
}
