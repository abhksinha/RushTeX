//! POSIX extended regular expressions as pdfTeX's `\pdfmatch` uses them (utils.c
//! `matchstrings`: `regcomp` with `REG_EXTENDED`, optionally `REG_ICASE`, then `regexec`), in
//! our own small form: a parsed tree matched by backtracking over bytes.
//!
//! `regexec` finds the leftmost match and, of those starting there, the longest; the match
//! here does the same by trying every way through the pattern from each start and keeping
//! the longest end. Of equally long matches the first found keeps its subexpressions, which
//! for greedy patterns is POSIX's own choice. The syntax is POSIX ERE: `.`, bracket
//! expressions with ranges and `[:class:]`, `^` and `$`, groups, `|`, `*`, `+`, `?`, intervals
//! `{m}`, `{m,}`, `{m,n}`, and `\c` for a literal `c`.
//!
//! Where POSIX leaves a pattern undefined, the rules are those of the reference pdfTeX's
//! regex library (the BSD one), measured with `\pdfmatch`: an empty pattern or an empty
//! branch (`a|`, `(|a)`) is an error, an empty group `()` is not; a repetition with nothing to
//! repeat (`*a`, `^*a`, `a**`, `a{1}*`) is an error; `{` followed by a digit must begin a
//! valid interval, and any other `{` is a literal.

/// One node of a pattern.
#[derive(Debug)]
enum Node {
    Byte(u8),
    Any,
    Set(Box<[u64; 4]>),
    Start,
    End,
    Group(usize, Box<Node>),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Repeat(Box<Node>, u32, Option<u32>),
}

/// A compiled pattern and the number of its subexpressions.
#[derive(Debug)]
pub struct Regex {
    root: Node,
    groups: usize,
}

/// Where each subexpression matched (0 the whole match), as `regmatch_t` gives it.
pub type Captures = Vec<Option<(usize, usize)>>;

/// The work a match may do before it is abandoned, so that no pattern can hang the engine.
const BUDGET: u32 = 1 << 22;

struct Parser<'p> {
    p: &'p [u8],
    at: usize,
    groups: usize,
    icase: bool,
}

fn set_has(set: &[u64; 4], c: u8) -> bool {
    set[(c >> 6) as usize] & (1 << (c & 63)) != 0
}

fn set_add(set: &mut [u64; 4], c: u8) {
    set[(c >> 6) as usize] |= 1 << (c & 63);
}

