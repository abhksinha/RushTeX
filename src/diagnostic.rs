//! # Compiler Diagnostic and Human-Readable Error Reporting (`diagnostic`)
//!
//! Provides a dedicated, high-performance error reporter for LaTeX compilation.
//!
//! ## Architectural Principles
//!
//! - **Zero Fast-Path Overhead**: During normal compilation, the parser assumes there are
//!   no errors. It maintains only a raw byte pointer (`byte_offset: usize`) without line
//!   counting, column tracking, or intermediate allocations.
//! - **Lazy Error Finding Mode**: Only when compilation fails does the diagnostic engine enter
//!   error-finding mode. It scans the source text up to the failure point in a single $O(K)$ pass
//!   to determine the exact 1-based line number and character column.
//! - **Human-Readable Format**: Emits errors in the clear, human-friendly format:
//!   `[ERROR] Line 54 from character 17: <message>`
//!   followed by an aligned visual snippet with a caret (`^`) pointing to the offending character.
//! - **Zero Dependencies**: Implemented strictly with the Rust standard library.
//!
//! ## Public APIs
//!
//! - [`Diagnostic`]: Complete compiler diagnostic holding line, character, message, and visual snippet.
//! - [`locate_diagnostic`]: Enters error finding mode to compute line and character coordinates from a byte offset.

use std::fmt;

/// Human-readable compiler diagnostic report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number.
    pub line: usize,
    /// 1-based character column.
    pub character: usize,
    /// Byte offset in the original source code where the error occurred.
    pub byte_offset: usize,
    /// Diagnostic description.
    pub message: String,
    /// Optional visual source code line with a caret (`^`) indicator.
    pub snippet: Option<String>,
}

impl Diagnostic {
    /// Returns the simplified one-line header requested:
    /// `[ERROR] Line <line> from character <character>`
    pub fn simplified_header(&self) -> String {
        format!("[ERROR] Line {} from character {}", self.line, self.character)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[ERROR] Line {} from character {}: {}",
            self.line, self.character, self.message
        )?;
        if let Some(ref snippet) = self.snippet {
            write!(f, "\n{snippet}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Enters "error finding mode" to locate the exact 1-based line and character column
/// for a failure occurring at `byte_offset`.
///
/// **Time Complexity**: $O(K)$ where $K$ is `byte_offset`, scanning only up to the error.
/// **Space Complexity**: $O(1)$ auxiliary memory (zero heap allocations during coordinate calculation).
pub fn locate_diagnostic(
    source_code: &str,
    byte_offset: usize,
    message: impl Into<String>,
) -> Diagnostic {
    let safe_offset = byte_offset.min(source_code.len());
    let prefix = &source_code[..safe_offset];

    // 1. Calculate 1-based line number by counting newlines up to byte_offset
    let line_number = prefix.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1;

    // 2. Find start byte of current line
    let line_start_byte = prefix.rfind('\n').map(|idx| idx + 1).unwrap_or(0);

    // 3. Calculate 1-based character column (counting UTF-8 characters, not just bytes)
    let character_index = prefix[line_start_byte..].chars().count() + 1;

    // 4. Extract current line for visual snippet
    let remaining_after_start = &source_code[line_start_byte..];
    let line_end_byte = remaining_after_start
        .find('\n')
        .map(|idx| line_start_byte + idx)
        .unwrap_or(source_code.len());
    let raw_line = &source_code[line_start_byte..line_end_byte];
    let sanitized_line = raw_line.trim_end_matches(['\r', '\n']);

    // Build aligned visual snippet
    let line_prefix = format!(" {:>4} | ", line_number);
    let mut snippet = String::with_capacity(line_prefix.len() + sanitized_line.len() + 32);
    snippet.push_str(&line_prefix);
    snippet.push_str(sanitized_line);
    snippet.push('\n');

    // Pointer line: spaces matching indent + spaces matching character offset before error
    let indent_spaces = " ".repeat(line_prefix.len());
    snippet.push_str(&indent_spaces);
    let char_offset = character_index.saturating_sub(1);
    snippet.push_str(&" ".repeat(char_offset));
    snippet.push('^');

    Diagnostic {
        line: line_number,
        character: character_index,
        byte_offset: safe_offset,
        message: message.into(),
        snippet: Some(snippet),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_first_line() {
        let src = "\\begin{invalid}";
        let diag = locate_diagnostic(src, 7, "unknown environment");
        assert_eq!(diag.line, 1);
        assert_eq!(diag.character, 8);
        assert_eq!(diag.simplified_header(), "[ERROR] Line 1 from character 8");
        assert!(diag.to_string().starts_with("[ERROR] Line 1 from character 8: unknown environment"));
    }

    #[test]
    fn test_diagnostic_multiline() {
        let src = "\\documentclass{article}\n\\begin{document}\nThis is a \\badcommand here\n\\end{document}";
        // Line 3: "This is a \badcommand here"
        // Offset of '\badcommand' is 24 + 17 + 10 = 51
        let offset = src.find("\\badcommand").unwrap();
        let diag = locate_diagnostic(src, offset, "unknown command \\badcommand");
        assert_eq!(diag.line, 3);
        assert_eq!(diag.character, 11); // "This is a " is 10 chars -> 11th char
        assert_eq!(diag.simplified_header(), "[ERROR] Line 3 from character 11");
        let formatted = diag.to_string();
        assert!(formatted.contains("[ERROR] Line 3 from character 11: unknown command \\badcommand"));
        assert!(formatted.contains("    3 | This is a \\badcommand here"));
        assert!(formatted.contains("^"));
    }

    #[test]
    fn test_diagnostic_empty_or_eof() {
        let src = "";
        let diag = locate_diagnostic(src, 0, "unexpected end of file");
        assert_eq!(diag.line, 1);
        assert_eq!(diag.character, 1);
        assert_eq!(diag.simplified_header(), "[ERROR] Line 1 from character 1");
    }
}
