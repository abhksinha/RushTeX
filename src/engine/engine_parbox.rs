//! `\parbox` and `minipage` on the GDL path: latex.ltx's `\@iiiparbox`, `\@begin@tempboxa`,
//! `\@iiiminipage` and `\endminipage`.
//!
//! LaTeX sets the text in `\vbox{\hsize w \@parboxrestore text\@@par}` in `\@tempboxa`, then
//! puts that list in a `\vtop` (`[t]`), a `\vbox` (`[b]`) or `$\vcenter{..}$` (the default).
//! `\endminipage` boxes its list a second time through `\@iiiparbox` with `\unvbox`, which
//! moves the same list into a box of the same dimensions; here the minipage's own box is
//! used. The `\if@nobreak`, `\if@noskipsec` and `\\` base that `\@parboxrestore` resets
//! locally are kept on a stack and restored when the box closes. A height (`[t][3cm]`) and
//! an inner position (`[t][3cm][c]`) make the outer box `\vtop to 3cm{\vss\unvbox..\vss}`
//! (`\@parboxto`, `\bm@c` with `\let\hss\vss`). A footnote inside and a `minipage`
//! inside a list go to the engine.

use super::super::engine_box::{Ctx, VBOX_GROUP};
use super::super::engine_eqtb::{Reg, Region};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_catcode::Cat;
use super::super::engine_token::Token;
use super::{TEMP_BOX, R_LINEWIDTH};

/// What a box's `\@parboxrestore` hides, restored at its end: `\if@nobreak`,
/// `\if@noskipsec`, the lists `\\` looks through, and the outer position (`b'c'`, `b't'`,
/// `b'b'`) the box is put in.
#[derive(PartialEq, Clone, Copy)]
pub(super) struct Saved {
    nobreak: bool,
    noskipsec: bool,
    cr_base: usize,
    pos: u8,
    /// The height (`\@parboxto`), none for the box's natural height.
    height: Option<i32>,
    /// The inner position (`\bm@<p>` with `\let\hss\vss`): `b's'` (none), `b'c'`, `b't'`,
    /// `b'b'`.
    inner: u8,
    /// A minipage (its lists count from depth zero, `\@mplistdepth`) opened inside a
    /// list or trivlist.
    nested: bool,
}

/// Where a box goes: outer position, height and inner position (`\@iiiparbox#1#2[#3]`).
#[derive(Clone, Copy)]
struct BoxAt {
    pos: u8,
    height: Option<i32>,
    inner: u8,
}