/// A POSIX character class (`[:alpha:]`) in the C locale.
fn class(name: &[u8], c: u8) -> Option<bool> {
    Some(match name {
        b"alpha" => c.is_ascii_alphabetic(),
        b"digit" => c.is_ascii_digit(),
        b"alnum" => c.is_ascii_alphanumeric(),
        b"upper" => c.is_ascii_uppercase(),
        b"lower" => c.is_ascii_lowercase(),
        b"space" => matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C),
        b"blank" => matches!(c, b' ' | b'\t'),
        b"punct" => c.is_ascii_punctuation(),
        b"print" => (0x20..0x7F).contains(&c),
        b"graph" => (0x21..0x7F).contains(&c),
        b"cntrl" => c < 0x20 || c == 0x7F,
        b"xdigit" => c.is_ascii_hexdigit(),
        _ => return None,
    })
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.p.get(self.at).copied()
    }

    fn alt(&mut self) -> Result<Node, ()> {
        let mut alts = vec![self.seq()?];
        while self.peek() == Some(b'|') {
            self.at += 1;
            alts.push(self.seq()?);
        }
        // BSD regcomp: no empty branch.
        if alts.len() > 1 && alts.iter().any(|a| matches!(a, Node::Seq(v) if v.is_empty())) {
            return Err(());
        }
        Ok(if alts.len() == 1 { alts.pop().unwrap_or(Node::Seq(Vec::new())) } else { Node::Alt(alts) })
    }

    fn seq(&mut self) -> Result<Node, ()> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == b'|' || c == b')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quantified(atom)?);
        }
        Ok(Node::Seq(items))
    }

    /// An interval `{m}`, `{m,}` or `{m,n}` at `self.at` (just past `{`), if there is one.
    fn interval(&mut self) -> Option<(u32, Option<u32>)> {
        let save = self.at;
        let num = |s: &mut Self| {
            let start = s.at;
            while s.peek().is_some_and(|c| c.is_ascii_digit()) {
                s.at += 1;
            }
            std::str::from_utf8(&s.p[start..s.at]).ok().and_then(|t| t.parse::<u32>().ok())
        };
        let Some(m) = num(self) else {
            self.at = save;
            return None;
        };
        let n = if self.peek() == Some(b',') {
            self.at += 1;
            num(self)
        } else {
            Some(m)
        };
        if self.peek() != Some(b'}') || n.is_some_and(|n| n < m) {
            self.at = save;
            return None;
        }
        self.at += 1;
        Some((m, n))
    }

    /// A repetition operator at `self.at`, consumed: `Ok(None)` when there is none, an error
    /// for `{` and a digit that do not make a valid interval.
    fn repetition(&mut self) -> Result<Option<(u32, Option<u32>)>, ()> {
        let q = match self.peek() {
            Some(b'*') => (0, None),
            Some(b'+') => (1, None),
            Some(b'?') => (0, Some(1)),
            Some(b'{') if self.p.get(self.at + 1).is_some_and(u8::is_ascii_digit) => {
                self.at += 1;
                return self.interval().map(Some).ok_or(());
            }
            _ => return Ok(None),
        };
        self.at += 1;
        Ok(Some(q))
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, ()> {
        let Some((min, max)) = self.repetition()? else { return Ok(atom) };
        // Nothing to repeat after `^`, and no second operator on the same atom.
        if matches!(atom, Node::Start) || self.repetition()?.is_some() {
            return Err(());
        }
        Ok(Node::Repeat(Box::new(atom), min, max))
    }

    fn atom(&mut self) -> Result<Node, ()> {
        let c = self.peek().ok_or(())?;
        self.at += 1;
        Ok(match c {
            b'.' => Node::Any,
            b'^' => Node::Start,
            b'$' => Node::End,
            b'(' => {
                self.groups += 1;
                let n = self.groups;
                let inner = self.alt()?;
                if self.peek() != Some(b')') {
                    return Err(());
                }
                self.at += 1;
                Node::Group(n, Box::new(inner))
            }
            b'[' => self.bracket()?,
            b'\\' => {
                let c = self.peek().ok_or(())?;
                self.at += 1;
                self.byte(c)
            }
            b'*' | b'+' | b'?' => return Err(()),
            b'{' if self.peek().is_some_and(|d| d.is_ascii_digit()) => return Err(()),
            _ => self.byte(c),
        })
    }

    /// A literal byte; with `icase`, a set of both cases.
    fn byte(&self, c: u8) -> Node {
        if self.icase && c.is_ascii_alphabetic() {
            let mut set = [0u64; 4];
            set_add(&mut set, c.to_ascii_lowercase());
            set_add(&mut set, c.to_ascii_uppercase());
            Node::Set(Box::new(set))
        } else {
            Node::Byte(c)
        }
    }

    /// A bracket expression, just past `[`.
    fn bracket(&mut self) -> Result<Node, ()> {
        let mut set = [0u64; 4];
        let negate = self.peek() == Some(b'^');
        if negate {
            self.at += 1;
        }
        let mut first = true;
        loop {
            let c = self.peek().ok_or(())?;
            if c == b']' && !first {
                self.at += 1;
                break;
            }
            first = false;
            if c == b'[' && self.p.get(self.at + 1) == Some(&b':') {
                let end = self.p[self.at + 2..].windows(2).position(|w| w == b":]").ok_or(())? + self.at + 2;
                let name = &self.p[self.at + 2..end];
                for b in 0..=255u8 {
                    if class(name, b).ok_or(())? {
                        set_add(&mut set, b);
                    }
                }
                self.at = end + 2;
                continue;
            }
            self.at += 1;
            let hi = if self.peek() == Some(b'-') && self.p.get(self.at + 1).is_some_and(|&d| d != b']') {
                let d = self.p[self.at + 1];
                self.at += 2;
                d
            } else {
                c
            };
            if hi < c {
                return Err(());
            }
            for b in c..=hi {
                set_add(&mut set, b);
            }
        }
        if self.icase {
            for b in b'a'..=b'z' {
                if set_has(&set, b) || set_has(&set, b.to_ascii_uppercase()) {
                    set_add(&mut set, b);
                    set_add(&mut set, b.to_ascii_uppercase());
                }
            }
        }
        if negate {
            for w in &mut set {
                *w = !*w;
            }
        }
        Ok(Node::Set(Box::new(set)))
    }
}

struct Matcher<'s> {
    s: &'s [u8],
    caps: Captures,
    best: Option<(usize, Captures)>,
    budget: u32,
}

