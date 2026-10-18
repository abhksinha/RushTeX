//! AST models and registry for document indexing (ToC, LoF, LoT).
//!
//! Provides clean, zero-dependency data structures for tracking document headings,
//! figure captions, table captions, and their resolved 1-based page numbers.

use crate::frontend::ast::Inline;

/// A single entry in the Table of Contents.
#[derive(Debug, Clone)]
pub struct TocEntry<'a> {
    /// Heading hierarchy depth: 1 for `\section`, 2 for `\subsection`, 3 for `\subsubsection`.
    pub level: u8,
    /// Hierarchical numbering components: `[section, subsection, subsubsection, paragraph]`.
    pub number: [u16; 4],
    /// Title inlines.
    pub title: Vec<Inline<'a>>,
    /// Resolved 1-based page number.
    pub page: u32,
}

impl<'a> TocEntry<'a> {
    /// Formats the hierarchical section number (e.g. "1", "1.2", "1.2.3").
    #[inline]
    pub fn format_number(&self) -> String {
        match self.level {
            1 => format!("{}", self.number[0]),
            2 => format!("{}.{}", self.number[0], self.number[1]),
            3 => format!("{}.{}.{}", self.number[0], self.number[1], self.number[2]),
            _ => format!(
                "{}.{}.{}.{}",
                self.number[0], self.number[1], self.number[2], self.number[3]
            ),
        }
    }
}

/// A single entry in the List of Figures.
#[derive(Debug, Clone)]
pub struct FigureEntry<'a> {
    /// 1-based sequential figure number.
    pub number: u32,
    /// Caption inlines.
    pub caption: Vec<Inline<'a>>,
    /// Resolved 1-based page number.
    pub page: u32,
}

/// A single entry in the List of Tables.
#[derive(Debug, Clone)]
pub struct TableEntry<'a> {
    /// 1-based sequential table number.
    pub number: u32,
    /// Caption inlines.
    pub caption: Vec<Inline<'a>>,
    /// Resolved 1-based page number.
    pub page: u32,
}

/// Central catalog registry tracking all document structural entities and their resolved page targets.
#[derive(Debug, Clone)]
pub struct IndexRegistry<'a> {
    /// Collected Table of Contents entries.
    pub toc_entries: Vec<TocEntry<'a>>,
    /// Collected List of Figures entries.
    pub figure_entries: Vec<FigureEntry<'a>>,
    /// Collected List of Tables entries.
    pub table_entries: Vec<TableEntry<'a>>,
    /// Maximum heading depth displayed in Table of Contents (default 3: down to subsubsection).
    pub tocdepth: u8,
}

impl<'a> Default for IndexRegistry<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IndexRegistry<'a> {
    /// Creates a new empty index registry with default `tocdepth = 3`.
    pub fn new() -> Self {
        Self {
            toc_entries: Vec::new(),
            figure_entries: Vec::new(),
            table_entries: Vec::new(),
            tocdepth: 3,
        }
    }

    /// Clears all collected entries while preserving capacity.
    pub fn clear(&mut self) {
        self.toc_entries.clear();
        self.figure_entries.clear();
        self.table_entries.clear();
    }

    /// Returns `true` if no entries of any kind have been registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.toc_entries.is_empty()
            && self.figure_entries.is_empty()
            && self.table_entries.is_empty()
    }
}
