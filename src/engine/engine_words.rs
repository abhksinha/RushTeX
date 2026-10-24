//! GDL's word table: the main loop's result for a word read straight from the source.
//!
//! §1034–§1040 turn a word's characters into character, ligature and kern nodes (and an
//! empty discretionary after the font's hyphen character in a paragraph). That result
//! depends only on the font (its lig/kern program and boundary character), its hyphen
//! character, whether the list is a paragraph, and the characters, when the word starts
//! afresh (no `\noboundary` before it) and ends at a token that is not a character. So when
//! the next characters of the file being read form such a word, its nodes are made once and
//! reused every later time: the characters are neither tokenized nor run through the
//! lig/kern program again. The space factor still follows each character's `\sfcode`
//! (§1034), which may change between uses.
//!
//! In the native job's paragraphs a word is one [`Kind::Word`] node standing for its
//! entry, whose width, height and depth are summed once: line breaking, packing and
//! shipping read the entry instead of every character. Elsewhere (the engine, restricted
//! horizontal lists, a word with a discretionary) the entry's nodes are copied.
//!
//! `RT_WORD_CHECK=1` makes every word the slow way as well and stops at the first that
//! differs from its entry; `RT_WORD_NODES=0` copies the nodes everywhere.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_hyph::WordHash;
use super::engine_names::END_LINE_CHAR;
use super::engine_node::{Kind, Node};
use super::engine_run::{Machine, Mode};
use super::engine_token::Token;
use std::collections::HashMap;

/// A word entry's nodes and its natural size (§656 for its characters and kerns).
#[derive(Clone, Copy, Debug, Default)]
pub struct WordRec {
    pub start: u32,
    pub end: u32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    /// The nodes hold a discretionary (a break), so the word is never one node.
    pub disc: bool,
}

/// Every entry's nodes; a ligature's `b` is where its characters start in `ligs`.
#[derive(Debug, Default)]
pub struct WordStore {
    pub recs: Vec<WordRec>,
    pub items: Vec<Node>,
    pub ligs: Vec<u8>,
}

impl WordStore {
    #[inline]
    pub fn range(&self, id: u32) -> (usize, usize) {
        let r = &self.recs[id as usize];
        (r.start as usize, r.end as usize)
    }

    #[inline]
    pub fn items(&self, id: u32) -> &[Node] {
        let (s, e) = self.range(id);
        &self.items[s..e]
    }

    #[inline]
    pub fn rec(&self, id: u32) -> &WordRec {
        &self.recs[id as usize]
    }

    /// The characters an entry's ligature was made from.
    pub fn lig_from(&self, n: &Node) -> &[u8] {
        let at = n.b as usize;
        &self.ligs[at..at + (n.sub >> 2) as usize]
    }
}

pub struct Words {
    /// Key: font, hyphen character, paragraph, the characters; value: the entry.
    map: HashMap<Box<[u8]>, u32, WordHash>,
    /// The key being looked up, and the word's characters after the first, reused.
    key: Vec<u8>,
    run: Vec<u8>,
    check: bool,
    nodes: bool,
    /// Per entry, language and hyphen minima: what §894–§903 make of the word.
    forms: Forms,
}

/// A word's hyphenated forms (§903's result), with no node owned: a discretionary's `a`
/// indexes `discs`, whose lists are ranges of `items`; a ligature's `b` indexes `ligs`.
#[derive(Default)]
struct Forms {
    map: HashMap<u64, Form, WordHash>,
    items: Vec<Node>,
    ligs: Vec<u8>,
    discs: Vec<[(u32, u32); 2]>,
}

#[derive(Clone, Copy)]
enum Form {
    /// No hyphen: the word stays one node.
    Whole,
    /// The nodes that replace the word.
    Nodes(u32, u32),
    /// Nodes a form cannot hold: hyphenated every time.
    Again,
}

