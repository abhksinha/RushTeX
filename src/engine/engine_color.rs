//! pdfTeX's colour stacks (pdftex.web `pdf_out_colorstack`, utils.c `newcolorstack`,
//! `colorstackpush` and kin), in our own form: page stacks only (forms come with `\pdfxform`).
//!
//! A stack's current literal survives from page to page; a stack made with `page` repeats
//! its current literal at the top of every page unless it is the default `0 g 0 G`.

use super::engine_run::Machine;
use super::engine_value::str_toks;
use super::engine_token::Token;

/// pdfTeX's literal modes.
pub const SET_ORIGIN: u8 = 0;
pub const DIRECT_PAGE: u8 = 1;
pub const DIRECT_ALWAYS: u8 = 2;

/// utils.c `COLOR_DEFAULT`.
const DEFAULT: &str = "0 g 0 G";

pub struct ColorStack {
    pub mode: u8,
    page_start: bool,
    current: Option<String>,
    saved: Vec<Option<String>>,
}

pub struct ColorStacks(pub Vec<ColorStack>);

impl Default for ColorStacks {
    /// `colstacks_first_init`: stack 0 is the default colour, direct, repeated per page.
    fn default() -> Self {
        ColorStacks(vec![ColorStack { mode: DIRECT_ALWAYS, page_start: true, current: Some(DEFAULT.into()), saved: Vec::new() }])
    }
}

fn some(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

impl ColorStacks {
    /// `colorstackset`: the stack's mode, the literal to write.
    pub fn set(&mut self, n: usize, s: String) -> Option<(u8, String)> {
        let st = self.0.get_mut(n)?;
        st.current = some(s.clone());
        Some((st.mode, s))
    }

    pub fn push(&mut self, n: usize, s: String) -> Option<(u8, String)> {
        let st = self.0.get_mut(n)?;
        st.saved.push(st.current.take());
        st.current = some(s.clone());
        Some((st.mode, s))
    }

    /// `colorstackpop`: the literal now current (nothing written on an empty stack).
    pub fn pop(&mut self, n: usize) -> Option<(u8, String)> {
        let st = self.0.get_mut(n)?;
        let prev = st.saved.pop()?;
        st.current = prev;
        Some((st.mode, st.current.clone().unwrap_or_default()))
    }

    pub fn current(&self, n: usize) -> Option<(u8, String)> {
        let st = self.0.get(n)?;
        Some((st.mode, st.current.clone().unwrap_or_default()))
    }

    /// `pdf_out_colorstack_startpage`: the literals that begin a page.
    pub fn page_start(&self) -> Vec<(u8, String)> {
        self.0
            .iter()
            .filter(|st| st.page_start && st.current.as_deref() != Some(DEFAULT))
            .filter_map(|st| st.current.clone().map(|c| (st.mode, c)))
            .collect()
    }
}

impl Machine<'_, '_> {
    /// `\pdfcolorstackinit [page] [direct|page] {text}`: a new stack; its number.
    pub(super) fn color_stack_init(&mut self, out: &mut Vec<Token>) {
        let page_start = self.scan_keyword("page");
        let mode = if self.scan_keyword("direct") {
            DIRECT_ALWAYS
        } else if self.scan_keyword("page") {
            DIRECT_PAGE
        } else {
            SET_ORIGIN
        };
        let s = self.general_text_string();
        self.colors.0.push(ColorStack { mode, page_start, current: some(s), saved: Vec::new() });
        str_toks(&(self.colors.0.len() - 1).to_string(), out);
    }
}
