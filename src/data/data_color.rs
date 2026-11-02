//! The colours color.sty and xcolor predefine, as pdftex.def's literals: `\current@color`
//! after `\color{name}`, read from pdfLaTeX (`\typeout{\current@color}` for each name with
//! each package). SoA: the names sorted for `binary_search`, the literals and the package
//! flag read on a hit.

/// The names, sorted.
const NAMES: [&str; 19] = ["black", "blue", "brown", "cyan", "darkgray", "gray", "green", "lightgray", "lime", "magenta", "olive", "orange", "pink", "purple", "red", "teal", "violet", "white", "yellow"];

/// Each name's literal.
const LITERALS: [&str; 19] = [
    "0 g 0 G",
    "0 0 1 rg 0 0 1 RG",
    "0.75 0.5 0.25 rg 0.75 0.5 0.25 RG",
    "1 0 0 0 k 1 0 0 0 K",
    "0.25 g 0.25 G",
    "0.5 g 0.5 G",
    "0 1 0 rg 0 1 0 RG",
    "0.75 g 0.75 G",
    "0.75 1 0 rg 0.75 1 0 RG",
    "0 1 0 0 k 0 1 0 0 K",
    "0 0 1 0.5 k 0 0 1 0.5 K",
    "1 0.5 0 rg 1 0.5 0 RG",
    "1 0.75 0.75 rg 1 0.75 0.75 RG",
    "0.75 0 0.25 rg 0.75 0 0.25 RG",
    "1 0 0 rg 1 0 0 RG",
    "0 0.5 0.5 rg 0 0.5 0.5 RG",
    "0.5 0 0.5 rg 0.5 0 0.5 RG",
    "1 g 1 G",
    "0 0 1 0 k 0 0 1 0 K",
];

/// Whether only xcolor defines the name (color.sty has the eight of `\DefineNamedColor`'s
/// base set).
const XCOLOR_ONLY: [bool; 19] = [false, false, true, false, true, true, false, true, true, false, true, true, true, true, false, true, true, false, false];

/// The literal of predefined colour `name`, with xcolor or with color.sty.
pub fn named(name: &str, xcolor: bool) -> Option<&'static str> {
    let i = NAMES.binary_search(&name).ok()?;
    (xcolor || !XCOLOR_ONLY[i]).then_some(LITERALS[i])
}

