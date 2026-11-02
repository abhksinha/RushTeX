//! graphics.sty's other commands on the GDL path: `\graphicspath`, `\scalebox`,
//! `\resizebox` (graphics.sty 2024/08/06 v1.4g, pdftex.def as the driver).
//!
//! `\graphicspath{{dir/}..}` is `\def\Ginput@path{..}`: a grouped list of prefixes that
//! `\Ginclude@graphics` lets `\input@path` be, so `\IfFileExists` tries the name as given,
//! then each prefix before it (`Machine::graphics_file`). It is kept as an index into the
//! lists seen ([`GPATH`], grouped as the macro is).
//!
//! `\scalebox{sx}[sy]{text}` is `\Gscale@box`: `\leavevmode`, `\setbox\z@\hbox{{text}}` (the
//! engine sets it), then the scaled box from box 0 (`gscale_box`, shared with
//! `\includegraphics`). `\resizebox{w}{h}{text}` is `\Gscale@@box`: with both sizes `!` an
//! `\mbox`, else `\@begin@tempboxa\hbox{text}`, the factor `\Gscale@div` of the size asked
//! over the box's `\width` (or `\height`, `\totalheight` for `\resizebox*`), and
//! `\Gscale@box\@tempa[\@tempa]{\box\@tempboxa}` inside the group.

use super::super::engine_box::Ctx;
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::{Reg, Region};
use super::super::engine_graphics::{angle_number, scale_div, signed_times};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::TEMP_BOX;

/// `\Ginput@path` as an index plus one into [`GxState::paths`] (0: the kernel's, empty).
pub(super) const GPATH: Reg = Reg::register(Region::Int, 32745);

/// graphicx's state: the `\graphicspath` lists, and the scalings and resizings waiting for
/// their box.
#[derive(Default, PartialEq, Clone)]
pub(super) struct GxState {
    paths: Vec<Vec<String>>,
    /// `\Gscale@x`, `\Gscale@y` of each `\Gscale@box` whose box 0 is being set.
    scales: Vec<(String, String)>,
    /// Each `\resizebox`'s sizes and whether it is starred, while `\@tempboxa` is set.
    resizes: Vec<(Vec<Token>, Vec<Token>, bool)>,
    /// Each `\rotatebox`'s angle while its box 0 is set.
    angles: Vec<i32>,
}

pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@graphicspath", |m| m.graphics_path_def()),
    ("rt@scalebox", |m| m.scalebox()),
    ("rt@gscale", |m| m.gscale_end()),
    ("rt@resizebox", |m| m.resizebox()),
    ("rt@resizeboxend", |m| m.resizebox_end()),
    ("rt@rotatebox", |m| m.rotatebox()),
    ("rt@grot", |m| m.grot_end()),
];

/// The prefixes of a `\graphicspath` argument: brace groups of characters, spaces between
/// them skipped (`\@tfor` reads undelimited arguments). `None` for anything else.
fn path_list(arg: &[Token]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for t in arg {
        match (t.cat(), cur.as_mut()) {
            (Some(Cat::BeginGroup), None) => cur = Some(String::new()),
            (Some(Cat::EndGroup), Some(_)) => out.push(cur.take()?),
            (Some(Cat::Space), None) => {}
            (Some(Cat::BeginGroup | Cat::EndGroup) | None, _) | (_, None) => return None,
            (Some(_), Some(s)) => s.push(t.chr()),
        }
    }
    cur.is_none().then_some(out)
}

