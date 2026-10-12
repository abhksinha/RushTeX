//! Zero-copy LaTeX mathematical expression parser.
//!
//! Parses math tokens, superscripts, subscripts, fractions, big operators,
//! radicals, accents, matrices, and extensible delimiters.

use crate::backend::fonts::Face;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, MathChoiceData, MathNode, MathStyle, MatrixData,
    MatrixDelim, MatrixEnvKind, DELIMITER_OPT_NONE, FRACTION_OPT_EXACT, FRACTION_OPT_NOAXIS,
    FRACTION_OPT_NONE, FRACTION_OPT_NORULE,
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
                    || self.remaining().starts_with("\\crcr")
                    || self.remaining().starts_with("\\cr")
                    || (!env_name.is_empty() && self.is_at_end_env(env_name))
                {
                    break;
                }
            }

            if self.remaining().starts_with("\\right") {
                break;
            }

            // TeXbook Chapter 19: Display equation numbering (\eqno / \leqno)
            if self.remaining().starts_with("\\eqno") || self.remaining().starts_with("\\leqno") {
                let is_left = self.eat("\\leqno");
                if !is_left {
                    self.eat("\\eqno");
                }
                let formula = if let Some(nodes) = rest.take() {
                    MathNode::Row(nodes)
                } else if let Some(node) = first.take() {
                    node
                } else {
                    MathNode::Row(Vec::new())
                };
                let tag_node = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                return MathNode::Tagged {
                    formula: Box::new(formula),
                    tag: Box::new(tag_node),
                    is_left,
                    starred: false,
                };
            }

            // LaTeX \tag{...} and \tag*{...}
            if self.remaining().starts_with("\\tag") {
                self.eat("\\tag");
                let starred = self.eat("*");
                let tag_node = self.parse_text_arg();
                let formula = if let Some(nodes) = rest.take() {
                    MathNode::Row(nodes)
                } else if let Some(node) = first.take() {
                    node
                } else {
                    MathNode::Row(Vec::new())
                };
                let rest_expr = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                let full_formula = match (formula, rest_expr) {
                    (MathNode::Row(mut n1), MathNode::Row(mut n2)) => {
                        n1.append(&mut n2);
                        MathNode::Row(n1)
                    }
                    (MathNode::Row(mut n1), single) => {
                        if !matches!(single, MathNode::Row(ref r) if r.is_empty()) {
                            n1.push(single);
                        }
                        MathNode::Row(n1)
                    }
                    (single, MathNode::Row(mut n2)) => {
                        let mut v = Vec::with_capacity(1 + n2.len());
                        if !matches!(single, MathNode::Row(ref r) if r.is_empty()) {
                            v.push(single);
                        }
                        v.append(&mut n2);
                        MathNode::Row(v)
                    }
                    (s1, s2) => {
                        let mut v = Vec::new();
                        if !matches!(s1, MathNode::Row(ref r) if r.is_empty()) {
                            v.push(s1);
                        }
                        if !matches!(s2, MathNode::Row(ref r) if r.is_empty()) {
                            v.push(s2);
                        }
                        if v.len() == 1 {
                            v.pop().unwrap()
                        } else {
                            MathNode::Row(v)
                        }
                    }
                };
                return MathNode::Tagged {
                    formula: Box::new(full_formula),
                    tag: Box::new(tag_node),
                    is_left: false,
                    starred,
                };
            }

            // LaTeX \label{...}, \notag, \nonumber
            if self.remaining().starts_with("\\label") {
                self.eat("\\label");
                let _label_key = self.parse_mandatory_arg();
                continue;
            }
            if self.remaining().starts_with("\\notag") || self.remaining().starts_with("\\nonumber") {
                if !self.eat("\\notag") {
                    self.eat("\\nonumber");
                }
                continue;
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
                // If sup already exists and came from prime(s), append to the superscript row (as in y'^2 -> y^{\prime 2})
                if let Some(existing_sup) = sup.take() {
                    let is_prime_or_row = match &*existing_sup {
                        MathNode::Symbol { code: Some(0xA2), .. } => true,
                        MathNode::Row(items) => items.iter().any(|item| matches!(item, MathNode::Symbol { code: Some(0xA2), .. })),
                        _ => false,
                    };
                    if is_prime_or_row {
                        match *existing_sup {
                            MathNode::Row(mut items) => {
                                items.push(sup_node);
                                sup = Some(Box::new(MathNode::Row(items)));
                            }
                            single => {
                                sup = Some(Box::new(MathNode::Row(vec![single, sup_node])));
                            }
                        }
                    } else {
                        let prev_subsup = MathNode::SubSup {
                            nucleus: Box::new(nucleus),
                            sub: sub.take(),
                            sup: Some(existing_sup),
                        };
                        nucleus = prev_subsup;
                        sup = Some(Box::new(sup_node));
                    }
                } else {
                    sup = Some(Box::new(sup_node));
                }
            } else if self.eat("'") {
                // The TeXbook Ch. 16: Consecutive primes are gathered into a single superscript row
                let mut primes = vec![MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Symbol,
                    glyph: "′",
                    code: Some(0xA2),
                }];
                while self.eat("'") {
                    primes.push(MathNode::Symbol {
                        kind: MathAtomKind::Ord,
                        face: Face::Symbol,
                        glyph: "′",
                        code: Some(0xA2),
                    });
                }
                let prime_row = if primes.len() == 1 {
                    primes.pop().unwrap()
                } else {
                    MathNode::Row(primes)
                };

                if let Some(existing_sup) = sup.take() {
                    match *existing_sup {
                        MathNode::Row(mut items) => {
                            match prime_row {
                                MathNode::Row(p_items) => items.extend(p_items),
                                single => items.push(single),
                            }
                            sup = Some(Box::new(MathNode::Row(items)));
                        }
                        single => {
                            let mut items = vec![single];
                            match prime_row {
                                MathNode::Row(p_items) => items.extend(p_items),
                                p => items.push(p),
                            }
                            sup = Some(Box::new(MathNode::Row(items)));
                        }
                    }
                } else {
                    sup = Some(Box::new(prime_row));
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
            match inner {
                MathNode::Row(mut nodes) if nodes.len() == 1 => nodes.pop().unwrap(),
                other => other,
            }
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

        // Check for single-character punctuation escapes: `\,`, `\:`, `\;`, `\!`, `\ `
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
            } else if ch == ' ' {
                self.pos += 1;
                return Some(MathNode::Space(1.0 / 3.0 * 10.0)); // standard control space
            } else if ch == '{' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Open,
                    face: Face::Symbol,
                    glyph: "{",
                    code: Some(0x7B),
                });
            } else if ch == '}' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Close,
                    face: Face::Symbol,
                    glyph: "}",
                    code: Some(0x7D),
                });
            } else if ch == '|' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Symbol,
                    glyph: "||",
                    code: Some(0x88),
                });
            } else if ch == '%' || ch == '$' || ch == '&' || ch == '_' {
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
            "sfrac" | "Uskewed" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                    options: crate::math::math_ast::FRACTION_OPT_SKEWED,
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

            // TeX Appendix G Rule 8: `\vcenter{...}`
            "vcenter" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Vcenter(content))
            }

            // TeX Restricted horizontal box inside math mode: `\hbox{...}` or `\mbox{...}`
            "hbox" | "mbox" => {
                self.skip_ws();
                if self.eat("{") {
                    let start = self.pos;
                    let mut depth = 1;
                    while let Some(ch) = self.peek_char() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        self.pos += ch.len_utf8();
                    }
                    let raw = &self.src[start..self.pos];
                    self.eat("}");
                    let trimmed = raw.trim();
                    if let Some(dollar_idx) = trimmed.find('$') {
                        if let Some(end_dollar) = trimmed.rfind('$') {
                            if end_dollar > dollar_idx {
                                let inner = &trimmed[dollar_idx + 1..end_dollar];
                                let mut p = MathParser::new(inner);
                                return Some(p.parse_expression(None));
                            }
                        }
                    }
                    // Otherwise parse as text:
                    let mut p = MathParser::new(raw);
                    return Some(p.parse_expression(None));
                }
                None
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

            // Atom class coercion primitives (\mathord, \mathop, \mathbin, \mathrel, \mathopen, \mathclose, \mathpunct, \mathinner)
            "mathord" | "mathop" | "mathbin" | "mathrel" | "mathopen" | "mathclose" | "mathpunct" | "mathinner" => {
                let kind = match cmd {
                    "mathord" => MathAtomKind::Ord,
                    "mathop" => MathAtomKind::Op,
                    "mathbin" => MathAtomKind::Bin,
                    "mathrel" => MathAtomKind::Rel,
                    "mathopen" => MathAtomKind::Open,
                    "mathclose" => MathAtomKind::Close,
                    "mathpunct" => MathAtomKind::Punct,
                    "mathinner" => MathAtomKind::Inner,
                    _ => unreachable!(),
                };
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Coerce { kind, content })
            }

            // Operator name (\operatorname, \operatorname*)
            "operatorname" => {
                let _starred = self.eat("*");
                let inner = Box::new(self.parse_mandatory_arg());
                let content = Box::new(MathNode::StyleChange {
                    face: Face::Roman,
                    content: inner,
                });
                Some(MathNode::Coerce {
                    kind: MathAtomKind::Op,
                    content,
                })
            }

            // The TeXbook Chapter 21, 22, 26: Strut, Phantoms, Smashes, and Laps
            "strut" => Some(MathNode::Strut),
            "phantom" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Phantom {
                    content,
                    width: true,
                    height_depth: true,
                })
            }
            "hphantom" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Phantom {
                    content,
                    width: true,
                    height_depth: false,
                })
            }
            "vphantom" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Phantom {
                    content,
                    width: false,
                    height_depth: true,
                })
            }
            "smash" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox {
                    content,
                    zero_width: false,
                    shift_left: false,
                    zero_height_depth: true,
                })
            }
            "rlap" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox {
                    content,
                    zero_width: true,
                    shift_left: false,
                    zero_height_depth: false,
                })
            }
            "llap" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox {
                    content,
                    zero_width: true,
                    shift_left: true,
                    zero_height_depth: false,
                })
            }

            // The TeXbook Chapter 22 & 26: Plain TeX Mathematical Alignment Primitives
            "matrix" | "pmatrix" | "cases" | "eqalign" | "eqalignno" | "leqalignno" | "bordermatrix" => {
                self.skip_ws();
                if self.eat("{") {
                    let (delim, env_kind, col_alignments) = match cmd {
                        "pmatrix" | "bordermatrix" => (MatrixDelim::Parentheses, MatrixEnvKind::Matrix, Vec::new()),
                        "cases" => (MatrixDelim::LeftBrace, MatrixEnvKind::Cases, vec![ColAlign::Left, ColAlign::Left]),
                        "eqalign" => (MatrixDelim::None, MatrixEnvKind::Aligned, vec![ColAlign::Right, ColAlign::Left]),
                        "eqalignno" => (MatrixDelim::None, MatrixEnvKind::Aligned, vec![ColAlign::Right, ColAlign::Left, ColAlign::Right]),
                        "leqalignno" => (MatrixDelim::None, MatrixEnvKind::Aligned, vec![ColAlign::Left, ColAlign::Right, ColAlign::Left]),
                        _ => (MatrixDelim::None, MatrixEnvKind::Matrix, Vec::new()),
                    };
                    Some(self.parse_matrix_body_braced(delim, env_kind, col_alignments))
                } else {
                    None
                }
            }

            "halign" => {
                self.skip_ws();
                if self.remaining().starts_with("to") {
                    self.eat("to");
                    self.skip_ws();
                    while let Some(ch) = self.peek_char() {
                        if ch == '{' || self.is_eof() {
                            break;
                        }
                        self.pos += ch.len_utf8();
                    }
                }
                self.skip_ws();
                if self.eat("{") {
                    Some(self.parse_matrix_body_braced(MatrixDelim::None, MatrixEnvKind::Matrix, Vec::new()))
                } else {
                    None
                }
            }

            "openup" => {
                self.skip_ws();
                while let Some(ch) = self.peek_char() {
                    if ch.is_ascii_whitespace() || ch == '\\' || ch == '{' {
                        break;
                    }
                    self.pos += ch.len_utf8();
                }
                self.skip_ws();
                if self.remaining().starts_with("\\jot") {
                    self.eat("\\jot");
                }
                None
            }

            "jot" => Some(MathNode::Space(3.0)),
            "cr" | "crcr" | "omit" => None,
            "multispan" => {
                self.skip_ws();
                if self.eat("{") {
                    let _ = self.parse_single_atom();
                    self.eat("}");
                } else {
                    let _ = self.parse_single_atom();
                }
                None
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
            "int" | "iint" | "iiint" | "oint" | "oiint" | "oiiint" => Some(MathNode::BigOp(Box::new(BigOpData {
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

            // The TeXbook Chapter 17: Delimiter sizing hierarchy (\big, \Big, \bigg, \Bigg and directional variants)
            "big" | "bigl" | "bigr" | "bigm"
            | "Big" | "Bigl" | "Bigr" | "Bigm"
            | "bigg" | "biggl" | "biggr" | "biggm"
            | "Bigg" | "Biggl" | "Biggr" | "Biggm" => {
                let size_tier = match cmd {
                    "big" | "bigl" | "bigr" | "bigm" => 0,
                    "Big" | "Bigl" | "Bigr" | "Bigm" => 1,
                    "bigg" | "biggl" | "biggr" | "biggm" => 2,
                    _ => 3,
                };
                let kind = match cmd {
                    "bigl" | "Bigl" | "biggl" | "Biggl" => MathAtomKind::Open,
                    "bigr" | "Bigr" | "biggr" | "Biggr" => MathAtomKind::Close,
                    "bigm" | "Bigm" | "biggm" | "Biggm" => MathAtomKind::Rel,
                    _ => MathAtomKind::Ord,
                };
                self.skip_ws();
                let delim = self.parse_delim_char();
                let is_left = match kind {
                    MathAtomKind::Open => true,
                    MathAtomKind::Close => false,
                    _ => !matches!(delim, ")" | "]" | "}" | "\\rangle" | "\\rfloor" | "\\rceil"),
                };
                Some(MathNode::SizedDelim {
                    delim,
                    size_tier,
                    kind,
                    is_left,
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

            // Punctuation and ellipses (The TeXbook Chapter 18)
            "colon" => Some(MathNode::Symbol {
                kind: MathAtomKind::Punct,
                face: Face::Roman,
                glyph: ":",
                code: Some(0x3A),
            }),
            // TeX mlist.c line 4557 conditional math glue primitive
            "nonscript" => {
                self.skip_ws();
                Some(MathNode::Nonscript)
            },
            "ldots" | "dotso" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Low)),
            "cdots" | "dotsb" | "dotsm" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Centered)),
            "vdots" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Vertical)),
            "ddots" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Diagonal)),
            "dots" => {
                // Knuth / AMS-LaTeX intelligent dots:
                // Centered (\cdots) if followed by binary or relational operator; Low (\ldots) otherwise.
                let rem = self.remaining().trim_start();
                let is_binary_or_rel = rem.starts_with('+')
                    || rem.starts_with('-')
                    || rem.starts_with('=')
                    || rem.starts_with('<')
                    || rem.starts_with('>')
                    || rem.starts_with("\\pm")
                    || rem.starts_with("\\mp")
                    || rem.starts_with("\\times")
                    || rem.starts_with("\\cdot")
                    || rem.starts_with("\\cap")
                    || rem.starts_with("\\cup")
                    || rem.starts_with("\\le")
                    || rem.starts_with("\\ge")
                    || rem.starts_with("\\leq")
                    || rem.starts_with("\\geq")
                    || rem.starts_with("\\in")
                    || rem.starts_with("\\subset")
                    || rem.starts_with("\\subseteq");
                if is_binary_or_rel {
                    Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Centered))
                } else {
                    Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Low))
                }
            }

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

            // Accents (mlist.c top, bottom, and overlay accents)
            "vec" | "hat" | "dot" | "ddot" | "dddot" | "ddddot" | "bar" | "tilde" | "check"
            | "breve" | "acute" | "grave" | "mathring" | "overline" | "underline" | "widehat"
            | "widetilde" | "overrightarrow" | "overleftarrow"
            | "utilde" | "underbar" | "not" | "cancel" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent {
                    accent: cmd,
                    nucleus,
                })
            }
            "underaccent" => {
                let _acc = self.parse_mandatory_arg();
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent {
                    accent: "utilde",
                    nucleus,
                })
            }

            // Extensible horizontal braces and brackets (mlist.c make_delimiter_over/under)
            "overbrace" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Overbrace(nucleus))
            }
            "underbrace" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Underbrace(nucleus))
            }
            "overbracket" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Overbracket(nucleus))
            }
            "underbracket" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Underbracket(nucleus))
            }

            // Extensible horizontal arrows (mlist.c make_hextension)
            "xrightarrow" | "xleftarrow" | "xleftrightarrow" => {
                self.skip_ws();
                let sub = if self.eat("[") {
                    let sub_node = self.parse_expression(Some("]"));
                    self.eat("]");
                    Some(sub_node)
                } else {
                    None
                };
                let sup_node = self.parse_mandatory_arg();
                let sup = if matches!(sup_node, MathNode::Row(ref r) if r.is_empty()) {
                    None
                } else {
                    Some(sup_node)
                };
                let arrow_type = match cmd {
                    "xleftarrow" => "leftarrow",
                    "xleftrightarrow" => "leftrightarrow",
                    _ => "rightarrow",
                };
                Some(MathNode::ExtensibleArrow(Box::new(
                    crate::math::math_ast::ExtensibleArrowData {
                        arrow: arrow_type,
                        sub,
                        sup,
                    },
                )))
            }

            // Font switches
            "text" => {
                let node = self.parse_text_arg();
                Some(node)
            }
            "mathrm" => {
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
            "boldsymbol" | "bm" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::BoldItalic,
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
                let content = self.parse_mandatory_arg();
                Some(transform_to_sans(content))
            }
            "mathcal" | "mathscr" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::Symbol,
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
            "mathfrak" | "frak" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::AmsA,
                    content,
                })
            }

            // Environments: `\begin{matrix}`, `\begin{align*}`, etc.
            "begin" => {
                let env = self.parse_ident_arg();
                let (delim, env_kind, col_alignments) = match env {
                    "pmatrix" => (MatrixDelim::Parentheses, MatrixEnvKind::Matrix, Vec::new()),
                    "bmatrix" => (MatrixDelim::Brackets, MatrixEnvKind::Matrix, Vec::new()),
                    "Bmatrix" => (MatrixDelim::Braces, MatrixEnvKind::Matrix, Vec::new()),
                    "vmatrix" => (MatrixDelim::VBar, MatrixEnvKind::Matrix, Vec::new()),
                    "Vmatrix" => (MatrixDelim::DoubleVBar, MatrixEnvKind::Matrix, Vec::new()),
                    "smallmatrix" => (MatrixDelim::None, MatrixEnvKind::SmallMatrix, Vec::new()),
                    "cases" => (
                        MatrixDelim::LeftBrace,
                        MatrixEnvKind::Cases,
                        vec![ColAlign::Left, ColAlign::Left],
                    ),
                    "aligned" | "align" | "align*" | "alignat" | "alignat*" | "split" => (
                        MatrixDelim::None,
                        MatrixEnvKind::Aligned,
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
                    "gather" | "gather*" | "gathered" => (
                        MatrixDelim::None,
                        MatrixEnvKind::Gather,
                        vec![ColAlign::Center; 8],
                    ),
                    "multline" | "multline*" => {
                        (MatrixDelim::None, MatrixEnvKind::Multline, Vec::new())
                    }
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
                        (MatrixDelim::None, MatrixEnvKind::Matrix, aligns)
                    }
                    _ => (MatrixDelim::None, MatrixEnvKind::Matrix, Vec::new()),
                };
                let matrix_node = self.parse_matrix_body(env, delim, env_kind, col_alignments);
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
        if let Some(ch) = self.peek_char() {
            if ch != '\\' {
                let start = self.pos;
                self.pos += ch.len_utf8();
                return &self.src[start..self.pos];
            }
        } else {
            return "(";
        }
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

    /// Parses a `\text{...}` argument, preserving literal whitespace as inter-word spacing.
    fn parse_text_arg(&mut self) -> MathNode<'a> {
        self.skip_ws();
        if !self.eat("{") {
            let atom = self.parse_single_atom().unwrap_or(MathNode::Row(Vec::new()));
            return MathNode::StyleChange {
                face: Face::Roman,
                content: Box::new(atom),
            };
        }
        let start = self.pos;
        let mut depth = 1;
        while let Some(ch) = self.peek_char() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            self.pos += ch.len_utf8();
        }
        let raw = &self.src[start..self.pos];
        self.eat("}");

        let mut nodes = Vec::new();
        let bytes = raw.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                nodes.push(MathNode::Space(3.3333));
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            } else {
                let w_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let word = &raw[w_start..i];
                nodes.push(MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Roman,
                    glyph: word,
                    code: None,
                });
            }
        }

        let content = if nodes.len() == 1 {
            nodes.pop().unwrap()
        } else {
            MathNode::Row(nodes)
        };

        MathNode::StyleChange {
            face: Face::Roman,
            content: Box::new(content),
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

    /// Parses a matrix environment body with rows separated by `\\`, `\cr`, or `\crcr` and columns by `&`.
    fn parse_matrix_body_inner(
        &mut self,
        env_name: Option<&str>,
        delim: MatrixDelim,
        env_kind: MatrixEnvKind,
        col_alignments: Vec<ColAlign>,
    ) -> MathNode<'a> {
        let mut rows = Vec::with_capacity(4);

        while !self.is_eof() {
            self.skip_ws();
            if let Some(name) = env_name {
                if self.is_at_end_env(name) {
                    self.eat_end_env(name);
                    break;
                }
            } else if self.remaining().starts_with("}") {
                self.eat("}");
                break;
            }

            if self.eat("\\hline") {
                continue;
            }
            if self.remaining().starts_with("\\noalign") {
                self.eat("\\noalign");
                self.skip_group();
                continue;
            }

            let mut row = Vec::with_capacity(4);
            let mut col_idx = 0usize;
            loop {
                self.skip_ws();
                if self.is_eof() {
                    break;
                }
                if let Some(name) = env_name {
                    if self.is_at_end_env(name) {
                        break;
                    }
                } else if self.remaining().starts_with("}") {
                    break;
                }
                if self.eat("\\hline") {
                    continue;
                }
                if self.remaining().starts_with("\\noalign") {
                    self.eat("\\noalign");
                    self.skip_group();
                    continue;
                }
                let mut cell = self.parse_cell_expression_opt(env_name);

                // AMS Parity: prepend empty Ord atom `{}` for operator spacing:
                // 1. In `aligned` odd columns (col 1, 3, 5...), LaTeX uses `${}#$`.
                //    If cell starts with a Rel or Bin, `{}` gives it its left clearance.
                // 2. In `multline` subsequent rows (row > 0), binary operators like `+`
                //    must retain binary operator spacing instead of unary promotion.
                let needs_phantom_ord = match env_kind {
                    MatrixEnvKind::Aligned => {
                        (col_idx % 2 == 1)
                            && matches!(
                                cell.first_atom_kind(),
                                MathAtomKind::Rel | MathAtomKind::Bin
                            )
                    }
                    MatrixEnvKind::Multline => {
                        !rows.is_empty() && cell.first_atom_kind() == MathAtomKind::Bin
                    }
                    _ => false,
                };

                if needs_phantom_ord {
                    match cell {
                        MathNode::Row(ref mut nodes) => {
                            nodes.insert(0, MathNode::Row(Vec::new()));
                        }
                        other => {
                            cell = MathNode::Row(vec![MathNode::Row(Vec::new()), other]);
                        }
                    }
                }

                row.push(cell);
                col_idx += 1;
                self.skip_ws();
                if self.eat("&") {
                    continue;
                }
                if !self.eat("\\\\") && !self.eat("\\crcr") {
                    self.eat("\\cr");
                }
                break;
            }
            if !row.is_empty() {
                rows.push(row);
            }
        }

        MathNode::Matrix(Box::new(MatrixData {
            rows,
            delim,
            env_kind,
            col_alignments,
        }))
    }

    #[inline(always)]
    fn parse_matrix_body(
        &mut self,
        env_name: &str,
        delim: MatrixDelim,
        env_kind: MatrixEnvKind,
        col_alignments: Vec<ColAlign>,
    ) -> MathNode<'a> {
        self.parse_matrix_body_inner(Some(env_name), delim, env_kind, col_alignments)
    }

    #[inline(always)]
    fn parse_matrix_body_braced(
        &mut self,
        delim: MatrixDelim,
        env_kind: MatrixEnvKind,
        col_alignments: Vec<ColAlign>,
    ) -> MathNode<'a> {
        self.parse_matrix_body_inner(None, delim, env_kind, col_alignments)
    }

    /// Parses a single cell expression inside a matrix until `&`, `\\`, `\cr`, or delimiter.
    fn parse_cell_expression_opt(&mut self, env_name: Option<&str>) -> MathNode<'a> {
        if let Some(name) = env_name {
            self.parse_expression_inner(None, Some(name))
        } else {
            self.parse_expression_inner(Some("}"), Some(""))
        }
    }

    fn skip_group(&mut self) {
        self.skip_ws();
        if !self.eat("{") {
            return;
        }
        let mut depth = 1;
        while let Some(ch) = self.peek_char() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    self.pos += 1;
                    break;
                }
            }
            self.pos += ch.len_utf8();
        }
    }
}

/// Convenience function to parse a LaTeX math string into a `MathNode`.
pub fn parse_math_str<'a>(src: &'a str) -> MathNode<'a> {
    let mut parser = MathParser::new(src);
    parser.parse_expression(None)
}

/// Recursively transforms ASCII letters into canonical Unicode Mathematical Sans-Serif symbols.
fn transform_to_sans<'a>(node: MathNode<'a>) -> MathNode<'a> {
    match node {
        MathNode::Variable(ch) => {
            if let Some(s) = crate::math::fonts::to_unicode_sans(ch) {
                MathNode::Variable(s)
            } else {
                MathNode::Variable(ch)
            }
        }
        MathNode::Row(nodes) => {
            MathNode::Row(nodes.into_iter().map(transform_to_sans).collect())
        }
        MathNode::SubSup { nucleus, sub, sup } => {
            MathNode::SubSup {
                nucleus: Box::new(transform_to_sans(*nucleus)),
                sub: sub.map(|s| Box::new(transform_to_sans(*s))),
                sup: sup.map(|s| Box::new(transform_to_sans(*s))),
            }
        }
        other => other,
    }
}
