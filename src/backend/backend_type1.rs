//! Type 1 subsetting, as pdfTeX's `writet1.c` subsets an embedded font: the CharStrings
//! dictionary keeps `.notdef`, the glyphs the document uses and the `seac` components of
//! those (`t1_mark_glyphs`), and its count is rewritten (`t1_flush_cs`). A Subrs entry no
//! kept glyph calls becomes a bare `return` (`t1_subset_charstrings`), and a built-in
//! encoding keeps only the codes used (`t1_subset_ascii_part`).
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

/// Where `what` first occurs in `t` from `from`: a scan for its first byte, compared only
/// there (a comparison at every position was most of a font's subsetting cost).
fn find(t: &[u8], from: usize, what: &[u8]) -> Option<usize> {
    let (&head, rest) = what.split_first()?;
    let mut i = from;
    while i + what.len() <= t.len() {
        i += t[i..t.len() - rest.len()].iter().position(|&b| b == head)?;
        if &t[i + 1..i + what.len()] == rest {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The CharStrings dictionary of decrypted binary text `t`: where its count is, the
/// entries (each from its `/` through its line's end), and where the last one ends.
fn entries(t: &[u8], from: usize) -> Option<(std::ops::Range<usize>, Vec<Entry>, usize)> {
    let at = find(t, from, b"/CharStrings")? + b"/CharStrings".len();
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

/// The Subrs array of decrypted binary text `t`: each entry's line (`dup i len RD <bytes>
/// NP`) and where its charstring's bytes are, indexed by subroutine number.
fn subrs(t: &[u8], at: usize) -> Option<Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>> {
    let at = at + b"/Subrs".len();
    let n = token(t, skip_space(t, at));
    let count: usize = std::str::from_utf8(&t[n.clone()]).ok()?.parse().ok()?;
    let mut i = token(t, skip_space(t, n.end)).end;
    let mut v = vec![(0..0, 0..0); count];
    loop {
        let start = skip_space(t, i);
        let dup = token(t, start);
        if &t[dup.clone()] != b"dup" {
            return Some(v);
        }
        let kr = token(t, skip_space(t, dup.end));
        let k: usize = std::str::from_utf8(&t[kr.clone()]).ok()?.parse().ok()?;
        let len_at = token(t, skip_space(t, kr.end));
        let len: usize = std::str::from_utf8(&t[len_at.clone()]).ok()?.parse().ok()?;
        let rd = token(t, skip_space(t, len_at.end));
        let cs = rd.end + 1..rd.end + 1 + len;
        t.get(cs.clone())?;
        let mut end = cs.end;
        while end < t.len() && t[end] != b'\n' && t[end] != b'\r' {
            end += 1;
        }
        if end < t.len() {
            end += 1;
        }
        *v.get_mut(k)? = (start..end, cs);
        i = end;
    }
}

/// The subroutines charstring `cs` (in clear, its first `len_iv` bytes random) calls
/// directly, numbers below `n`: `callsubr` with its number, and hint replacement's `n 1 3
/// callothersubr pop callsubr` (writet1.c `cs_mark`). Each charstring is read with a stack
/// of its own, so what it calls does not depend on who calls it: the calls form a graph,
/// and what a glyph needs is what it reaches.
fn calls(cs: &[u8], len_iv: usize, n: usize, out: &mut Vec<u32>) {
    let mut stack: Vec<i32> = Vec::new();
    let mut ps: Vec<i32> = Vec::new();
    let mut i = len_iv;
    while i < cs.len() {
        let b = cs[i];
        i += 1;
        match b {
            32..=246 => stack.push(b as i32 - 139),
            247..=250 => {
                let Some(&x) = cs.get(i) else { return };
                stack.push((b as i32 - 247) * 256 + x as i32 + 108);
                i += 1;
            }
            251..=254 => {
                let Some(&x) = cs.get(i) else { return };
                stack.push(-(b as i32 - 251) * 256 - x as i32 - 108);
                i += 1;
            }
            255 => {
                let Some(x) = cs.get(i..i + 4) else { return };
                stack.push(i32::from_be_bytes([x[0], x[1], x[2], x[3]]));
                i += 4;
            }
            10 => {
                // An operand that is no subroutine number ends the reading, as `cs_mark`'s.
                let Some(k) = stack.pop().and_then(|k| usize::try_from(k).ok()) else { return };
                if k < n && !out.contains(&(k as u32)) {
                    out.push(k as u32);
                }
            }
            12 => {
                let Some(&op) = cs.get(i) else { return };
                i += 1;
                match op {
                    // callothersubr: its arguments go to the PostScript stack.
                    16 => {
                        let (Some(_), Some(n)) = (stack.pop(), stack.pop()) else { return };
                        let n = (n.max(0) as usize).min(stack.len());
                        ps = stack.split_off(stack.len() - n);
                    }
                    // pop: one value back from the PostScript stack.
                    17 => stack.push(ps.pop().unwrap_or(0)),
                    _ => stack.clear(),
                }
            }
            _ => stack.clear(),
        }
    }
}

/// `seac`'s two StandardEncoding codes (base, accent) in charstring `cs` (in clear, its
/// first `len_iv` bytes random), if it ends that way.
fn seac(cs: &[u8], len_iv: usize) -> Option<(u8, u8)> {
    // `seac` ends its charstring, so a charstring that does not end in its two bytes has none.
    if !cs.ends_with(&[12, 6]) {
        return None;
    }
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

/// Clear text `c` with its built-in `/Encoding` cut to the codes `used` holds, one `dup
/// code /name put` line each, and `/FontName` given its subset `tag`, as writet1.c's
/// `t1_subset_ascii_part` writes them; a `StandardEncoding` font's text is unchanged there.
pub fn subset_clear(c: &[u8], used: &dyn Fn(usize) -> bool, tag: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(c.len());
    let mut from = 0;
    if let Some(at) = find(c, 0, b"/FontName") {
        let name = token(c, skip_space(c, at + b"/FontName".len()));
        if c.get(name.start) == Some(&b'/') {
            out.extend_from_slice(&c[..name.start + 1]);
            out.extend_from_slice(tag.as_bytes());
            out.push(b'+');
            from = name.start + 1;
        }
    }
    let Some(enc) = find(c, from, b"/Encoding") else {
        out.extend_from_slice(&c[from..]);
        return out;
    };
    let Some(first) = find(c, enc, b"dup ") else {
        out.extend_from_slice(&c[from..]);
        return out;
    };
    let Some(end) = find(c, first, b"readonly def").or_else(|| find(c, first, b" def")) else {
        out.extend_from_slice(&c[from..]);
        return out;
    };
    out.extend_from_slice(&c[from..first]);
    let mut i = first;
    let mut tail = first;
    while let Some(d) = find(c, i, b"dup ").filter(|&d| d < end) {
        // `dup 65 /A put`, or `dup 65/A put`: the code's digits, then the name from its `/`.
        let code_at = skip_space(c, d + 4);
        let mut code_end = code_at;
        while code_end < c.len() && c[code_end].is_ascii_digit() {
            code_end += 1;
        }
        let code = code_at..code_end;
        let name_at = skip_space(c, code_end);
        let mut name_end = name_at + 1;
        while name_end < c.len() && !c[name_end].is_ascii_whitespace() && !matches!(c[name_end], b'/' | b'[' | b'{') {
            name_end += 1;
        }
        if c.get(name_at) != Some(&b'/') {
            break;
        }
        let put = token(c, skip_space(c, name_end));
        if &c[put.clone()] != b"put" {
            break;
        }
        let keep = std::str::from_utf8(&c[code]).ok().and_then(|s| s.parse::<usize>().ok()).is_some_and(used);
        if keep {
            out.extend_from_slice(&c[d..put.end]);
            out.push(b'\n');
        }
        i = put.end;
        tail = skip_space(c, put.end);
    }
    out.extend_from_slice(&c[tail..]);
    out
}

fn push_encrypted(out: &mut Vec<u8>, cs: &[u8]) {
    let at = out.len();
    out.extend_from_slice(cs);
    crate::data::data_embed::encrypt_charstring(&mut out[at..]);
}

/// Text copied as it is, with any charstring in it encrypted: a Subrs array the parse did
/// not read whole is copied this way, so its entries are encrypted as before.
fn push_scanned(out: &mut Vec<u8>, text: &[u8]) {
    let at = out.len();
    out.extend_from_slice(text);
    crate::data::data_embed::encrypt_charstrings(&mut out[at..]);
}

/// Where a decrypted binary text's Subrs and CharStrings are, read once: what [`subset`]
/// needs of the program apart from the glyphs a document uses. `rushtex-gen` stores it
/// beside each carried `.pfb` (`data_embed`), so a run reads the font's layout from the
/// table instead of scanning the program; a font from disk is indexed when it is subset.
/// Offsets are into the binary text. Each Subrs and CharStrings entry is one row, read
/// whole when the entry is copied; the calls between charstrings are one pool with each
/// row's start (compressed sparse rows).
pub struct Index {
    len_iv: u32,
    /// The Subrs array is read and its unused entries become `return`: it lies wholly
    /// before the CharStrings count.
    subrs_read: bool,
    count: [u32; 2],
    /// The first entry's line start and the last's line end (both the dictionary's end
    /// with no entries).
    first: u32,
    last: u32,
    /// Per subroutine: `[line start, line end, cs start, cs end]`, all zero for a number the
    /// array leaves out.
    sub: Vec<[u32; 4]>,
    /// Per CharStrings entry, in program order: `[name start, name end, line start, line
    /// end, cs start, cs end]`.
    ent: Vec<[u32; 6]>,
    /// Per entry: its `seac` codes (`base << 8 | accent`), or `u32::MAX`.
    seac: Vec<u32>,
    /// The entries in name order.
    order: Vec<u32>,
    /// Where each charstring's direct calls start in `calls`: the subroutines' rows, then
    /// the entries', and the end.
    call_at: Vec<u32>,
    calls: Vec<u32>,
}

/// The stored form's version.
const LAYOUT: u32 = 2;

impl Index {
    /// The index as numbers for the table. A row is packed from its line start: the name
    /// (or `dup k n RD `) length, the bytes up to the charstring, the charstring's length
    /// and the line's tail each fit their field in every carried font; `None` when one
    /// does not, and the font is then indexed when it is subset.
    pub fn to_u32s(&self) -> Option<Vec<u32>> {
        let mut v = vec![LAYOUT, self.len_iv, self.subrs_read as u32, self.count[0], self.count[1], self.first, self.last, self.sub.len() as u32, self.ent.len() as u32, self.calls.len() as u32];
        // `[line start, cs start - line start (8 bits) | cs length (16) | tail (8)]`.
        let pack = |line: u32, cs: [u32; 2], end: u32| -> Option<[u32; 2]> {
            let (off, len, tail) = (cs[0].checked_sub(line)?, cs[1].checked_sub(cs[0])?, end.checked_sub(cs[1])?);
            (off < 256 && len < 65536 && tail < 256).then_some([line, off << 24 | len << 8 | tail])
        };
        for &[l0, l1, c0, c1] in &self.sub {
            v.extend(pack(l0, [c0, c1], l1)?);
        }
        for (e, &[n0, n1, l0, l1, c0, c1]) in self.ent.iter().enumerate() {
            // A name starts after the line's `/`; its length shares the seac word's top.
            if n0 != l0 + 1 || n1 - n0 >= 256 || self.order[e] >= 65536 {
                return None;
            }
            v.extend(pack(l0, [c0, c1], l1)?);
            let seac = if self.seac[e] == u32::MAX { 0 } else { self.seac[e] | 1 << 16 };
            v.push((n1 - n0) << 24 | seac);
            v.push(self.order[e]);
        }
        v.extend(&self.call_at);
        v.extend(&self.calls);
        Some(v)
    }

    /// The index [`Index::to_u32s`] wrote; `None` if `v` is not one of this layout.
    pub fn from_u32s(v: &[u32]) -> Option<Index> {
        let [LAYOUT, len_iv, subrs_read, c0, c1, first, last, ns, ne, nc] = *v.get(..10)? else { return None };
        let (ns, ne, nc) = (ns as usize, ne as usize, nc as usize);
        let rows = v.get(10..10 + 2 * ns + 4 * ne + ns + ne + 1 + nc)?;
        let unpack = |r: &[u32]| {
            let (line, w) = (r[0], r[1]);
            let cs = line + (w >> 24);
            let end = cs + (w >> 8 & 0xffff);
            [line, end + (w & 255), cs, end]
        };
        let sub = rows[..2 * ns].as_chunks::<2>().0.iter().map(|r| unpack(r)).collect();
        let (mut ent, mut seac, mut order) = (Vec::with_capacity(ne), Vec::with_capacity(ne), Vec::with_capacity(ne));
        for r in rows[2 * ns..2 * ns + 4 * ne].as_chunks::<4>().0 {
            let [l0, l1, c0, c1] = unpack(&r[..2]);
            ent.push([l0 + 1, l0 + 1 + (r[2] >> 24), l0, l1, c0, c1]);
            seac.push(if r[2] & 1 << 16 == 0 { u32::MAX } else { r[2] & 0xffff });
            order.push(r[3]);
        }
        let call_at = rows[2 * ns + 4 * ne..2 * ns + 4 * ne + ns + ne + 1].to_vec();
        let calls = rows[2 * ns + 4 * ne + ns + ne + 1..].to_vec();
        Some(Index { len_iv, subrs_read: subrs_read != 0, count: [c0, c1], first, last, sub, ent, seac, order, call_at, calls })
    }
}

fn range(r: [u32; 2]) -> std::ops::Range<usize> {
    r[0] as usize..r[1] as usize
}

/// The layout of decrypted binary text `t`; `None` if its dictionary cannot be read.
pub fn index(t: &[u8]) -> Option<Index> {
    // Each search covers only the part of the program its key lives in: the CharStrings
    // after the Subrs, and `/lenIV` among the Private dictionary's lines before either,
    // where writet1.c's `t1_scan_param` reads it.
    let subrs_at = find(t, 0, b"/Subrs");
    let sub = subrs_at.and_then(|at| subrs(t, at));
    let from = sub.as_ref().and_then(|s| s.iter().map(|e| e.0.end).max()).unwrap_or(0);
    let (count, list, end) = entries(t, from).or_else(|| entries(t, 0))?;
    let head_end = subrs_at.unwrap_or(count.start).min(count.start);
    let len_iv = find(&t[..head_end], 0, b"/lenIV").map_or(Some(4), |p| std::str::from_utf8(&t[token(t, skip_space(t, p + 6))]).ok()?.parse::<usize>().ok())?;
    let sub = sub.filter(|s| s.iter().all(|e| e.1.start <= count.start)).unwrap_or_default();
    let mut order: Vec<u32> = (0..list.len() as u32).collect();
    order.sort_by(|&a, &b| t[list[a as usize].name.clone()].cmp(&t[list[b as usize].name.clone()]));
    let (mut call_at, mut pool) = (Vec::with_capacity(sub.len() + list.len() + 1), Vec::new());
    for cs in sub.iter().map(|e| &e.1).chain(list.iter().map(|e| &e.cs)) {
        call_at.push(pool.len() as u32);
        let mut c = Vec::new();
        if !sub.is_empty() && !cs.is_empty() {
            calls(&t[cs.clone()], len_iv, sub.len(), &mut c);
        }
        pool.extend(c);
    }
    call_at.push(pool.len() as u32);
    let r = |r: &std::ops::Range<usize>| [r.start as u32, r.end as u32];
    Some(Index {
        len_iv: len_iv as u32,
        subrs_read: !sub.is_empty(),
        count: r(&count),
        first: list.first().map_or(end, |e| e.line.start) as u32,
        last: list.last().map_or(end, |e| e.line.end) as u32,
        sub: sub.iter().map(|(line, cs)| { let (a, b) = (r(line), r(cs)); [a[0], a[1], b[0], b[1]] }).collect(),
        seac: list.iter().map(|e| seac(&t[e.cs.clone()], len_iv).map_or(u32::MAX, |(b, a)| (b as u32) << 8 | a as u32)).collect(),
        ent: list.iter().map(|e| { let (n, l, c) = (r(&e.name), r(&e.line), r(&e.cs)); [n[0], n[1], l[0], l[1], c[0], c[1]] }).collect(),
        order,
        call_at,
        calls: pool,
    })
}

/// The binary text `t` (decrypted) with its CharStrings cut to `.notdef`, the glyphs
/// `used` names and their `seac` components, every charstring written encrypted again
/// (only the eexec layer is left to the caller); `None` if the dictionary cannot be read
/// (the caller then keeps the font whole).
pub fn subset(t: &[u8], used: &[&str]) -> Option<Vec<u8>> {
    subset_with(t, &index(t)?, used, false)
}

/// [`subset`] with the program's layout already known. With `ready`, `t`'s charstrings
/// are encrypted already (a carried font, `data_embed`'s `PFB_READY`), so what is kept
/// is copied.
pub fn subset_with(t: &[u8], ix: &Index, used: &[&str], ready: bool) -> Option<Vec<u8>> {
    let push_encrypted = |out: &mut Vec<u8>, cs: &[u8]| if ready { out.extend_from_slice(cs) } else { push_encrypted(out, cs) };
    let push_scanned = |out: &mut Vec<u8>, text: &[u8]| if ready { out.extend_from_slice(text) } else { push_scanned(out, text) };
    let n = ix.ent.len();
    if ix.last as usize > t.len() {
        return None;
    }
    let name = |e: usize| &t[range([ix.ent[e][0], ix.ent[e][1]])];
    // Each name the document uses, found among the entries in name order.
    let mut kept = vec![false; n];
    let keep = |want: &[u8], kept: &mut Vec<bool>| {
        if let Ok(k) = ix.order.binary_search_by(|&e| name(e as usize).cmp(want)) {
            kept[ix.order[k] as usize] = true;
        }
    };
    for u in used {
        keep(u.as_bytes(), &mut kept);
    }
    keep(b".notdef", &mut kept);
    // The `seac` components of the glyphs used (not of the components themselves).
    let parts: Vec<u32> = (0..n).filter(|&e| kept[e] && ix.seac[e] != u32::MAX).map(|e| ix.seac[e]).collect();
    for v in parts {
        keep(STANDARD[(v >> 8) as usize].as_bytes(), &mut kept);
        keep(STANDARD[(v & 255) as usize].as_bytes(), &mut kept);
    }
    let mut out = Vec::with_capacity(t.len());
    let len_iv = ix.len_iv as usize;
    // Subrs 0 to 3 (flex and hint replacement) always stay, as writet1.c keeps them.
    let mut from = 0;
    if ix.subrs_read {
        // What the kept glyphs reach through the calls; 0 to 3 are kept and not followed,
        // as `cs_mark` meets them already marked.
        let ns = ix.sub.len();
        let mut marked: Vec<bool> = (0..ns).map(|k| k < 4).collect();
        let mut todo: Vec<usize> = (0..n).filter(|&e| kept[e]).map(|e| ns + e).collect();
        while let Some(c) = todo.pop() {
            for &k in &ix.calls[range([ix.call_at[c], ix.call_at[c + 1]])] {
                if !marked[k as usize] {
                    marked[k as usize] = true;
                    todo.push(k as usize);
                }
            }
        }
        for (k, &[l0, l1, c0, c1]) in ix.sub.iter().enumerate() {
            let (line, cs) = (l0 as usize..l1 as usize, c0 as usize..c1 as usize);
            if line.is_empty() {
                continue;
            }
            push_scanned(&mut out, &t[from..line.start]);
            if marked[k] || cs.len() <= len_iv {
                out.extend_from_slice(&t[line.start..cs.start]);
                push_encrypted(&mut out, &t[cs.clone()]);
            } else {
                // `dup k n RD`, the random bytes, `return`, and the line's own ending.
                out.extend_from_slice(b"dup ");
                crate::backend::backend_pdftex::print_int(&mut out, k as i64);
                out.push(b' ');
                crate::backend::backend_pdftex::print_int(&mut out, len_iv as i64 + 1);
                out.extend_from_slice(&t[cs.start - 4..cs.start]);
                let at = out.len();
                out.extend_from_slice(&t[cs.start..cs.start + len_iv]);
                if ready {
                    crate::data::data_embed::decrypt_charstring(&mut out[at..]);
                }
                out.push(11);
                crate::data::data_embed::encrypt_charstring(&mut out[at..]);
            }
            out.extend_from_slice(&t[cs.end..line.end]);
            from = line.end;
        }
    }
    let count = range(ix.count);
    push_scanned(&mut out, &t[from..count.start]);
    crate::backend::backend_pdftex::print_int(&mut out, kept.iter().filter(|&&k| k).count() as i64);
    push_scanned(&mut out, &t[count.end..ix.first as usize]);
    for e in (0..n).filter(|&e| kept[e]) {
        let [_, _, l0, l1, c0, c1] = ix.ent[e];
        out.extend_from_slice(&t[l0 as usize..c0 as usize]);
        push_encrypted(&mut out, &t[c0 as usize..c1 as usize]);
        out.extend_from_slice(&t[c1 as usize..l1 as usize]);
    }
    // The space before the dictionary's `end`.
    push_scanned(&mut out, &t[ix.last as usize..]);
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
        let (_, all, _) = entries(&binary, 0).expect("a CharStrings dictionary");
        assert!(all.len() > 100);
        // A carried font comes with its layout and its charstrings encrypted.
        let ix = emb.layout("cmr12.pfb").and_then(|v| Index::from_u32s(&v)).expect("a stored layout");
        let out = subset_with(&binary, &ix, &["A", "B"], true).expect("subset");
        let (count, kept, _) = entries(&out, 0).expect("the subset reads back");
        let names: Vec<&[u8]> = kept.iter().map(|e| &out[e.name.clone()]).collect();
        assert_eq!(names.len(), 3, "{:?}", names.iter().map(|n| String::from_utf8_lossy(n)).collect::<Vec<_>>());
        assert_eq!(&out[count], b"3");
        assert!(out.len() < binary.len() / 2);
    }
}
