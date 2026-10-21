//! Category codes (tex.web §207): how the tokenizer reads each character. The table holds
//! codes 0–255, as pdfTeX's does; any character above 255 reads as `Other`. The initial state
//! is LaTeX's format state: IniTeX's §232 defaults, then latex.ltx's assignments (`{`, `}`,
//! `$`, `&`, `#`, `^`, `_`, tab, form feed, `~`), with `@` other after the final `\makeatother`.
//! Grouping lives in [`super::Eqtb`]; this file is the codes and the plain table.

/// tex.web §207's sixteen category codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Cat {
    Escape = 0,
    BeginGroup = 1,
    EndGroup = 2,
    MathShift = 3,
    AlignTab = 4,
    EndLine = 5,
    Param = 6,
    Superscript = 7,
    Subscript = 8,
    Ignored = 9,
    Space = 10,
    Letter = 11,
    Other = 12,
    Active = 13,
    Comment = 14,
    Invalid = 15,
}

impl Cat {
    /// The code `n` means, if it is one of the sixteen (`\catcode` takes 0–15).
    pub const fn from_u8(n: u8) -> Option<Cat> {
        Some(match n {
            0 => Cat::Escape,
            1 => Cat::BeginGroup,
            2 => Cat::EndGroup,
            3 => Cat::MathShift,
            4 => Cat::AlignTab,
            5 => Cat::EndLine,
            6 => Cat::Param,
            7 => Cat::Superscript,
            8 => Cat::Subscript,
            9 => Cat::Ignored,
            10 => Cat::Space,
            11 => Cat::Letter,
            12 => Cat::Other,
            13 => Cat::Active,
            14 => Cat::Comment,
            15 => Cat::Invalid,
            _ => return None,
        })
    }
}

/// The catcode of each character 0–255: 256 bytes, copied at no cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatTable(pub [u8; 256]);

impl CatTable {
    /// §232 IniTeX: everything other, letters, `\` escape, `%` comment, space, carriage
    /// return end of line, `^^@` ignored, `^^?` invalid.
    pub const fn initex() -> CatTable {
        let mut t = [Cat::Other as u8; 256];
        let mut c = b'A';
        while c <= b'Z' {
            t[c as usize] = Cat::Letter as u8;
            t[(c + 32) as usize] = Cat::Letter as u8;
            c += 1;
        }
        t[b'\\' as usize] = Cat::Escape as u8;
        t[b'%' as usize] = Cat::Comment as u8;
        t[b' ' as usize] = Cat::Space as u8;
        t[b'\r' as usize] = Cat::EndLine as u8;
        t[0] = Cat::Ignored as u8;
        t[127] = Cat::Invalid as u8;
        CatTable(t)
    }

    /// latex.ltx's state when a document starts: IniTeX's table plus the format's
    /// assignments (latex.ltx lines 307–316, `~` active, `@` other after `\makeatother`).
    pub const fn latex() -> CatTable {
        let mut t = CatTable::initex().0;
        t[b'{' as usize] = Cat::BeginGroup as u8;
        t[b'}' as usize] = Cat::EndGroup as u8;
        t[b'$' as usize] = Cat::MathShift as u8;
        t[b'&' as usize] = Cat::AlignTab as u8;
        t[b'#' as usize] = Cat::Param as u8;
        t[b'^' as usize] = Cat::Superscript as u8;
        t[b'_' as usize] = Cat::Subscript as u8;
        t[b'\t' as usize] = Cat::Space as u8;
        t[0x0c] = Cat::Active as u8;
        t[b'~' as usize] = Cat::Active as u8;
        CatTable(t)
    }

    /// The catcode of `c`; a character above 255 is `Other`.
    #[inline]
    pub fn get(&self, c: char) -> Cat {
        match self.0.get(c as usize) {
            // Every entry is written through `Cat`, so it is in range.
            Some(&n) => Cat::from_u8(n).unwrap_or(Cat::Other),
            None => Cat::Other,
        }
    }
}

impl Default for CatTable {
    fn default() -> Self {
        CatTable::latex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latex_state() {
        let t = CatTable::latex();
        assert_eq!(t.get('\\'), Cat::Escape);
        assert_eq!(t.get('{'), Cat::BeginGroup);
        assert_eq!(t.get('a'), Cat::Letter);
        assert_eq!(t.get('@'), Cat::Other);
        assert_eq!(t.get('~'), Cat::Active);
        assert_eq!(t.get('\t'), Cat::Space);
        assert_eq!(t.get('é'), Cat::Other);
        assert_eq!(t.get('\u{2200}'), Cat::Other);
        assert_eq!(CatTable::initex().get('{'), Cat::Other);
    }

    #[test]
    fn codes_round_trip() {
        for n in 0..16 {
            assert_eq!(Cat::from_u8(n).map(|c| c as u8), Some(n));
        }
        assert_eq!(Cat::from_u8(16), None);
    }
}
