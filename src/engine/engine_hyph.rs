//! Hyphenation: tex.web §891–§966 (finding a word, the exception table and the patterns,
//! reconstituting the word with discretionaries, `\patterns`, `\hyphenation`), in our own
//! form.
//!
//! TeX keeps every language's patterns in one packed trie, built when the first paragraph
//! is hyphenated. Here each language keeps its patterns as compact records (the letters
//! after `\lccode` mapping, then the digits), which is what a format stores; a language's
//! trie is built the first time a word of it is hyphenated, so a document pays only for
//! the languages it uses. The trie is TeX's packed one (§920): a node's child for a
//! character is found by one indexed load and one comparison, and patterns with the same
//! digits share one op list. Exceptions are a hash map from the
//! letters and the language to the hyphen positions; a later `\hyphenation` of the same
//! word replaces the earlier one, as in TeX Live's TeX.

use std::sync::Arc;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::engine_catcode::Cat;
use super::engine_eqtb::{Code, Reg, Region};
use super::engine_font::{restart, NON_CHAR, STOP_FLAG};
use super::engine_node::{Kind, Node};
use super::engine_prim::{Meaning, Prim};
use super::engine_run::Machine;
#[cfg(feature = "oracle")]
use super::engine_snapshot::{Reader, Writer};
use super::engine_whatsit::LANGUAGE;

/// §236 `uc_hyph_code`.
const UC_HYPH: u32 = 38;

/// A language's exceptions: each word's letters and its hyphen positions.
pub type Exceptions<'a> = Vec<(&'a [u8], &'a [u8])>;

/// §934's exception table: a word's letters and language, to its hyphen positions.
type ExceptionTable = HashMap<Box<[u8]>, Box<[u8]>>;

/// The hyphenation tables (§920–§941), kept in a format.
#[derive(PartialEq, Debug, Default, Clone)]
pub struct Hyph {
    /// Per language: records of `k`, the `k` letters, then the `k+1` digits (§961–§965)
    /// two to a byte, the first in the low nibble.
    patterns: Arc<Vec<Vec<u8>>>,
    /// §934: the letters and the language, to the positions of the hyphens.
    exceptions: Arc<ExceptionTable>,
    /// The negation of §891's `trie_not_ready`: `\patterns` is too late once set.
    pub ready: bool,
    /// §963's duplicate check, while patterns are entered.
    seen: Arc<HashSet<Box<[u8]>>>,
    /// Built tries, per language (shared by a checkpoint's copy: fixed once built).
    tries: Vec<Option<Arc<Trie>>>,
    /// Each word hyphenated so far, as its §923–§934 result: the key is the lowercase
    /// letters `hc[1..=hn]`, the language, `\lefthyphenmin` and `\righthyphenmin`; the value
    /// bit `j` is an odd `hyf[j]` after the minima are applied. Those are all §923 and §934
    /// read, and the native job's pre-scan refuses `\patterns`, `\hyphenation` and
    /// `\lccode`, so the patterns, exceptions and lowercase codes do not change under it;
    /// `enter_pattern` and `add_exception` clear it all the same.
    memo: HashMap<Box<[u8]>, u64, WordHash>,
    /// The key being looked up, reused.
    memo_key: Vec<u8>,
    /// §892–§918's state, lent to each word in turn.
    spare: Word,
}

impl Hyph {
    /// A copy without the memo of words hyphenated (a cache that refills), for a checkpoint.
    pub fn without_memo(&self) -> Hyph {
        let Hyph { patterns, exceptions, ready, seen, tries, memo: _, memo_key: _, spare: _ } = self;
        Hyph { patterns: patterns.clone(), exceptions: exceptions.clone(), ready: *ready, seen: seen.clone(), tries: tries.clone(), memo: HashMap::default(), memo_key: Vec::new(), spare: Word::default() }
    }

    /// A word's replacement list, once read, back to make the next one in.
    pub(super) fn give_back(&mut self, out: Vec<Node>) {
        if out.capacity() > self.spare.out.capacity() {
            self.spare.out = out;
        }
    }
}

/// FNV-1a over a word's bytes: the memo's keys are short, and SipHash's set-up per lookup
/// would cost more than the word.
#[derive(Clone, Copy, Default)]
pub struct WordHash;

impl std::hash::BuildHasher for WordHash {
    type Hasher = Fnv;
    fn build_hasher(&self) -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

pub struct Fnv(u64);

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// A pattern while a trie is built: its letters and its op list.
type Pat<'a> = (&'a [u8], Vec<u16>);

/// A language's patterns as a tree while the trie is built: node `i`'s children are
/// `chars`/`to[first[i]..][..count[i]]`, sorted by character; `ops[i]` is 0 or one past the
/// start of its op list in `pool` (entries `distance << 8 | digit`, ended by 0).
#[derive(Default)]
struct Tree {
    first: Vec<u32>,
    count: Vec<u16>,
    ops: Vec<u32>,
    chars: Vec<u8>,
    to: Vec<u32>,
    pool: Vec<u16>,
}

/// A language's patterns as TeX's packed trie (§920–§921, §944–§958): node `z` is a slot,
/// and its child for character `c` is slot `base(z) + c` when that slot holds `c`. Each
/// slot is one word, read whole: its base (bits 0–31), its op (one past the start of its
/// op list in `pool`, or 0; bits 32–47) and its character plus one (bits 48–56; 0 for a
/// free slot). The root is slot 0. Every base is used by one node only, so a slot holding
/// `c` at `base(z) + c` is `z`'s child: another node's child there would have the same
/// base. A node without children has base 0, whose slots `c` hold other characters.
#[derive(Debug, Default, PartialEq, Clone)]
struct Trie {
    slots: Vec<u64>,
    pool: Vec<u16>,
}