impl Machine<'_, '_> {
    /// `\parbox[pos]{width}{text}`: `\leavevmode`, then `\begingroup \setbox\@tempboxa
    /// \vbox{\hsize width \@parboxrestore text\@@par}` and the outer box.
    pub(super) fn parbox(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("\\parbox in a formula");
        }
        if self.leave_vmode() {
            return;
        }
        let Some(at) = self.box_position() else { return };
        let (mut w, mut text) = (Vec::new(), Vec::new());
        self.read_arg(&mut w);
        self.read_arg(&mut text);
        let Some(width) = self.enit_value(&w, false).map(|g| g.width) else { return };
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.box_open(width, at, false);
        let par = self.named_cs(named!("par"));
        let close = self.doc_id(doc!("rt@pbclose"));
        text.extend([par, close]);
        self.back_list(&text);
    }

    /// `[pos][height][inner]` before a box's width (`\parbox`, `\@iparbox`, `\@iiparbox`):
    /// `c\relax[s]` when absent, `{pos}\relax[s]` for a position alone, `{pos}{height}[pos]`
    /// for a height with no inner position. An empty height is no height (`\ifx\relax#2`
    /// is false and skips the `\edef\@parboxto`). The height is read now, not after the box
    /// as `\@iiiparbox` does: the box's group has ended there, so only a global assignment
    /// inside the box, or `\height` and kin (refused), could tell the two apart. `None`
    /// (and the job declined) for any other position.
    fn box_position(&mut self) -> Option<BoxAt> {
        let mut at = BoxAt { pos: b'c', height: None, inner: b's' };
        let one = |arg: &[Token], ok: &[char]| match arg {
            [t] if t.cs_id().is_none() && ok.contains(&t.chr()) => Some(t.chr() as u8),
            _ => None,
        };
        if !self.next_is('[') {
            return Some(at);
        }
        let arg = self.bracket_arg();
        let Some(pos) = one(&arg, &['c', 't', 'b']) else {
            self.not_native("a box position other than c, t or b");
            return None;
        };
        at.pos = pos;
        if !self.next_is('[') {
            return Some(at);
        }
        let h = self.bracket_arg();
        at.inner = pos;
        if self.next_is('[') {
            let arg = self.bracket_arg();
            let Some(inner) = one(&arg, &['c', 't', 'b', 's']) else {
                self.not_native("a box inner position other than c, t, b or s");
                return None;
            };
            at.inner = inner;
        }
        if h.iter().any(|t| t.cs_id().is_some_and(|id| matches!(self.e.cs.name(id), "width" | "height" | "depth" | "totalheight"))) {
            self.not_native("a box height naming the box's size");
            return None;
        }
        if !h.is_empty() {
            at.height = Some(self.enit_value(&h, false)?.width);
        }
        Some(at)
    }

    /// `\setbox\@tempboxa\vbox{\color@begingroup \hsize width \@parboxrestore`: the box
    /// opened and what the restore hides saved.
    fn box_open(&mut self, width: i32, at: BoxAt, minipage: bool) {
        self.open_box(VBOX_GROUP, Ctx::Set { n: TEMP_BOX, global: false }, false, 0, false);
        if self.colored() {
            self.begin_group(SEMI_SIMPLE_GROUP);
        }
        self.set_dimen_named(param!("hsize"), width);
        let doc = self.doc.as_mut().expect("a native document");
        doc.pbox.push(Saved { nobreak: doc.nobreak, noskipsec: doc.noskipsec, cr_base: doc.cr_base, pos: at.pos, height: at.height, inner: at.inner, nested: minipage && !doc.lists.is_empty() });
        (doc.nobreak, doc.noskipsec, doc.cr_base) = (false, false, doc.lists.len());
        self.par_box_restore();
    }

    /// `\color@endgroup` and the `\vbox`'s `}` (after the text's `\@@par`), then the outer
    /// box of `\@iiiparbox` and `\@end@tempboxa`.
    pub(super) fn parbox_close(&mut self) {
        self.after_color_group(|m| m.parbox_close_box());
    }

    /// `\color@endgroup`, then `then` after the group's `\aftergroup` tokens (a `\color`'s
    /// `\reset@color`, set before the box's `}`); `then` at once with no colour package.
    pub(super) fn after_color_group(&mut self, then: fn(&mut Machine)) {
        if !self.colored() {
            return then(self);
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.steps.push(vec![super::Op::Call(then)]);
        }
        let step = self.doc_id(super::STEP);
        self.input.back(step);
        self.end_group(SEMI_SIMPLE_GROUP);
    }

    /// [`Self::parbox_close`] after the colour group.
    fn parbox_close_box(&mut self) {
        self.package();
        let doc = self.doc.as_mut().expect("a native document");
        let Some(s) = doc.pbox.pop() else { return };
        (doc.nobreak, doc.noskipsec, doc.cr_base) = (s.nobreak, s.noskipsec, s.cr_base);
        self.outer_box(s);
    }

    /// `\vtop{\unvbox\@tempboxa}`, `\vbox{..}` or `$\vcenter{..}\m@th$`, each `to` the
    /// height when there is one and with `\vss` before (`\bm@c`, `\bm@b`) and after
    /// (`\bm@c`, `\bm@t`) the list by the inner position, then `\endgroup`.
    fn outer_box(&mut self, s: Saved) {
        let pos = s.pos;
        let cs = |m: &Self, i| m.named_cs(i);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let math = Token::char(Cat::MathShift, '$');
        let mut t = Vec::with_capacity(16);
        match pos {
            b't' => t.push(cs(self, named!("vtop"))),
            b'b' => t.push(cs(self, named!("vbox"))),
            _ => t.extend([math, cs(self, named!("vcenter"))]),
        }
        if let Some(h) = s.height {
            self.word_tokens("to", &mut t);
            Self::sp_tokens(h, &mut t);
        }
        t.push(open);
        if matches!(s.inner, b'c' | b'b') {
            t.push(cs(self, named!("vss")));
        }
        t.push(cs(self, named!("unvbox")));
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.push(Token::char(Cat::Space, ' '));
        if matches!(s.inner, b'c' | b't') {
            t.push(cs(self, named!("vss")));
        }
        t.push(close);
        if pos == b'c' {
            t.extend([cs(self, named!("mathsurround")), Token::char(Cat::Other, '0'), Token::char(Cat::Letter, 'p'), Token::char(Cat::Letter, 't'), Token::char(Cat::Space, ' '), math]);
        }
        t.push(self.named_cs(named!("endgroup")));
        self.back_list(&t);
    }

    /// `\begin{minipage}[pos]{width}` inside `\begin`'s group: `\leavevmode`, then
    /// `\setbox\@tempboxa\vbox\bgroup \color@begingroup \hsize width \textwidth\hsize
    /// \columnwidth\hsize \@parboxrestore`, the list depth from zero (`\@mplistdepth`) and
    /// `\@setminipage`.
    pub(super) fn minipage_open(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("a minipage in a formula");
        }
        if self.leave_vmode() {
            return;
        }
        let Some(at) = self.box_position() else { return };
        let mut w = Vec::new();
        self.read_arg(&mut w);
        let Some(width) = self.enit_value(&w, false).map(|g| g.width) else { return };
        self.box_open(width, at, true);
        for name in ["textwidth", "columnwidth"] {
            let id = self.e.cs.intern(name);
            if let super::super::engine_prim::Meaning::Reg(r @ Reg { region: Region::Dimen, .. }) = self.e.eqtb.meaning(id) {
                self.e.eqtb.set_word(r, width, false);
            }
        }
        self.set_rw(R_LINEWIDTH, width, false);
        self.set_minipage();
    }

    /// `\endminipage` after its `\par`: `\unskip \@minipagefalse \color@endgroup \egroup`,
    /// then the outer box. The `\begingroup` the outer box ends is `\@begin@tempboxa`'s,
    /// opened here.
    pub(super) fn minipage_close_box(&mut self) {
        self.unskip();
        if let Some(doc) = self.doc.as_mut() {
            doc.floats.minipage = false;
        }
        self.after_color_group(|m| {
            m.package();
            let doc = m.doc.as_mut().expect("a native document");
            let Some(s) = doc.pbox.pop() else { return };
            (doc.nobreak, doc.noskipsec, doc.cr_base) = (s.nobreak, s.noskipsec, s.cr_base);
            m.begin_group(SEMI_SIMPLE_GROUP);
            m.outer_box(s);
        });
    }

    /// `\fbox{text}`: `\leavevmode \setbox\@tempboxa\hbox{\color@begingroup
    /// \kern\fboxsep{text}\kern\fboxsep\color@endgroup}`, then `\@frameb@x\relax`.
    pub(super) fn fbox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let sep = self.e.eqtb.word(super::FBOXSEP);
        let mut t = vec![self.named_cs(named!("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([self.named_cs(named!("hbox")), open]);
        let colored = self.colored();
        if colored {
            t.push(self.named_cs(named!("begingroup")));
        }
        t.push(self.named_cs(named!("kern")));
        Self::sp_tokens(sep, &mut t);
        t.push(open);
        t.extend(text);
        t.extend([close, self.named_cs(named!("kern"))]);
        Self::sp_tokens(sep, &mut t);
        if colored {
            t.push(self.named_cs(named!("endgroup")));
        }
        t.extend([close, self.doc_id(doc!("rt@framebox"))]);
        self.back_list(&t);
    }

    /// `\makebox[width][pos]{text}` (latex.ltx): `\leavevmode`, then without `[` `\mbox`;
    /// `\@imakebox`: `\@begin@tempboxa\hbox{text}` (`\begingroup \setbox\@tempboxa\hbox
    /// {\color@begingroup text\color@endgroup}`, then `\width`, `\height`, `\depth` and
    /// `\totalheight` defined for the width), `\setlength\@tempdima{width}`, `\hb@xt@
    /// \@tempdima{\bm@pos}` (`\bm@c` `\hss\unhbox\@tempboxa\hss`, `\bm@l` and `\bm@t`
    /// `\unhbox\@tempboxa\hss`, `\bm@r` and `\bm@b` `\hss\unhbox\@tempboxa`, `\bm@s`
    /// `\unhbox\@tempboxa`), `\@end@tempboxa` (`\endgroup`). The picture form `\makebox(x,y)`
    /// and a width naming `\width` and kin are not native.
    pub(super) fn makebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.next_is('(') {
            return self.not_native("\\makebox in a picture");
        }
        if !self.next_is('[') {
            let t = self.named_cs(named!("mbox"));
            return self.input.back(t);
        }
        let w = self.bracket_arg();
        let mut pos = 'c';
        if self.next_is('[') {
            match self.bracket_arg().as_slice() {
                [t] if t.cs_id().is_none() && matches!(t.chr(), 'c' | 'l' | 'r' | 's' | 't' | 'b') => pos = t.chr(),
                _ => return self.not_native("a \\makebox position other than c, l, r, s, t or b"),
            }
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if w.iter().any(|t| t.cs_id().is_some_and(|id| matches!(self.e.cs.name(id), "width" | "height" | "depth" | "totalheight"))) {
            return self.not_native("a \\makebox width naming the box's size");
        }
        let Some(width) = self.enit_value(&w, false).map(|g| g.width) else { return };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let temp = |t: &mut Vec<Token>| t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        let colored = self.colored();
        let mut t = Vec::with_capacity(text.len() + 32);
        t.extend([self.named_cs(named!("begingroup")), self.named_cs(named!("setbox"))]);
        temp(&mut t);
        t.extend([self.named_cs(named!("hbox")), open]);
        if colored {
            t.push(self.named_cs(named!("begingroup")));
        }
        t.extend(text);
        if colored {
            t.push(self.named_cs(named!("endgroup")));
        }
        t.extend([close, self.named_cs(named!("hbox"))]);
        self.word_tokens("to", &mut t);
        Self::sp_tokens(width, &mut t);
        t.push(open);
        if matches!(pos, 'c' | 'r' | 'b') {
            t.push(self.named_cs(named!("hss")));
        }
        t.push(self.named_cs(named!("unhbox")));
        temp(&mut t);
        t.push(Token::char(Cat::Space, ' '));
        if matches!(pos, 'c' | 'l' | 't') {
            t.push(self.named_cs(named!("hss")));
        }
        t.extend([close, self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// `\raisebox{lift}[height][depth]{text}` (latex.ltx): `\leavevmode`, then `\@irsbox`
    /// or `\@iirsbox`: `\@begin@tempboxa\hbox{text}`, then the lengths, read with `\width`,
    /// `\height` and `\depth` naming the box (`rt@raisebox`, `raisebox_rest`).
    pub(super) fn raisebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        let mut lift = Vec::new();
        self.read_arg(&mut lift);
        let height = self.next_is('[').then(|| self.bracket_arg());
        let depth = if height.is_some() && self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
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
        t.extend([close, self.doc_id(doc!("rt@raisebox"))]);
        let doc = self.doc.as_mut().expect("a native document");
        doc.raises.push([Some(lift), height, depth]);
        self.back_list(&t);
    }

    /// The rest of `\@irsbox`/`\@iirsbox` once `\@tempboxa` is set: `\setlength\@tempdima
    /// {lift}`, the height (unless empty, `\ifx\\#2\\`) and the depth, `\setbox\@tempboxa
    /// \hbox{\raise\@tempdima\box\@tempboxa}`, `\ht` and `\dp` set, `\box\@tempboxa`,
    /// `\endgroup`. `\width`, `\height` and `\depth` (`\def\height{\ht\@tempboxa}`) become
    /// `\wd`, `\ht`, `\dp` of the box (`box_length`).
    pub(super) fn raisebox_rest(&mut self) {
        let Some([lift, height, depth]) = self.doc.as_mut().and_then(|d| d.raises.pop()) else { return };
        let temp = |t: &mut Vec<Token>| t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        let len = |m: &mut Self, arg: Option<Vec<Token>>| -> Result<Option<i32>, ()> {
            match arg.filter(|a| !a.is_empty()) {
                None => Ok(None),
                Some(arg) => m.box_length(arg).map(Some).ok_or(()),
            }
        };
        let Ok(Some(lift)) = len(self, lift) else { return };
        let Ok(height) = len(self, height) else { return };
        let Ok(depth) = len(self, depth) else { return };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.named_cs(named!("setbox"))];
        temp(&mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("raise"))]);
        Self::sp_tokens(lift, &mut t);
        t.push(self.named_cs(named!("box")));
        temp(&mut t);
        t.extend([Token::char(Cat::Space, ' '), close]);
        for (v, prim) in [(height, named!("ht")), (depth, named!("dp"))] {
            if let Some(v) = v {
                t.push(self.named_cs(prim));
                temp(&mut t);
                t.push(Token::char(Cat::Other, '='));
                Self::sp_tokens(v, &mut t);
            }
        }
        t.push(self.named_cs(named!("box")));
        temp(&mut t);
        t.extend([Token::char(Cat::Space, ' '), self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// `\kern<k>sp`, `\@frameb@x`'s `#1` for `\framebox`.
    fn inner_kern(&mut self, inner: Option<i32>, t: &mut Vec<Token>) {
        if let Some(k) = inner {
            t.push(self.named_cs(named!("kern")));
            Self::sp_tokens(k, t);
        }
    }

    /// `\framebox[width][pos]{text}` (latex.ltx): without `[` it is `\fbox`; `\@iframebox`:
    /// `\leavevmode \@begin@tempboxa\hbox{text}`, then `rt@iframebox` (`framebox_rest`). The
    /// picture form `\framebox(x,y)` is not native.
    pub(super) fn framebox(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.next_is('(') {
            return self.not_native("\\framebox in a picture");
        }
        if !self.next_is('[') {
            let t = self.doc_id(doc!("fbox"));
            return self.input.back(t);
        }
        let w = self.bracket_arg();
        let mut pos = 'c';
        if self.next_is('[') {
            match self.bracket_arg().as_slice() {
                [t] if t.cs_id().is_none() && matches!(t.chr(), 'c' | 'l' | 'r' | 's' | 't' | 'b') => pos = t.chr(),
                _ => return self.not_native("a \\framebox position other than c, l, r, s, t or b"),
            }
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
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
        t.extend([close, self.doc_id(doc!("rt@iframebox"))]);
        let doc = self.doc.as_mut().expect("a native document");
        doc.raises.push([Some(w), Some(vec![Token::char(Cat::Other, pos)]), None]);
        self.back_list(&t);
    }

    /// The rest of `\@iframebox` once `\@tempboxa` is set: `\setlength\@tempdima{width}`
    /// (`\width` and kin naming the box), `\setbox\@tempboxa\hb@xt@\@tempdima{\kern\fboxsep
    /// \bm@<pos>\kern\fboxsep}`, `\@frameb@x{\kern-\fboxrule}`, `\@end@tempboxa`.
    pub(super) fn framebox_rest(&mut self) {
        let Some([Some(w), Some(pos), _]) = self.doc.as_mut().and_then(|d| d.raises.pop()) else { return };
        let pos = pos.first().map_or('c', |t| t.chr());
        let Some(width) = self.box_length(w) else { return };
        let sep = self.e.eqtb.word(super::FBOXSEP);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let temp = |t: &mut Vec<Token>| t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        let mut t = vec![self.named_cs(named!("setbox"))];
        temp(&mut t);
        t.push(self.named_cs(named!("hbox")));
        self.word_tokens("to", &mut t);
        Self::sp_tokens(width, &mut t);
        t.extend([open, self.named_cs(named!("kern"))]);
        Self::sp_tokens(sep, &mut t);
        if matches!(pos, 'c' | 'r' | 'b') {
            t.push(self.named_cs(named!("hss")));
        }
        t.push(self.named_cs(named!("unhbox")));
        temp(&mut t);
        t.push(Token::char(Cat::Space, ' '));
        if matches!(pos, 'c' | 'l' | 't') {
            t.push(self.named_cs(named!("hss")));
        }
        t.push(self.named_cs(named!("kern")));
        Self::sp_tokens(sep, &mut t);
        t.extend([close, self.doc_id(doc!("rt@framekern")), self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// `\@frameb@x{\kern-\fboxrule}` (`framebox_rest`).
    pub(super) fn frame_kern(&mut self) {
        let rule = self.e.eqtb.word(super::FBOXRULE);
        self.frame_box_with(Some(-rule));
    }

    /// A length of `\@begin@tempboxa`'s box (`\raisebox`, `\framebox`): `\width`, `\height`,
    /// `\depth` (`\def\height{\ht\@tempboxa}`) become `\wd`, `\ht`, `\dp` of it;
    /// `\totalheight`, a register set once, declines.
    fn box_length(&mut self, arg: Vec<Token>) -> Option<i32> {
        let mut t = Vec::with_capacity(arg.len() + 8);
        for tok in arg {
            let prim = match tok.cs_id().map(|id| self.e.cs.name(id)) {
                Some("width") => named!("wd"),
                Some("height") => named!("ht"),
                Some("depth") => named!("dp"),
                Some("totalheight") => {
                    self.not_native("\\totalheight in a box length");
                    return None;
                }
                _ => {
                    t.push(tok);
                    continue;
                }
            };
            t.push(self.named_cs(prim));
            t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
            t.push(Token::char(Cat::Space, ' '));
        }
        self.enit_value(&t, false).map(|g| g.width)
    }

    /// `\@frameb@x\relax`: `\hbox{\lower d\hbox{\vbox{\hrule\@height\fboxrule
    /// \hbox{\vrule\@width\fboxrule \vbox{\vskip\fboxsep \box\@tempboxa \vskip\fboxsep}
    /// \vrule\@width\fboxrule}\hrule\@height\fboxrule}}}`, `d` the rule, the separation
    /// and the box's depth.
    pub(super) fn frame_box(&mut self) {
        self.frame_box_with(None);
    }

    /// `\@frameb@x{#1}` with `#1` a `\kern` (`\framebox`'s `\kern-\fboxrule`) inside each
    /// side's rule, or nothing (`\fbox`'s `\relax`).
    fn frame_box_with(&mut self, inner: Option<i32>) {
        let (sep, rule) = (self.e.eqtb.word(super::FBOXSEP), self.e.eqtb.word(super::FBOXRULE));
        let depth = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].depth);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        if self.doc.as_ref().is_some_and(|d| d.packages.contains(&"xcolor")) {
            return self.xc_frame_box(sep, rule, depth, inner);
        }
        let mut t = vec![self.named_cs(named!("hbox")), open, self.named_cs(named!("lower"))];
        Self::sp_tokens(rule + sep + depth, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("vbox")), open, self.named_cs(named!("hrule"))]);
        self.word_tokens("height", &mut t);
        Self::sp_tokens(rule, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("vrule"))]);
        self.word_tokens("width", &mut t);
        Self::sp_tokens(rule, &mut t);
        self.inner_kern(inner, &mut t);
        t.extend([self.named_cs(named!("vbox")), open, self.named_cs(named!("vskip"))]);
        Self::sp_tokens(sep, &mut t);
        t.push(self.named_cs(named!("box")));
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Space, ' '), self.named_cs(named!("vskip"))]);
        Self::sp_tokens(sep, &mut t);
        t.push(close);
        self.inner_kern(inner, &mut t);
        t.push(self.named_cs(named!("vrule")));
        self.word_tokens("width", &mut t);
        Self::sp_tokens(rule, &mut t);
        t.extend([close, self.named_cs(named!("hrule"))]);
        self.word_tokens("height", &mut t);
        Self::sp_tokens(rule, &mut t);
        t.extend([close, close, close]);
        self.back_list(&t);
    }

    /// xcolor's `\XC@frameb@x\relax`: `\hbox{\setbox\@tempboxa\hbox{\lower d\hbox{\vbox{
    /// \kern\fboxrule \hbox{\kern\fboxrule \vbox{\kern\fboxsep\box\@tempboxa\kern\fboxsep}
    /// \kern\fboxrule}\kern\fboxrule}}}`, then `xc_frame` (the box, a kern back over it and
    /// `\boxframe`) inside the `\hbox`.
    fn xc_frame_box(&mut self, sep: i32, rule: i32, depth: i32, inner: Option<i32>) {
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let kern = self.named_cs(named!("kern"));
        let mut t = vec![self.named_cs(named!("hbox")), open, self.named_cs(named!("setbox"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("lower"))]);
        Self::sp_tokens(rule + sep + depth, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("vbox")), open, kern]);
        Self::sp_tokens(rule, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, kern]);
        Self::sp_tokens(rule, &mut t);
        self.inner_kern(inner, &mut t);
        t.extend([self.named_cs(named!("vbox")), open, kern]);
        Self::sp_tokens(sep, &mut t);
        t.push(self.named_cs(named!("box")));
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Space, ' '), kern]);
        Self::sp_tokens(sep, &mut t);
        t.push(close);
        self.inner_kern(inner, &mut t);
        t.push(kern);
        Self::sp_tokens(rule, &mut t);
        t.extend([close, kern]);
        Self::sp_tokens(rule, &mut t);
        t.extend([close, close, close, self.doc_id(doc!("rt@xcframe")), close]);
        self.back_list(&t);
    }

    /// `\box\@tempboxa \kern-W \boxframe{W}{H}{D}` with xcolor's rule-drawn `\boxframe`
    /// (no `xcdraw`): `\hbox{\lower D\vbox{\hrule\@height r \kern t \hbox{\vrule\@width r
    /// \@height(H+D-r)\@depth\z@ \kern(W-2r) \vrule..}\kern-.5r \hrule\@height r}}`, `t`
    /// being `-.5r`, one more `sp` down when `r` is odd.
    pub(super) fn xc_frame(&mut self) {
        let rule = self.e.eqtb.word(super::FBOXRULE);
        let (w, h, d) = self.e.eqtb.box_reg(TEMP_BOX).map_or((0, 0, 0), |b| {
            let b = &self.e.nodes.boxes[b as usize];
            (b.width, b.height, b.depth)
        });
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let kern = self.named_cs(named!("kern"));
        let half = (rule as i64 * 32768 / 65536) as i32;
        let top = -half - (rule & 1);
        let mut t = vec![self.named_cs(named!("box"))];
        t.extend(TEMP_BOX.to_string().chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::Space, ' '), kern]);
        Self::sp_tokens(-w, &mut t);
        t.extend([self.named_cs(named!("hbox")), open, self.named_cs(named!("lower"))]);
        Self::sp_tokens(d, &mut t);
        t.extend([self.named_cs(named!("vbox")), open, self.named_cs(named!("hrule"))]);
        self.word_tokens("height", &mut t);
        Self::sp_tokens(rule, &mut t);
        t.push(kern);
        Self::sp_tokens(top, &mut t);
        t.push(self.named_cs(named!("hbox")));
        t.push(open);
        for k in 0..2 {
            t.push(self.named_cs(named!("vrule")));
            self.word_tokens("width", &mut t);
            Self::sp_tokens(rule, &mut t);
            self.word_tokens("height", &mut t);
            Self::sp_tokens(h + d - rule, &mut t);
            self.word_tokens("depth", &mut t);
            Self::sp_tokens(0, &mut t);
            if k == 0 {
                t.push(kern);
                Self::sp_tokens(w - 2 * rule, &mut t);
            }
        }
        t.extend([close, kern]);
        Self::sp_tokens(-half, &mut t);
        t.push(self.named_cs(named!("hrule")));
        self.word_tokens("height", &mut t);
        Self::sp_tokens(rule, &mut t);
        t.extend([close, close]);
        self.back_list(&t);
    }

    /// xcolor's `\colorbox[model]{spec}{text}` (`\color@box`) and `\fcolorbox[model]{frame}
    /// [model]{spec}{text}` (`\color@fb@x`), both `\color@b@x`: `\leavevmode
    /// \setbox\z@\hbox{\kern\fboxsep{\set@color text}\kern\fboxsep}`, then `rt@colorbox`.
    pub(super) fn colorbox(&mut self, framed: bool) {
        if self.leave_vmode() {
            return;
        }
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        // `\color[model]{spec}` as tokens for `rt@color`.
        let color = |m: &mut Self| {
            let mut t = vec![m.doc_id(doc!("rt@color"))];
            if m.next_is('[') {
                t.push(Token::char(Cat::Other, '['));
                t.extend(m.bracket_arg());
                t.push(Token::char(Cat::Other, ']'));
            }
            let mut arg = Vec::new();
            m.read_arg(&mut arg);
            t.push(open);
            t.extend(arg);
            t.push(close);
            t
        };
        let frame = if framed { Some(color(self)) } else { None };
        let back = color(self);
        let mut text = Vec::new();
        self.read_arg(&mut text);
        let sep = self.e.eqtb.word(super::FBOXSEP);
        let doc = self.doc.as_mut().expect("a native document");
        doc.colorboxes.push((frame.unwrap_or_default(), back));
        let mut t = vec![self.named_cs(named!("setbox")), Token::char(Cat::Other, '0'), self.named_cs(named!("hbox")), open, self.named_cs(named!("kern"))];
        Self::sp_tokens(sep, &mut t);
        t.extend([open, self.doc_id(doc!("rt@setcolor"))]);
        t.extend(text);
        t.extend([close, self.named_cs(named!("kern"))]);
        Self::sp_tokens(sep, &mut t);
        t.extend([close, self.doc_id(doc!("rt@colorbox"))]);
        self.back_list(&t);
    }

    /// The rest of `\color@b@x`: `\ht\z@` and `\dp\z@` grown by `\fboxsep`, then
    /// `{#1{#2\color@block{\wd\z@}{\ht\z@}{\dp\z@}\box\z@}}` with `\color@block`
    /// `{\set@color\rlap{\vrule\@width..\@height..\@depth..}}`; for `\fcolorbox` `#1` is
    /// `\fboxsep\z@\color{frame}\XC@fbox`.
    pub(super) fn colorbox_rest(&mut self) {
        let sep = self.e.eqtb.word(super::FBOXSEP);
        let (w, h, d) = match self.e.eqtb.box_reg(0) {
            Some(b) => {
                let r = &mut self.e.nodes.boxes[b as usize];
                r.height += sep;
                r.depth += sep;
                (r.width, r.height, r.depth)
            }
            None => (0, 0, 0),
        };
        let Some((frame, back)) = self.doc.as_mut().and_then(|d| d.colorboxes.pop()) else { return };
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![open];
        if frame.is_empty() {
            t.push(self.named_cs(named!("relax")));
        } else {
            t.push(self.doc_id(doc!("rt@fboxsepzero")));
            t.extend(frame);
            t.push(self.doc_id(doc!("fbox")));
        }
        t.push(open);
        t.extend(back);
        t.extend([open, self.doc_id(doc!("rt@setcolor")), self.named_cs(named!("hbox"))]);
        self.word_tokens("to", &mut t);
        Self::sp_tokens(0, &mut t);
        t.extend([open, self.named_cs(named!("vrule"))]);
        self.word_tokens("width", &mut t);
        Self::sp_tokens(w, &mut t);
        self.word_tokens("height", &mut t);
        Self::sp_tokens(h, &mut t);
        self.word_tokens("depth", &mut t);
        Self::sp_tokens(d, &mut t);
        t.extend([self.named_cs(named!("hss")), close, close, self.named_cs(named!("box")), Token::char(Cat::Other, '0'), Token::char(Cat::Space, ' '), close, close]);
        self.back_list(&t);
    }

    /// Whether a list opened now would be in a minipage that is itself in a list or
    /// trivlist: `\@mplistdepth` starts its depth again, which the native lists do not.
    pub(super) fn list_in_nested_minipage(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pbox.iter().any(|s| s.nested))
    }

    /// Whether a `\parbox` or `minipage` is open (its footnotes are not native).
    pub(super) fn in_parbox(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| !d.pbox.is_empty())
    }
}