/// xcolor's `svgnames` (svgnam.def, `\preparecolorset{rgb}`): each name and its rgb values
/// as written, in the file's order.
pub const SVG_NAMES: [(&str, &str); 151] = [("AliceBlue", ".94,.972,1"), ("AntiqueWhite", ".98,.92,.844"), ("Aqua", "0,1,1"), ("Aquamarine", ".498,1,.83"), ("Azure", ".94,1,1"), ("Beige", ".96,.96,.864"), ("Bisque", "1,.894,.77"), ("Black", "0,0,0"), ("BlanchedAlmond", "1,.92,.804"), ("Blue", "0,0,1"), ("BlueViolet", ".54,.17,.888"), ("Brown", ".648,.165,.165"), ("BurlyWood", ".87,.72,.53"), ("CadetBlue", ".372,.62,.628"), ("Chartreuse", ".498,1,0"), ("Chocolate", ".824,.41,.116"), ("Coral", "1,.498,.312"), ("CornflowerBlue", ".392,.585,.93"), ("Cornsilk", "1,.972,.864"), ("Crimson", ".864,.08,.235"), ("Cyan", "0,1,1"), ("DarkBlue", "0,0,.545"), ("DarkCyan", "0,.545,.545"), ("DarkGoldenrod", ".72,.525,.044"), ("DarkGray", ".664,.664,.664"), ("DarkGreen", "0,.392,0"), ("DarkGrey", ".664,.664,.664"), ("DarkKhaki", ".74,.716,.42"), ("DarkMagenta", ".545,0,.545"), ("DarkOliveGreen", ".332,.42,.185"), ("DarkOrange", "1,.55,0"), ("DarkOrchid", ".6,.196,.8"), ("DarkRed", ".545,0,0"), ("DarkSalmon", ".912,.59,.48"), ("DarkSeaGreen", ".56,.736,.56"), ("DarkSlateBlue", ".284,.24,.545"), ("DarkSlateGray", ".185,.31,.31"), ("DarkSlateGrey", ".185,.31,.31"), ("DarkTurquoise", "0,.808,.82"), ("DarkViolet", ".58,0,.828"), ("DeepPink", "1,.08,.576"), ("DeepSkyBlue", "0,.75,1"), ("DimGray", ".41,.41,.41"), ("DimGrey", ".41,.41,.41"), ("DodgerBlue", ".116,.565,1"), ("FireBrick", ".698,.132,.132"), ("FloralWhite", "1,.98,.94"), ("ForestGreen", ".132,.545,.132"), ("Fuchsia", "1,0,1"), ("Gainsboro", ".864,.864,.864"), ("GhostWhite", ".972,.972,1"), ("Gold", "1,.844,0"), ("Goldenrod", ".855,.648,.125"), ("Gray", ".5,.5,.5"), ("Green", "0,.5,0"), ("GreenYellow", ".68,1,.185"), ("Grey", ".5,.5,.5"), ("Honeydew", ".94,1,.94"), ("HotPink", "1,.41,.705"), ("IndianRed", ".804,.36,.36"), ("Indigo", ".294,0,.51"), ("Ivory", "1,1,.94"), ("Khaki", ".94,.9,.55"), ("Lavender", ".9,.9,.98"), ("LavenderBlush", "1,.94,.96"), ("LawnGreen", ".488,.99,0"), ("LemonChiffon", "1,.98,.804"), ("LightBlue", ".68,.848,.9"), ("LightCoral", ".94,.5,.5"), ("LightCyan", ".88,1,1"), ("LightGoldenrod", ".933,.867,.51"), ("LightGoldenrodYellow", ".98,.98,.824"), ("LightGray", ".828,.828,.828"), ("LightGreen", ".565,.932,.565"), ("LightGrey", ".828,.828,.828"), ("LightPink", "1,.712,.756"), ("LightSalmon", "1,.628,.48"), ("LightSeaGreen", ".125,.698,.668"), ("LightSkyBlue", ".53,.808,.98"), ("LightSlateBlue", ".518,.44,1"), ("LightSlateGray", ".468,.532,.6"), ("LightSlateGrey", ".468,.532,.6"), ("LightSteelBlue", ".69,.77,.87"), ("LightYellow", "1,1,.88"), ("Lime", "0,1,0"), ("LimeGreen", ".196,.804,.196"), ("Linen", ".98,.94,.9"), ("Magenta", "1,0,1"), ("Maroon", ".5,0,0"), ("MediumAquamarine", ".4,.804,.668"), ("MediumBlue", "0,0,.804"), ("MediumOrchid", ".73,.332,.828"), ("MediumPurple", ".576,.44,.86"), ("MediumSeaGreen", ".235,.7,.444"), ("MediumSlateBlue", ".484,.408,.932"), ("MediumSpringGreen", "0,.98,.604"), ("MediumTurquoise", ".284,.82,.8"), ("MediumVioletRed", ".78,.084,.52"), ("MidnightBlue", ".098,.098,.44"), ("MintCream", ".96,1,.98"), ("MistyRose", "1,.894,.884"), ("Moccasin", "1,.894,.71"), ("NavajoWhite", "1,.87,.68"), ("Navy", "0,0,.5"), ("NavyBlue", "0,0,.5"), ("OldLace", ".992,.96,.9"), ("Olive", ".5,.5,0"), ("OliveDrab", ".42,.556,.136"), ("Orange", "1,.648,0"), ("OrangeRed", "1,.27,0"), ("Orchid", ".855,.44,.84"), ("PaleGoldenrod", ".932,.91,.668"), ("PaleGreen", ".596,.985,.596"), ("PaleTurquoise", ".688,.932,.932"), ("PaleVioletRed", ".86,.44,.576"), ("PapayaWhip", "1,.936,.835"), ("PeachPuff", "1,.855,.725"), ("Peru", ".804,.52,.248"), ("Pink", "1,.752,.796"), ("Plum", ".868,.628,.868"), ("PowderBlue", ".69,.88,.9"), ("Purple", ".5,0,.5"), ("Red", "1,0,0"), ("RosyBrown", ".736,.56,.56"), ("RoyalBlue", ".255,.41,.884"), ("SaddleBrown", ".545,.27,.075"), ("Salmon", ".98,.5,.448"), ("SandyBrown", ".956,.644,.376"), ("SeaGreen", ".18,.545,.34"), ("Seashell", "1,.96,.932"), ("Sienna", ".628,.32,.176"), ("Silver", ".752,.752,.752"), ("SkyBlue", ".53,.808,.92"), ("SlateBlue", ".415,.352,.804"), ("SlateGray", ".44,.5,.565"), ("SlateGrey", ".44,.5,.565"), ("Snow", "1,.98,.98"), ("SpringGreen", "0,1,.498"), ("SteelBlue", ".275,.51,.705"), ("Tan", ".824,.705,.55"), ("Teal", "0,.5,.5"), ("Thistle", ".848,.75,.848"), ("Tomato", "1,.39,.28"), ("Turquoise", ".25,.88,.815"), ("Violet", ".932,.51,.932"), ("VioletRed", ".816,.125,.565"), ("Wheat", ".96,.87,.7"), ("White", "1,1,1"), ("WhiteSmoke", ".96,.96,.96"), ("Yellow", "1,1,0"), ("YellowGreen", ".604,.804,.196")];