const CHAR_SHIFT: u32 = 48;

impl Trie {
    fn build(records: &[u8]) -> Trie {
        Tree::build(records).pack()
    }

    /// Node `z`'s child for character `c`.
    #[inline(always)]
    fn child(&self, z: usize, c: u16) -> Option<usize> {
        let s = (self.slots[z] as u32 as usize).checked_add(c as usize).filter(|_| c < 256)?;
        (self.slots.get(s).is_some_and(|&e| (e >> CHAR_SHIFT) as u16 == c + 1)).then_some(s)
    }

    /// §923: the largest digit of every pattern found in `hc[0..=hn+1]`, into `hyf`.
    fn find(&self, hc: &[u16; 66], hn: usize, r_hyf: usize, hyf: &mut [u8; 66]) {
        for j in 0..=(hn + 1).saturating_sub(r_hyf) {
            let (mut z, mut l) = (0usize, j);
            while l <= hn + 1 {
                let Some(n) = self.child(z, hc[l]) else { break };
                z = n;
                let op = (self.slots[z] >> 32) as u16;
                if op != 0 {
                    // §924
                    for &e in self.pool[op as usize - 1..].iter().take_while(|&&e| e != 0) {
                        if let Some(i) = l.checked_sub((e >> 8) as usize) {
                            hyf[i] = hyf[i].max(e as u8);
                        }
                    }
                }
                l += 1;
            }
        }
    }

    /// The slots as `data_hyph` stores them: their low and high halves, and the op pool.
    fn parts(&self) -> [Vec<u32>; 3] {
        [self.slots.iter().map(|&e| e as u32).collect(), self.slots.iter().map(|&e| (e >> 32) as u32).collect(), self.pool.iter().map(|&x| x as u32).collect()]
    }

    /// The trie from [`Trie::parts`]; `None` unless every base, op and pool entry is in
    /// bounds, so `find` never panics on a table.
    fn from_parts(p: [Vec<u32>; 3]) -> Option<Trie> {
        let [lo, hi, pool] = p;
        if lo.len() != hi.len() || lo.is_empty() {
            return None;
        }
        let pool = pool.into_iter().map(|x| u16::try_from(x).ok()).collect::<Option<Vec<u16>>>()?;
        let slots: Vec<u64> = lo.iter().zip(&hi).map(|(&l, &h)| l as u64 | (h as u64) << 32).collect();
        let n = slots.len();
        let ok = slots.iter().all(|&e| {
            let op = (e >> 32) as u16 as usize;
            (e as u32 as usize + 256 <= n) && (e >> CHAR_SHIFT) <= 256 && (op == 0 || op <= pool.len() && pool[op - 1..].contains(&0))
        });
        ok.then_some(Trie { slots, pool })
    }
}

impl Tree {
    fn build(records: &[u8]) -> Tree {
        // Later patterns replace earlier ones, as §963's `trie_o[q]:=v` does.
        let mut pats: BTreeMap<&[u8], Vec<u16>> = BTreeMap::new();
        let mut at = 0;
        // A format's records are not checked when it loads: a malformed tail ends the list.
        while let Some(&k) = records.get(at) {
            let k = k as usize;
            let (Some(letters), Some(digits)) = (records.get(at + 1..at + 1 + k), records.get(at + 1 + k..at + 1 + k + k / 2 + 1)) else { break };
            let digit = |l: usize| digits[l / 2] >> (4 * (l % 2)) & 15;
            let ops: Vec<u16> = (0..=k).rev().filter(|&l| digit(l) != 0).map(|l| ((k - l) as u16) << 8 | digit(l) as u16).collect();
            pats.insert(letters, ops);
            at += 1 + k + k / 2 + 1;
        }
        let keys: Vec<Pat> = pats.into_iter().collect();
        let mut t = Tree::default();
        let mut shared: HashMap<Vec<u16>, u32> = HashMap::new();
        t.node(&keys, 0, &mut shared);
        t
    }

    /// The node for the common prefix of length `d` of `keys` (sorted).
    fn node(&mut self, keys: &[Pat], d: usize, shared: &mut HashMap<Vec<u16>, u32>) -> u32 {
        let me = self.first.len();
        self.first.push(0);
        self.count.push(0);
        self.ops.push(0);
        let mut rest = keys;
        if let Some((k, ops)) = keys.first().filter(|(k, _)| k.len() == d) {
            debug_assert_eq!(k.len(), d);
            if !ops.is_empty() {
                let pool = &mut self.pool;
                let at = *shared.entry(ops.clone()).or_insert_with(|| {
                    let at = pool.len() as u32;
                    pool.extend_from_slice(ops);
                    pool.push(0);
                    at
                });
                self.ops[me] = at + 1;
            }
            rest = &keys[1..];
        }
        let mut groups: Vec<(u8, &[Pat])> = Vec::new();
        let mut i = 0;
        while i < rest.len() {
            let c = rest[i].0[d];
            let j = i + rest[i..].iter().take_while(|(k, _)| k[d] == c).count();
            groups.push((c, &rest[i..j]));
            i = j;
        }
        let first = self.chars.len();
        self.first[me] = first as u32;
        self.count[me] = groups.len() as u16;
        for (c, _) in &groups {
            self.chars.push(*c);
            self.to.push(0);
        }
        for (g, (_, sub)) in groups.iter().enumerate() {
            let child = self.node(sub, d + 1, shared);
            self.to[first + g] = child;
        }
        me as u32
    }

