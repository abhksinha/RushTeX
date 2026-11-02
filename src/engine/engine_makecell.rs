//! makecell.sty (2009/08/03 V0.1e) on the GDL path: `\makecell[pos]{text}` and
//! `\thead[pos]{text}` (and their starred forms) outside math.
//!
//! `\mcell@@@tabular` is `\noindent\null\mcell@left \tabg@pe{\hbox{\t@bset\tabular[v]{@{}h@{}}
//! text\endtabular}}\mcell@right\null`: `\mcell@multirow` gobbles its arguments outside
//! `\multirowcell`, `\t@bset` is `\cellset` (`\def\arraystretch{1}\extrarowheight\z@
//! \nomakegapedcells`, the defaults, which the pre-scan keeps: a document that changes
//! `\arraystretch` declines) and for `\thead` also `\theadfont` (`\footnotesize`) and
//! `\theadset` (empty). `\tabg@pe` is `\cellgape` (empty) for `\makecell`, `\gape` for
//! `\makecell*` and `\thead`: `\makeg@pe` boxes the text in box 0 and adds `\jot` to its
//! height and depth (`\mcell@mb@tb`). The positions follow `\mcell@setalign` (`mcell_align`).

use super::super::engine_catcode::Cat;
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;

pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@makecell", |m| m.makecell(false)), ("rt@thead", |m| m.makecell(true)), ("rt@mcgape", |m| m.makecell_gape())];

/// `\jot` (latex.ltx `\jot=3pt`; the native job takes no other value).
const JOT: i32 = 3 * 65536;

/// `\mcell@setalign` run on `\cellalign` (`cc`) and then on the position given: the vertical
/// position (`\mcell@i`), the column letter (`\mcell@ii`), and whether `\mcell@left` and
/// `\mcell@right` stay `\hfill`. `None` for a letter outside `l, r, t, b, c`.
fn mcell_align(pos: &str) -> Option<(char, char, bool, bool)> {
    let (mut i, mut ii) = ('c', 'c');
    let (mut left, mut right) = (true, true);
    for arg in ["cc", pos] {
        let mut ch = arg.chars();
        let (a, b) = (ch.next(), ch.next());
        if ch.next().is_some() {
            return None;
        }
        (left, right) = (true, true);
        // `\mcell@c`: `\def\mcell@ii{c}` until `l`/`r` make it `\mcell@ic`, `t`/`b` `\mcell@iic`.
        let mut c_sets_i = false;
        for x in [a, b].into_iter().flatten() {
            match x {
                'l' | 'r' => {
                    ii = x;
                    c_sets_i = true;
                    if x == 'l' { left = false } else { right = false }
                }
                't' | 'b' => {
                    i = x;
                    c_sets_i = false;
                }
                'c' => {
                    if c_sets_i { i = 'c' } else { ii = 'c' }
                }
                _ => return None,
            }
        }
        if a == Some('c') {
            if c_sets_i { i = 'c' } else { ii = 'c' }
        }
    }
    Some((i, ii, left, right))
}

impl Machine<'_, '_> {
    /// `\makecell` or `\thead` (`head`), with `*`, `[pos]` and the text.
    fn makecell(&mut self, head: bool) {
        if self.mode == Mode::Math {
            return self.not_native("\\makecell in math");
        }
        let star = self.next_is('*');
        let pos = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let pos: Option<String> = match &pos {
            None => Some(String::new()),
            Some(p) => (!p.is_empty() && p.iter().all(|t| t.cat() == Some(Cat::Letter))).then(|| p.iter().map(|t| t.chr()).collect()),
        };
        let Some((v, h, left, right)) = pos.as_deref().and_then(mcell_align) else {
            return self.not_native("a \\makecell position other than l, r, t, b, c");
        };
        if self.e.eqtb.word(param!("parindent")) < 0 {
            return self.not_native("\\makecell with a negative \\parindent");
        }
        let gape = head || star;
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let cs = |m: &Self, i: usize| m.named_cs(i);
        let mut t = Vec::with_capacity(text.len() + 40);
        t.extend([cs(self, named!("noindent")), cs(self, named!("hbox")), open, close]);
        if left {
            t.push(cs(self, named!("hfill")));
        }
        if gape {
            // `\makeg@pe`: `\setbox\z@\hbox{\color@begingroup#1\color@endgroup}`.
            t.extend([cs(self, named!("setbox")), Token::char(Cat::Other, '0'), cs(self, named!("hbox")), open]);
            if self.colored() {
                t.push(cs(self, named!("begingroup")));
            }
        }
        t.extend([cs(self, named!("hbox")), open]);
        if head {
            t.push(self.doc_id(doc!("footnotesize")));
        }
        t.extend([cs(self, named!("begin")), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, Token::char(Cat::Other, '['), Token::char(Cat::Letter, v), Token::char(Cat::Other, ']'), open, Token::char(Cat::Other, '@'), open, close, Token::char(Cat::Letter, h), Token::char(Cat::Other, '@'), open, close, close]);
        t.extend(text);
        t.extend([cs(self, named!("end")), open]);
        self.word_tokens("tabular", &mut t);
        t.extend([close, close]);
        if gape {
            if self.colored() {
                t.push(cs(self, named!("endgroup")));
            }
            t.extend([close, self.doc_id(doc!("rt@mcgape"))]);
        }
        if right {
            t.push(cs(self, named!("hfill")));
        }
        t.extend([cs(self, named!("hbox")), open, close]);
        self.back_list(&t);
    }

    /// The rest of `\makeg@pe`: `\mcell@mb@tb\z@\jot` (box 0's height and depth plus `\jot`),
    /// `\box\z@`.
    fn makecell_gape(&mut self) {
        let Some(b) = self.e.eqtb.box_reg(0) else { return self.not_native("a \\makecell box that is void") };
        let r = &mut self.e.nodes.boxes[b as usize];
        r.height += JOT;
        r.depth += JOT;
        let t = [self.named_cs(named!("box")), Token::char(Cat::Other, '0'), Token::char(Cat::Space, ' ')];
        self.back_list(&t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makecell_positions() {
        assert_eq!(mcell_align(""), Some(('c', 'c', true, true)));
        assert_eq!(mcell_align("l"), Some(('c', 'l', false, true)));
        assert_eq!(mcell_align("r"), Some(('c', 'r', true, false)));
        assert_eq!(mcell_align("t"), Some(('t', 'c', true, true)));
        assert_eq!(mcell_align("lt"), Some(('t', 'l', false, true)));
        assert_eq!(mcell_align("tl"), Some(('t', 'l', false, true)));
        assert_eq!(mcell_align("p"), None);
    }
}
