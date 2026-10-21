//! # Code Listing & Verbatim Typesetting Module (`listing`)
//!
//! Provides data structures, parsing, formatting, and pagination for code listings,
//! verbatim blocks, and inline monospace snippets in `rushtex`.
//!
//! ## Architectural Design
//!
//! - **Zero-Allocation Slicing**: Code blocks borrow directly from the input document slice (`&'a str`),
//!   preserving raw whitespace, tabs, and indentation without heap reallocations.
//! - **TeX semantics**: `verbatim` is LaTeX's `\@verbatim` (a trivlist of one line box per
//!   source line in the typewriter face), and `lstlisting` follows the `listings` package
//!   defaults (the document font in fixed columns of 0.6em, `\medskipamount` above and below).
//! - **Configurable Syntax Options**:
//!   - `\begin{verbatim} ... \end{verbatim}`: Standard raw literal block.
//!   - `\begin{lstlisting}[language=...] ... \end{lstlisting}`: Structured code listings.
//!   - `\texttt{...}`: Inline monospace text.
//!   - `\verb|...|`: Delimited inline verbatim text.
//! - **Pagination & Layout**: every line is a box for the page builder, so listings break
//!   across pages where TeX would break them.
//!
//! ## Public APIs
//!
//! - [`ListingOptions`]: Configuration settings (line numbers, frame, font size, tab spaces).
//! - [`CodeListingData`]: Structured representation of a verbatim or listing block.
//! - [`parse_listing_options`]: Extracts key-value options from `[language=Rust, numbers=left]`.
//! - [`expand_tabs_into`]: Expands `\t` characters into uniform spaces for column alignment.

/// Configuration parameters controlling code listing layout and rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ListingOptions<'a> {
    /// Optional programming language name (e.g. `"Rust"`, `"Python"`).
    pub language: Option<&'a str>,
    /// Font size in points; `0.0` (the default) is the document's size.
    pub font_size: f32,
    /// Number of columns per tab stop. Defaults to `8`, as in `listings`.
    pub tab_spaces: u8,
    /// Whether to print line numbers left of the text (`numbers=left`).
    pub show_line_numbers: bool,
    /// Whether to draw a frame around the listing (`frame=single`). Defaults to `false`.
    pub frame: bool,
    /// A LaTeX `verbatim` environment rather than an `lstlisting`.
    pub verbatim: bool,
    /// `basicstyle` selects the typewriter face; `listings` uses the document font otherwise.
    pub mono: bool,
}

impl<'a> Default for ListingOptions<'a> {
    fn default() -> Self {
        Self {
            language: None,
            font_size: 0.0,
            tab_spaces: 8,
            show_line_numbers: false,
            frame: false,
            verbatim: false,
            mono: false,
        }
    }
}

/// Structured representation of a code listing or verbatim block in the AST.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeListingData<'a> {
    /// Borrowed raw text of the code listing.
    pub raw_code: &'a str,
    /// Display and formatting options.
    pub options: ListingOptions<'a>,
    /// The environment starts in vertical mode (after a blank line), so a trivlist adds
    /// `\partopsep` to `\topsep`.
    pub in_vmode: bool,
}

impl<'a> CodeListingData<'a> {
    /// Constructs a LaTeX `verbatim` block.
    pub fn new(raw_code: &'a str) -> Self {
        Self {
            raw_code,
            options: ListingOptions { verbatim: true, mono: true, ..ListingOptions::default() },
            in_vmode: false,
        }
    }

    /// Constructs a code listing block with explicit options.
    pub fn with_options(raw_code: &'a str, options: ListingOptions<'a>) -> Self {
        Self { raw_code, options, in_vmode: false }
    }

    /// Returns an iterator over the lines of the code listing.
    pub fn lines(&self) -> impl Iterator<Item = &'a str> {
        self.raw_code.lines()
    }
}

/// Parses comma-separated key-value options from a LaTeX bracketed option list `[...]`.
///
/// Understood options:
/// - `language=<name>`
/// - `numbers=left` or `numbers=right` (enables line numbering)
/// - `numbers=none` (disables line numbering)
/// - `frame=single` / `frame=none`
/// - `tabsize=<int>`
/// - `fontsize=<int|float>`
pub fn parse_listing_options<'a>(raw_options: &'a str) -> ListingOptions<'a> {
    let mut options = ListingOptions::default();
    let trimmed = raw_options
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');

    for entry in trimmed.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        if let Some((key, value)) = entry.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "language" | "lang" => {
                    options.language = Some(value);
                }
                "numbers" => {
                    options.show_line_numbers = value == "left" || value == "yes";
                }
                "frame" => {
                    options.frame = value != "none";
                }
                "tabsize" => {
                    if let Ok(ts) = value.parse::<usize>() {
                        options.tab_spaces = ts.clamp(1, 32) as u8;
                    }
                }
                "fontsize" => {
                    if let Ok(fs) = value.parse::<f32>() {
                        options.font_size = fs.max(4.0);
                    }
                }
                "basicstyle" => {
                    options.mono = value.contains("\\ttfamily");
                    // The size commands at 10pt (LaTeX's `size10.clo`).
                    for (cmd, pt) in [("\\tiny", 5.0), ("\\scriptsize", 7.0), ("\\footnotesize", 8.0), ("\\small", 9.0)] {
                        if value.contains(cmd) {
                            options.font_size = pt;
                        }
                    }
                }
                _ => {}
            }
        } else {
            // Bare flags
            match entry {
                "numbers" => options.show_line_numbers = true,
                "frame" => options.frame = true,
                "noframe" => options.frame = false,
                _ => {}
            }
        }
    }

    options
}

/// Expands tab characters (`\t`) in `source_line` into aligned spaces according to `tab_size`.
/// Writes the resulting characters into `destination_buffer`.
pub fn expand_tabs_into(source_line: &str, tab_size: usize, destination_buffer: &mut String) {
    let tab_stop = tab_size.max(1);
    let mut current_column = 0;

    for character in source_line.chars() {
        if character == '\t' {
            let spaces_needed = tab_stop - (current_column % tab_stop);
            for _ in 0..spaces_needed {
                destination_buffer.push(' ');
            }
            current_column += spaces_needed;
        } else {
            destination_buffer.push(character);
            current_column += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_listing_options() {
        assert!(std::mem::size_of::<ListingOptions>() <= 32);
        let opts = parse_listing_options("[language=Rust, numbers=left, tabsize=2]");
        assert_eq!(opts.language, Some("Rust"));
        assert!(opts.show_line_numbers);
        assert_eq!(opts.tab_spaces, 2);
    }

    #[test]
    fn test_expand_tabs_into() {
        let mut buf = String::new();
        expand_tabs_into("a\tb", 4, &mut buf);
        assert_eq!(buf, "a   b");

        buf.clear();
        expand_tabs_into("\tdef", 4, &mut buf);
        assert_eq!(buf, "    def");
    }
}