impl Matcher<'_> {
    /// Tries `node` at `pos`, then the rest of the pattern (`k`); `true` stops the search.
    fn node(&mut self, node: &Node, pos: usize, k: &mut dyn FnMut(&mut Self, usize) -> bool) -> bool {
        if self.budget == 0 {
            return true;
        }
        self.budget -= 1;
        match node {
            Node::Byte(c) => self.s.get(pos) == Some(c) && k(self, pos + 1),
            Node::Any => pos < self.s.len() && k(self, pos + 1),
            Node::Set(set) => self.s.get(pos).is_some_and(|&c| set_has(set, c)) && k(self, pos + 1),
            Node::Start => pos == 0 && k(self, pos),
            Node::End => pos == self.s.len() && k(self, pos),
            Node::Group(n, inner) => {
                let n = *n;
                let saved = self.caps[n];
                let stop = self.node(inner, pos, &mut |m: &mut Self, end| {
                    let before = m.caps[n];
                    m.caps[n] = Some((pos, end));
                    let stop = k(m, end);
                    m.caps[n] = before;
                    stop
                });
                self.caps[n] = saved;
                stop
            }
            Node::Seq(items) => self.seq(items, pos, k),
            Node::Alt(alts) => alts.iter().any(|a| self.node(a, pos, k)),
            Node::Repeat(inner, min, max) => self.repeat(inner, *min, *max, 0, pos, k),
        }
    }

    fn seq(&mut self, items: &[Node], pos: usize, k: &mut dyn FnMut(&mut Self, usize) -> bool) -> bool {
        match items.split_first() {
            None => k(self, pos),
            Some((first, rest)) => self.node(first, pos, &mut |m: &mut Self, p| m.seq(rest, p, k)),
        }
    }

    /// `count` repetitions done; greedy, and a repetition that matches nothing ends the loop.
    fn repeat(&mut self, inner: &Node, min: u32, max: Option<u32>, count: u32, pos: usize, k: &mut dyn FnMut(&mut Self, usize) -> bool) -> bool {
        if max.is_none_or(|m| count < m) {
            let more = self.node(inner, pos, &mut |m: &mut Self, p| {
                if p == pos && count >= min {
                    return false;
                }
                m.repeat(inner, min, max, count + 1, p, k)
            });
            if more {
                return true;
            }
        }
        count >= min && k(self, pos)
    }
}

impl Regex {
    /// `regcomp`; `None` when the pattern is not a valid ERE (pdfTeX then gives `-1`).
    pub fn new(pattern: &[u8], icase: bool) -> Option<Regex> {
        if pattern.is_empty() {
            return None;
        }
        let mut p = Parser { p: pattern, at: 0, groups: 0, icase };
        let root = p.alt().ok()?;
        if p.at != pattern.len() {
            return None;
        }
        Some(Regex { root, groups: p.groups })
    }

    /// `regexec`: the leftmost-longest match in `s` and its subexpressions, or `None`.
    pub fn exec(&self, s: &[u8]) -> Option<Captures> {
        let mut m = Matcher { s, caps: vec![None; self.groups + 1], best: None, budget: BUDGET };
        for start in 0..=s.len() {
            m.node(&self.root, start, &mut |m: &mut Matcher, end| {
                if m.best.as_ref().is_none_or(|b| end > b.0) {
                    let mut caps = m.caps.clone();
                    caps[0] = Some((start, end));
                    m.best = Some((end, caps));
                }
                end == m.s.len()
            });
            if let Some((_, caps)) = m.best.take() {
                return Some(caps);
            }
            if m.budget == 0 {
                return None;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(p: &str, s: &str) -> Option<(usize, usize)> {
        Regex::new(p.as_bytes(), false).expect("valid").exec(s.as_bytes()).and_then(|c| c[0])
    }

    #[test]
    fn posix_ere_basics() {
        assert_eq!(find("b+", "abbbc"), Some((1, 4)));
        assert_eq!(find("^(dvips|ps2pdf)$", "ps2pdf"), Some((0, 6)));
        assert_eq!(find("^(dvips|ps2pdf)$", "pdftex"), None);
        assert_eq!(find("()", "a"), Some((0, 0)));
        assert_eq!(find("a{,2}", "a{,2}"), Some((0, 5)));
        for bad in ["", "(|a)", "a|", "a||b", "a**", "a{1}*", "^*a", "{1}a", "a{3,2}", "a{1x}"] {
            assert!(Regex::new(bad.as_bytes(), false).is_none(), "{bad}");
        }
        // Leftmost, then longest: not the first alternative.
        assert_eq!(find("a|ab|abc", "xabcd"), Some((1, 4)));
        assert_eq!(find("[0-9]{2,3}", "a12345"), Some((1, 4)));
        assert_eq!(find("[[:alpha:]]+", "12ab3"), Some((2, 4)));
        assert_eq!(find("[^a-c]", "abcd"), Some((3, 4)));
        assert_eq!(find("\\.", "a.b"), Some((1, 2)));
        assert!(Regex::new(b"(ab", false).is_none());
        assert!(Regex::new(b"*a", false).is_none());
    }

    #[test]
    fn hyperref_pdfborder_pattern() {
        // hyperref's `pdfborder` check with `\HyPat@NonNegativeReal/` expanded.
        let real = "\\ *([0-9]+\\.?[0-9]*|\\.[0-9]+)\\ *";
        let pat = format!("^{real} {real} {real}( ?\\[{real}( {real})*])?$");
        assert!(find(&pat, "0 0 0").is_some());
        assert!(find(&pat, "0 0 1 [3 2]").is_some());
        assert!(find(&pat, "0 0").is_none());
    }

    #[test]
    fn icase_and_subexpressions() {
        let r = Regex::new(b"(a+)(b*)", true).expect("valid");
        let c = r.exec(b"xAAbB").expect("match");
        assert_eq!(c[0], Some((1, 5)));
        assert_eq!(c[1], Some((1, 3)));
        assert_eq!(c[2], Some((3, 5)));
    }
}
