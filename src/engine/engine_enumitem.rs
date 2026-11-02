//! enumitem's `description` with keys on the GDL path (`2026-10-31-w8-lists-pages.md`).
//!
//! `\enit@description@i` (enumitem.sty:1461) inside `\list`'s second argument:
//! `\labelwidth\z@ \enit@align@left \let\makelabel\descriptionlabel \enit@style@standard`,
//! the keys (`\enit@preset`), then `\enit@calcleft` (enumitem.sty:874) computes the one
//! horizontal length the keys left to it (`\enit@calc`: 0 `\labelindent`, 1 `\labelwidth`,
//! 2 `\labelsep`, 3 `\leftmargin`, 4 `\itemindent`). The boxed styles (`standard`,
//! `normal`) make their label as the default list does (`item_rest`, `item_labels`); the
//! unboxed ones (`sameline`, `unboxed`, `nextline`) go through `\enit@postlabel`
//! (enumitem.sty:1532), which remakes `\@labels` and `\@item`'s `\everypar`: here
//! `enit_desc_labels` and `enit_desc_everypar`.
//!
//! A `*` value measures the empty `\@itemlabel` (no `widest`). Not native (each declines):
//! `multiline` (a `\parbox` label), `description<n>` `\setlist` keys, `labelindent`, `left`, `align`, `font`/`format`, `widest` and every other key.

use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_token::Token;
use super::{enit_pairs, Make, R_ITEMINDENT, R_LABELSEP, R_LABELWIDTH, R_LEFTMARGIN, R_LISTDEPTH, R_LISTPARINDENT, R_RIGHTMARGIN, TEMP_BOX};

impl Machine<'_, '_> {
    /// enumitem's `description` after `\list`'s `\@list<d>` (`list_open`) and `\labelwidth\z@`:
    /// the optional argument's keys in order, then `\enit@calcleft`, then `\@trivlist`.
    pub(super) fn enit_description(&mut self) {
        // `\enit@preset`'s `\enit@setkeys`: the `\setlist` keys of `list`, `list<\@listdepth>`
        // and `description`, then the optional argument's. Those of `description<n>` (its own
        // depth, `\enitdp@description`) are not read yet.
        let ld = self.rw(R_LISTDEPTH) as u8;
        let mut keys: Vec<Token> = Vec::new();
        if let Some(doc) = self.doc.as_ref() {
            if doc.pkg.enit.set.iter().any(|s| s.0 == 3 && s.1 > 0) {
                return self.not_native("enumitem: \\setlist keys for description at a level");
            }
            for (n, l) in [(0, 0), (0, ld), (3, 0)] {
                if let Some(k) = doc.pkg.enit.set.iter().find(|s| s.0 == n && s.1 == l) {
                    keys.extend(k.2.iter().copied());
                    keys.push(Token::char(Cat::Other, ','));
                }
            }
        }
        if self.next_is('[') {
            keys.extend(self.bracket_arg());
        }
        // `\enit@style@standard`: `\enit@boxdesctrue \enit@calcset\itemindent\enit@iv!`.
        let (mut calc, mut make) = (4u8, Make::Description);
        // `\ifenit@calcwidest`: a `*` value, the length computed from the widest label.
        let mut widest = false;
        let bang = [Token::char(Cat::Other, '!')];
        let star = [Token::char(Cat::Other, '*')];
        for (key, v) in enit_pairs(&keys) {
            match key.as_str() {
                "topsep" | "itemsep" | "parsep" | "partopsep" | "nosep" | "noitemsep" | "nolistsep" => {
                    if !self.enit_vertical(&key, &v) {
                        return;
                    }
                }
                // `\enit@calcset#1#2#3`: `!` makes the length the computed one; a value
                // sets it, and when it was the computed one, `\labelindent` is computed instead.
                "labelwidth" | "labelsep" | "leftmargin" | "itemindent" => {
                    let (id, r) = match key.as_str() {
                        "labelwidth" => (1, R_LABELWIDTH),
                        "labelsep" => (2, R_LABELSEP),
                        "leftmargin" => (3, R_LEFTMARGIN),
                        _ => (4, R_ITEMINDENT),
                    };
                    if v == bang || v == star {
                        calc = id;
                        widest = v == star;
                    } else {
                        let Some(g) = self.enit_value(&v, false) else { return };
                        if calc == id {
                            (calc, widest) = (0, false);
                        }
                        self.set_rw(r, g.width, false);
                    }
                }
                "rightmargin" | "listparindent" => {
                    let Some(g) = self.enit_value(&v, false) else { return };
                    self.set_rw(if key == "rightmargin" { R_RIGHTMARGIN } else { R_LISTPARINDENT }, g.width, false);
                }
                // `style` (default `normal`): `\enit@nextlinefalse \enit@boxdescfalse`, then
                // `\enit@style@<name>` (enumitem.sty:456-482).
                "style" => {
                    let name: String = v.iter().map(|t| t.chr()).collect();
                    widest = false;
                    (calc, make) = match if v.is_empty() { "normal" } else { name.as_str() } {
                        "standard" | "normal" => (4, Make::Description),
                        "unboxed" => (4, Make::DescUnboxed(false)),
                        "sameline" => (1, Make::DescUnboxed(false)),
                        "nextline" => (1, Make::DescUnboxed(true)),
                        _ => return self.not_native("an enumitem description style the native job does not take"),
                    };
                }
                _ => return self.not_native("an enumitem key the native job does not take"),
            }
        }
        // `\enit@calcleft`: `\dimen@` is `\leftmargin+\itemindent-\labelsep-\labelindent`
        // (`\labelindent` 0, `\enit@preset`'s `\labelindent\z@skip`); without a widest
        // label `\enit@calcwidth` takes `\labelwidth` off it, but for case 1.
        let (lm, ii, ls, mut lw) = (self.rw(R_LEFTMARGIN), self.rw(R_ITEMINDENT), self.rw(R_LABELSEP), self.rw(R_LABELWIDTH));
        let d = lm + ii - ls;
        // `\enit@calcwidth` with `*` (not for case 1): `\settowidth\labelwidth{\@itemlabel}`,
        // empty in description (`\list{}`), then `\advance\labelwidth-\labelsep`.
        if widest && calc != 1 {
            lw = -ls;
            self.set_rw(R_LABELWIDTH, lw, false);
        }
        match calc {
            1 => self.set_rw(R_LABELWIDTH, d, false),
            2 => self.set_rw(R_LABELSEP, ls + d - lw, false),
            3 => self.set_rw(R_LEFTMARGIN, lm - (d - lw), false),
            4 => self.set_rw(R_ITEMINDENT, ii - (d - lw), false),
            // `\labelindent`, which nothing reads.
            _ => {}
        }
        if let Some(list) = self.doc.as_mut().and_then(|d| d.lists.last_mut()) {
            list.make = make;
        }
        self.trivlist(|m| m.list_tail());
    }

