//! pdfTeX's colour stacks (pdftex.web `pdf_out_colorstack`, utils.c `newcolorstack`,
//! `colorstackpush` and kin), in our own form.
//!
//! A stack's current literal survives from page to page; a stack made with `page` repeats
//! its current literal at the top of every page unless it is the default `0 g 0 G`. A form
//! (`\pdfxform`) is shipped with each stack's form side (utils.c `form_current`, begun at
//! the stack's initial literal and never reset), swapped in by [`ColorStacks::swap_form`].

use super::engine_run::Machine;
use super::engine_value::str_toks;
use super::engine_token::Token;

/// pdfTeX's literal modes.
pub const SET_ORIGIN: u8 = 0;
pub const DIRECT_PAGE: u8 = 1;
pub const DIRECT_ALWAYS: u8 = 2;

/// utils.c `COLOR_DEFAULT`.
const DEFAULT: &str = "0 g 0 G";

#[derive(PartialEq, Clone)]
pub struct ColorStack {
    pub mode: u8,
    page_start: bool,
    current: Option<String>,
    saved: Vec<Option<String>>,
    form_current: Option<String>,
    form_saved: Vec<Option<String>>,
}

impl ColorStack {
    fn new(mode: u8, page_start: bool, init: Option<String>) -> Self {
        ColorStack { mode, page_start, current: init.clone(), saved: Vec::new(), form_current: init, form_saved: Vec::new() }
    }
}

#[derive(PartialEq, Clone)]
pub struct ColorStacks(pub Vec<ColorStack>);

impl Default for ColorStacks {
    /// `colstacks_first_init`: stack 0 is the default colour, direct, repeated per page.
    fn default() -> Self {
        ColorStacks(vec![ColorStack::new(DIRECT_ALWAYS, true, Some(DEFAULT.into()))])
    }
}

fn some(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

impl ColorStacks {
    /// Every stack's whole state, into `h` (a session's shipped-page key, `engine_ship`).
    pub fn digest(&self, h: &mut crate::backend::backend_hash::ContentHasher) {
        let opt = |h: &mut crate::backend::backend_hash::ContentHasher, s: &Option<String>| match s {
            Some(s) => {
                h.u64(1);
                h.bytes(s.as_bytes());
            }
            None => h.u64(0),
        };
        h.u64(self.0.len() as u64);
        for st in &self.0 {
            h.u64(((st.mode as u64) << 1) | st.page_start as u64);
            opt(h, &st.current);
            opt(h, &st.form_current);
            h.u64(st.saved.len() as u64);
            st.saved.iter().for_each(|s| opt(h, s));
            h.u64(st.form_saved.len() as u64);
            st.form_saved.iter().for_each(|s| opt(h, s));
        }
    }

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

    /// utils.c `page_mode`: the form side of every stack becomes current (and back again),
    /// as a form begins (and ends) shipping.
    pub fn swap_form(&mut self) {
        for st in &mut self.0 {
            std::mem::swap(&mut st.current, &mut st.form_current);
            std::mem::swap(&mut st.saved, &mut st.form_saved);
        }
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
        self.colors.0.push(ColorStack::new(mode, page_start, some(s)));
        str_toks(&(self.colors.0.len() - 1).to_string(), out);
    }
}
