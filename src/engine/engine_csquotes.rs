//! csquotes.sty (2024-04-04 v5.2o) natively: `\enquote` and `\enquote*`, nested, in the
//! American and British English styles.
//!
//! `\enquote{text}` is `\csq@quote@i{}{}{text}`: `\csq@bqgroup` (`\begingroup\leavevmode`),
//! `\csq@oqopen@i` (the level, a local count, up one; `\enquote*` starts at 2), `\csq@qopen`:
//! `\csq@init` (`\csq@setsfcodes`: the encoding's quote glyphs `\sfcode` 0
//! (`\DeclareQuoteGlyph`, csquotes.def), `)` and `]` 0, and under `\nonfrenchspacing`
//! `\csq@setencodes`; `\csq@init@kern` for the kerns of adjoining marks),
//! `\csq@addkern@open`, the mark (outer for odd levels, inner for even), the marker
//! `\penalty\@M\hskip-13sp\hskip13sp`, `\csq@fixkern`; the text; `\csq@qclose@ii`:
//! `\csq@addkern@close`, the closing mark, at levels past 1 the marker `\kern-13sp\kern13sp`,
//! `\csq@eqgroup` (`\endgroup`), `\csq@fixkern`. The native job types the same list: the
//! tests on `\lastskip` and `\lastkern` as TeX's own conditionals, the kerns from the font's
//! lig/kern program.
//!
//! `\csq@fixkern` puts back the font's kern the marker broke: when the next token is a letter
//! or other character, `\settowidth` of the mark and the character less that of the mark,
//! `\kern\z@` and the character, as `\penalty\@M\hskip` when not zero. With a right boundary
//! character (the EC fonts' `^^W`) the second box also has the mark's kern with it. A
//! control sequence next is expanded three times first: one that is not a macro is never a
//! character, a macro declines (the native job does not expand ahead).
//!
//! The style: without babel, csquotes.def's `default` (`american`); with babel
//! (`autostyle=try`, csquotes.cfg), the language's at each quote (`\csq@resetstyle`), here
//! english's `american` or british's `british`. Other languages, `\frenchspacing`, a font
//! switch left open in the text (the `\csq@font` restore) decline.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Code, Reg, Region};
use super::super::engine_font::NON_CHAR;
use super::super::engine_prim::{Meaning, Prim, PROTECTED};
use super::super::engine_run::Machine;
use super::super::engine_token::Token;

/// csquotes' natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@enquote", |m| m.enquote()), ("rt@csqlevel", |m| m.csq_level())];

/// `\csq@qlevel`, local as the count's assignments are.
const CSQ_LEVEL: Reg = Reg::register(Region::Int, 32718);

/// The font declarations whose change `\csq@quote@i`'s `\csq@font` test would undo before the
/// closing mark: declined in the text outside a group.
const FONT_SWITCHES: &[&str] = &["rmfamily", "sffamily", "ttfamily", "bfseries", "mdseries", "itshape", "slshape", "scshape", "upshape", "normalfont", "em", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "selectfont", "fontsize", "usefont", "fontfamily", "fontseries", "fontshape", "rm", "sf", "tt", "bf", "it", "sl", "sc", "frenchspacing", "nonfrenchspacing"];

