//! # Engine state (`engine`)
//!
//! What TeX's macros read and assign: integer, dimension and glue parameters and registers,
//! their names, and grouping. tex.web defines the semantics (§220–§283); the representation
//! is ours: flat typed arrays indexed by number, a static parameter table, one save stack.
//! The expander drives it; layout reads it.

pub mod engine_assign;
pub mod engine_align;
pub mod engine_box;
pub mod engine_build;
pub mod engine_words;
pub mod engine_catcode;
pub mod engine_chars;
pub mod engine_checkpoint;
pub mod engine_chunk;
pub mod engine_cond;
pub mod engine_eqtb;
pub mod engine_explain;
pub mod engine_expr;
pub mod engine_font;
pub mod engine_gc;
pub mod engine_input;
pub mod engine_io;
#[cfg(feature = "oracle")]
pub mod engine_job;
#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "oracle")]
pub mod engine_host;
pub mod engine_lex;
pub mod engine_macro;
pub mod engine_md5;
pub mod engine_mlist;
pub mod engine_doc;
pub mod engine_geometry;
pub mod engine_kernel;
#[cfg(feature = "mprof")]
pub mod engine_mprof;
pub mod engine_native;
pub mod engine_names;
pub mod engine_noad;
pub mod engine_node;
pub mod engine_linebreak;
pub mod engine_hyph;
pub mod engine_pack;
pub mod engine_page;
pub mod engine_whatsit;
pub mod engine_prim;
pub mod engine_run;
pub mod engine_scan;
pub mod engine_show;
pub mod engine_pdfdoc;
pub mod engine_image;
pub mod engine_graphics;
pub mod engine_xform;
pub mod engine_microtype;
pub mod engine_regex;
#[cfg(feature = "oracle")]
pub mod engine_resume;
pub mod engine_ship;
pub mod engine_showbox;
pub mod engine_vf;
pub mod engine_color;
pub mod engine_tounicode;
#[cfg(feature = "oracle")]
pub mod engine_snapshot;
pub mod engine_tables;
pub mod engine_token;
pub mod engine_value;

pub use engine_catcode::{Cat, CatTable};
pub use engine_eqtb::{Code, Eqtb, Glue, GroupOverflow, Reg, Region};
pub use engine_lex::Lexer;
pub use engine_names::{param, Names};
pub use engine_prim::{prim, Macro, Meaning, Prim};
pub use engine_run::{Machine, Mode};
pub use engine_token::{CsTable, Span, Token, Tokens};

/// The engine's state: the equivalents, the names that reach them, control-sequence names
/// and the stored token lists (macro texts).
#[derive(Debug, Clone, PartialEq)]
pub struct Engine {
    pub eqtb: Eqtb,
    pub names: Names,
    pub cs: CsTable,
    pub toks: Tokens,
    pub fonts: engine_font::Fonts,
    /// Boxes and their lists (§133).
    pub nodes: engine_node::Nodes,
    /// Hyphenation patterns and exceptions (§920–§941).
    pub hyph: engine_hyph::Hyph,
    /// `\pdfglyphtounicode`'s table (pdfTeX's tounicode.c).
    pub glyph_unicode: engine_tounicode::GlyphUnicode,
}

impl Engine {
    /// A checkpoint's copy of the engine (`engine_checkpoint`): its big arrays frozen, each
    /// sharing the chunks that did not change since the last checkpoint (`engine_chunk`);
    /// the hyphenation memo (a cache) left out.
    pub fn freeze(&mut self) -> Engine {
        let Engine { eqtb, names, cs, toks, fonts, nodes, hyph, glyph_unicode } = self;
        Engine { eqtb: eqtb.freeze(), names: names.clone(), cs: cs.freeze(), toks: toks.freeze(), fonts: fonts.freeze(), nodes: nodes.freeze(), hyph: hyph.without_memo(), glyph_unicode: glyph_unicode.clone() }
    }

    /// Whether two frozen copies are equal but for their node arenas.
    pub fn same_but_nodes(&self, other: &Engine) -> bool {
        let Engine { eqtb, names, cs, toks, fonts, nodes: _, hyph, glyph_unicode } = self;
        *eqtb == other.eqtb && *names == other.names && *cs == other.cs && *toks == other.toks && *fonts == other.fonts && *hyph == other.hyph && *glyph_unicode == other.glyph_unicode
    }

    /// A live engine of a frozen copy.
    pub fn thaw(&self) -> Engine {
        let Engine { eqtb, names, cs, toks, fonts, nodes, hyph, glyph_unicode } = self;
        Engine { eqtb: eqtb.thaw(), names: names.clone(), cs: cs.thaw(), toks: toks.thaw(), fonts: fonts.thaw(), nodes: nodes.thaw(), hyph: hyph.clone(), glyph_unicode: glyph_unicode.clone() }
    }
}

/// Control-sequence ids fixed at startup: active character `c` is id `c` (§222
/// `active_base`), and the current font is id 256 (§230 `cur_font_loc`). Reading them is an
/// index, not a name lookup.
pub const CUR_FONT_ID: u32 = 256;
/// §230 `math_font_base`: `\textfont n` is id `FAM_BASE_ID + n`, `\scriptfont n` 16 later,
/// `\scriptscriptfont n` 32 later; kept as meanings, so they are grouped like any other.
pub const FAM_BASE_ID: u32 = 257;
/// §780 `frozen_end_template`: `\endtemplate`, which ends every alignment template; its
/// name cannot be typed.
pub const END_TEMPLATE_ID: u32 = FAM_BASE_ID + 48;

impl Default for Engine {
    /// §1336 IniTeX's state: every primitive and parameter name entered once with its
    /// meaning (§226), so looking one up is an array read like any other name.
    fn default() -> Self {
        let mut e = Engine { eqtb: Eqtb::default(), names: Names::default(), cs: CsTable::default(), toks: Tokens::default(), fonts: engine_font::Fonts::default(), nodes: engine_node::Nodes::default(), hyph: engine_hyph::Hyph::default(), glyph_unicode: Default::default() };
        for c in 0..256u32 {
            let mut buf = [0u8; 5];
            e.cs.intern(engine_run::active_key(char::from_u32(c).unwrap_or('\0'), &mut buf));
        }
        let font = e.cs.intern(engine_font::CUR_FONT);
        debug_assert_eq!(font, CUR_FONT_ID);
        for k in 0..48 {
            let id = e.cs.intern(&format!("\0fam{k}"));
            debug_assert_eq!(id, FAM_BASE_ID + k);
        }
        let id = e.cs.intern("\0endtemplate");
        debug_assert_eq!(id, END_TEMPLATE_ID);
        for name in engine_prim::builtin_names() {
            let id = e.cs.intern(name);
            e.eqtb.set_meaning(id, engine_prim::builtin(name), true);
        }
        e
    }
}

impl Engine {
    /// The quantity `name` means (without the backslash), if any.
    #[inline]
    pub fn lookup(&self, name: &str) -> Option<Reg> {
        self.names.lookup(name)
    }

    /// A dimension's value in scaled points, by name.
    pub fn dimen_sp(&self, name: &str) -> Option<i32> {
        self.lookup(name).filter(|r| r.region != Region::Int).map(|r| self.eqtb.word(r))
    }
}