    /// The tree packed (§952–§958): nodes in breadth-first order, each node's children
    /// at the first base, from the lowest free slot on, whose slots for all its children
    /// are free and which no other node has.
    fn pack(&self) -> Trie {
        let mut slots: Vec<u64> = vec![0; 256];
        let mut taken: Vec<bool> = vec![false; 256];
        let mut free = 1;
        let mut queue = std::collections::VecDeque::from([(0u32, 0usize)]);
        while let Some((node, slot)) = queue.pop_front() {
            let (at, k) = (self.first[node as usize] as usize, self.count[node as usize] as usize);
            if k == 0 {
                continue;
            }
            let kids = &self.chars[at..at + k];
            while free < slots.len() && slots[free] >> CHAR_SHIFT != 0 {
                free += 1;
            }
            let mut base = free.saturating_sub(kids[0] as usize).max(1);
            loop {
                if slots.len() < base + 256 {
                    slots.resize(base + 256, 0);
                    taken.resize(base + 256, false);
                }
                if !taken[base] && kids.iter().all(|&c| slots[base + c as usize] >> CHAR_SHIFT == 0) {
                    break;
                }
                base += 1;
            }
            taken[base] = true;
            slots[slot] = slots[slot] & !0xFFFF_FFFF | base as u64;
            for (i, &c) in kids.iter().enumerate() {
                let child = self.to[at + i];
                let s = base + c as usize;
                slots[s] = (c as u64 + 1) << CHAR_SHIFT | (self.ops[child as usize] as u64) << 32;
                queue.push_back((child, s));
            }
        }
        // Every node's slots for all 256 characters are in the table.
        let top = slots.iter().map(|&e| e as u32 as usize + 256).max().unwrap_or(256);
        slots.truncate(top.max(256));
        assert!(self.pool.len() < u16::MAX as usize, "a pattern op pool too large for a packed trie");
        Trie { slots, pool: self.pool.clone() }
    }
}

impl Hyph {
    fn trie(&mut self, lang: usize) -> Option<&Trie> {
        let records = self.patterns.get(lang).filter(|r| !r.is_empty())?;
        if self.tries.len() <= lang {
            self.tries.resize_with(lang + 1, || None);
        }
        if self.tries[lang].is_none() {
            self.tries[lang] = Some(Arc::new(Trie::build(records)));
        }
        self.tries[lang].as_deref()
    }

    /// Language `lang`'s trie as `data_hyph` stores it ([`Trie::parts`]), built from its
    /// patterns.
    pub fn trie_parts(&self, lang: u8) -> Option<[Vec<u32>; 3]> {
        let records = self.patterns.get(lang as usize).filter(|r| !r.is_empty())?;
        Some(Trie::build(records).parts())
    }

    /// Language `lang`'s trie from `data_hyph`, in place of building it from the patterns
    /// at the first hyphenated word; `false` (and it is built as usual) if it is malformed.
    pub fn set_trie(&mut self, lang: u8, parts: [Vec<u32>; 3]) -> bool {
        let Some(t) = Trie::from_parts(parts) else { return false };
        self.memo.clear();
        let l = lang as usize;
        if self.tries.len() <= l {
            self.tries.resize_with(l + 1, || None);
        }
        self.tries[l] = Some(Arc::new(t));
        true
    }

    /// Whether language `lang`'s stored trie is the one its patterns build (`rushtex-gen`).
    pub fn trie_matches(&self, lang: u8) -> bool {
        let l = lang as usize;
        match (self.tries.get(l).and_then(|t| t.as_deref()), self.patterns.get(l)) {
            (Some(t), Some(p)) => *t == Trie::build(p),
            _ => false,
        }
    }

    /// Language `lang`'s pattern records and its exceptions (letters without the language
    /// byte, positions), sorted: what `data_hyph` carries for the native job.
    pub fn language(&self, lang: u8) -> (&[u8], Exceptions<'_>) {
        let patterns = self.patterns.get(lang as usize).map_or(&[][..], |p| p.as_slice());
        let mut ex: Vec<(&[u8], &[u8])> = self.exceptions.iter().filter(|(k, _)| k.last() == Some(&lang)).map(|(k, v)| (&k[..k.len() - 1], &v[..])).collect();
        ex.sort();
        (patterns, ex)
    }

    /// Language `lang` from its records and exceptions, as a format would leave it: the
    /// trie ready, built on first use.
    pub fn set_language<'e>(&mut self, lang: u8, patterns: &[u8], exceptions: impl Iterator<Item = (&'e [u8], &'e [u8])>) {
        let l = lang as usize;
        self.memo.clear();
        let pats = Arc::make_mut(&mut self.patterns);
        if pats.len() <= l {
            pats.resize_with(l + 1, Vec::new);
        }
        pats[l] = patterns.to_vec();
        if let Some(t) = self.tries.get_mut(l) {
            *t = None;
        }
        for (k, v) in exceptions {
            let mut key = k.to_vec();
            key.push(lang);
            Arc::make_mut(&mut self.exceptions).insert(key.into_boxed_slice(), v.into());
        }
        self.ready = true;
    }

    #[cfg(feature = "oracle")]
    pub(super) fn dump(&self, w: &mut Writer) {
        w.count(self.patterns.len());
        for p in self.patterns.iter() {
            w.raw(p);
        }
        let mut ex: Vec<_> = self.exceptions.iter().collect();
        ex.sort();
        w.count(ex.len());
        for (k, v) in ex {
            w.all(k);
            w.all(v);
        }
    }

    #[cfg(feature = "oracle")]
    pub(super) fn undump(r: &mut Reader) -> Option<Hyph> {
        let n = r.count()?;
        let mut patterns = Vec::with_capacity(n);
        for _ in 0..n {
            patterns.push(r.raw()?);
        }
        let n = r.count()?;
        let mut exceptions = HashMap::with_capacity(n);
        for _ in 0..n {
            let k: Vec<u8> = r.all()?;
            let v: Vec<u8> = r.all()?;
            exceptions.insert(k.into_boxed_slice(), v.into_boxed_slice());
        }
        // §1325: a format's trie is always ready.
        Some(Hyph { patterns: Arc::new(patterns), exceptions: Arc::new(exceptions), ready: true, seen: Default::default(), tries: Vec::new(), memo: HashMap::default(), memo_key: Vec::new(), spare: Word::default() })
    }
}

