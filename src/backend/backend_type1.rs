//! Type 1 subsetting, as pdfTeX's `writet1.c` subsets an embedded font: the CharStrings
//! dictionary keeps `.notdef`, the glyphs the document uses and the `seac` components of
//! those (`t1_mark_glyphs`), and its count is rewritten (`t1_flush_cs`). The clear text and
//! the Subrs are kept whole.
//!
//! It works on the decrypted program (eexec and charstrings in clear), which is how
//! `data_embed` stores it: only what is kept is encrypted again.

/// `writet1.c`'s `standard_glyph_names`: Adobe StandardEncoding, for `seac`'s codes.
const STANDARD: [&str; 256] = {
    let mut t = [".notdef"; 256];
    let names: [(usize, &str); 149] = [
        (0x20, "space"), (0x21, "exclam"), (0x22, "quotedbl"), (0x23, "numbersign"), (0x24, "dollar"), (0x25, "percent"), (0x26, "ampersand"), (0x27, "quoteright"),
        (0x28, "parenleft"), (0x29, "parenright"), (0x2a, "asterisk"), (0x2b, "plus"), (0x2c, "comma"), (0x2d, "hyphen"), (0x2e, "period"), (0x2f, "slash"),
        (0x30, "zero"), (0x31, "one"), (0x32, "two"), (0x33, "three"), (0x34, "four"), (0x35, "five"), (0x36, "six"), (0x37, "seven"),
        (0x38, "eight"), (0x39, "nine"), (0x3a, "colon"), (0x3b, "semicolon"), (0x3c, "less"), (0x3d, "equal"), (0x3e, "greater"), (0x3f, "question"),
        (0x40, "at"), (0x41, "A"), (0x42, "B"), (0x43, "C"), (0x44, "D"), (0x45, "E"), (0x46, "F"), (0x47, "G"),
        (0x48, "H"), (0x49, "I"), (0x4a, "J"), (0x4b, "K"), (0x4c, "L"), (0x4d, "M"), (0x4e, "N"), (0x4f, "O"),
        (0x50, "P"), (0x51, "Q"), (0x52, "R"), (0x53, "S"), (0x54, "T"), (0x55, "U"), (0x56, "V"), (0x57, "W"),
        (0x58, "X"), (0x59, "Y"), (0x5a, "Z"), (0x5b, "bracketleft"), (0x5c, "backslash"), (0x5d, "bracketright"), (0x5e, "asciicircum"), (0x5f, "underscore"),
        (0x60, "quoteleft"), (0x61, "a"), (0x62, "b"), (0x63, "c"), (0x64, "d"), (0x65, "e"), (0x66, "f"), (0x67, "g"),
        (0x68, "h"), (0x69, "i"), (0x6a, "j"), (0x6b, "k"), (0x6c, "l"), (0x6d, "m"), (0x6e, "n"), (0x6f, "o"),
        (0x70, "p"), (0x71, "q"), (0x72, "r"), (0x73, "s"), (0x74, "t"), (0x75, "u"), (0x76, "v"), (0x77, "w"),
        (0x78, "x"), (0x79, "y"), (0x7a, "z"), (0x7b, "braceleft"), (0x7c, "bar"), (0x7d, "braceright"), (0x7e, "asciitilde"),
        (0xa1, "exclamdown"), (0xa2, "cent"), (0xa3, "sterling"), (0xa4, "fraction"), (0xa5, "yen"), (0xa6, "florin"), (0xa7, "section"), (0xa8, "currency"),
        (0xa9, "quotesingle"), (0xaa, "quotedblleft"), (0xab, "guillemotleft"), (0xac, "guilsinglleft"), (0xad, "guilsinglright"), (0xae, "fi"), (0xaf, "fl"),
        (0xb1, "endash"), (0xb2, "dagger"), (0xb3, "daggerdbl"), (0xb4, "periodcentered"), (0xb6, "paragraph"), (0xb7, "bullet"), (0xb8, "quotesinglbase"),
        (0xb9, "quotedblbase"), (0xba, "quotedblright"), (0xbb, "guillemotright"), (0xbc, "ellipsis"), (0xbd, "perthousand"), (0xbf, "questiondown"),
        (0xc1, "grave"), (0xc2, "acute"), (0xc3, "circumflex"), (0xc4, "tilde"), (0xc5, "macron"), (0xc6, "breve"), (0xc7, "dotaccent"), (0xc8, "dieresis"),
        (0xca, "ring"), (0xcb, "cedilla"), (0xcd, "hungarumlaut"), (0xce, "ogonek"), (0xcf, "caron"), (0xd0, "emdash"),
        (0xe1, "AE"), (0xe3, "ordfeminine"), (0xe8, "Lslash"), (0xe9, "Oslash"), (0xea, "OE"), (0xeb, "ordmasculine"),
        (0xf1, "ae"), (0xf5, "dotlessi"), (0xf8, "lslash"), (0xf9, "oslash"), (0xfa, "oe"), (0xfb, "germandbls"),
    ];
    let mut i = 0;
    while i < names.len() {
        t[names[i].0] = names[i].1;
        i += 1;
    }
    t
};