impl Machine<'_, '_> {
    /// The prefixes `\Ginclude@graphics` tries after the name as given: `\Ginput@path`.
    pub(crate) fn graphics_paths(&self) -> Vec<String> {
        let i = self.e.eqtb.word(GPATH);
        match self.doc.as_ref() {
            Some(d) if i > 0 => d.pkg.gx.paths.get(i as usize - 1).cloned().unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// `\graphicspath{list}`.
    fn graphics_path_def(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(list) = path_list(&arg) else {
            return self.not_native("a \\graphicspath that is not a list of folders");
        };
        let Some(doc) = self.doc.as_mut() else { return };
        doc.pkg.gx.paths.push(list);
        let i = doc.pkg.gx.paths.len() as i32;
        self.e.eqtb.set_word(GPATH, i, false);
    }

    /// `\scalebox{sx}[sy]{text}`: `\Gscale@box{sx}[sy]`'s `\leavevmode` and box 0, then
    /// `rt@gscale`.
    fn scalebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.mode == Mode::Math {
            return self.not_native("\\scalebox in math");
        }
        let mut sx = Vec::new();
        self.read_arg(&mut sx);
        let sy = if self.next_is('[') { self.bracket_arg() } else { sx.clone() };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let plain = |t: &[Token]| t.iter().all(|x| x.cat().is_some()).then(|| t.iter().map(|x| x.chr()).collect::<String>());
        let (Some(sx), Some(sy)) = (plain(&sx), plain(&sy)) else {
            return self.not_native("a \\scalebox factor given by commands");
        };
        if signed_times(&sx, 65536).is_none() || signed_times(&sy, 65536).is_none() {
            return self.not_native("a \\scalebox factor that is not a decimal");
        }
        self.gscale_start(sx, sy, text);
    }

    /// `\setbox\z@\hbox{{text}}` and `rt@gscale` with the factors waiting.
    fn gscale_start(&mut self, sx: String, sy: String, text: Vec<Token>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = Vec::with_capacity(text.len() + 8);
        t.extend([self.named_cs(named!("setbox")), Token::char(Cat::Other, '0'), self.named_cs(named!("hbox")), open, open]);
        t.extend(text);
        t.extend([close, close, self.doc_id(doc!("rt@gscale"))]);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.pkg.gx.scales.push((sx, sy));
        self.back_list(&t);
    }

    /// The rest of `\Gscale@box` once box 0 is set: box 2 and the box of the scaled width
    /// (`gscale_box`), from a copy of box 0 (`\copy\z@`).
    fn gscale_end(&mut self) {
        let Some((sx, sy)) = self.doc.as_mut().and_then(|d| d.pkg.gx.scales.pop()) else { return };
        let Some(b0) = self.e.eqtb.box_reg(0) else { return self.not_native("a scaled box that is void") };
        let r = self.e.nodes.boxes[b0 as usize];
        let copy = self.e.nodes.copy_box(b0);
        let Some(b) = self.gscale_box(copy, &sx, &sy, [r.width, r.height, r.depth]) else {
            return self.not_native("a box scaled out of range");
        };
        let id = self.e.nodes.add_box(b);
        // `\hb@xt@` ends with §1076's `box_end`: the box appended, the space factor 1000.
        self.box_end(Ctx::Shift(0), Some(id));
    }

    /// graphicx's `\rotatebox{angle}{text}` (`\Grot@box@std`): `\leavevmode`,
    /// `\setbox\z@\hbox{{text}}`, then `rt@grot`. The key-value form `[origin=..]` and an angle
    /// that is not an integer (trig.sty names its values by `\number`) decline.
    fn rotatebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.mode == Mode::Math {
            return self.not_native("\\rotatebox in math");
        }
        if self.next_is('[') {
            return self.not_native("\\rotatebox with options");
        }
        let (mut a, mut text) = (Vec::new(), Vec::new());
        self.read_arg(&mut a);
        self.read_arg(&mut text);
        let angle = a.iter().all(|x| x.cat().is_some()).then(|| a.iter().map(|x| x.chr()).collect::<String>()).as_deref().and_then(angle_number);
        let Some(angle) = angle else { return self.not_native("a \\rotatebox angle that is not an integer") };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = Vec::with_capacity(text.len() + 8);
        t.extend([self.named_cs(named!("setbox")), Token::char(Cat::Other, '0'), self.named_cs(named!("hbox")), open, open]);
        t.extend(text);
        t.extend([close, close, self.doc_id(doc!("rt@grot"))]);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.pkg.gx.angles.push(angle);
        self.back_list(&t);
    }

    /// `\Grot@box` once box 0 is set (`grot_box`, in its own group: box 0 is used there and
    /// comes back after it, so a copy is rotated), then `\leavevmode\box\z@`.
    fn grot_end(&mut self) {
        let Some(angle) = self.doc.as_mut().and_then(|d| d.pkg.gx.angles.pop()) else { return };
        let Some(b0) = self.e.eqtb.box_reg(0) else { return self.not_native("a rotated box that is void") };
        let copy = self.e.nodes.copy_box(b0);
        let Some(b) = self.grot_box(copy, angle) else { return self.not_native("a box rotated out of range") };
        let id = self.e.nodes.add_box(b);
        self.box_end(Ctx::Shift(0), Some(id));
    }