/// A word to hyphenate: §892's `hc`, `hu`, `hn`, `hf`, `hyf`, and the boundary state of §903.
#[derive(PartialEq, Clone)]
struct Word {
    hu: [u16; 66],
    hyf: [u8; 66],
    hn: usize,
    hf: u16,
    init_list: Vec<u8>,
    init_lig: bool,
    init_lft: bool,
    hyphen_passed: usize,
    /// §907 `hold_head`: the last translation.
    hold: Vec<Node>,
    /// §914–§918's lists, `lig_stack` and a ligature's characters, kept for their room.
    pre: Vec<Node>,
    post: Vec<Node>,
    major: Vec<Node>,
    out: Vec<Node>,
    lig_stack: Vec<Item>,
    from: Vec<u8>,
}

impl Default for Word {
    fn default() -> Self {
        Word { hu: [0; 66], hyf: [0; 66], hn: 0, hf: 0, init_list: Vec::new(), init_lig: false, init_lft: false, hyphen_passed: 0, hold: Vec::new(), pre: Vec::new(), post: Vec::new(), major: Vec::new(), out: Vec::new(), lig_stack: Vec::new(), from: Vec::new() }
    }
}

impl std::fmt::Debug for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Word")
    }
}

impl Word {
    /// A new word's state; the lists keep their room.
    fn reset(&mut self) {
        self.hu = [0; 66];
        self.hyf = [0; 66];
        (self.hn, self.hf, self.init_lig, self.init_lft, self.hyphen_passed) = (0, 0, false, false, 0);
        self.init_list.clear();
        self.hold.clear();
        self.major.clear();
    }
}

/// A `lig_stack` entry (§907): a pending character and the original it replaces, if any.
#[derive(PartialEq, Clone, Copy)]
struct Item {
    c: u16,
    ptr: Option<u16>,
}

impl Machine<'_, '_> {
    fn lc(&self, c: u32) -> u16 {
        if c > 255 {
            0
        } else {
            self.e.eqtb.code(Code::Lc, c) as u16
        }
    }