/// dvipsnam.def (graphics 2016/06/17 v3.0m): each `\DefineNamedColor{named}` name and its
/// cmyk values as written, in the file's order (xcolor's and color.sty's `dvipsnames`).
pub const DVIPS_NAMES: [(&str, &str); 68] = [("GreenYellow", "0.15,0,0.69,0"), ("Yellow", "0,0,1,0"), ("Goldenrod", "0,0.10,0.84,0"), ("Dandelion", "0,0.29,0.84,0"), ("Apricot", "0,0.32,0.52,0"), ("Peach", "0,0.50,0.70,0"), ("Melon", "0,0.46,0.50,0"), ("YellowOrange", "0,0.42,1,0"), ("Orange", "0,0.61,0.87,0"), ("BurntOrange", "0,0.51,1,0"), ("Bittersweet", "0,0.75,1,0.24"), ("RedOrange", "0,0.77,0.87,0"), ("Mahogany", "0,0.85,0.87,0.35"), ("Maroon", "0,0.87,0.68,0.32"), ("BrickRed", "0,0.89,0.94,0.28"), ("Red", "0,1,1,0"), ("OrangeRed", "0,1,0.50,0"), ("RubineRed", "0,1,0.13,0"), ("WildStrawberry", "0,0.96,0.39,0"), ("Salmon", "0,0.53,0.38,0"), ("CarnationPink", "0,0.63,0,0"), ("Magenta", "0,1,0,0"), ("VioletRed", "0,0.81,0,0"), ("Rhodamine", "0,0.82,0,0"), ("Mulberry", "0.34,0.90,0,0.02"), ("RedViolet", "0.07,0.90,0,0.34"), ("Fuchsia", "0.47,0.91,0,0.08"), ("Lavender", "0,0.48,0,0"), ("Thistle", "0.12,0.59,0,0"), ("Orchid", "0.32,0.64,0,0"), ("DarkOrchid", "0.40,0.80,0.20,0"), ("Purple", "0.45,0.86,0,0"), ("Plum", "0.50,1,0,0"), ("Violet", "0.79,0.88,0,0"), ("RoyalPurple", "0.75,0.90,0,0"), ("BlueViolet", "0.86,0.91,0,0.04"), ("Periwinkle", "0.57,0.55,0,0"), ("CadetBlue", "0.62,0.57,0.23,0"), ("CornflowerBlue", "0.65,0.13,0,0"), ("MidnightBlue", "0.98,0.13,0,0.43"), ("NavyBlue", "0.94,0.54,0,0"), ("RoyalBlue", "1,0.50,0,0"), ("Blue", "1,1,0,0"), ("Cerulean", "0.94,0.11,0,0"), ("Cyan", "1,0,0,0"), ("ProcessBlue", "0.96,0,0,0"), ("SkyBlue", "0.62,0,0.12,0"), ("Turquoise", "0.85,0,0.20,0"), ("TealBlue", "0.86,0,0.34,0.02"), ("Aquamarine", "0.82,0,0.30,0"), ("BlueGreen", "0.85,0,0.33,0"), ("Emerald", "1,0,0.50,0"), ("JungleGreen", "0.99,0,0.52,0"), ("SeaGreen", "0.69,0,0.50,0"), ("Green", "1,0,1,0"), ("ForestGreen", "0.91,0,0.88,0.12"), ("PineGreen", "0.92,0,0.59,0.25"), ("LimeGreen", "0.50,0,1,0"), ("YellowGreen", "0.44,0,0.74,0"), ("SpringGreen", "0.26,0,0.76,0"), ("OliveGreen", "0.64,0,0.95,0.40"), ("RawSienna", "0,0.72,1,0.45"), ("Sepia", "0,0.83,1,0.70"), ("Brown", "0,0.81,1,0.60"), ("Tan", "0.14,0.42,0.56,0"), ("Gray", "0,0,0,0.50"), ("Black", "0,0,0,1"), ("White", "0,0,0,0")];

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_sorted() {
        assert!(super::NAMES.windows(2).all(|w| w[0] < w[1]));
    }
}