    /// `\enit@postlabel@i`'s `\@labels` (enumitem.sty:1576): `\unhbox\@labels
    /// \hskip\itemindent \hskip-\labelwidth \hskip-\labelsep`, then the label wider than
    /// `\labelwidth` as `\enit@align{\unhbox\@tempboxa}\unskip` (`\nobreak` and the
    /// label; the `\hfil` unskipped), else `\hbox to\labelwidth{\enit@align{…}}`; then
    /// `\hskip\labelsep`. A `nextline` label wider than `\labelwidth` makes the item's
    /// `\enit@lblpenalty` `\newline\@nobreaktrue`.
    pub(super) fn enit_desc_labels(&mut self) {
        let nextline = self.doc.as_ref().and_then(|d| d.lists.last()).is_some_and(|l| l.make == Make::DescUnboxed(true));
        let w = self.rw(R_LABELWIDTH);
        let sep = self.rw(R_LABELSEP);
        let indent = self.rw(R_ITEMINDENT);
        self.push_nest(Mode::Horizontal, true, 1000);
        self.unhbox_labels();
        for g in [indent, -w, -sep] {
            let n = self.e.nodes.glue_node(Glue { width: g, ..Glue::default() }, 0);
            self.cur.push(n);
        }
        let mut wide = false;
        if let Some(tmp) = self.e.eqtb.box_reg(TEMP_BOX) {
            self.e.eqtb.replace_box(TEMP_BOX, None);
            let r = self.e.nodes.boxes[tmp as usize];
            wide = r.width > w;
            if !wide {
                self.push_nest(Mode::Horizontal, true, 1000);
            }
            self.cur.push(Node::new(Kind::Penalty, 10000));
            self.cur.extend_from_slice(self.e.nodes.get(r.list));
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(tmp);
            if !wide {
                let fil = self.glue_of("0pt plus 1fil");
                let n = self.e.nodes.glue_node(fil, 0);
                self.cur.push(n);
                let start = self.start as usize;
                let b = self.hpack(start, w, true);
                self.pop_nest();
                let b = self.e.nodes.add_box(b);
                self.cur.push(Node::new(Kind::Box, b as i32));
            }
        }
        let n = self.e.nodes.glue_node(Glue { width: sep, ..Glue::default() }, 0);
        self.cur.push(n);
        self.set_labels();
        if let Some(doc) = self.doc.as_mut() {
            doc.pkg.enit.newline = nextline && wide;
        }
        if self.microtype() {
            return self.left_protrusion();
        }
        self.ignore_spaces();
    }

