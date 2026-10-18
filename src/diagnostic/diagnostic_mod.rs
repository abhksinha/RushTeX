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
    /// Optional secondary contextual note.
    pub note: Option<String>,
    /// Optional actionable fix suggestion or help text.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Returns the simplified one-line header requested:
    /// `[ERROR] Line <line> from character <character>`
    pub fn simplified_header(&self) -> String {
        format!(
            "[ERROR] Line {} from character {}",
            self.line, self.character
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "==========================================")?;
        writeln!(f, "RUSHTEX - THE ULTRA FAST TEX FILE COMPILER")?;
        writeln!(f, "==========================================")?;
        write!(
            f,
            "[ERROR] Line {} from character {}: {}",
            self.line, self.character, self.message
        )?;
        if let Some(ref snippet) = self.snippet {
            write!(f, "\n{snippet}")?;
        }
        if let Some(ref note) = self.note {
            write!(f, "\n NOTE: {note}")?;
        }
        if let Some(ref help) = self.help {
            write!(f, "\n HELP: {help}")?;
        }
        write!(f, "\n==========END OF COMPILER LOGS============")?;
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
    let span_len = if byte_offset < source_code.len() {
        let rem = &source_code[byte_offset..];
        if let Some(cmd) = rem.strip_prefix('\\') {
            let cmd_len = cmd.chars().take_while(|c| c.is_ascii_alphabetic()).count();
            1 + cmd_len
        } else {
            1
        }
    } else {
        1
    };
    locate_diagnostic_detailed(
        source_code,
        byte_offset,
        span_len,
        message,
        None,
        None,
        None,
    )
}

/// Detailed diagnostic locator with ANSI yellow background highlighting (`\x1b[43;30m`),
/// contextual notes, and actionable help suggestions.
pub fn locate_diagnostic_detailed(
    source_code: &str,
    byte_offset: usize,
    span_len: usize,
    message: impl Into<String>,
    _label: Option<&str>,
    note: Option<&str>,
    help: Option<&str>,
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
    let char_indices: Vec<(usize, char)> = sanitized_line.char_indices().collect();
    let total_chars = char_indices.len();
    let char_offset = character_index.saturating_sub(1);

    let offending_token = if total_chars == 0 || char_offset >= total_chars {
        ""
    } else {
        let start_byte = char_indices[char_offset].0;
        let end_char = (char_offset + span_len.max(1)).min(total_chars);
        let end_byte = if end_char < total_chars {
            char_indices[end_char].0
        } else {
            sanitized_line.len()
        };
        &sanitized_line[start_byte..end_byte]
    };

    let raw_message = message.into();
    let final_message = highlight_token_in_message(&raw_message, offending_token);

    Diagnostic {
        line: line_number,
        character: character_index,
        byte_offset: safe_offset,
        message: final_message,
        snippet: None,
        note: note.map(str::to_string),
        help: help.map(str::to_string),
    }
}

fn highlight_token_in_message(raw_message: &str, offending_token: &str) -> String {
    if offending_token.is_empty() {
        return raw_message.to_string();
    }
    let yellow_token = format!("\x1b[43;30m{offending_token}\x1b[0m");
    if offending_token.starts_with('\\') && raw_message.contains(offending_token) {
        return raw_message.replacen(offending_token, &yellow_token, 1);
    }
    let quoted = format!("'{offending_token}'");
    if raw_message.contains(&quoted) {
        return raw_message.replacen(&quoted, &format!("'{yellow_token}'"), 1);
    }
    let braced = format!("{{{offending_token}}}");
    if raw_message.contains(&braced) {
        return raw_message.replacen(&braced, &format!("{{{yellow_token}}}"), 1);
    }
    if offending_token.starts_with('\\') {
        return format!("{raw_message} {yellow_token}");
    }
    raw_message.to_string()
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
        let formatted = diag.to_string();
        assert!(formatted.starts_with("==========================================\nRUSHTEX - THE ULTRA FAST TEX FILE COMPILER\n=========================================="));
        assert!(formatted.contains("[ERROR] Line 1 from character 8: unknown environment"));
        assert!(formatted.ends_with("==========END OF COMPILER LOGS============"));
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
        assert!(formatted.starts_with("=========================================="));
        assert!(formatted.contains(
            "[ERROR] Line 3 from character 11: unknown command \x1b[43;30m\\badcommand\x1b[0m"
        ));
        assert!(!formatted.contains("3 |"));
        assert!(formatted.ends_with("==========END OF COMPILER LOGS============"));
    }

    #[test]
    fn test_diagnostic_empty_or_eof() {
        let src = "";
        let diag = locate_diagnostic(src, 0, "unexpected end of file");
        assert_eq!(diag.line, 1);
        assert_eq!(diag.character, 1);
        assert_eq!(diag.simplified_header(), "[ERROR] Line 1 from character 1");
    }

    #[test]
    fn test_diagnostic_note_and_help() {
        let src = "\\begin{center}\nhello";
        let diag = locate_diagnostic_detailed(
            src,
            0,
            14,
            "unclosed \\begin{center}",
            Some("never closed"),
            Some("reached EOF without closing \\end{center}"),
            Some("add '\\end{center}' before end of document"),
        );
        let formatted = diag.to_string();
        assert!(formatted.starts_with("=========================================="));
        assert!(formatted.contains(
            "[ERROR] Line 1 from character 1: unclosed \x1b[43;30m\\begin{center}\x1b[0m"
        ));
        assert!(!formatted.contains("1 |"));
        assert!(formatted.contains("\n NOTE: reached EOF without closing \\end{center}"));
        assert!(formatted.contains("\n HELP: add '\\end{center}' before end of document"));
        assert!(formatted.ends_with("==========END OF COMPILER LOGS============"));
    }
}
