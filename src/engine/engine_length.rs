//! The document's own lengths natively (latex.ltx's ltlength.dtx): `\newlength{\name}` and
//! `\settowidth`, `\settoheight`, `\settodepth` of one. `\setlength` and `\addtolength` of
//! them are the engine's own assignments (`set_length`), and the lengths are read wherever a
//! glue or dimension is scanned.
//!
//! `\newlength` is `\newskip` (`\e@alloc`'s `\global\skipdef` of the next skip register): a
//! native document cannot read the register's number (`\skip` and `@` names stop the
//! pre-scan, GDL R9), so the native job gives the `k`th one the hidden register
//! [`LENGTH_BASE`]` + k`, zero.

use super::super::engine_box::{Ctx, HBOX_GROUP};
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_prim::Meaning;
use super::super::engine_run::Machine;
use super::super::engine_token::Token;
use super::super::engine_catcode::Cat;

/// The skip register of the document's first `\newlength`.
pub(super) const LENGTH_BASE: u32 = 32000;
/// How many lengths a document may make (to the next hidden registers).
const LENGTH_ROOM: u32 = 500;

/// The lengths' natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("newlength", |m| m.new_length()),
    ("settowidth", |m| m.set_to_dim(0)),
    ("settoheight", |m| m.set_to_dim(1)),
    ("settodepth", |m| m.set_to_dim(2)),
    ("rt@settobox", |m| {
        m.input.align_state += 1;
        m.open_box(HBOX_GROUP, Ctx::Set { n: super::TEMP_BOX, global: false }, false, 0, false);
    }),
    ("rt@settodim", |m| m.set_to_dim_end()),
];

impl Machine<'_, '_> {
    /// Whether `reg` is one of the document's own lengths.
    fn own_length(reg: Reg) -> bool {
        reg.region == Region::Glue && (LENGTH_BASE..LENGTH_BASE + LENGTH_ROOM).contains(&reg.slot)
    }

    /// `\newlength{\name}`: `\@ifdefinable\name{\newskip\name}`, a new skip register, zero.
    fn new_length(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let [t] = arg[..] else { return self.not_native("\\newlength of more than one token") };
        let Some(id) = t.cs_id() else { return self.not_native("\\newlength of a character") };
        if self.meaning(t) != Meaning::Undefined {
            return self.not_native("\\newlength of a defined name");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.own_lengths >= LENGTH_ROOM as u16 {
            return self.not_native("more lengths than the native job keeps");
        }
        let reg = Reg::register(Region::Glue, LENGTH_BASE + u32::from(doc.own_lengths));
        doc.own_lengths += 1;
        self.e.eqtb.set_glue(reg, Glue::default(), true);
        self.e.eqtb.set_meaning(id, Meaning::Reg(reg), true);
    }

    /// `\settowidth{\len}{text}` and kin (`\@settodim`): `\setbox\@tempboxa\hbox{{text}}
    /// \len\wd\@tempboxa \setbox\@tempboxa\box\voidb@x`, the assignment local. Only the
    /// document's own lengths: a parameter the native job reads elsewhere declines.
    fn set_to_dim(&mut self, which: u8) {
        let mut target = Vec::new();
        self.read_arg(&mut target);
        let reg = match target[..] {
            [t] => t.cs_id().and_then(|id| match self.e.eqtb.meaning(id) {
                Meaning::Reg(r) if Self::own_length(r) => Some(r),
                _ => None,
            }),
            _ => None,
        };
        let Some(reg) = reg else { return self.not_native("\\settowidth of a length not the document's") };
        let mut text = Vec::new();
        self.read_arg(&mut text);
        if let Some(doc) = self.doc.as_mut() {
            doc.set_to.push((reg, which));
        }
        let mut t = Vec::with_capacity(text.len() + 5);
        t.extend([self.doc_id(doc!("rt@settobox")), Token::char(Cat::BeginGroup, '{')]);
        t.extend_from_slice(&text);
        t.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::EndGroup, '}'), self.doc_id(doc!("rt@settodim"))]);
        self.back_list(&t);
    }

    /// The box made: its width, height or depth into the length, the box voided.
    fn set_to_dim_end(&mut self) {
        let Some((reg, which)) = self.doc.as_mut().and_then(|d| d.set_to.pop()) else { return };
        let Some(b) = self.e.eqtb.box_reg(super::TEMP_BOX) else { return self.not_native("\\settowidth's box") };
        let r = self.e.nodes.boxes[b as usize];
        let v = [r.width, r.height, r.depth][which as usize];
        self.e.eqtb.replace_box(super::TEMP_BOX, None);
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
        self.e.eqtb.set_glue(reg, Glue { width: v, ..Glue::default() }, false);
    }
}