impl Forms {
    /// `list` (nodes §903 made) as a form: its range, or `None` for a node it cannot hold.
    fn capture(&mut self, nodes: &super::engine_node::Nodes, list: &[Node]) -> Option<(u32, u32)> {
        let mut out = Vec::with_capacity(list.len());
        for n in list {
            match n.kind {
                Kind::Char | Kind::Kern => out.push(*n),
                Kind::Lig => {
                    let at = self.ligs.len() as i32;
                    self.ligs.extend_from_slice(nodes.lig_from(n));
                    out.push(Node { b: at, ..*n });
                }
                Kind::Disc if n.a < 0 => out.push(*n),
                Kind::Disc => {
                    let [pre, post] = nodes.discs[n.a as usize];
                    let pre = self.capture(nodes, nodes.get(pre))?;
                    let post = self.capture(nodes, nodes.get(post))?;
                    self.discs.push([pre, post]);
                    out.push(Node { a: self.discs.len() as i32 - 1, ..*n });
                }
                _ => return None,
            }
        }
        let s = self.items.len() as u32;
        self.items.extend(out);
        Some((s, self.items.len() as u32))
    }

    /// The nodes of form range `r`, each ligature and discretionary with lists of its own.
    fn make(&self, nodes: &mut super::engine_node::Nodes, (s, e): (u32, u32), out: &mut Vec<Node>) {
        for &n in &self.items[s as usize..e as usize] {
            match n.kind {
                Kind::Lig => {
                    let at = n.b as usize;
                    let from = &self.ligs[at..at + (n.sub >> 2) as usize];
                    out.push(nodes.lig_node(n.font, n.a as u8, from, n.sub & 3));
                }
                Kind::Disc if n.a >= 0 => {
                    let [pre, post] = self.discs[n.a as usize];
                    let mut list = Vec::new();
                    self.make(nodes, pre, &mut list);
                    let pre = nodes.store(&list);
                    list.clear();
                    self.make(nodes, post, &mut list);
                    let post = nodes.store(&list);
                    out.push(Node { a: nodes.add_disc([pre, post]) as i32, ..n });
                }
                _ => out.push(n),
            }
        }
    }

    /// A canonical text of form range `r`, to compare two forms.
    fn text(&self, (s, e): (u32, u32), out: &mut Vec<i64>) {
        for n in &self.items[s as usize..e as usize] {
            out.extend([n.kind as i64, n.sub as i64, n.font as i64]);
            match n.kind {
                Kind::Lig => out.extend(self.ligs[n.b as usize..n.b as usize + (n.sub >> 2) as usize].iter().map(|&c| c as i64).chain([n.a as i64])),
                Kind::Disc if n.a >= 0 => {
                    let [pre, post] = self.discs[n.a as usize];
                    out.push(-1);
                    self.text(pre, out);
                    out.push(-2);
                    self.text(post, out);
                    out.push(-3);
                }
                _ => out.push(n.a as i64),
            }
        }
    }
}

impl Default for Words {
    fn default() -> Self {
        let check = std::env::var_os("RT_WORD_CHECK").is_some_and(|v| v == "1");
        let nodes = std::env::var_os("RT_WORD_NODES").is_none_or(|v| v != "0");
        Words { map: HashMap::default(), key: Vec::new(), run: Vec::new(), check, nodes, forms: Forms::default() }
    }
}

