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

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_sorted() {
        assert!(super::NAMES.windows(2).all(|w| w[0] < w[1]));
    }
}
