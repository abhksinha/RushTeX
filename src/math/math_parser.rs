//! Zero-copy LaTeX mathematical expression parser.
//!
//! Parses math tokens, superscripts, subscripts, fractions, big operators,
//! radicals, accents, matrices, and extensible delimiters.

use crate::backend::fonts::Face;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, MathChoiceData, MathNode, MathStyle, MatrixData,
    MatrixDelim, DELIMITER_OPT_NONE, FRACTION_OPT_EXACT, FRACTION_OPT_NOAXIS, FRACTION_OPT_NONE,
    FRACTION_OPT_NORULE,
};
use crate::math::math_fonts::{
    lookup_math_command, mathcode_atom_kind, mathcode_char, mathcode_family, MATHCODE_TABLE,
};
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

            // Batch 1: Item 51 - InfixFractionParser & Item 52 - FractionOptionDecoder
            let rem = self.remaining();
            let is_candidate = rem.starts_with("\\over")
                || rem.starts_with("\\atop")
                || rem.starts_with("\\above");

            if is_candidate {
                let (is_infix, has_rule, with_delims, is_above) = if self.eat("\\overwithdelims") {
                    (true, true, true, false)
                } else if self.eat("\\atopwithdelims") {
                    (true, false, true, false)
                } else if self.eat("\\abovewithdelims") {
                    (true, true, true, true)
                } else if rem.starts_with("\\over")
                    && !rem["\\over".len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic())
                {
                    self.pos += "\\over".len();
                    (true, true, false, false)
                } else if rem.starts_with("\\atop")
                    && !rem["\\atop".len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic())
                {
                    self.pos += "\\atop".len();
                    (true, false, false, false)
                } else if rem.starts_with("\\above")
                    && !rem["\\above".len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic())
                {
                    self.pos += "\\above".len();
                    (true, true, false, true)
                } else {
                    (false, false, false, false)
                };

                if is_infix {
                    let (ldelim, rdelim) = if with_delims {
                        self.skip_ws();
                        let l = self.parse_delim_char();
                        self.skip_ws();
                        let r = self.parse_delim_char();
                        (Some(l), Some(r))
                    } else {
                        (None, None)
                    };

                    let mut options = FRACTION_OPT_NONE;
                    if is_above {
                        loop {
                            self.skip_ws();
                            if self.eat("exact") {
                                options |= FRACTION_OPT_EXACT;
                            } else if self.eat("norule") {
                                options |= FRACTION_OPT_NORULE;
                            } else if self.eat("noaxis") {
                                options |= FRACTION_OPT_NOAXIS;
                            } else {
                                break;
                            }
                        }
                        self.skip_ws();
                        self.scan_dimen();
                    }

                    // Numerator is everything parsed before the infix operator
                    let num = if let Some(nodes) = rest.take() {
                        MathNode::Row(nodes)
                    } else if let Some(node) = first.take() {
                        node
                    } else {
                        MathNode::Row(Vec::new())
                    };

                    // Denominator is parsed recursively for the remainder of this scope
                    let den = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                    let frac_rule = has_rule && (options & FRACTION_OPT_NORULE == 0);
                    let frac = MathNode::Fraction {
                        num: Box::new(num),
                        den: Box::new(den),
                        has_rule: frac_rule,
                        options,
                    };

                    return if let (Some(l), Some(r)) = (ldelim, rdelim) {
                        MathNode::Delimited(Box::new(DelimitedData::new(l, r, frac)))
                    } else {
                        frac
                    };
                }
            }

            // Batch 2: Item 57 - EmptyNucleusScriptAttacher
            // If expression begins with '_' or '^', inject an empty ordinary nucleus
            if self.remaining().starts_with('_') || self.remaining().starts_with('^') {
                let empty_nucleus = MathNode::Row(Vec::new());
                let attached = self.attach_scripts(empty_nucleus);
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
                continue;
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
        let mut allow_sub = true;
        let mut allow_sup = true;

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
            } else if self.eat("\\nosuperscript") {
                // Batch 2: Item 59 - NoScriptFlagEnforcer
                allow_sup = false;
                continue;
            } else if self.eat("\\nosubscript") {
                // Batch 2: Item 59 - NoScriptFlagEnforcer
                allow_sub = false;
                continue;
            }

            if self.eat("_") {
                self.skip_ws();
                let sub_node = self.parse_script_argument();
                if !allow_sub {
                    continue;
                }
                // Batch 2: Item 58 - DoubleScriptConflictDetector
                // If sub is already present (e.g. x_1_2), treat like x_1{}_2
                if sub.is_some() {
                    let prev_subsup = MathNode::SubSup {
                        nucleus: Box::new(nucleus),
                        sub,
                        sup: sup.take(),
                    };
                    nucleus = prev_subsup;
                    sub = Some(Box::new(sub_node));
                } else {
                    sub = Some(Box::new(sub_node));
                }
            } else if self.eat("^") {
                self.skip_ws();
                let sup_node = self.parse_script_argument();
                if !allow_sup {
                    continue;
                }
                // Batch 2: Item 58 - DoubleScriptConflictDetector
                // If sup is already present (e.g. x^1^2), treat like x^1{}^2
                if sup.is_some() {
                    let prev_subsup = MathNode::SubSup {
                        nucleus: Box::new(nucleus),
                        sub: sub.take(),
                        sup,
                    };
                    nucleus = prev_subsup;
                    sup = Some(Box::new(sup_node));
                } else {
                    sup = Some(Box::new(sup_node));
                }
            } else if self.eat("'") {
                // Prime operator
                let prime_sym = MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Symbol,
                    glyph: "'",
                    code: Some(b'\''),
                };
                if sup.is_some() {
                    let prev_subsup = MathNode::SubSup {
                        nucleus: Box::new(nucleus),
                        sub: sub.take(),
                        sup,
                    };
                    nucleus = prev_subsup;
                    sup = Some(Box::new(prime_sym));
                } else {
                    sup = Some(Box::new(prime_sym));
                }
            } else {
                break;
            }
        }

        // Batch 2: Item 61 - SuperSubOrderInvariance
        // Canonical ordering: sub and sup are stored in canonical fields regardless of order of encounter
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
            // Batch 1: Item 53 - SubformulaFlattener: unwrapping singleton groups
            let flattened = match inner {
                MathNode::Row(mut nodes) if nodes.len() == 1 => nodes.pop().unwrap(),
                other => other,
            };
            return Some(flattened);
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

        // 5. Operators, Relational Glyphs, Delimiters & Punctuation via Canonical MATHCODE_TABLE
        self.pos += ch.len_utf8();
        let glyph = &self.src[self.pos - ch.len_utf8()..self.pos];
        if let Some(mcode) = MATHCODE_TABLE.lookup(ch) {
            if mcode != 0x8000 {
                let kind = mathcode_atom_kind(mcode);
                let fam = mathcode_family(mcode);
                let char_code = mathcode_char(mcode);
                let face = match fam {
                    0 => Face::Roman,
                    1 => Face::MathItalic,
                    2 => Face::Symbol,
                    3 => Face::Extension,
                    _ => Face::Roman,
                };
                let code = match ch {
                    '-' => Some(0x2D),
                    '<' => Some(b'<'),
                    '>' => Some(b'>'),
                    _ => Some(char_code),
                };
                return Some(MathNode::Symbol {
                    kind,
                    face,
                    glyph,
                    code,
                });
            }
        }

        Some(MathNode::Symbol {
            kind: MathAtomKind::Ord,
            face: Face::Roman,
            glyph,
            code: None,
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
                    options: FRACTION_OPT_NONE,
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
                        options: FRACTION_OPT_NONE,
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
                        options: FRACTION_OPT_NONE,
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
                    options: FRACTION_OPT_NONE,
                };
                Some(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", frac,
                ))))
            }
            "dbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                    options: FRACTION_OPT_NONE,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Display,
                    content: Box::new(frac),
                };
                Some(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", scope,
                ))))
            }
            "tbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                    options: FRACTION_OPT_NONE,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Text,
                    content: Box::new(frac),
                };
                Some(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", scope,
                ))))
            }

            // Continued fraction `\cfrac{num}{den}`
            "cfrac" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: true,
                    options: FRACTION_OPT_NONE,
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

            // Batch 1: Item 54 - MathChoiceQuadBranches `\mathchoice{D}{T}{S}{SS}`
            "mathchoice" => {
                let display = self.parse_mandatory_arg();
                let text = self.parse_mandatory_arg();
                let script = self.parse_mandatory_arg();
                let script_script = self.parse_mandatory_arg();
                Some(MathNode::MathChoice(Box::new(MathChoiceData {
                    display,
                    text,
                    script,
                    script_script,
                })))
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
                let mut core = MathNode::Fraction {
                    num,
                    den,
                    has_rule,
                    options: FRACTION_OPT_NONE,
                };

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
                    Some(MathNode::Delimited(Box::new(DelimitedData::new(
                        open_arg, close_arg, core,
                    ))))
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
                Some(MathNode::Delimited(Box::new(DelimitedData::new(
                    open, close, content,
                ))))
            }

            // Batch 1: Item 56 - MiddleDelimiterSupport `\middle |`
            "middle" => {
                self.skip_ws();
                let delim = self.parse_delim_char();
                Some(MathNode::MiddleDelim {
                    delim,
                    options: DELIMITER_OPT_NONE,
                })
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
                let parens = MathNode::Delimited(Box::new(DelimitedData::new("(", ")", inner)));
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
                    face: Face::AmsB,
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
            // Batch 1: Item 53 - SubformulaFlattener
            match node {
                MathNode::Row(mut nodes) if nodes.len() == 1 => nodes.pop().unwrap(),
                other => other,
            }
        } else if let Some(atom) = self.parse_single_atom() {
            atom
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// Scans an optional dimension value (e.g. `1pt`, `0.5em`) for `\above` and delimiter options.
    fn scan_dimen(&mut self) -> f32 {
        self.skip_ws();
        let start = self.pos;
        let mut has_dot = false;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.pos += 1;
            } else if ch == '.' && !has_dot {
                has_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        let num_str = &self.src[start..self.pos];
        let val: f32 = num_str.parse().unwrap_or(0.0);
        self.skip_ws();
        let rem = self.remaining();
        if rem.starts_with("pt")
            || rem.starts_with("em")
            || rem.starts_with("ex")
            || rem.starts_with("cm")
            || rem.starts_with("mm")
            || rem.starts_with("in")
            || rem.starts_with("mu")
        {
            self.pos += 2;
        }
        val
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