    /// §960 `new_patterns` and §934 `new_hyph_exceptions`.
    pub(super) fn hyph_data(&mut self, p: Prim) {
        let lang = self.cur_lang() as usize;
        if p == Prim::Patterns && self.e.hyph.ready {
            self.error("Too late for \\patterns");
            let _ = self.scan_braced_text(false);
            return;
        }
        self.scan_left_brace();
        let (mut hc, mut hyf) = ([0u16; 66], [0u8; 66]);
        let (mut k, mut digit_sensed) = (0usize, false);
        let mut pos: Vec<u8> = Vec::new();
        loop {
            let Some(x) = self.get_x_token(false) else { return };
            // §1038's letters, and (for `\hyphenation`) `\chardef` characters and `\char`.
            let chr = match x.m {
                Meaning::Char(c) if matches!(c.cat(), Some(Cat::Letter | Cat::Other)) => Some(c.chr() as u32),
                Meaning::CharDef(n) if p == Prim::Hyphenation => Some(n),
                Meaning::Prim(Prim::Char) if p == Prim::Hyphenation => Some(self.scan_char_num() as u32),
                _ => None,
            };
            let end_group = matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::EndGroup));
            let ends = end_group || matches!(x.m, Meaning::Char(c) if c.cat() == Some(Cat::Space));
            match chr {
                Some(c) if p == Prim::Patterns => {
                    // §962
                    if digit_sensed || !(b'0' as u32..=b'9' as u32).contains(&c) {
                        let c = if c == '.' as u32 {
                            0
                        } else {
                            let l = self.lc(c);
                            if l == 0 {
                                self.error("Nonletter");
                            }
                            l
                        };
                        if k < 63 {
                            k += 1;
                            hc[k] = c;
                            hyf[k] = 0;
                            digit_sensed = false;
                        }
                    } else if k < 63 {
                        hyf[k] = (c - '0' as u32) as u8;
                        digit_sensed = true;
                    }
                }
                Some(c) => {
                    // §937
                    if c == '-' as u32 {
                        if k < 63 {
                            pos.push(k as u8);
                        }
                    } else {
                        let l = self.lc(c);
                        if l == 0 {
                            self.error("Not a letter");
                        } else if k < 63 {
                            k += 1;
                            hc[k] = l;
                        }
                    }
                }
                None if ends => {
                    if p == Prim::Patterns && k > 0 {
                        self.enter_pattern(lang, &hc[1..=k], &mut hyf[..=k]);
                    } else if p == Prim::Hyphenation && k > 1 {
                        // §939
                        let mut key: Vec<u8> = hc[1..=k].iter().map(|&c| c as u8).collect();
                        key.push(lang as u8);
                        Arc::make_mut(&mut self.e.hyph.exceptions).insert(key.into_boxed_slice(), std::mem::take(&mut pos).into_boxed_slice());
                        self.e.hyph.memo.clear();
                    }
                    if end_group {
                        return;
                    }
                    k = 0;
                    hyf[0] = 0;
                    digit_sensed = false;
                    pos.clear();
                }
                None if p == Prim::Patterns => self.error("Bad \\patterns"),
                None => self.error("Improper \\hyphenation will be flushed"),
            }
        }
    }

    /// §963–§965: a pattern of letters `hc` and digits `hyf` for language `lang`.
    fn enter_pattern(&mut self, lang: usize, hc: &[u16], hyf: &mut [u8]) {
        self.e.hyph.memo.clear();
        let k = hc.len();
        if hc[0] == 0 {
            hyf[0] = 0;
        }
        if hc[k - 1] == 0 {
            hyf[k] = 0;
        }
        let mut key: Vec<u8> = Vec::with_capacity(k + 1);
        key.push(lang as u8);
        key.extend(hc.iter().map(|&c| c as u8));
        if !Arc::make_mut(&mut self.e.hyph.seen).insert(key.into_boxed_slice()) {
            self.error("Duplicate pattern");
        }
        let h = &mut self.e.hyph;
        let pats = Arc::make_mut(&mut h.patterns);
        if pats.len() <= lang {
            pats.resize_with(lang + 1, Vec::new);
        }
        let rec = &mut pats[lang];
        rec.push(k as u8);
        rec.extend(hc.iter().map(|&c| c as u8));
        rec.extend(hyf.chunks(2).map(|d| d[0] | d.get(1).map_or(0, |&e| e << 4)));
    }

    /// §891–§903 for a whole paragraph before its second pass: the word after every glue
    /// node is hyphenated (§894), with the language state that whatsits set on the way
    /// (§1362, §1363).
    ///
    /// Each hyphenated word's replacement is kept as an edit, and the paragraph is rebuilt
    /// once at the end: splicing each word in moved the paragraph's tail once per word.
    /// The walk reads only nodes after the word it replaces, and a replacement holds no
    /// glue, math or whatsit node, so the walk is the same either way.
    pub(super) fn hyphenate_par(&mut self, par: &mut Vec<Node>, packed: i32) {
        let mut lang = (packed % 0o200000).clamp(0, 255) as usize;
        let mut l_hyf = (packed / 0o20000000).max(1) as usize;
        let mut r_hyf = ((packed / 0o200000) % 0o100).max(1) as usize;
        let mut k = 0;
        // §866: no hyphenation inside math (`auto_breaking`).
        let mut auto_breaking = true;
        let mut edits: Vec<(usize, usize, Vec<Node>)> = Vec::new();
        while k < par.len() {
            let n = par[k];
            match n.kind {
                Kind::Whatsit if n.sub == LANGUAGE => {
                    lang = n.a as usize;
                    (l_hyf, r_hyf) = ((n.b >> 6) as usize, (n.b & 63) as usize);
                }
                Kind::Math => auto_breaking = n.sub == 1,
                Kind::Glue if auto_breaking && par.get(k + 1).is_some_and(|n| n.kind == Kind::Word) => {
                    if let Some(id) = self.hyphenate_word(par, k, lang, l_hyf, r_hyf, &mut edits) {
                        par[k + 1].a = id as i32;
                    }
                }
                Kind::Glue if auto_breaking => self.try_hyphenate(par, k, &mut lang, &mut l_hyf, &mut r_hyf, &mut edits),
                _ => {}
            }
            k += 1;
        }
        if edits.is_empty() {
            return;
        }
        let old = std::mem::take(par);
        par.reserve(old.len() + edits.iter().map(|e| e.2.len()).sum::<usize>());
        let mut at = 0;
        for (keep, hb, out) in edits {
            par.extend_from_slice(&old[at..keep]);
            // §903: the word's old nodes are flushed (`flush_node_list`).
            for &n in &old[keep..=hb] {
                self.e.nodes.flush_node(n);
            }
            par.extend(out);
            at = hb + 1;
        }
        par.extend_from_slice(&old[at..]);
    }

    /// §894: the word after glue node `g`.
    pub(super) fn try_hyphenate(&mut self, par: &[Node], g: usize, lang: &mut usize, l_hyf: &mut usize, r_hyf: &mut usize, edits: &mut Vec<(usize, usize, Vec<Node>)>) {
        // One `Word` serves every word: its lists keep their room from word to word.
        let mut w = std::mem::take(&mut self.e.hyph.spare);
        w.reset();
        self.try_hyphenate_in(&mut w, par, g, lang, l_hyf, r_hyf, edits);
        self.e.hyph.spare = w;
    }

    #[allow(clippy::too_many_arguments)]
    fn try_hyphenate_in(&mut self, w: &mut Word, par: &[Node], g: usize, lang: &mut usize, l_hyf: &mut usize, r_hyf: &mut usize, edits: &mut Vec<(usize, usize, Vec<Node>)>) {
        let mut hc = [0u16; 66];
        // §896: to the node before the first letter.
        let mut prev_s = g;
        let mut s = g + 1;
        loop {
            let Some(&n) = par.get(s) else { return };
            let c = match n.kind {
                Kind::Char => {
                    w.hf = n.font;
                    n.a as u32
                }
                Kind::Lig => match self.e.nodes.lig_from(&n).first() {
                    None => 256,
                    Some(&c) => {
                        w.hf = n.font;
                        c as u32
                    }
                },
                Kind::Kern if n.sub == 0 => 256,
                Kind::Whatsit => {
                    // §1363
                    if n.sub == LANGUAGE {
                        *lang = n.a as usize;
                        (*l_hyf, *r_hyf) = ((n.b >> 6) as usize, (n.b & 63) as usize);
                    }
                    256
                }
                _ => return,
            };
            if c < 256 {
                let l = self.lc(c);
                if l != 0 {
                    if l as u32 == c || self.e.eqtb.word(Reg { region: Region::Int, slot: UC_HYPH }) > 0 {
                        break;
                    }
                    return;
                }
            }
            prev_s = s;
            s += 1;
        }
        let hyf_char = self.e.fonts.list[w.hf as usize].hyphen_char;
        if !(0..=255).contains(&hyf_char) {
            return;
        }
        let ha = prev_s;
        if *l_hyf + *r_hyf > 63 {
            return;
        }
        // §897: the letters, to node `hb`.
        let (font_bchar, _) = self.e.fonts.list[w.hf as usize].metrics.boundary();
        let mut hb = None;
        let mut hyf_bchar = NON_CHAR;
        while let Some(&n) = par.get(s) {
            match n.kind {
                Kind::Char => {
                    if n.font != w.hf {
                        break;
                    }
                    let c = n.a as u16;
                    hyf_bchar = c;
                    let l = self.lc(c as u32);
                    if l == 0 || w.hn == 63 {
                        break;
                    }
                    hb = Some(s);
                    w.hn += 1;
                    w.hu[w.hn] = c;
                    hc[w.hn] = l;
                    hyf_bchar = NON_CHAR;
                }
                Kind::Lig => {
                    // §898
                    if n.font != w.hf {
                        break;
                    }
                    let from = self.e.nodes.lig_from(&n);
                    if let Some(&c) = from.first() {
                        hyf_bchar = c as u16;
                    }
                    let mut j = w.hn;
                    let mut all = true;
                    for &c in from {
                        let l = self.lc(c as u32);
                        if l == 0 || j == 63 {
                            all = false;
                            break;
                        }
                        j += 1;
                        w.hu[j] = c as u16;
                        hc[j] = l;
                    }
                    if !all {
                        break;
                    }
                    hb = Some(s);
                    w.hn = j;
                    hyf_bchar = if n.sub & 1 != 0 { font_bchar } else { NON_CHAR };
                }
                Kind::Kern if n.sub == 0 => {
                    hb = Some(s);
                    hyf_bchar = font_bchar;
                }
                _ => break,
            }
            s += 1;
        }
        // §899: what follows the word must allow it.
        if w.hn < *l_hyf + *r_hyf {
            return;
        }
        loop {
            let Some(&n) = par.get(s) else { return };
            match n.kind {
                Kind::Char | Kind::Lig => {}
                Kind::Kern if n.sub == 0 => {}
                Kind::Kern | Kind::Whatsit | Kind::Glue | Kind::Penalty | Kind::Ins | Kind::Adjust | Kind::Mark => break,
                _ => return,
            }
            s += 1;
        }
        let Some(hb) = hb else { return };
        // §923: hyphen positions from the exceptions, else the patterns.
        let hn = w.hn;
        let hy = &mut self.e.hyph;
        hy.memo_key.clear();
        hy.memo_key.extend(hc[1..=hn].iter().map(|&c| c as u8));
        hy.memo_key.extend_from_slice(&[*lang as u8, *l_hyf as u8, *r_hyf as u8]);
        let mask = match hy.memo.get(hy.memo_key.as_slice()) {
            Some(&m) => m,
            None => {
                // The exception key, the word and its language, is the memo key's start.
                if let Some(pos) = hy.exceptions.get(&hy.memo_key[..=hn]) {
                    for &p in pos.iter() {
                        w.hyf[p as usize] = 1;
                    }
                } else {
                    let Some(trie) = hy.trie(*lang) else { return };
                    hc[0] = 0;
                    hc[hn + 1] = 0;
                    trie.find(&hc, hn, *r_hyf, &mut w.hyf);
                }
                for j in 0..*l_hyf {
                    w.hyf[j] = 0;
                }
                for j in 0..*r_hyf {
                    w.hyf[hn - j] = 0;
                }
                let m = (0..=hn).filter(|&j| w.hyf[j] & 1 == 1).fold(0u64, |m, j| m | 1 << j);
                let hy = &mut self.e.hyph;
                let key: Box<[u8]> = hy.memo_key.as_slice().into();
                hy.memo.insert(key, m);
                m
            }
        };
        // §902
        if mask == 0 {
            return;
        }
        for j in 0..=hn {
            w.hyf[j] = (mask >> j & 1) as u8;
        }
        let (keep, out) = self.replace_word(par, ha, w, hyf_bchar, hyf_char as u16);
        edits.push((keep, hb, out));
    }

    /// §903, §913–§918: nodes `ha..=hb` replaced by the word with its discretionaries.
    /// Returns where the replaced nodes start (`keep`) and the nodes that replace `keep..=hb`.
    fn replace_word(&mut self, par: &[Node], ha: usize, w: &mut Word, hyf_bchar: u16, hyf_char: u16) -> (usize, Vec<Node>) {
        let bchar = hyf_bchar;
        let hn = w.hn;
        let hf = w.hf;
        let a = par[ha];
        // `keep`: the nodes kept before the reconstitution; `j` where it starts.
        let found2 = |w: &mut Word| {
            w.hu[0] = 256;
            w.init_lig = false;
            w.init_list.clear();
        };
        let (keep, mut j) = match a.kind {
            Kind::Char => {
                if a.font != hf {
                    found2(w);
                    (ha + 1, 0)
                } else {
                    w.init_list.clear();
                    w.init_list.push(a.a as u8);
                    w.init_lig = false;
                    w.hu[0] = a.a as u16;
                    (ha, 0)
                }
            }
            Kind::Lig => {
                if a.font != hf {
                    found2(w);
                    (ha + 1, 0)
                } else {
                    w.init_list.clear();
                    w.init_list.extend_from_slice(self.e.nodes.lig_from(&a));
                    w.init_lig = true;
                    w.init_lft = a.sub & 3 > 1;
                    w.hu[0] = a.a as u16;
                    if w.init_list.is_empty() && w.init_lft {
                        w.hu[0] = 256;
                        w.init_lig = false;
                    }
                    (ha, 0)
                }
            }
            _ => {
                // No punctuation: a left boundary ligature in the word means starting from it.
                let r = par[ha + 1];
                if r.kind == Kind::Lig && r.sub & 3 > 1 {
                    found2(w);
                    (ha + 1, 0)
                } else {
                    (ha + 1, 1)
                }
            }
        };
        // §913
        let mut out = std::mem::take(&mut w.out);
        out.clear();
        let font_bchar = self.e.fonts.list[hf as usize].metrics.boundary().0;
        let has_bchar_label = self.e.fonts.list[hf as usize].metrics.boundary().1.is_some();
        let hyf_exists = self.e.fonts.list[hf as usize].metrics.exists(hyf_char as u8);
        loop {
            let mut l = j;
            j = self.reconstitute(w, j, hn, bchar, hyf_char) + 1;
            if w.hyphen_passed == 0 {
                out.append(&mut w.hold);
                if w.hyf[j - 1] & 1 == 1 {
                    l = j;
                    w.hyphen_passed = j - 1;
                    w.hold.clear();
                }
            }
            if w.hyphen_passed > 0 {
                // §914
                loop {
                    std::mem::swap(&mut w.major, &mut w.hold);
                    w.hold.clear();
                    let mut r_count = w.major.len();
                    let mut i = w.hyphen_passed;
                    w.hyf[i] = 0;
                    // §915: `hu[l..=i]` and a hyphen.
                    w.pre.clear();
                    let mut c = 0;
                    if hyf_exists {
                        i += 1;
                        c = w.hu[i];
                        w.hu[i] = hyf_char;
                    }
                    while l <= i {
                        l = self.reconstitute(w, l, i, font_bchar, NON_CHAR) + 1;
                        w.pre.append(&mut w.hold);
                    }
                    if hyf_exists {
                        w.hu[i] = c;
                        l = i;
                        i -= 1;
                    }
                    let _ = i;
                    // §916: `hu[i+1..]`, until both branches end together.
                    w.post.clear();
                    let mut c_loc = 0;
                    if has_bchar_label {
                        l -= 1;
                        c = w.hu[l];
                        c_loc = l;
                        w.hu[l] = 256;
                    }
                    while l < j {
                        loop {
                            l = self.reconstitute(w, l, hn, bchar, NON_CHAR) + 1;
                            if c_loc > 0 {
                                w.hu[c_loc] = c;
                                c_loc = 0;
                            }
                            w.post.append(&mut w.hold);
                            if l >= j {
                                break;
                            }
                        }
                        while l > j {
                            // §917
                            j = self.reconstitute(w, j, hn, bchar, NON_CHAR) + 1;
                            r_count += w.hold.len();
                            w.major.append(&mut w.hold);
                        }
                    }
                    // §918
                    if r_count <= 127 {
                        let pre = self.e.nodes.store(&w.pre);
                        let post = self.e.nodes.store(&w.post);
                        let d = self.e.nodes.add_disc([pre, post]);
                        out.push(Node { kind: Kind::Disc, sub: r_count as u8, font: 0, a: d as i32, b: 0 });
                    }
                    out.append(&mut w.major);
                    w.hyphen_passed = j - 1;
                    w.hold.clear();
                    if w.hyf[j - 1] & 1 == 0 {
                        break;
                    }
                }
            }
            if j > hn {
                break;
            }
        }
        (keep, out)
    }

    /// §905–§911 `reconstitute`: the translation of `hu[j..=n]` up to its first cut, into
    /// `w.hold`; returns the last index it used.
    // `wrap_lig!` clears flags that some of its call sites set again at once, as TeX's does.
    #[allow(unused_assignments)]
    fn reconstitute(&mut self, w: &mut Word, mut j: usize, n: usize, mut bchar: u16, mut hchar: u16) -> usize {
        let hf = w.hf;
        // The font is read while ligature nodes are made: two disjoint parts of the engine.
        let (fonts, nodes) = (&self.e.fonts, &mut self.e.nodes);
        let font = &fonts.list[hf as usize];
        let (prog, _) = font.metrics.lig_kern();
        let bchar_label = font.metrics.boundary().1;
        let ch = |c: u16| Node { kind: Kind::Char, sub: 0, font: hf, a: c as i32, b: 0 };
        w.hyphen_passed = 0;
        w.hold.clear();
        let mut kern = 0;
        let (mut ligature_present, mut lft_hit, mut rt_hit) = (false, false, false);
        // §908
        let mut cur_l = w.hu[j];
        let mut cur_q = 0usize;
        if j == 0 {
            ligature_present = w.init_lig;
            if ligature_present {
                lft_hit = w.init_lft;
            }
            for &c in &w.init_list {
                w.hold.push(ch(c as u16));
            }
        } else if cur_l < NON_CHAR {
            w.hold.push(ch(cur_l));
        }
        let mut lig_stack = std::mem::take(&mut w.lig_stack);
        lig_stack.clear();
        let (mut cur_r, mut cur_rh);
        macro_rules! set_cur_r {
            () => {
                cur_r = if j < n { w.hu[j + 1] } else { bchar };
                cur_rh = if w.hyf[j] & 1 == 1 { hchar } else { NON_CHAR };
            };
        }
        set_cur_r!();
        // §910 `wrap_lig`.
        macro_rules! wrap_lig {
            ($rt:expr) => {
                if ligature_present {
                    w.from.clear();
                    w.from.extend(w.hold.drain(cur_q..).map(|n| n.a as u8));
                    let mut hits = 0;
                    if lft_hit {
                        hits = 2;
                        lft_hit = false;
                    }
                    if $rt && lig_stack.is_empty() {
                        hits += 1;
                        rt_hit = false;
                    }
                    let p = nodes.lig_node(hf, cur_l as u8, &w.from, hits);
                    w.hold.push(p);
                    ligature_present = false;
                }
            };
        }
        macro_rules! pop_lig_stack {
            () => {
                let top = lig_stack.pop().expect("a lig_stack item");
                if let Some(c) = top.ptr {
                    w.hold.push(ch(c));
                    j += 1;
                }
                match lig_stack.last() {
                    None => {
                        set_cur_r!();
                    }
                    Some(t) => cur_r = t.c,
                }
            };
        }
        'cont: loop {
            // §909: the program at the cursor.
            let start = if cur_l == NON_CHAR {
                bchar_label
            } else {
                u8::try_from(cur_l).ok().and_then(|c| font.metrics.lig_kern_start(c))
            };
            'done: {
                let Some(mut k) = start else { break 'done };
                let test_char = if cur_rh < NON_CHAR { cur_rh } else { cur_r };
                loop {
                    let q = prog.get(k);
                    let (skip, next, op, rem) = ((q >> 24) as u8, (q >> 16 & 0xFF) as u16, (q >> 8 & 0xFF) as u8, (q & 0xFF) as u16);
                    if next == test_char && skip <= STOP_FLAG {
                        if cur_rh < NON_CHAR {
                            w.hyphen_passed = j;
                            hchar = NON_CHAR;
                            cur_rh = NON_CHAR;
                            continue 'cont;
                        }
                        if hchar < NON_CHAR && w.hyf[j] & 1 == 1 {
                            w.hyphen_passed = j;
                            hchar = NON_CHAR;
                        }
                        if op < STOP_FLAG {
                            // §911
                            if cur_l == NON_CHAR {
                                lft_hit = true;
                            }
                            if j == n && lig_stack.is_empty() {
                                rt_hit = true;
                            }
                            match op {
                                1 | 5 => {
                                    cur_l = rem;
                                    ligature_present = true;
                                }
                                2 | 6 => {
                                    cur_r = rem;
                                    match lig_stack.last_mut() {
                                        Some(t) => t.c = cur_r,
                                        None => {
                                            let ptr = if j == n {
                                                bchar = NON_CHAR;
                                                None
                                            } else {
                                                Some(w.hu[j + 1])
                                            };
                                            lig_stack.push(Item { c: cur_r, ptr });
                                        }
                                    }
                                }
                                3 => {
                                    cur_r = rem;
                                    lig_stack.push(Item { c: cur_r, ptr: None });
                                }
                                7 | 11 => {
                                    wrap_lig!(false);
                                    cur_q = w.hold.len();
                                    cur_l = rem;
                                    ligature_present = true;
                                }
                                _ => {
                                    cur_l = rem;
                                    ligature_present = true;
                                    if !lig_stack.is_empty() {
                                        pop_lig_stack!();
                                    } else if j == n {
                                        break 'done;
                                    } else {
                                        w.hold.push(ch(cur_r));
                                        j += 1;
                                        set_cur_r!();
                                    }
                                }
                            }
                            if op > 4 && op != 7 {
                                break 'done;
                            }
                            continue 'cont;
                        }
                        kern = font.kern(restart(q) - 256 * STOP_FLAG as usize, &fonts.info);
                        break 'done;
                    }
                    if skip >= STOP_FLAG {
                        if cur_rh == NON_CHAR {
                            break 'done;
                        }
                        cur_rh = NON_CHAR;
                        continue 'cont;
                    }
                    k += skip as usize + 1;
                }
            }
            // §910
            wrap_lig!(rt_hit);
            if kern != 0 {
                w.hold.push(Node::new(Kind::Kern, kern));
                kern = 0;
            }
            if let Some(&top) = lig_stack.last() {
                cur_q = w.hold.len();
                cur_l = top.c;
                ligature_present = true;
                pop_lig_stack!();
                continue 'cont;
            }
            w.lig_stack = lig_stack;
            return j;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uc_hyph_slot() {
        assert_eq!(super::super::engine_names::param("uchyph"), Some(Reg { region: Region::Int, slot: UC_HYPH }));
    }

    /// `data_hyph`'s stored trie is the one language 0's patterns build.
    #[test]
    fn stored_trie_is_the_built_one() {
        let Some(h) = crate::data::data_hyph::english() else { return };
        let t = h.trie.as_ref().expect("the table has a trie");
        let mut hy = Hyph::default();
        hy.set_language(0, h.patterns, h.exceptions());
        assert!(hy.set_trie(0, t.each_ref().map(|s| s.iter().collect())));
        assert!(hy.trie_matches(0));
    }

    /// Liang's example: `hy3ph he2n hena4 hen5at 1na n2at 1tio 2io o2n` hyphenate
    /// "hyphenation" as hy-phen-ation.
    #[test]
    fn trie_finds_the_largest_digits() {
        let mut rec = Vec::new();
        for p in ["hy3ph", "he2n", "hena4", "hen5at", "1na", "n2at", "1tio", "2io", "o2n"] {
            let (mut letters, mut digits) = (Vec::new(), vec![0u8]);
            for c in p.bytes() {
                if c.is_ascii_digit() {
                    *digits.last_mut().unwrap() = c - b'0';
                } else {
                    letters.push(c);
                    digits.push(0);
                }
            }
            rec.push(letters.len() as u8);
            rec.extend_from_slice(&letters);
            rec.extend(digits.chunks(2).map(|d| d[0] | d.get(1).map_or(0, |&e| e << 4)));
        }
        let t = Trie::build(&rec);
        let word = b"hyphenation";
        let mut hc = [0u16; 66];
        for (i, &c) in word.iter().enumerate() {
            hc[i + 1] = c as u16;
        }
        let mut hyf = [0u8; 66];
        t.find(&hc, word.len(), 3, &mut hyf);
        let cuts: Vec<usize> = (1..word.len()).filter(|&j| hyf[j] & 1 == 1).collect();
        assert_eq!(cuts, vec![2, 6]);
    }
}
