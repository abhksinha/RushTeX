//! Zero-copy LaTeX mathematical expression parser.
//!
//! Parses math tokens, superscripts, subscripts, fractions, big operators,
//! radicals, accents, matrices, and extensible delimiters.

use crate::backend::fonts::Face;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, MathNode, MathStyle, MatrixData, MatrixDelim,
};
use crate::math::math_fonts::lookup_math_command;
use crate::math::math_spacing::MathAtomKind;

pub struct MathParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> MathParser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    #[inline(always)]
    pub fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    #[inline(always)]
    fn remaining(&self) -> &'a str {
        if self.pos < self.src.len() {
            &self.src[self.pos..]
        } else {
            ""
        }
    }

    #[inline(always)]
    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    #[inline(always)]
    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    #[inline(always)]
    fn eat(&mut self, s: &str) -> bool {
        if self.remaining().starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Parses a complete formula until EOF or stop token (e.g. '$' or '\]').
    pub fn parse_expression(&mut self, stop_at_delim: Option<&str>) -> MathNode<'a> {
        self.parse_expression_inner(stop_at_delim, None)
    }

    #[inline(always)]
    fn is_at_end_env(&self, env_name: &str) -> bool {
        let rem = self.remaining();
        if let Some(rest) = rem.strip_prefix("\\end{") {
            if let Some(rest2) = rest.strip_prefix(env_name) {
                return rest2.starts_with('}');
            }
        }
        false
    }

    #[inline(always)]
    fn eat_end_env(&mut self, env_name: &str) -> bool {
        if self.is_at_end_env(env_name) {
            self.pos += "\\end{".len() + env_name.len() + 1;
            true
        } else {
            false
        }
    }

    fn parse_expression_inner(
        &mut self,
        stop_at_delim: Option<&str>,
        stop_at_cell: Option<&str>,
    ) -> MathNode<'a> {
        let mut first: Option<MathNode<'a>> = None;
        let mut rest: Option<Vec<MathNode<'a>>> = None;

        loop {
            self.skip_ws();
            if self.is_eof() {
                break;
            }

            if let Some(stop) = stop_at_delim {
                if self.remaining().starts_with(stop) {
                    break;
                }
            }

            if let Some(env_name) = stop_at_cell {
                if self.remaining().starts_with("&")
                    || self.remaining().starts_with("\\\\")
                    || self.is_at_end_env(env_name)
                {
                    break;
                }
            }

            if self.remaining().starts_with("\\right") {
                break;
            }

            if let Some(node) = self.parse_primary(stop_at_delim, stop_at_cell) {
                // Check if followed by subscript '_' or superscript '^'
                let attached = self.attach_scripts(node);
                if let Some(ref mut nodes) = rest {
                    nodes.push(attached);
                } else if let Some(first_node) = first.take() {
                    let mut nodes = Vec::with_capacity(4);
                    nodes.push(first_node);
                    nodes.push(attached);
                    rest = Some(nodes);
                } else {
                    first = Some(attached);
                }
            } else {
                // If nothing was parsed, break to avoid infinite loop
                break;
            }
        }

        if let Some(nodes) = rest {
            MathNode::Row(nodes)
        } else if let Some(node) = first {
            node
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// Attaches subscripts and superscripts to a nucleus node.
    fn attach_scripts(&mut self, mut nucleus: MathNode<'a>) -> MathNode<'a> {
        if let MathNode::BigOp(ref mut data) = nucleus {
            self.skip_ws();
            if self.eat("\\limits") {
                data.limits = true;
            } else if self.eat("\\nolimits") {
                data.limits = false;
            } else if self.eat("\\displaylimits") {
                data.limits = true;
            }
        }

        let mut sub: Option<Box<MathNode<'a>>> = None;
        let mut sup: Option<Box<MathNode<'a>>> = None;

        loop {
            self.skip_ws();
            if self.eat("\\limits") {
                if let MathNode::BigOp(ref mut data) = nucleus {
                    data.limits = true;
                }
                continue;
            } else if self.eat("\\nolimits") {
                if let MathNode::BigOp(ref mut data) = nucleus {
                    data.limits = false;
                }
                continue;
            } else if self.eat("\\displaylimits") {
                if let MathNode::BigOp(ref mut data) = nucleus {
                    data.limits = true;
                }
                continue;
            }
            if self.eat("_") {
                self.skip_ws();
                let sub_node = self.parse_script_argument();
                sub = Some(Box::new(sub_node));
            } else if self.eat("^") {
                self.skip_ws();
                let sup_node = self.parse_script_argument();
                sup = Some(Box::new(sup_node));
            } else if self.eat("'") {
                // Prime operator
                let prime_sym = MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Symbol,
                    glyph: "'",
                    code: Some(b'\''),
                };
                sup = Some(Box::new(prime_sym));
            } else {
                break;
            }
        }

        if sub.is_some() || sup.is_some() {
            // Check if nucleus is BigOp with limits
            if let MathNode::BigOp(mut data) = nucleus {
                data.sub = sub;
                data.sup = sup;
                return MathNode::BigOp(data);
            }

            MathNode::SubSup {
                nucleus: Box::new(nucleus),
                sub,
                sup,
            }
        } else {
            nucleus
        }
    }

    /// Parses an argument for a script (`_` or `^`), which can be a single character or braced group `{...}`.
    fn parse_script_argument(&mut self) -> MathNode<'a> {
        self.skip_ws();
        if self.eat("{") {
            let inner = self.parse_expression(Some("}"));
            self.eat("}");
            inner
        } else if let Some(primary) = self.parse_single_atom() {
            primary
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// Parses a single atomic element without checking for trailing scripts.
    fn parse_single_atom(&mut self) -> Option<MathNode<'a>> {
        self.skip_ws();
        let rem = self.remaining();
        if rem.is_empty() {
            return None;
        }

        // 1. Group `{ ... }`
        if self.eat("{") {
            let inner = self.parse_expression(Some("}"));
            self.eat("}");
            return Some(inner);
        }

        // 2. Command `\cmd`
        if rem.starts_with('\\') {
            return self.parse_command(None, None);
        }

        // 3. Number (digits)
        let ch = rem.chars().next()?;
        if ch.is_ascii_digit() {
            let start = self.pos;
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() || c == '.' {
                    self.pos += c.len_utf8();
                } else {
                    break;
                }
            }
            return Some(MathNode::Number(&self.src[start..self.pos]));
        }

        // 4. Letter Variable
        if ch.is_ascii_alphabetic() {
            self.pos += ch.len_utf8();
            return Some(MathNode::Variable(ch));
        }

        // 5. Operators & Relational Glyphs
        self.pos += ch.len_utf8();
        let (kind, face, glyph, code) = match ch {
            '-' => (MathAtomKind::Bin, Face::Symbol, "-", Some(0x2D)),
            '+' => (MathAtomKind::Bin, Face::Roman, "+", None),
            '*' => (MathAtomKind::Bin, Face::Roman, "*", None),
            '=' | '<' | '>' => (
                MathAtomKind::Rel,
                Face::Roman,
                &self.src[self.pos - 1..self.pos],
                None,
            ),
            '(' | '[' => (
                MathAtomKind::Open,
                Face::Roman,
                &self.src[self.pos - 1..self.pos],
                None,
            ),
            ')' | ']' => (
                MathAtomKind::Close,
                Face::Roman,
                &self.src[self.pos - 1..self.pos],
                None,
            ),
            ',' | ';' => (
                MathAtomKind::Punct,
                Face::Roman,
                &self.src[self.pos - 1..self.pos],
                None,
            ),
            '/' | '!' | '|' => (
                MathAtomKind::Ord,
                Face::Roman,
                &self.src[self.pos - 1..self.pos],
                None,
            ),
            _ => (
                MathAtomKind::Ord,
                Face::Roman,
                &self.src[self.pos - ch.len_utf8()..self.pos],
                None,
            ),
        };

        Some(MathNode::Symbol {
            kind,
            face,
            glyph,
            code,
        })
    }

    /// Parses a primary element (numbers, variables, commands, brackets, fractions).
    fn parse_primary(
        &mut self,
        stop_at_delim: Option<&str>,
        stop_at_cell: Option<&str>,
    ) -> Option<MathNode<'a>> {
        self.skip_ws();
        let rem = self.remaining();
        if rem.is_empty() {
            return None;
        }

        // 1. Group `{ ... }`
        if self.eat("{") {
            let inner = self.parse_expression(Some("}"));
            self.eat("}");
            return Some(inner);
        }

        // 2. Command `\cmd`
        if rem.starts_with('\\') {
            return self.parse_command(stop_at_delim, stop_at_cell);
        }

        // 3. Fallback to single atom
        self.parse_single_atom()
    }

    /// Parses a LaTeX math command starting with `\`.
    fn parse_command(
        &mut self,
        stop_at_delim: Option<&str>,
        stop_at_cell: Option<&str>,
    ) -> Option<MathNode<'a>> {
        if !self.eat("\\") {
            return None;
        }

        // Check for single-character punctuation escapes: `\,`, `\:`, `\;`, `\!`
        if let Some(ch) = self.peek_char() {
            if ch == ',' {
                self.pos += 1;
                return Some(MathNode::Space(3.0 / 18.0 * 10.0)); // 3 mu thin space
            } else if ch == ':' {
                self.pos += 1;
                return Some(MathNode::Space(4.0 / 18.0 * 10.0)); // 4 mu medium space
            } else if ch == ';' {
                self.pos += 1;
                return Some(MathNode::Space(5.0 / 18.0 * 10.0)); // 5 mu thick space
            } else if ch == '!' {
                self.pos += 1;
                return Some(MathNode::Space(-3.0 / 18.0 * 10.0)); // negative thin space
            } else if ch == '{' || ch == '}' || ch == '%' || ch == '$' || ch == '&' || ch == '_' {
                self.pos += 1;
                let glyph = &self.src[self.pos - 1..self.pos];
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Roman,
                    glyph,
                    code: None,
                });
            }
        }

        // Extract alphabetical command name
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphabetic() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        let cmd = &self.src[start..self.pos];

        match cmd {
            // Explicit mathematical style scopes
            "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                let target_style = match cmd {
                    "displaystyle" => MathStyle::Display,
                    "textstyle" => MathStyle::Text,
                    "scriptstyle" => MathStyle::Script,
                    "scriptscriptstyle" => MathStyle::ScriptScript,
                    _ => unreachable!(),
                };
                let rest = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                Some(MathNode::StyleScope {
                    style: target_style,
                    content: Box::new(rest),
                })
            }

            // Fraction `\frac{num}{den}`, `\dfrac`, `\tfrac`
            "frac" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Fraction {
                    num,
                    den,
                    has_rule: true,
                })
            }
            "dfrac" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleScope {
                    style: MathStyle::Display,
                    content: Box::new(MathNode::Fraction {
                        num,
                        den,
                        has_rule: true,
                    }),
                })
            }
            "tfrac" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleScope {
                    style: MathStyle::Text,
                    content: Box::new(MathNode::Fraction {
                        num,
                        den,
                        has_rule: true,
                    }),
                })
            }

            // Generalized binomial coefficients `\binom{n}{k}`, `\dbinom`, `\tbinom`
            "binom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                };
                Some(MathNode::Delimited(Box::new(DelimitedData {
                    open: "(",
                    close: ")",
                    content: frac,
                })))
            }
            "dbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Display,
                    content: Box::new(frac),
                };
                Some(MathNode::Delimited(Box::new(DelimitedData {
                    open: "(",
                    close: ")",
                    content: scope,
                })))
            }
            "tbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Text,
                    content: Box::new(frac),
                };
                Some(MathNode::Delimited(Box::new(DelimitedData {
                    open: "(",
                    close: ")",
                    content: scope,
                })))
            }

            // Continued fraction `\cfrac{num}{den}`
            "cfrac" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: true,
                };
                Some(MathNode::StyleScope {
                    style: MathStyle::Display,
                    content: Box::new(frac),
                })
            }

            // TeX Appendix G Rule 12: `\vcenter{...}`
            "vcenter" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Vcenter(content))
            }

            // Generalized fraction / binomial primitive `\genfrac{ld}{rd}{thick}{style}{num}{den}`
            "genfrac" => {
                let open_arg = self.parse_ident_arg();
                let close_arg = self.parse_ident_arg();
                let thick_arg = self.parse_ident_arg();
                let style_arg = self.parse_ident_arg();
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());

                let has_rule = thick_arg != "0pt";
                let mut core = MathNode::Fraction { num, den, has_rule };

                if !style_arg.is_empty() {
                    let style = match style_arg {
                        "0" => MathStyle::Display,
                        "1" => MathStyle::Text,
                        "2" => MathStyle::Script,
                        "3" => MathStyle::ScriptScript,
                        _ => MathStyle::Display,
                    };
                    core = MathNode::StyleScope {
                        style,
                        content: Box::new(core),
                    };
                }

                if !open_arg.is_empty() || !close_arg.is_empty() {
                    Some(MathNode::Delimited(Box::new(DelimitedData {
                        open: open_arg,
                        close: close_arg,
                        content: core,
                    })))
                } else {
                    Some(core)
                }
            }

            // Radical `\sqrt[deg]{rad}`
            "sqrt" => {
                self.skip_ws();
                let degree = if self.eat("[") {
                    let deg = self.parse_expression(Some("]"));
                    self.eat("]");
                    Some(Box::new(deg))
                } else {
                    None
                };
                let radicand = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Radical { degree, radicand })
            }

            // Dirac reduced Planck constant
            "hbar" => Some(MathNode::HBar),

            // Big operators with limits
            "sum" | "prod" | "coprod" | "bigcap" | "bigcup" | "bigvee" | "bigwedge"
            | "bigoplus" | "bigotimes" | "bigodot" | "biguplus" | "bigsqcup" | "lim" | "max"
            | "min" | "sup" | "inf" | "gcd" | "det" | "liminf" | "limsup" | "Pr" => {
                Some(MathNode::BigOp(Box::new(BigOpData {
                    op: cmd,
                    sub: None,
                    sup: None,
                    limits: true,
                })))
            }

            // Integrals (default limits to the right)
            "int" | "iint" | "iiint" | "oint" => Some(MathNode::BigOp(Box::new(BigOpData {
                op: cmd,
                sub: None,
                sup: None,
                limits: false,
            }))),

            // Dynamic delimiters `\left( ... \right)`
            "left" => {
                self.skip_ws();
                let open = self.parse_delim_char();
                let content = self.parse_expression(None);
                let close = if self.eat("\\right") {
                    self.skip_ws();
                    self.parse_delim_char()
                } else {
                    ")"
                };
                Some(MathNode::Delimited(Box::new(DelimitedData {
                    open,
                    close,
                    content,
                })))
            }

            // Spacing
            "quad" => Some(MathNode::Space(10.0)),
            "qquad" => Some(MathNode::Space(20.0)),
            "enspace" => Some(MathNode::Space(5.0)),
            "thinspace" => Some(MathNode::Space(3.0 / 18.0 * 10.0)),
            "medspace" => Some(MathNode::Space(4.0 / 18.0 * 10.0)),
            "thickspace" => Some(MathNode::Space(5.0 / 18.0 * 10.0)),
            "negthinspace" => Some(MathNode::Space(-3.0 / 18.0 * 10.0)),
            "negmedspace" => Some(MathNode::Space(-4.0 / 18.0 * 10.0)),
            "negthickspace" => Some(MathNode::Space(-5.0 / 18.0 * 10.0)),

            // Modular arithmetic
            "bmod" => {
                let mod_sym = MathNode::StyleChange {
                    face: Face::Roman,
                    content: Box::new(MathNode::Row(vec![
                        MathNode::Variable('m'),
                        MathNode::Variable('o'),
                        MathNode::Variable('d'),
                    ])),
                };
                Some(MathNode::Row(vec![
                    MathNode::Space(4.0 / 18.0 * 10.0),
                    mod_sym,
                    MathNode::Space(4.0 / 18.0 * 10.0),
                ]))
            }
            "pmod" => {
                let arg = self.parse_mandatory_arg();
                let mod_text = MathNode::StyleChange {
                    face: Face::Roman,
                    content: Box::new(MathNode::Row(vec![
                        MathNode::Variable('m'),
                        MathNode::Variable('o'),
                        MathNode::Variable('d'),
                    ])),
                };
                let inner = MathNode::Row(vec![mod_text, MathNode::Space(4.0 / 18.0 * 10.0), arg]);
                let parens = MathNode::Delimited(Box::new(DelimitedData {
                    open: "(",
                    close: ")",
                    content: inner,
                }));
                Some(MathNode::Row(vec![MathNode::Space(5.0), parens]))
            }

            // Accents
            "vec" | "hat" | "dot" | "ddot" | "dddot" | "ddddot" | "bar" | "tilde" | "check"
            | "breve" | "acute" | "grave" | "mathring" | "overline" | "underline" | "widehat"
            | "widetilde" | "overrightarrow" | "overleftarrow" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent {
                    accent: cmd,
                    nucleus,
                })
            }

            // Font switches
            "mathrm" | "text" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Roman,
                    content,
                })
            }
            "mathbf" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Bold,
                    content,
                })
            }
            "mathit" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Italic,
                    content,
                })
            }
            "mathtt" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Mono,
                    content,
                })
            }
            "mathsf" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Roman,
                    content,
                })
            }
            "mathcal" | "mathscr" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Italic,
                    content,
                })
            }
            "mathbb" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Bold,
                    content,
                })
            }

            // Environments: `\begin{matrix}`, `\begin{align*}`, etc.
            "begin" => {
                let env = self.parse_ident_arg();
                let (delim, col_alignments) = match env {
                    "pmatrix" => (MatrixDelim::Parentheses, Vec::new()),
                    "bmatrix" => (MatrixDelim::Brackets, Vec::new()),
                    "Bmatrix" => (MatrixDelim::Braces, Vec::new()),
                    "vmatrix" => (MatrixDelim::VBar, Vec::new()),
                    "Vmatrix" => (MatrixDelim::DoubleVBar, Vec::new()),
                    "cases" => (MatrixDelim::LeftBrace, vec![ColAlign::Left, ColAlign::Left]),
                    "aligned" | "align" | "align*" | "alignat" | "alignat*" | "split" => (
                        MatrixDelim::None,
                        vec![
                            ColAlign::Right,
                            ColAlign::Left,
                            ColAlign::Right,
                            ColAlign::Left,
                            ColAlign::Right,
                            ColAlign::Left,
                            ColAlign::Right,
                            ColAlign::Left,
                        ],
                    ),
                    "gather" | "gather*" | "gathered" => {
                        (MatrixDelim::None, vec![ColAlign::Center; 8])
                    }
                    "multline" | "multline*" => (
                        MatrixDelim::None,
                        vec![ColAlign::Left, ColAlign::Center, ColAlign::Right],
                    ),
                    "array" => {
                        let align_str = self.parse_ident_arg();
                        let aligns: Vec<ColAlign> = align_str
                            .chars()
                            .filter_map(|c| match c {
                                'l' | 'L' => Some(ColAlign::Left),
                                'r' | 'R' => Some(ColAlign::Right),
                                'c' | 'C' => Some(ColAlign::Center),
                                _ => None,
                            })
                            .collect();
                        (MatrixDelim::None, aligns)
                    }
                    _ => (MatrixDelim::None, Vec::new()),
                };
                let matrix_node = self.parse_matrix_body(env, delim, col_alignments);
                Some(matrix_node)
            }

            // Greek letters and STEM symbols
            _ => {
                if let Some(sym) = lookup_math_command(cmd) {
                    Some(MathNode::Symbol {
                        kind: sym.kind,
                        face: sym.face,
                        glyph: sym.glyph,
                        code: sym.code,
                    })
                } else {
                    // Unknown command: fallback to Roman text
                    Some(MathNode::Symbol {
                        kind: MathAtomKind::Ord,
                        face: Face::Roman,
                        glyph: cmd,
                        code: None,
                    })
                }
            }
        }
    }

    /// Parses a delimiter character after `\left` or `\right` (e.g. `(`, `)`, `[`, `]`, `\{`, `.`, `\langle`, `\vert`).
    fn parse_delim_char(&mut self) -> &'a str {
        self.skip_ws();
        if self.eat("\\{") {
            "{"
        } else if self.eat("\\}") {
            "}"
        } else if self.eat("\\|") {
            "||"
        } else if self.eat("\\langle") {
            "\\langle"
        } else if self.eat("\\rangle") {
            "\\rangle"
        } else if self.eat("\\lfloor") {
            "\\lfloor"
        } else if self.eat("\\rfloor") {
            "\\rfloor"
        } else if self.eat("\\lceil") {
            "\\lceil"
        } else if self.eat("\\rceil") {
            "\\rceil"
        } else if self.eat("\\vert") {
            "|"
        } else if self.eat("\\Vert") {
            "||"
        } else if self.eat("\\backslash") {
            "\\"
        } else if let Some(ch) = self.peek_char() {
            let start = self.pos;
            self.pos += ch.len_utf8();
            &self.src[start..self.pos]
        } else {
            "("
        }
    }

    /// Parses a mandatory `{ ... }` argument.
    fn parse_mandatory_arg(&mut self) -> MathNode<'a> {
        self.skip_ws();
        if self.eat("{") {
            let node = self.parse_expression(Some("}"));
            self.eat("}");
            node
        } else if let Some(atom) = self.parse_single_atom() {
            atom
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// Parses `{ident}` string slice.
    fn parse_ident_arg(&mut self) -> &'a str {
        self.skip_ws();
        if !self.eat("{") {
            return "";
        }
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '}' {
                break;
            }
            self.pos += ch.len_utf8();
        }
        let res = &self.src[start..self.pos];
        self.eat("}");
        res.trim()
    }

    /// Parses a matrix environment body with rows separated by `\\` and columns by `&`.
    fn parse_matrix_body(
        &mut self,
        env_name: &str,
        delim: MatrixDelim,
        col_alignments: Vec<ColAlign>,
    ) -> MathNode<'a> {
        let mut rows = Vec::with_capacity(4);

        while !self.is_eof() {
            self.skip_ws();
            if self.is_at_end_env(env_name) {
                self.eat_end_env(env_name);
                break;
            }

            if self.eat("\\hline") {
                continue;
            }

            let mut row = Vec::with_capacity(4);
            loop {
                self.skip_ws();
                if self.is_eof() || self.is_at_end_env(env_name) {
                    break;
                }
                if self.eat("\\hline") {
                    continue;
                }
                let cell = self.parse_cell_expression(env_name);
                row.push(cell);
                self.skip_ws();
                if self.eat("&") {
                    continue;
                }
                self.eat("\\\\");
                break;
            }
            if !row.is_empty() {
                rows.push(row);
            }
        }

        MathNode::Matrix(Box::new(MatrixData {
            rows,
            delim,
            col_alignments,
        }))
    }

    /// Parses a single cell expression inside a matrix until `&`, `\\`, or `\end{env_name}`.
    fn parse_cell_expression(&mut self, env_name: &str) -> MathNode<'a> {
        self.parse_expression_inner(None, Some(env_name))
    }
}

/// Convenience function to parse a LaTeX math string into a `MathNode`.
pub fn parse_math_str<'a>(src: &'a str) -> MathNode<'a> {
    let mut parser = MathParser::new(src);
    parser.parse_expression(None)
}