impl Machine<'_, '_> {
    /// `\enquote` and `\enquote*`.
    fn enquote(&mut self) {
        let star = self.next_is('*');
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let mut depth = 0i32;
        for t in &text {
            match t.cat() {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => depth -= 1,
                _ if depth == 0 => {
                    if let Some(id) = t.cs_id() {
                        if FONT_SWITCHES.contains(&self.e.cs.name(id)) {
                            return self.not_native("csquotes: a font switch in a quotation");
                        }
                    }
                }
                _ => {}
            }
        }
        // The style (`\csq@resetstyle`): babel's language in force, else `american`.
        let british = match self.doc.as_ref().and_then(|d| d.babel.as_ref()) {
            Some(b) => {
                let k = self.e.eqtb.word(super::babel::BABEL_LANG) as usize;
                if k != b.main {
                    return self.not_native("csquotes: a quotation in a language other than the main one");
                }
                match k {
                    0 => false,
                    1 => true,
                    _ => return self.not_native("csquotes: a language other than English"),
                }
            }
            None => false,
        };
        // `\csq@setsfcodes`: `\csq@setfrcodes` under `\frenchspacing`.
        if self.e.eqtb.code(Code::Sf, '.' as u32) <= 2000 {
            return self.not_native("csquotes under \\frenchspacing");
        }
        let t1 = self.doc.as_ref().is_some_and(|d| d.t1);
        let level = self.e.eqtb.word(CSQ_LEVEL);
        let level = if star && level == 0 { 2 } else { level + 1 };
        // The marks: `american` `\textquotedblleft` `\textquotedblright`, `\textquoteleft`
        // `\textquoteright`; `british` the other way round (csquotes.def).
        let (dbl, sgl) = (("textquotedblleft", "textquotedblright"), ("textquoteleft", "textquoteright"));
        let (outer, inner) = if british { (sgl, dbl) } else { (dbl, sgl) };
        let (open, close) = if level % 2 == 1 { outer } else { inner };
        let code = |name: &str| -> u8 {
            match (name, t1) {
                ("textquotedblleft", false) => 92,
                ("textquotedblleft", true) => 16,
                ("textquotedblright", false) => 34,
                ("textquotedblright", true) => 17,
                ("textquoteleft", _) => 96,
                _ => 39,
            }
        };
        let f = self.cur_font();
        // `\csq@init@kern{open}{o}{i}` and `{close}{i}{o}`: the kern of the two marks, the
        // second box broken by `\unkern` (which takes the boundary kern too); none, the
        // style's `0.05em`.
        let pair = |m: &Self, a: &str, b: &str| -> Option<String> {
            match m.lig_kern_step(f, code(a), code(b)) {
                Ok(Some(k)) if k != 0 => Some(format!("{k}sp")),
                Ok(_) => Some("0.05em".into()),
                Err(_) => None,
            }
        };
        let (Some(kern_open), Some(kern_close)) = (pair(self, outer.0, inner.0), pair(self, inner.1, outer.1)) else {
            return self.not_native("csquotes: marks that make a ligature");
        };
        let Some(fix_open) = self.csq_fixkern(f, code(open), text.first().copied()) else {
            return self.not_native("csquotes: a quotation beginning with a macro or a ligature with its mark");
        };
        let next = self.get_token();
        if let Some(t) = next {
            self.input.back(t);
        }
        let Some(fix_close) = self.csq_fixkern(f, code(close), next) else {
            return self.not_native("csquotes: a macro or a ligature with the mark after a quotation");
        };
        let glyphs: &[u8] = if t1 { &[13, 14, 15, 16, 17, 18, 19, 20, 34, 39, 96] } else { &[34, 39, 92, 96] };
        let mut s = format!("{} ", level);
        for g in glyphs {
            s.push_str(&format!("\\sfcode{g}=0 "));
        }
        s.push_str("\\sfcode41=0 \\sfcode93=0 \\sfcode44=1250 \\sfcode59=1500 \\sfcode58=2000 \\sfcode46=3000 \\sfcode33=3001 \\sfcode63=3002 ");
        s.push_str(&format!("\\ifdim\\lastskip=13sp \\unskip\\unskip\\unpenalty\\unkern\\penalty10000 \\hskip{kern_open}\\relax\\fi\\{open}\\penalty10000 \\hskip-13sp\\relax\\hskip13sp\\relax "));
        if fix_open != 0 {
            s.push_str(&format!("\\penalty10000 \\hskip{fix_open}sp\\relax "));
        }
        let mut list = vec![self.named_cs(named!("begingroup"))];
        let head = self.tex_tokens(&s);
        // `\leavevmode`, then the level: `rt@csqlevel` reads it.
        list.push(self.doc_id(super::LEAVE));
        list.push(self.doc_id(doc!("rt@csqlevel")));
        list.extend(head);
        list.extend(text);
        let mut s = format!("\\ifdim\\lastkern=13sp \\unkern\\unkern\\penalty10000 \\hskip{kern_close}\\relax\\fi\\{close}");
        if level > 1 {
            s.push_str("\\kern-13sp\\relax\\kern13sp\\relax");
        }
        s.push_str("\\endgroup");
        if fix_close != 0 {
            s.push_str(&format!("\\penalty10000 \\hskip{fix_close}sp\\relax"));
        }
        list.extend(self.tex_tokens(&s));
        self.back_list(&list);
    }

    /// `\csq@qlevel` set (a local assignment in the quotation's group): the number after.
    fn csq_level(&mut self) {
        let mut n = 0;
        while let Some(t) = self.get_token() {
            match t.cat() {
                Some(Cat::Other) if t.chr().is_ascii_digit() => n = n * 10 + t.chr() as i32 - '0' as i32,
                _ => break,
            }
        }
        self.e.eqtb.set_word(CSQ_LEVEL, n, false);
    }

    /// `\csq@fixkern` after mark `q` in font `f` with `next` the token that follows: the
    /// difference of the two boxes' widths in sp (0 for none), `None` when the native job
    /// cannot tell (a macro next, a ligature).
    fn csq_fixkern(&self, f: u16, q: u8, next: Option<Token>) -> Option<i32> {
        let Some(t) = next else { return Some(0) };
        let c = match t.cs_id() {
            // An active character is not a letter for `\ifcat` after `\noexpand`.
            Some(id) if id < 256 => return Some(0),
            Some(id) => match self.e.eqtb.meaning(id) {
                Meaning::Char(x) if matches!(x.cat(), Some(Cat::Letter | Cat::Other)) => return None,
                Meaning::Macro(m) if m.flags & PROTECTED == 0 => return None,
                Meaning::Undefined | Meaning::Unset => return None,
                // The enclosing quotation's close (`\csq@qclose@i`, here its `\ifdim\lastkern`):
                // csquotes' three expansions end on a macro, so no kern.
                Meaning::Prim(Prim::IfDim) => return Some(0),
                Meaning::Prim(p) if p.expandable() => return None,
                _ => return Some(0),
            },
            None if matches!(t.cat(), Some(Cat::Letter | Cat::Other)) => t.chr() as u32,
            None => return Some(0),
        };
        let Ok(c) = u8::try_from(c) else { return None };
        let font = &self.e.fonts.list[f as usize];
        let (bchar, label) = font.metrics.boundary();
        // A left boundary program would change the second box's character too.
        if label.is_some() {
            return None;
        }
        let k1 = self.lig_kern_step(f, q, c).ok()?.unwrap_or(0);
        let k2 = if bchar == NON_CHAR { 0 } else { self.lig_kern_step(f, q, bchar as u8).ok()?.unwrap_or(0) };
        Some(k1 - k2)
    }
}
