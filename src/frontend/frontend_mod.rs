//! # TeX Syntax Lexer & Recursive-Descent Parser (`frontend`)
//!
//! Provides a zero-copy lexical scanner, AST data structures, and
//! recursive-descent parser for LaTeX document syntax.

pub mod frontend_ast;
pub mod frontend_parser;

// Backward-compatible module aliases:
pub use frontend_ast as ast;
pub use frontend_parser as parser;

pub use frontend_ast::*;
pub use frontend_parser::parse;