    /// `\enit@postlabel@i`'s `\everypar` (enumitem.sty:1545): `\@item`'s, but the label is
    /// `\unhbox\@labels`, its last `\labelsep` taken off (`\skip@\lastskip \unskip`) and put
    /// back after `\penalty\z@`; or, for a wide `nextline` label, `\newline\@nobreaktrue`
    /// (`\@gnewline`: `\unskip \nobreak\hfil\break`).
    pub(super) fn enit_desc_everypar(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        doc.newlist = false;
        doc.floats.minipage = false;
        if doc.inlabel {
            doc.inlabel = false;
            let newline = doc.pkg.enit.newline;
            if !self.drop_last_box() {
                let k = -self.rw(R_ITEMINDENT);
                self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, k) });
            }
            self.unhbox_labels();
            let skip = match self.cur.last() {
                Some(n) if n.kind == Kind::Glue => self.e.nodes.glue(n),
                _ => Glue::default(),
            };
            self.unskip();
            if newline {
                self.unskip();
                self.cur.push(Node::new(Kind::Penalty, 10000));
                let fil = self.glue_of("0pt plus 1fil");
                let n = self.e.nodes.glue_node(fil, 0);
                self.cur.push(n);
                self.cur.push(Node::new(Kind::Penalty, -10000));
                if let Some(doc) = self.doc.as_mut() {
                    doc.nobreak = true;
                }
            } else {
                self.cur.push(Node::new(Kind::Penalty, 0));
                let n = self.e.nodes.glue_node(skip, 0);
                self.cur.push(n);
            }
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.nobreak {
            doc.nobreak = false;
            self.set_int_named(param!("clubpenalty"), 10000);
        } else {
            let v = self.rw(super::R_CLUBPENALTY);
            self.set_int_named(param!("clubpenalty"), v);
            self.set_everypar(if self.microtype() { super::MT_PROT } else { 0 });
        }
    }
}

/// enumitem's state (`Packages::enit`).
#[derive(Default, Clone, PartialEq)]
pub(super) struct Enit {
    /// enumitem's `\setlist` keys: the list (0 `list`, 1 `enumerate`, 2 `itemize`, 3
    /// `description`), the level (0 for all), the keys.
    pub(super) set: Vec<(u8, u8, Vec<Token>)>,
    /// enumitem's `\enit@lblpenalty` for the next item: `\newline\@nobreaktrue` (a
    /// `nextline` label wider than `\labelwidth`) instead of `\penalty\z@\hskip\skip@`.
    pub(super) newline: bool,
    /// The package option `shortlabels`: `\enit@shl` reads a first key without `=` as a
    /// label ([`short_label`]).
    pub(super) short: bool,
}

/// What a first key without `=` is to the native job under `shortlabels` (`\enit@first`,
/// enumitem.sty:660): `None` when it may be a key name (a letter of its own, other than
/// enumerate's one marker), else the marker's position (enumerate) or `Some(None)` (none).
/// Only labels whose letters are all inside control words are taken, so no key name of
/// enumitem's (all letters) can be read as a label.
fn short_marker(item: &[Token], enumerate: bool) -> Option<Option<usize>> {
    let mut marker = None;
    for (k, t) in item.iter().enumerate() {
        let c = t.chr();
        let is_marker = enumerate && t.cs_id().is_none() && (t.cat() == Some(Cat::Letter) && "aAiI".contains(c) || t.cat() == Some(Cat::Other) && c == '1');
        if is_marker {
            // `\enit@replace` for a second marker is enumitem's error (or another
            // marker's replacement): not taken.
            if marker.is_some() {
                return None;
            }
            marker = Some(k);
        } else if t.cs_id().is_none() && t.cat() == Some(Cat::Letter) {
            return None;
        }
    }
    Some(marker)
}

/// `\enit@shl{keys}` with `shortlabels` (enumitem.sty:1787): in `enumerate` and `itemize` a
/// first key that has no `=` and is not a key's name is `label=` it, enumerate's marker
/// (`\SetEnumerateShortLabel`: `a` `A` `i` `I` `1`) replaced by `\alph*` and kin. `None`
/// when the first key may be a key name ([`short_marker`]): the keys are then read as
/// written, and an unknown one declines.
pub(super) fn short_label(m: &mut Machine, keys: &[Token], enumerate: bool) -> Option<Vec<Token>> {
    let comma = Token::char(Cat::Other, ',');
    let mut depth = 0;
    let end = keys.iter().position(|t| {
        match t.cat() {
            Some(Cat::BeginGroup) => depth += 1,
            Some(Cat::EndGroup) => depth -= 1,
            _ => {}
        }
        depth == 0 && *t == comma
    }).unwrap_or(keys.len());
    let item = &keys[..end];
    let mut d = 0;
    let has_eq = item.iter().any(|t| {
        match t.cat() {
            Some(Cat::BeginGroup) => d += 1,
            Some(Cat::EndGroup) => d -= 1,
            _ => {}
        }
        d == 0 && *t == Token::char(Cat::Other, '=')
    });
    if has_eq || item.iter().all(|t| t.cat() == Some(Cat::Space)) {
        return None;
    }
    let marker = short_marker(item, enumerate)?;
    let mut out = m.tex_tokens("label=");
    for (k, &t) in item.iter().enumerate() {
        if Some(k) == marker {
            let name = match t.chr() {
                'a' => "alph",
                'A' => "Alph",
                'i' => "roman",
                'I' => "Roman",
                _ => "arabic",
            };
            out.push(Token::cs(m.e.cs.intern(name)));
            out.push(Token::char(Cat::Other, '*'));
        } else {
            out.push(t);
        }
    }
    out.extend_from_slice(&keys[end..]);
    Some(out)
}