impl Machine<'_, '_> {
    /// The word that starts with `first` and continues with the characters next in the file,
    /// appended from the table (made and entered the first time). `false` when the word is
    /// not one the table can hold; the main loop then reads it token by token.
    ///
    /// In a native job's paragraph, the space after the word and the word after that are
    /// taken here too, as the main control takes their tokens (§1030: `char_token`, then
    /// `app_space` for the space and the main loop for the character), without a pass
    /// through the token loop for each.
    pub(super) fn word(&mut self, first: u8) -> bool {
        if !self.word_one(first) {
            return false;
        }
        if self.doc.is_none() || self.mode != Mode::Horizontal || self.inner || !self.building() {
            return true;
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: END_LINE_CHAR });
        loop {
            let cats = self.e.eqtb.cats();
            let Some(lx) = self.input.file_next() else { return true };
            let Some(next) = lx.space_then_char(cats, elc) else { return true };
            self.dont_expand = false;
            self.note_command(Token::char(Cat::Space, ' '));
            self.append_space();
            let Some((c, cat)) = next else { return true };
            self.note_command(Token::char(cat, c as char));
            if !self.word_one(c) {
                // Not a word the table holds: the main loop reads it token by token.
                self.main_loop(c);
                return true;
            }
        }
    }

    fn word_one(&mut self, first: u8) -> bool {
        if self.cancel_boundary || self.halted {
            return false;
        }
        let f = self.cur_font();
        let paragraph = self.mode == Mode::Horizontal && !self.inner;
        let font = &self.e.fonts.list[f as usize];
        let hyphen = font.hyphen_char;
        let cats = self.e.eqtb.cats();
        let Some(lx) = self.input.file_next() else { return false };
        let Some(end) = lx.word_run(cats) else { return false };
        let w = &mut self.words;
        w.run.clear();
        w.run.extend(lx.run_bytes(end));
        w.key.clear();
        w.key.extend_from_slice(&f.to_le_bytes());
        w.key.extend_from_slice(&match u8::try_from(hyphen) {
            Ok(h) => [1, h],
            Err(_) => [0, 0],
        });
        w.key.push(paragraph as u8);
        w.key.push(first);
        w.key.extend_from_slice(&w.run);
        let n = w.run.len();
        let found = w.map.get(w.key.as_slice()).copied();
        // §581: a missing character ends the main loop early; such a word is not entered.
        // An entry was made from characters the font has, which it keeps.
        if found.is_none() && (!font.metrics.exists(first) || !w.run.iter().all(|&c| font.metrics.exists(c))) {
            return false;
        }
        if paragraph {
            self.fix_language();
        }
        // A word is one node in the native job's paragraphs.
        let as_node = self.words.nodes && paragraph && self.doc.is_some();
        match found {
            Some(id) if !self.words.check => self.append_word(first, id, as_node),
            _ => {
                let start = self.cur.len();
                let run = std::mem::take(&mut self.words.run);
                self.text_run = Some((run, 0));
                self.main_loop(first);
                let (run, used) = self.text_run.take().expect("the word's run");
                debug_assert_eq!(used, n, "a word whose characters all exist is read whole");
                self.words.run = run;
                if let Some(id) = self.enter_word(start, found) {
                    if as_node && !self.e.nodes.words.rec(id).disc {
                        for k in start..self.cur.len() {
                            let x = self.cur[k];
                            self.e.nodes.flush_node(x);
                        }
                        self.cur.truncate(start);
                        self.cur.push(Node { kind: Kind::Word, sub: 0, font: f, a: id as i32, b: 0 });
                    }
                }
            }
        }
        if let Some(lx) = self.input.file_next() {
            lx.skip_run(n);
        }
        true
    }

    /// Entry `id` appended, as one node or as its nodes, and the space factor of each
    /// character (§1034).
    fn append_word(&mut self, first: u8, id: u32, as_node: bool) {
        self.adjust_space_factor(first);
        for i in 0..self.words.run.len() {
            let c = self.words.run[i];
            self.adjust_space_factor(c);
        }
        let rec = *self.e.nodes.words.rec(id);
        if as_node && !rec.disc {
            let f = self.e.nodes.words.items[rec.start as usize].font;
            self.cur.push(Node { kind: Kind::Word, sub: 0, font: f, a: id as i32, b: 0 });
            return;
        }
        let start = self.cur.len();
        self.cur.push(Node { kind: Kind::Word, sub: 0, font: 0, a: id as i32, b: 0 });
        let mut tail = self.cur.split_off(start);
        self.e.nodes.expand_words(&mut tail);
        self.cur.append(&mut tail);
    }

    /// The nodes the main loop just made from `start` entered under the current key, and
    /// the entry; `None` for nodes a word cannot hold. With `RT_WORD_CHECK=1` and an entry
    /// `found` already there, the nodes are compared with it instead.
    fn enter_word(&mut self, start: usize, found: Option<u32>) -> Option<u32> {
        let nodes = &mut self.e.nodes;
        let (ws, lig_chars) = (&mut nodes.words, &nodes.lig_chars);
        let s = ws.items.len();
        let lig_mark = ws.ligs.len();
        let (mut width, mut height, mut depth, mut disc) = (0, 0, 0, false);
        let fonts = &self.e.fonts;
        let dims = |n: &Node| u8::try_from(n.a).ok().and_then(|c| fonts.dims(n.font, c)).unwrap_or_default();
        for n in &self.cur[start..] {
            match n.kind {
                Kind::Char | Kind::Lig => {
                    let [w, h, d, _] = dims(n);
                    width += w;
                    height = height.max(h);
                    depth = depth.max(d);
                    if n.kind == Kind::Char {
                        ws.items.push(*n);
                    } else {
                        let at = ws.ligs.len() as i32;
                        ws.ligs.extend_from_slice(&lig_chars[n.b as usize..n.b as usize + (n.sub >> 2) as usize]);
                        ws.items.push(Node { b: at, ..*n });
                    }
                }
                Kind::Kern if n.sub == 0 => {
                    width += n.a;
                    ws.items.push(*n);
                }
                Kind::Disc if n.a == -1 => {
                    disc = true;
                    ws.items.push(*n);
                }
                // Not a node the main loop makes for a word: not entered.
                _ => {
                    ws.items.truncate(s);
                    ws.ligs.truncate(lig_mark);
                    return None;
                }
            }
        }
        let e = ws.items.len();
        let rec = WordRec { start: s as u32, end: e as u32, width, height, depth, disc };
        match found {
            None => {
                let id = ws.recs.len() as u32;
                ws.recs.push(rec);
                self.words.map.insert(self.words.key.as_slice().into(), id);
                Some(id)
            }
            Some(id) => {
                let old = *ws.rec(id);
                let same = |a: &Node, b: &Node| (a.kind, a.sub, a.font, a.a) == (b.kind, b.sub, b.font, b.a) && (a.kind == Kind::Lig || a.b == b.b);
                let ligs = |r: (usize, usize)| -> Vec<u8> { ws.items[r.0..r.1].iter().filter(|n| n.kind == Kind::Lig).flat_map(|n| ws.lig_from(n).iter().copied()).collect() };
                let (os, oe) = (old.start as usize, old.end as usize);
                let ok = oe - os == e - s && ws.items[os..oe].iter().zip(&ws.items[s..e]).all(|(a, b)| same(a, b)) && ligs((os, oe)) == ligs((s, e)) && (old.width, old.height, old.depth, old.disc) == (width, height, depth, disc);
                assert!(ok, "RT_WORD_CHECK: the word table's entry for {:?} differs from the main loop's nodes", String::from_utf8_lossy(&self.words.key[5..]));
                ws.items.truncate(s);
                ws.ligs.truncate(lig_mark);
                Some(id)
            }
        }
    }

    /// Before §894 runs on a paragraph: the words whose hyphenation could read nodes outside
    /// them are expanded. A word is kept whole when TeX's walk from a glue can reach it
    /// only as the node right after that glue (the node before it is none of a character,
    /// ligature, font kern, whatsit or word, which §896 passes through) and, after a glue,
    /// when a glue or penalty follows it, which ends §897–§899 at the word's end.
    pub(super) fn expand_words_near(&mut self, par: &mut Vec<Node>) {
        let through = |n: &Node| matches!(n.kind, Kind::Char | Kind::Lig | Kind::Whatsit | Kind::Word) || (n.kind == Kind::Kern && n.sub == 0);
        let unsure = |par: &[Node], i: usize| match i.checked_sub(1).map(|j| &par[j]) {
            None => false,
            Some(p) if through(p) => true,
            Some(p) if p.kind == Kind::Glue => !par.get(i + 1).is_some_and(|n| matches!(n.kind, Kind::Glue | Kind::Penalty)),
            Some(_) => false,
        };
        if !(0..par.len()).any(|i| par[i].kind == Kind::Word && unsure(par, i)) {
            return;
        }
        let old = std::mem::take(par);
        par.reserve(old.len() * 2);
        for i in 0..old.len() {
            let n = old[i];
            if n.kind == Kind::Word && unsure(&old, i) {
                let mut one = vec![n];
                self.e.nodes.expand_words(&mut one);
                par.extend(one);
            } else {
                par.push(n);
            }
        }
    }

    /// §894–§903 for the word node after glue `g`, which a glue or penalty follows
    /// ([`Machine::expand_words_near`]): the word is hyphenated as its nodes would be, alone
    /// between that glue and a penalty, which is all §896–§899 read of the paragraph. A word
    /// without hyphens stays one node; whether it has any is kept per entry, language and
    /// hyphen minima.
    pub(super) fn hyphenate_word(&mut self, par: &[Node], g: usize, lang: usize, l_hyf: usize, r_hyf: usize, edits: &mut Vec<(usize, usize, Vec<Node>)>) {
        let word = par[g + 1];
        let key = (word.a as u64) << 24 | (lang as u64 & 0xFF) << 16 | (l_hyf as u64 & 0xFF) << 8 | (r_hyf as u64 & 0xFF);
        let known = self.words.forms.map.get(&key).copied();
        match known {
            Some(Form::Whole) if !self.words.check => return,
            Some(Form::Nodes(s, e)) if !self.words.check => {
                let mut nodes = Vec::with_capacity((e - s) as usize);
                self.words.forms.make(&mut self.e.nodes, (s, e), &mut nodes);
                edits.push((g + 1, g + 1, nodes));
                return;
            }
            _ => {}
        }
        let mut temp = vec![par[g], word];
        self.e.nodes.expand_words(&mut temp);
        temp.push(Node::new(Kind::Penalty, 0));
        let (mut lang, mut l_hyf, mut r_hyf) = (lang, l_hyf, r_hyf);
        let mut one = Vec::new();
        self.try_hyphenate(&temp, 0, &mut lang, &mut l_hyf, &mut r_hyf, &mut one);
        let last = temp.len() - 1;
        let Some((keep, hb, out)) = one.pop() else {
            assert!(!matches!(known, Some(Form::Nodes(..))), "RT_WORD_CHECK: a word's hyphenated form is not hyphenated again");
            self.words.forms.map.insert(key, Form::Whole);
            for &n in &temp[1..last] {
                self.e.nodes.flush_node(n);
            }
            return;
        };
        // §903: the replaced nodes go; the rest of the word moves into the paragraph.
        for &n in &temp[keep..=hb] {
            self.e.nodes.flush_node(n);
        }
        let mut nodes = Vec::with_capacity(last + out.len());
        nodes.extend_from_slice(&temp[1..keep]);
        nodes.extend(out);
        nodes.extend_from_slice(&temp[hb + 1..last]);
        let forms = &mut self.words.forms;
        let marks = (forms.items.len(), forms.ligs.len(), forms.discs.len());
        let form = forms.capture(&self.e.nodes, &nodes).map_or(Form::Again, |(s, e)| Form::Nodes(s, e));
        match (known, form) {
            (Some(Form::Nodes(os, oe)), Form::Nodes(s, e)) => {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                forms.text((os, oe), &mut a);
                forms.text((s, e), &mut b);
                assert!(a == b, "RT_WORD_CHECK: a word's hyphenated form differs from §903's nodes");
                forms.items.truncate(marks.0);
                forms.ligs.truncate(marks.1);
                forms.discs.truncate(marks.2);
            }
            (Some(Form::Whole), _) => panic!("RT_WORD_CHECK: a word kept whole is hyphenated"),
            _ => {
                forms.map.insert(key, form);
            }
        }
        edits.push((g + 1, g + 1, nodes));
    }
}