/// One CharStrings entry: its name and its line (`/name len RD <bytes> ND`), by offsets
/// into the binary text, and where its charstring's bytes are.
struct Entry {
    name: std::ops::Range<usize>,
    line: std::ops::Range<usize>,
    cs: std::ops::Range<usize>,
}

fn skip_space(t: &[u8], mut i: usize) -> usize {
    while i < t.len() && t[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn token(t: &[u8], i: usize) -> std::ops::Range<usize> {
    let mut j = i;
    while j < t.len() && !t[j].is_ascii_whitespace() {
        j += 1;
    }
    i..j
}

fn find(t: &[u8], from: usize, what: &[u8]) -> Option<usize> {
    t.get(from..)?.windows(what.len()).position(|w| w == what).map(|p| p + from)
}

/// The CharStrings dictionary of decrypted binary text `t`: where its count is, the
/// entries (each from its `/` through its line's end), and where the last one ends.
fn entries(t: &[u8]) -> Option<(std::ops::Range<usize>, Vec<Entry>, usize)> {
    let at = find(t, 0, b"/CharStrings")? + b"/CharStrings".len();
    let n = token(t, skip_space(t, at));
    std::str::from_utf8(&t[n.clone()]).ok()?.parse::<usize>().ok()?;
    let mut i = find(t, n.end, b"begin")? + 5;
    let mut v = Vec::new();
    loop {
        let start = skip_space(t, i);
        if t.get(start) != Some(&b'/') {
            return Some((n, v, i));
        }
        let name = token(t, start + 1);
        let len_at = token(t, skip_space(t, name.end));
        let len: usize = std::str::from_utf8(&t[len_at.clone()]).ok()?.parse().ok()?;
        let rd = token(t, skip_space(t, len_at.end));
        let cs = rd.end + 1..rd.end + 1 + len;
        t.get(cs.clone())?;
        // `ND` (or `|-`, `noaccess def`) to the end of its line, the line's end included.
        let mut end = cs.end;
        while end < t.len() && t[end] != b'\n' && t[end] != b'\r' {
            end += 1;
        }
        if end < t.len() {
            end += 1;
        }
        v.push(Entry { name, line: start..end, cs });
        i = end;
    }
}

/// `seac`'s two StandardEncoding codes (base, accent) in charstring `cs` (in clear, its
/// first `len_iv` bytes random), if it ends that way.
fn seac(cs: &[u8], len_iv: usize) -> Option<(u8, u8)> {
    let mut stack: Vec<i32> = Vec::new();
    let mut i = len_iv;
    while i < cs.len() {
        let b = cs[i];
        i += 1;
        match b {
            32..=246 => stack.push(b as i32 - 139),
            247..=250 => {
                stack.push((b as i32 - 247) * 256 + *cs.get(i)? as i32 + 108);
                i += 1;
            }
            251..=254 => {
                stack.push(-(b as i32 - 251) * 256 - *cs.get(i)? as i32 - 108);
                i += 1;
            }
            255 => {
                stack.push(i32::from_be_bytes(cs.get(i..i + 4)?.try_into().ok()?));
                i += 4;
            }
            12 => {
                let op = *cs.get(i)?;
                i += 1;
                if op == 6 && stack.len() >= 5 {
                    let n = stack.len();
                    return Some((u8::try_from(stack[n - 2]).ok()?, u8::try_from(stack[n - 1]).ok()?));
                }
                stack.clear();
            }
            _ => stack.clear(),
        }
    }
    None
}

/// The binary text `t` (decrypted) with its CharStrings cut to `.notdef`, the glyphs
/// `used` names and their `seac` components; `None` if the dictionary cannot be read
/// (the caller then keeps the font whole).
pub fn subset(t: &[u8], used: &[&str]) -> Option<Vec<u8>> {
    let (count, list, end) = entries(t)?;
    let len_iv = find(t, 0, b"/lenIV").map_or(Some(4), |p| std::str::from_utf8(&t[token(t, skip_space(t, p + 6))]).ok()?.parse::<usize>().ok())?;
    let name = |e: &Entry| &t[e.name.clone()];
    let mut keep: Vec<&[u8]> = used.iter().map(|n| n.as_bytes()).collect();
    keep.push(b".notdef");
    for e in &list {
        if keep.contains(&name(e)) {
            if let Some((b, a)) = seac(&t[e.cs.clone()], len_iv) {
                keep.push(STANDARD[b as usize].as_bytes());
                keep.push(STANDARD[a as usize].as_bytes());
            }
        }
    }
    let kept: Vec<&Entry> = list.iter().filter(|e| keep.contains(&name(e))).collect();
    let mut out = Vec::with_capacity(t.len());
    out.extend_from_slice(&t[..count.start]);
    out.extend_from_slice(kept.len().to_string().as_bytes());
    let first = list.first().map_or(end, |e| e.line.start);
    out.extend_from_slice(&t[count.end..first]);
    for e in &kept {
        out.extend_from_slice(&t[e.line.clone()]);
    }
    // The space before the dictionary's `end`.
    let last = list.last().map_or(end, |e| e.line.end);
    out.extend_from_slice(&t[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cmr12's program cut to two glyphs: `.notdef`, `A` and `Aring`'s `seac` parts stay,
    /// and the count says so.
    #[test]
    fn subset_keeps_used_glyphs_and_seac_parts() {
        let Some(emb) = crate::data::data_embed::embedded() else { return };
        let Some(pfb) = emb.read("cmr12.pfb#clear") else { return };
        let segs = |b: &[u8]| -> Vec<(u8, std::ops::Range<usize>)> {
            let (mut i, mut v) = (0, Vec::new());
            while i + 6 <= b.len() && b[i] == 0x80 && b[i + 1] != 3 {
                let n = u32::from_le_bytes([b[i + 2], b[i + 3], b[i + 4], b[i + 5]]) as usize;
                v.push((b[i + 1], i + 6..i + 6 + n));
                i += 6 + n;
            }
            v
        };
        let binary: Vec<u8> = segs(&pfb).into_iter().filter(|s| s.0 == 2).flat_map(|s| pfb[s.1].to_vec()).collect();
        let (_, all, _) = entries(&binary).expect("a CharStrings dictionary");
        assert!(all.len() > 100);
        let out = subset(&binary, &["A", "B"]).expect("subset");
        let (count, kept, _) = entries(&out).expect("the subset reads back");
        let names: Vec<&[u8]> = kept.iter().map(|e| &out[e.name.clone()]).collect();
        assert_eq!(names.len(), 3, "{:?}", names.iter().map(|n| String::from_utf8_lossy(n)).collect::<Vec<_>>());
        assert_eq!(&out[count], b"3");
        assert!(out.len() < binary.len() / 2);
    }
}