    /// `\resizebox{w}{h}{text}` and `\resizebox*`: `\leavevmode`, then `\Gscale@@box`.
    fn resizebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.mode == Mode::Math {
            return self.not_native("\\resizebox in math");
        }
        let star = self.next_is('*');
        let (mut w, mut h, mut text) = (Vec::new(), Vec::new(), Vec::new());
        self.read_arg(&mut w);
        self.read_arg(&mut h);
        self.read_arg(&mut text);
        // `\Gin@exclamation`: the argument is `!` alone (`\string#2`).
        let bang = |a: &[Token]| a.len() == 1 && a[0] == Token::char(Cat::Other, '!');
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        if bang(&w) && bang(&h) {
            let mut t = Vec::with_capacity(text.len() + 3);
            t.extend([self.named_cs(named!("mbox")), open]);
            t.extend(text);
            t.push(close);
            return self.back_list(&t);
        }
        // `\@begin@tempboxa\hbox{text}`: `\begingroup\setbox\@tempboxa\hbox{\color@begingroup
        // text\color@endgroup}`.
        let colored = self.colored();
        let mut t = Vec::with_capacity(text.len() + 12);
        t.extend([self.named_cs(named!("begingroup")), self.named_cs(named!("setbox"))]);
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([self.named_cs(named!("hbox")), open]);
        if colored {
            t.push(self.named_cs(named!("begingroup")));
        }
        t.extend(text);
        if colored {
            t.push(self.named_cs(named!("endgroup")));
        }
        t.extend([close, self.doc_id(doc!("rt@resizeboxend"))]);
        let Some(doc) = self.doc.as_mut() else { return };
        doc.pkg.gx.resizes.push((w, h, star));
        self.back_list(&t);
    }

    /// `\Gscale@box@dd` or `\Gscale@box@dddd` once `\@tempboxa` is set: the factors
    /// (`\setlength` of each size, `\width`, `\height`, `\depth` naming the box; `\Gscale@div`),
    /// then `\Gscale@box\@tempa[\@tempb]{\box\@tempboxa}` and `\@end@tempboxa`.
    fn resizebox_end(&mut self) {
        let Some((w, h, star)) = self.doc.as_mut().and_then(|d| d.pkg.gx.resizes.pop()) else { return };
        let Some(b) = self.e.eqtb.box_reg(TEMP_BOX) else { return self.not_native("a resized box that is void") };
        let r = self.e.nodes.boxes[b as usize];
        // `\height` or `\totalheight` (`\ht` plus `\dp` of the box).
        let tall = if star { r.height + r.depth } else { r.height };
        let bang = |a: &[Token]| a.len() == 1 && a[0] == Token::char(Cat::Other, '!');
        let factor = |m: &mut Self, size: Vec<Token>, den: i32| -> Option<String> {
            let v = m.box_length(size)?;
            scale_div(v, den)
        };
        let (sx, sy) = if bang(&h) {
            let Some(a) = factor(self, w, r.width) else { return self.not_native("a \\resizebox size") };
            (a.clone(), a)
        } else if bang(&w) {
            let Some(a) = factor(self, h, tall) else { return self.not_native("a \\resizebox size") };
            (a.clone(), a)
        } else {
            // `\ifGin@iso` is false outside `\includegraphics`.
            let (Some(a), Some(b)) = (factor(self, w, r.width), factor(self, h, tall)) else { return self.not_native("a \\resizebox size") };
            (a, b)
        };
        let mut text = vec![self.named_cs(named!("box"))];
        text.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        text.push(Token::char(Cat::Space, ' '));
        self.input.back(self.named_cs(named!("endgroup")));
        self.gscale_start(sx, sy, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_path_lists() {
        let g = |s: &str| -> Vec<Token> {
            s.chars().map(|c| match c {
                '{' => Token::char(Cat::BeginGroup, c),
                '}' => Token::char(Cat::EndGroup, c),
                ' ' => Token::char(Cat::Space, c),
                _ => Token::char(Cat::Other, c),
            }).collect()
        };
        assert_eq!(path_list(&g("{figs/}{img/}")), Some(vec!["figs/".to_string(), "img/".to_string()]));
        assert_eq!(path_list(&g("{a/} {b/}")), Some(vec!["a/".to_string(), "b/".to_string()]));
        assert_eq!(path_list(&g("figs/")), None);
        assert_eq!(path_list(&g("{a{b}}")), None);
    }
}
