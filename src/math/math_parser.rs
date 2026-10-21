//! Zero-copy LaTeX mathematical expression parser.
//!
//! Parses math tokens, superscripts, subscripts, fractions, big operators,
//! radicals, accents, matrices, and extensible delimiters.

use crate::backend::fonts::Face;
use crate::frontend::ast::PackedColor;
use crate::math::math_ast::{
    BigOpData, ColAlign, DelimitedData, MathChoiceData, MathNode, MathSkip, MathStyle, MatrixData, MatrixExtras,
    MatrixDelim, MatrixEnvKind, DELIMITER_OPT_NONE, OP_LIMITS_BOXED_NUCLEUS, OP_LIMITS_DISPLAY,
    OP_LIMITS_LIMITS, OP_LIMITS_MODE_MASK, OP_LIMITS_NOLIMITS, FRACTION_OPT_EXACT, FRACTION_OPT_NOAXIS,
    FRACTION_OPT_DELIMITED, FRACTION_OPT_NONE, FRACTION_OPT_NORULE,
};
use crate::math::math_fonts::{
    lookup_math_command, mathcode_atom_kind, mathcode_char, mathcode_family, MATHCODE_TABLE,
};
use crate::math::math_spacing::MathAtomKind;

pub struct MathParser<'a> {
    src: &'a str,
    pos: usize,
    /// Set when the last command expanded a macro into a row whose atoms belong in the
    /// enclosing list (a macro expansion is not a group in TeX).
    spliced_row: bool,
    /// `\definecolor` names of the document, for `\color` and `\textcolor`.
    colors: Vec<(&'a str, PackedColor)>,
    /// Set by `\notag`/`\nonumber` inside the current alignment row.
    notag_seen: bool,
    /// Alignment environments open around the current position.
    env_depth: u8,
    /// Set when a `split` nested in another alignment was just parsed.
    split_in_cell: bool,
    /// `\delimiterfactor` and `\delimitershortfall` (pt; NaN: the font's) in force.
    pub delim_params: (u16, f32),
    /// amsmath `\multlinegap` (pt) in force.
    pub multline_gap: f32,
}

impl<'a> MathParser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0, spliced_row: false, colors: Vec::new(), notag_seen: false, env_depth: 0, split_in_cell: false, delim_params: (901, f32::NAN), multline_gap: 10.0 }
    }

    /// The colour argument of `\color`/`\textcolor`: `[model]{spec}` or `{expression}`;
    /// black when it does not resolve.
    fn parse_color_arg(&mut self) -> PackedColor {
        self.skip_ws();
        let color = if self.eat("[") {
            let start = self.pos;
            while !self.is_eof() && !self.remaining().starts_with(']') {
                self.pos += 1;
            }
            let model = &self.src[start..self.pos];
            self.eat("]");
            self.skip_ws();
            let spec = self.raw_brace_arg();
            PackedColor::from_model(model, spec)
        } else {
            let expr = self.raw_brace_arg();
            PackedColor::resolve(expr, &self.colors)
        };
        color.unwrap_or(PackedColor::BLACK)
    }

    /// The raw text of a `{...}` argument (balanced braces), or "" when none follows.
    fn raw_brace_arg(&mut self) -> &'a str {
        self.skip_ws();
        if !self.eat("{") {
            return "";
        }
        let start = self.pos;
        let mut depth = 1usize;
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        let arg = &self.src[start..self.pos.min(bytes.len())];
        self.eat("}");
        arg
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
    /// Skips spaces and comments (`%` to the end of the line, TeX's category 14).
    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else if ch == '%' {
                self.pos += self.remaining().find('\n').map_or(self.remaining().len(), |p| p + 1);
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

            // The structural commands below all start with `\` (or `&`): most tokens skip them.
            let lead = self.remaining().as_bytes()[0];
            if let (Some(env_name), b'&' | b'\\') = (stop_at_cell, lead) {
                if self.remaining().starts_with("&")
                    || self.remaining().starts_with("\\\\")
                    || self.remaining().starts_with("\\crcr")
                    || self.remaining().starts_with("\\cr")
                    || (!env_name.is_empty() && self.is_at_end_env(env_name))
                {
                    break;
                }
            }

            if lead == b'\\' && self.at_right_delimiter() {
                break;
            }

            // TeXbook Chapter 19: Display equation numbering (\eqno / \leqno)
            if lead == b'\\' && (self.remaining().starts_with("\\eqno") || self.remaining().starts_with("\\leqno")) {
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
            if lead == b'\\' && self.remaining().starts_with("\\tag") {
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
            if lead == b'\\' && self.remaining().starts_with("\\label") {
                self.eat("\\label");
                let _label_key = self.parse_mandatory_arg();
                continue;
            }
            if lead == b'\\' && (self.remaining().starts_with("\\notag") || self.remaining().starts_with("\\nonumber")) {
                if !self.eat("\\notag") {
                    self.eat("\\nonumber");
                }
                self.notag_seen = true;
                continue;
            }

            // Batch 1: Item 51 - InfixFractionParser & Item 52 - FractionOptionDecoder
            let rem = self.remaining();
            let is_candidate = lead == b'\\'
                && (rem.starts_with("\\over")
                || rem.starts_with("\\atop")
                || rem.starts_with("\\above")
                || rem.starts_with("\\choose")
                || rem.starts_with("\\brace")
                || rem.starts_with("\\brack"));

            if is_candidate {
                // plain.tex: `\choose`, `\brace` and `\brack` are `\atopwithdelims` with fixed
                // delimiter pairs.
                let mut fixed_delims: Option<(&'a str, &'a str)> = None;
                for (name, l, r) in [("\\choose", "(", ")"), ("\\brace", "\\{", "\\}"), ("\\brack", "[", "]")] {
                    if rem.starts_with(name) && !rem[name.len()..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                        self.pos += name.len();
                        fixed_delims = Some((l, r));
                        break;
                    }
                }
                let (is_infix, has_rule, with_delims, is_above) = if fixed_delims.is_some() {
                    (true, false, true, false)
                } else if self.eat("\\overwithdelims") {
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
                    let (ldelim, rdelim) = if let Some((l, r)) = fixed_delims {
                        (Some(l), Some(r))
                    } else if with_delims {
                        self.skip_ws();
                        let l = self.parse_delim_char();
                        self.skip_ws();
                        let r = self.parse_delim_char();
                        (Some(l), Some(r))
                    } else {
                        (None, None)
                    };

                    let mut options = if with_delims { FRACTION_OPT_DELIMITED } else { FRACTION_OPT_NONE };
                    let mut thickness = -1.0f32;
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
                        let start = self.pos;
                        self.scan_dimen();
                        thickness = crate::config::parse_dimension_pt(self.src[start..self.pos].trim()).unwrap_or(-1.0);
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
                        options: options | crate::math::math_ast::FRACTION_OPT_INNER,
                        thickness,
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
            if self.remaining().starts_with('_') || self.remaining().starts_with('^') || self.remaining().starts_with('\'') {
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
                if std::mem::take(&mut self.spliced_row) {
                    if let MathNode::Row(mut items) = node {
                        // Scripts after a macro attach to the last atom of its expansion.
                        if let Some(last) = items.pop() {
                            let last = self.attach_scripts(last);
                            items.push(last);
                        }
                        let nodes = rest.get_or_insert_with(|| first.take().into_iter().collect());
                        nodes.extend(items);
                        continue;
                    }
                    continue;
                }
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
        self.skip_ws();
        if let MathNode::OpLimits { ref mut mode, .. } = nucleus {
            loop {
                let m = if self.eat("\\limits") {
                    OP_LIMITS_LIMITS
                } else if self.eat("\\nolimits") {
                    OP_LIMITS_NOLIMITS
                } else if self.eat("\\displaylimits") {
                    OP_LIMITS_DISPLAY
                } else {
                    break;
                };
                *mode = (*mode & !OP_LIMITS_MODE_MASK) | m;
                self.skip_ws();
            }
        }
        if let MathNode::BigOp(ref mut data) = nucleus {
            self.skip_ws();
            if self.eat("\\limits") {
                data.limits = OP_LIMITS_LIMITS;
            } else if self.eat("\\nolimits") {
                data.limits = OP_LIMITS_NOLIMITS;
            } else if self.eat("\\displaylimits") {
                data.limits = OP_LIMITS_DISPLAY;
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
                    data.limits = OP_LIMITS_LIMITS;
                }
                continue;
            } else if self.eat("\\nolimits") {
                if let MathNode::BigOp(ref mut data) = nucleus {
                    data.limits = OP_LIMITS_NOLIMITS;
                }
                continue;
            } else if self.eat("\\displaylimits") {
                if let MathNode::BigOp(ref mut data) = nucleus {
                    data.limits = OP_LIMITS_DISPLAY;
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
                    mathcode: 0,
                }];
                while self.eat("'") {
                    primes.push(MathNode::Symbol {
                        kind: MathAtomKind::Ord,
                        face: Face::Symbol,
                        glyph: "′",
                        code: Some(0xA2),
                        mathcode: 0,
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
        } else if let Some(primary) = self.parse_arg_atom() {
            primary
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// An undelimited macro argument or script: one token, so `\frac12` and `x^12` take a
    /// single digit.
    fn parse_arg_atom(&mut self) -> Option<MathNode<'a>> {
        self.skip_ws();
        match self.peek_char() {
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                self.pos += 1;
                Some(MathNode::Number(&self.src[start..self.pos]))
            }
            _ => self.parse_single_atom(),
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
            // A braced subformula is an Ord atom (TeXbook ch. 17), so `{=}` loses its spacing.
            if flattened.atom_kind() != MathAtomKind::Ord {
                return Some(MathNode::Coerce {
                    kind: MathAtomKind::Ord,
                    content: Box::new(flattened),
                });
            }
            return Some(flattened);
        }

        // 2. Command `\cmd` (a macro expansion here stays one node: it is an argument)
        if rem.starts_with('\\') {
            let node = self.parse_command(None, None);
            self.spliced_row = false;
            return node;
        }

        // 3. Number (digits)
        let ch = rem.chars().next()?;
        if ch.is_ascii_digit() {
            let start = self.pos;
            while let Some(c) = self.peek_char() {
                // Digits only: `.` is its own math character (mathcode "013A, cmmi).
                if c.is_ascii_digit() {
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
                    mathcode: 0,
                });
            }
        }

        Some(MathNode::Symbol {
            kind: MathAtomKind::Ord,
            face: Face::Roman,
            glyph,
            code: None,
            mathcode: 0,
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

        // 1. Group `{ ... }`: a braced subformula is an Ord atom (TeXbook ch. 17).
        if self.eat("{") {
            let inner = self.parse_expression(Some("}"));
            self.eat("}");
            if inner.atom_kind() != MathAtomKind::Ord {
                return Some(MathNode::Coerce {
                    kind: MathAtomKind::Ord,
                    content: Box::new(inner),
                });
            }
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
                return Some(MathNode::Space(MathSkip::Mu(3.0))); // 3 mu thin space
            } else if ch == ':' {
                self.pos += 1;
                return Some(MathNode::Space(MathSkip::Mu(4.0))); // 4 mu medium space
            } else if ch == ';' {
                self.pos += 1;
                return Some(MathNode::Space(MathSkip::Mu(5.0))); // 5 mu thick space
            } else if ch == '!' {
                self.pos += 1;
                return Some(MathNode::Space(MathSkip::Mu(-3.0))); // negative thin space
            } else if ch == ' ' {
                self.pos += 1;
                return Some(MathNode::Space(MathSkip::TextEm(1.0 / 3.0))); // standard control space
            } else if ch == '{' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Open,
                    face: Face::Symbol,
                    glyph: "{",
                    code: Some(0x7B),
                    mathcode: 0x4266,
                });
            } else if ch == '}' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Close,
                    face: Face::Symbol,
                    glyph: "}",
                    code: Some(0x7D),
                    mathcode: 0x5267,
                });
            } else if ch == '|' {
                self.pos += 1;
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Symbol,
                    glyph: "||",
                    code: Some(0x88),
                    mathcode: 0,
                });
            } else if ch == '%' || ch == '$' || ch == '&' || ch == '_' {
                self.pos += 1;
                let glyph = &self.src[self.pos - 1..self.pos];
                return Some(MathNode::Symbol {
                    kind: MathAtomKind::Ord,
                    face: Face::Roman,
                    glyph,
                    code: None,
                    mathcode: 0,
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

        if let Some(body) = lookup_math_macro(cmd) {
            let tex = self.expand_math_macro(body);
            let splice = matches!(tex, MathNode::Row(_)) && lookup_math_command(cmd).is_none();
            // Only the top-level expression loop splices; anywhere else the row stays whole.
            self.spliced_row = splice;
            // Keep the single-glyph Base-14 rendering for families without the TeX glyphs.
            return Some(match lookup_math_command(cmd) {
                Some(sym) => MathNode::FamilyChoice(Box::new((
                    tex,
                    MathNode::Symbol {
                        kind: sym.kind,
                        face: sym.face,
                        glyph: sym.glyph,
                        code: sym.code,
                        mathcode: 0,
                    },
                ))),
                None => tex,
            });
        }

        // fontmath.ltx builds `\cong` (`\@vereq\sim`, `=` below) and `\notin` (`\c@ncel\in`,
        // a `\mkern1mu/` overprinted) from other characters; Base-14 families keep their glyph.
        if cmd == "cong" || cmd == "notin" {
            if let Some(sym) = lookup_math_command(cmd) {
                let (rows, env_kind) = if cmd == "cong" {
                    (vec![vec![MathParser::new("\\sim").parse_expression(None)], vec![MathParser::new("=").parse_expression(None)]], MatrixEnvKind::Vereq)
                } else {
                    (vec![vec![MathParser::new("\\mkern1mu/").parse_expression(None)], vec![MathParser::new("\\in").parse_expression(None)]], MatrixEnvKind::Overlay)
                };
                let built = MathNode::Coerce {
                    kind: MathAtomKind::Rel,
                    content: Box::new(MathNode::Matrix(Box::new(MatrixData {
                        rows,
                        delim: MatrixDelim::None,
                        env_kind,
                        col_alignments: Box::new([ColAlign::Center]),
                        numbered_env: false,
                        display_env: false,
                        notag_mask: 0,
                        split: false,
                        extras: None,
                    }))),
                };
                return Some(MathNode::FamilyChoice(Box::new((
                    built,
                    MathNode::Symbol { kind: sym.kind, face: sym.face, glyph: sym.glyph, code: sym.code, mathcode: 0 },
                ))));
            }
        }

        match cmd {
            // TeX kerns and glue: `\mkern`, `\mskip`, `\kern`, `\hskip` (stretch is ignored).
            "mkern" | "mskip" | "kern" | "hskip" => Some(MathNode::Space(self.scan_skip())),
            // `\mathchar"XYZZ`: a math character given by its TeX math code.
            "mathchar" => {
                let mathcode = self.scan_int() as u16;
                let kind = mathcode_class_kind(mathcode);
                // A class-1 character makes an Op noad with a character nucleus (Rule 13).
                let op = kind == MathAtomKind::Op;
                let sym = MathNode::Symbol {
                    kind: if op { MathAtomKind::Ord } else { kind },
                    face: Face::Symbol,
                    glyph: "?",
                    code: None,
                    mathcode,
                };
                Some(if op { MathNode::OpLimits { content: Box::new(sym), mode: OP_LIMITS_DISPLAY } } else { sym })
            }
            // Commands with no typeset result in math lists.
            "nobreak" | "allowbreak" | "relax" | "DOTSB" | "DOTSI" | "DOTSX" => {
                Some(MathNode::Space(MathSkip::Pt(0.0)))
            }
            "penalty" => {
                let _ = self.scan_int();
                Some(MathNode::Space(MathSkip::Pt(0.0)))
            }

            // amsmath `\overset{a}{b}` / `\underset{a}{b}`: `\binrel@{b}` around
            // `\mathop{\kern0pt b}\limits^{a}` (or `_{a}`); latex.ltx `\stackrel{a}{b}`:
            // `\mathrel{\mathop{b}\limits^{a}}`.
            "overset" | "underset" | "stackrel" => {
                let script = Box::new(self.parse_mandatory_arg());
                let base = self.parse_mandatory_arg();
                let kind = match cmd {
                    "stackrel" => MathAtomKind::Rel,
                    _ => match base.atom_kind() {
                        k @ (MathAtomKind::Bin | MathAtomKind::Rel) => k,
                        _ => MathAtomKind::Ord,
                    },
                };
                let boxed = if cmd == "stackrel" { 0 } else { OP_LIMITS_BOXED_NUCLEUS };
                let op = Box::new(MathNode::OpLimits { content: Box::new(base), mode: OP_LIMITS_LIMITS | boxed });
                let (sub, sup) = if cmd == "underset" { (Some(script), None) } else { (None, Some(script)) };
                Some(MathNode::Coerce {
                    kind,
                    content: Box::new(MathNode::SubSup { nucleus: op, sub, sup }),
                })
            }

            // amsmath `\substack{a\\b}`: a centred `subarray`.
            "substack" => {
                self.skip_ws();
                self.eat("{");
                Some(self.parse_matrix_body_braced(MatrixDelim::None, MatrixEnvKind::Subarray, vec![ColAlign::Center]))
            }

            "boxed" => Some(MathNode::Boxed(Box::new(self.parse_mandatory_arg()))),
            "openbox" | "qedsymbol" => Some(MathNode::OpenBox),

            // xcolor. `\textcolor{c}{x}` is `{\color{c}x}`: the braces make an Ord atom.
            "textcolor" => {
                let color = self.parse_color_arg();
                let content = self.parse_mandatory_arg();
                Some(MathNode::Colored { color, content: Box::new(MathNode::Coerce { kind: MathAtomKind::Ord, content: Box::new(content) }) })
            }
            // `\color{c}` colours the rest of the current group.
            "color" => {
                let color = self.parse_color_arg();
                let content = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                Some(MathNode::Colored { color, content: Box::new(content) })
            }

            "sideset" => {
                let scripts = |node: MathNode<'a>| match node {
                    MathNode::SubSup { sub, sup, .. } => (sub, sup),
                    _ => (None, None),
                };
                let (pre_sub, pre_sup) = scripts(self.parse_mandatory_arg());
                let (post_sub, post_sup) = scripts(self.parse_mandatory_arg());
                self.skip_ws();
                let op = self.parse_single_atom().unwrap_or(MathNode::Row(Vec::new()));
                let pre = MathNode::SubSup {
                    nucleus: Box::new(MathNode::Phantom { content: Box::new(op.clone()), width: false, height_depth: true }),
                    sub: pre_sub,
                    sup: pre_sup,
                };
                let post = match op.clone() {
                    MathNode::BigOp(mut data) => {
                        data.limits = OP_LIMITS_NOLIMITS;
                        data.sub = post_sub;
                        data.sup = post_sup;
                        MathNode::BigOp(data)
                    }
                    other => MathNode::SubSup { nucleus: Box::new(other), sub: post_sub, sup: post_sup },
                };
                // The leading `\hbox to\dimen@{}` is an Ord atom of the enclosing list, so the
                // two atoms are spliced in; scripts that follow are the `\mathop`'s limits.
                self.spliced_row = true;
                let parts = Box::new((op, pre, post));
                Some(MathNode::Row(vec![
                    MathNode::SideSetLead(parts.clone()),
                    MathNode::OpLimits {
                        content: Box::new(MathNode::SideSet(parts)),
                        mode: OP_LIMITS_DISPLAY | OP_LIMITS_BOXED_NUCLEUS,
                    },
                ]))
            }

            "sf" => {
                let rest = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                Some(transform_to_sans(rest))
            }

            // LaTeX 2.09 font switches (`{\rm sup}`): like `\mathrm` but for the rest of
            // the current group.
            "rm" | "bf" | "it" | "tt" | "cal" => {
                let face = match cmd {
                    "rm" => Face::Roman,
                    "bf" => Face::Bold,
                    "it" => Face::Italic,
                    "tt" => Face::Mono,
                    _ => Face::Symbol,
                };
                let rest = self.parse_expression_inner(stop_at_delim, stop_at_cell);
                Some(MathNode::StyleChange {
                    face,
                    content: Box::new(rest),
                })
            }

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
                    thickness: -1.0,
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
                        thickness: -1.0,
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
                        thickness: -1.0,
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
                    thickness: -1.0,
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
                    options: FRACTION_OPT_DELIMITED,
                    thickness: -1.0,
                };
                Some(genfrac_ord(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", frac,
                )))))
            }
            "dbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                    options: FRACTION_OPT_DELIMITED,
                    thickness: -1.0,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Display,
                    content: Box::new(frac),
                };
                Some(genfrac_ord(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", scope,
                )))))
            }
            "tbinom" => {
                let num = Box::new(self.parse_mandatory_arg());
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num,
                    den,
                    has_rule: false,
                    options: FRACTION_OPT_DELIMITED,
                    thickness: -1.0,
                };
                let scope = MathNode::StyleScope {
                    style: MathStyle::Text,
                    content: Box::new(frac),
                };
                Some(genfrac_ord(MathNode::Delimited(Box::new(DelimitedData::new(
                    "(", ")", scope,
                )))))
            }

            // Continued fraction `\cfrac{num}{den}`
            // amsmath: `{\displaystyle\frac{\strut#2}{#3}}\kern-\nulldelimiterspace`
            // (the `[l]`/`[r]` alignment option is accepted and ignored).
            "cfrac" => {
                self.skip_ws();
                if self.eat("[") {
                    let _ = self.parse_expression(Some("]"));
                    self.eat("]");
                }
                let num = self.parse_mandatory_arg();
                let den = Box::new(self.parse_mandatory_arg());
                let frac = MathNode::Fraction {
                    num: Box::new(MathNode::Row(vec![MathNode::Strut, num])),
                    den,
                    has_rule: true,
                    options: FRACTION_OPT_NONE,
                    thickness: -1.0,
                };
                Some(MathNode::Row(vec![
                    MathNode::StyleScope {
                        style: MathStyle::Display,
                        content: Box::new(frac),
                    },
                    MathNode::Space(MathSkip::Pt(-1.2)),
                ]))
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
                    "mathop" => {
                        let content = Box::new(self.parse_mandatory_arg());
                        return Some(MathNode::OpLimits { content, mode: OP_LIMITS_DISPLAY });
                    }
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
            "varlimsup" | "varliminf" | "varinjlim" | "varprojlim" => {
                let kind = match cmd {
                    "varlimsup" => 0,
                    "varliminf" => 1,
                    "varinjlim" => 2,
                    _ => 3,
                };
                let lim = Box::new(MathNode::StyleChange { face: Face::Roman, content: Box::new(parse_math_str("lim")) });
                Some(MathNode::OpLimits {
                    content: Box::new(MathNode::VarLim { kind, lim }),
                    mode: OP_LIMITS_DISPLAY | OP_LIMITS_BOXED_NUCLEUS,
                })
            }
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
                // amsmath `\smash[t]` keeps the depth, `\smash[b]` the height.
                self.skip_ws();
                let smash = if self.eat("[") {
                    let rest = self.remaining();
                    let close = rest.find(']').unwrap_or(rest.len());
                    let opt = &rest[..close];
                    self.pos += (close + 1).min(rest.len());
                    match opt.trim() {
                        "t" => 1,
                        "b" => 2,
                        _ => 3,
                    }
                } else {
                    3
                };
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox { content, zero_width: false, shift_left: false, smash })
            }
            "rlap" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox {
                    content,
                    zero_width: true,
                    shift_left: false,
                    smash: 0,
                })
            }
            "llap" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::DimensionBox {
                    content,
                    zero_width: true,
                    shift_left: true,
                    smash: 0,
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

            "jot" => Some(MathNode::Space(MathSkip::Pt(3.0))),
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

                let thickness = match thick_arg.trim() {
                    "" => -1.0,
                    t => crate::config::parse_dimension_pt(t).unwrap_or(-1.0),
                };
                let has_rule = thickness != 0.0;
                let delimited = !open_arg.is_empty() || !close_arg.is_empty();
                let mut core = MathNode::Fraction {
                    num,
                    den,
                    has_rule,
                    options: if delimited { FRACTION_OPT_DELIMITED } else { FRACTION_OPT_NONE },
                    thickness: if has_rule { thickness } else { -1.0 },
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
                    Some(genfrac_ord(MathNode::Delimited(Box::new(DelimitedData::new(
                        open_arg, close_arg, core,
                    )))))
                } else {
                    Some(core)
                }
            }

            // Radical `\sqrt[deg]{rad}`
            "sqrt" => {
                self.skip_ws();
                let (mut up, mut left) = (0i8, 0i8);
                let degree = if self.eat("[") {
                    // amsmath `\root`: `\uproot` and `\leftroot` lead the degree, in either order.
                    loop {
                        self.skip_ws();
                        let slot = if self.eat("\\uproot") {
                            &mut up
                        } else if self.eat("\\leftroot") {
                            &mut left
                        } else {
                            break;
                        };
                        self.skip_ws();
                        *slot = self.raw_brace_arg().trim().parse::<i8>().unwrap_or(0);
                    }
                    let deg = self.parse_expression(Some("]"));
                    self.eat("]");
                    Some(Box::new(deg))
                } else {
                    None
                };
                let radicand = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Radical { degree, radicand, up, left })
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
                    limits: OP_LIMITS_DISPLAY,
                })))
            }

            // Integrals (default limits to the right)
            "int" | "iint" | "iiint" | "iiiint" | "idotsint" | "oint" | "oiint" | "oiiint" => Some(MathNode::BigOp(Box::new(BigOpData {
                op: cmd,
                sub: None,
                sup: None,
                limits: OP_LIMITS_NOLIMITS,
            }))),

            // Dynamic delimiters `\left( ... \right)`
            "left" => {
                self.skip_ws();
                let open = self.parse_delim_char();
                let content = self.parse_expression(None);
                let close = if self.at_right_delimiter() && self.eat("\\right") {
                    self.skip_ws();
                    self.parse_delim_char()
                } else {
                    ")"
                };
                let mut data = DelimitedData::new(open, close, content);
                (data.factor, data.shortfall) = self.delim_params;
                Some(MathNode::Delimited(Box::new(data)))
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
            "quad" => Some(MathNode::Space(MathSkip::TextEm(1.0))),
            "qquad" => Some(MathNode::Space(MathSkip::TextEm(2.0))),
            "enspace" => Some(MathNode::Space(MathSkip::TextEm(0.5))),
            "thinspace" => Some(MathNode::Space(MathSkip::Mu(3.0))),
            "medspace" => Some(MathNode::Space(MathSkip::Mu(4.0))),
            "thickspace" => Some(MathNode::Space(MathSkip::Mu(5.0))),
            "negthinspace" => Some(MathNode::Space(MathSkip::Mu(-3.0))),
            "negmedspace" => Some(MathNode::Space(MathSkip::Mu(-4.0))),
            "negthickspace" => Some(MathNode::Space(MathSkip::Mu(-5.0))),

            // Ellipses (The TeXbook Chapter 18); `\colon`, `\bmod` and `\pmod` are in MATH_MACROS.
            // TeX mlist.c line 4557 conditional math glue primitive
            "nonscript" => {
                self.skip_ws();
                Some(MathNode::Nonscript)
            },
            // amsmath dots: `\ldots` is the bare `\mathellipsis`; `\cdots` (`\extrap@`) and
            // `\dotso`/`\dotsc` add a thin space before punctuation, closing delimiters and the
            // end of the formula; `\dots` picks centered dots before a binary or relation.
            "ldots" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Low)),
            "hdotsfor" => {
                self.skip_ws();
                let mut space = 1.0;
                if self.eat("[") {
                    let start = self.pos;
                    while !self.is_eof() && !self.remaining().starts_with(']') {
                        self.pos += 1;
                    }
                    space = self.src[start..self.pos].trim().parse().unwrap_or(1.0);
                    self.eat("]");
                    self.skip_ws();
                }
                let cols = self.raw_brace_arg().trim().parse::<u8>().unwrap_or(1).max(1);
                Some(MathNode::HDotsFor { cols, space })
            }
            "vdots" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Vertical)),
            "ddots" => Some(MathNode::Ellipsis(crate::math::math_ast::EllipsisKind::Diagonal)),
            "cdots" | "dotsb" | "dotsm" | "dotsi" | "dotso" | "dotsc" | "dots" => {
                use crate::math::math_ast::EllipsisKind;
                let next = self.dots_lookahead();
                // amsmath: `\dotsb` and `\dotsm` are `\cdots`, whose `\extrap@` adds `\,`
                // before punctuation, a closing delimiter or the end of the formula;
                // `\dotsi` is `\!\@cdots`.
                if cmd == "dotsi" {
                    return Some(MathNode::Row(vec![
                        MathNode::Space(MathSkip::Mu(-3.0)),
                        MathNode::Ellipsis(EllipsisKind::Centered),
                    ]));
                }
                let (kind, thin) = match cmd {
                    "cdots" | "dotsb" | "dotsm" => {
                        (EllipsisKind::Centered, next != DotsNext::Other && next != DotsNext::BinRel)
                    }
                    "dotsc" => (EllipsisKind::Low, matches!(next, DotsNext::SemiOrPeriod | DotsNext::Closing)),
                    "dotso" => (EllipsisKind::Low, next == DotsNext::Closing),
                    _ => match next {
                        DotsNext::BinRel => (EllipsisKind::Centered, false),
                        DotsNext::Comma => (EllipsisKind::Low, false),
                        _ => (EllipsisKind::Low, next == DotsNext::Closing),
                    },
                };
                let dots = MathNode::Ellipsis(kind);
                Some(if thin {
                    MathNode::Row(vec![dots, MathNode::Space(MathSkip::Mu(3.0))])
                } else {
                    dots
                })
            }

            // Accents (mlist.c top, bottom, and overlay accents)
            "vec" | "hat" | "dot" | "ddot" | "dddot" | "ddddot" | "bar" | "tilde" | "check"
            | "breve" | "acute" | "grave" | "mathring" | "overline" | "underline" | "widehat"
            | "widetilde" | "overrightarrow" | "overleftarrow" | "overleftrightarrow"
            | "underrightarrow" | "underleftarrow" | "underleftrightarrow"
            | "utilde" | "underbar" | "not" | "cancel" => {
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent {
                    accent: cmd,
                    nucleus,
                })
            }
            // amsmath's capitalised accents are the lowercase ones (`\def\Hat{\hat}`).
            "Hat" | "Check" | "Tilde" | "Acute" | "Grave" | "Dot" | "Ddot" | "Breve" | "Bar" | "Vec" => {
                let accent = match cmd {
                    "Hat" => "hat",
                    "Check" => "check",
                    "Tilde" => "tilde",
                    "Acute" => "acute",
                    "Grave" => "grave",
                    "Dot" => "dot",
                    "Ddot" => "ddot",
                    "Breve" => "breve",
                    "Bar" => "bar",
                    _ => "vec",
                };
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent { accent, nucleus })
            }
            "underaccent" => {
                // The first argument names the accent glyph (`{\tilde}`); it must not be
                // parsed as an accent command, which would consume the following input.
                let accent = match self.parse_raw_group().trim().trim_start_matches('\\') {
                    "bar" | "underbar" => "underbar",
                    _ => "utilde",
                };
                let nucleus = Box::new(self.parse_mandatory_arg());
                Some(MathNode::Accent { accent, nucleus })
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
            // amsmath `\text` keeps the current text font.
            "text" => {
                let node = self.parse_text_arg();
                Some(match node {
                    MathNode::StyleChange { face: Face::Roman, content } => MathNode::TextFont(content),
                    MathNode::Row(parts) => MathNode::Row(
                        parts
                            .into_iter()
                            .map(|p| match p {
                                MathNode::StyleChange { face: Face::Roman, content } => MathNode::TextFont(content),
                                other => other,
                            })
                            .collect(),
                    ),
                    other => other,
                })
            }
            "mathrm" => {
                let content = Box::new(join_ligature_letters(self.parse_mandatory_arg()));
                Some(MathNode::StyleChange {
                    face: Face::Roman,
                    content,
                })
            }
            "mathbf" => {
                let content = Box::new(join_ligature_letters(self.parse_mandatory_arg()));
                Some(MathNode::StyleChange {
                    face: Face::Bold,
                    content,
                })
            }
            "pmb" => Some(MathNode::Pmb(Box::new(self.parse_mandatory_arg()))),
            "boldsymbol" | "bm" => {
                let content = Box::new(self.parse_mandatory_arg());
                Some(MathNode::StyleChange {
                    face: Face::BoldItalic,
                    content,
                })
            }
            "mathit" => {
                let content = Box::new(join_ligature_letters(self.parse_mandatory_arg()));
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
                    "subarray" => {
                        let align = match self.parse_ident_arg() {
                            "l" => ColAlign::Left,
                            "r" => ColAlign::Right,
                            _ => ColAlign::Center,
                        };
                        (MatrixDelim::None, MatrixEnvKind::Subarray, vec![align])
                    }
                    "cases" => (
                        MatrixDelim::LeftBrace,
                        MatrixEnvKind::Cases,
                        vec![ColAlign::Left, ColAlign::Left],
                    ),
                    "alignedat" | "alignat" | "alignat*" => {
                        // An undelimited macro argument: a group or one token (`alignat*}3`).
                        self.skip_ws();
                        if self.src[self.pos..].starts_with('{') {
                            let _ = self.parse_ident_arg();
                        } else if let Some(ch) = self.peek_char() {
                            self.pos += ch.len_utf8();
                        }
                        (
                            MatrixDelim::None,
                            MatrixEnvKind::AlignedAt,
                            [ColAlign::Right, ColAlign::Left].repeat(4),
                        )
                    }
                    "aligned" | "align" | "align*" | "split" => (
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
                        (MatrixDelim::None, MatrixEnvKind::Array, aligns)
                    }
                    _ => (MatrixDelim::None, MatrixEnvKind::Matrix, Vec::new()),
                };
                let matrix_node = self.parse_matrix_body(env, delim, env_kind, col_alignments);
                self.split_in_cell = env == "split" && self.env_depth >= 1;
                if self.env_depth >= 1 && matches!(env, "align" | "align*" | "alignat" | "alignat*" | "flalign" | "flalign*") {
                    // Starred, `\align@` clears `\@eqnswtrue` globally: the outer row goes
                    // unnumbered too.
                    if env.ends_with('*') {
                        self.notag_seen = true;
                    }
                    return Some(MathNode::InnerAlign(Box::new(matrix_node)));
                }
                Some(matrix_node)
            }

            // Greek letters and STEM symbols
            _ => {
                let mathcode = crate::math::math_mathchar::lookup_mathchar(cmd).unwrap_or(0);
                if let Some(sym) = lookup_math_command(cmd) {
                    Some(MathNode::Symbol {
                        kind: if mathcode != 0 { mathcode_class_kind(mathcode) } else { sym.kind },
                        face: sym.face,
                        glyph: sym.glyph,
                        code: sym.code,
                        mathcode,
                    })
                } else if mathcode != 0 {
                    // Declared by LaTeX but absent from the Base-14 table: only Computer
                    // Modern can draw it, so the command name is kept as the fallback glyph.
                    Some(MathNode::Symbol {
                        kind: mathcode_class_kind(mathcode),
                        face: Face::Symbol,
                        glyph: cmd,
                        code: None,
                        mathcode,
                    })
                } else {
                    // Unknown command: fallback to Roman text
                    Some(MathNode::Symbol {
                        kind: MathAtomKind::Ord,
                        face: Face::Roman,
                        glyph: cmd,
                        code: None,
                        mathcode: 0,
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
        } else if self.eat("\\vert") || self.eat("\\lvert") || self.eat("\\rvert") {
            "|"
        } else if self.eat("\\Vert") || self.eat("\\lVert") || self.eat("\\rVert") {
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
        } else if let Some(atom) = self.parse_arg_atom() {
            atom
        } else {
            MathNode::Row(Vec::new())
        }
    }

    /// Parses a `\text{...}` argument, preserving literal whitespace as inter-word spacing.
    /// Returns the raw text of a `{...}` group (or a single control word) without parsing it.
    fn parse_raw_group(&mut self) -> &'a str {
        self.skip_ws();
        if !self.eat("{") {
            let start = self.pos;
            if self.eat("\\") {
                while matches!(self.peek_char(), Some(c) if c.is_ascii_alphabetic()) {
                    self.pos += 1;
                }
            } else if let Some(ch) = self.peek_char() {
                self.pos += ch.len_utf8();
            }
            return &self.src[start..self.pos];
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
        raw
    }

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

        // `$...$` inside the text is a formula of its own; the text around it is in roman.
        let mut parts = Vec::new();
        let mut rest = raw;
        while let Some(open) = unescaped_dollar(rest) {
            let after = &rest[open + 1..];
            let Some(close) = unescaped_dollar(after) else { break };
            if open > 0 {
                parts.push(text_words(&rest[..open]));
            }
            let mut inner = MathParser::new(&after[..close]);
            inner.colors = self.colors.clone();
            parts.push(MathNode::Formula(Box::new(inner.parse_expression(None))));
            rest = &after[close + 1..];
        }
        if parts.is_empty() {
            return text_words(raw);
        }
        if !rest.is_empty() {
            parts.push(text_words(rest));
        }
        MathNode::Row(parts)
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

    /// Classifies the token after a dots command the way amsmath's `\\mdots@`, `\\extrap@`
    /// and `\\rightdelim@` do.
    fn dots_lookahead(&self) -> DotsNext {
        let rem = self.remaining().trim_start();
        let Some(c) = rem.chars().next() else {
            return DotsNext::Closing;
        };
        match c {
            ',' => return DotsNext::Comma,
            ';' | '.' => return DotsNext::SemiOrPeriod,
            ')' | ']' => return DotsNext::Closing,
            // `&` ends an alignment cell: TeX inserts the v-template, which starts with the
            // cell's closing `$` (`\extra@`'s end-of-formula case).
            '&' => return DotsNext::Closing,
            '+' | '=' | '<' | '>' | '-' | '*' | ':' => return DotsNext::BinRel,
            '\\' => {}
            _ => return DotsNext::Other,
        }
        let name: &str = {
            let body = &rem[1..];
            let end = body.find(|ch: char| !ch.is_ascii_alphabetic()).unwrap_or(body.len());
            if end == 0 { &body[..body.chars().next().map_or(0, char::len_utf8)] } else { &body[..end] }
        };
        if matches!(
            name,
            "}" | "rbrack" | "rbrace" | "rangle" | "rceil" | "rfloor" | "rgroup" | "rmoustache"
                | "right" | "bigr" | "Bigr" | "biggr" | "Biggr"
        ) {
            return DotsNext::Closing;
        }
        if name == "not" {
            return DotsNext::BinRel;
        }
        let class = crate::math::math_mathchar::lookup_mathchar(name)
            .map(mathcode_class_kind)
            .or_else(|| lookup_math_command(name).map(|s| s.kind));
        if matches!(class, Some(MathAtomKind::Bin | MathAtomKind::Rel)) {
            DotsNext::BinRel
        } else {
            DotsNext::Other
        }
    }

    /// Whether the input is at `\\right` itself, not a longer control word such as `\\rightarrow`.
    fn at_right_delimiter(&self) -> bool {
        let rem = self.remaining();
        rem.starts_with("\\right") && !rem[6..].starts_with(|c: char| c.is_ascii_alphabetic())
    }

    /// Expands a [`MATH_MACROS`] body; `#1` is replaced by one mandatory argument.
    fn expand_math_macro(&mut self, body: &'static str) -> MathNode<'a> {
        let Some((pre, post)) = body.split_once("#1") else {
            return MathParser::new(body).parse_expression(None);
        };
        let arg = self.parse_mandatory_arg();
        let mut nodes = Vec::with_capacity(8);
        for part in [MathParser::new(pre).parse_expression(None), arg, MathParser::new(post).parse_expression(None)] {
            match part {
                MathNode::Row(items) => nodes.extend(items),
                other => nodes.push(other),
            }
        }
        MathNode::Row(nodes)
    }

    /// Scans a TeX integer: decimal, `"` hexadecimal, `'` octal or `` ` `` character code.
    fn scan_int(&mut self) -> u32 {
        self.skip_ws();
        let (radix, skip) = match self.peek_char() {
            Some('"') => (16, 1),
            Some('\'') => (8, 1),
            Some('`') => {
                self.pos += 1;
                if self.remaining().starts_with('\\') {
                    self.pos += 1;
                }
                let ch = self.peek_char().unwrap_or('\0');
                self.pos += ch.len_utf8();
                return ch as u32;
            }
            _ => (10, 0),
        };
        self.pos += skip;
        let start = self.pos;
        while self.peek_char().is_some_and(|c| c.is_digit(radix)) {
            self.pos += 1;
        }
        u32::from_str_radix(&self.src[start..self.pos], radix).unwrap_or(0)
    }

    /// Scans a TeX skip or kern (`-3mu`, `2pt`, `.5em`, `-\thinmuskip`), ignoring any
    /// `plus`/`minus` stretch, and returns it in its unit.
    fn scan_skip(&mut self) -> MathSkip {
        self.skip_ws();
        let mut sign = 1.0f32;
        while let Some(c) = self.peek_char() {
            match c {
                '-' => sign = -sign,
                '+' => {}
                _ if c.is_ascii_whitespace() => {}
                _ => break,
            }
            self.pos += 1;
        }
        let skip = if self.eat("\\thinmuskip") {
            MathSkip::Mu(3.0 * sign)
        } else if self.eat("\\medmuskip") {
            MathSkip::Mu(4.0 * sign)
        } else if self.eat("\\thickmuskip") {
            MathSkip::Mu(5.0 * sign)
        } else {
            let start = self.pos;
            while self.peek_char().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == ',') {
                self.pos += 1;
            }
            let v = self.src[start..self.pos].replace(',', ".").parse::<f32>().unwrap_or(1.0) * sign;
            self.skip_ws();
            self.scan_skip_unit(v)
        };
        let mut skip = skip;
        for keyword in ["plus", "minus"] {
            self.skip_ws();
            if self.eat(keyword) {
                self.skip_ws();
                let start = self.pos;
                let v = self.scan_skip_component();
                // Finite mu stretch can stretch a paragraph line; other parts are not kept.
                if let (MathSkip::Mu(mu), "plus", true) = (skip, keyword, self.src[start..self.pos].trim_end().ends_with("mu")) {
                    skip = MathSkip::MuGlue(mu, v);
                }
            }
        }
        skip
    }

    /// Consumes the number and unit of a stretch/shrink component (`1fil`, `2mu`).
    fn scan_skip_component(&mut self) -> f32 {
        let start = self.pos;
        while self.peek_char().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-') {
            self.pos += 1;
        }
        let v = self.src[start..self.pos].parse::<f32>().unwrap_or(1.0);
        self.skip_ws();
        while self.peek_char().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        v
    }

    fn scan_skip_unit(&mut self, v: f32) -> MathSkip {
        const UNITS: [(&str, f32); 8] = [
            ("pt", 1.0),
            ("bp", 72.27 / 72.0),
            ("mm", 72.27 / 25.4),
            ("cm", 72.27 / 2.54),
            ("in", 72.27),
            ("pc", 12.0),
            ("dd", 1238.0 / 1157.0),
            ("sp", 1.0 / 65536.0),
        ];
        if self.eat("mu") {
            return MathSkip::Mu(v);
        }
        if self.eat("em") {
            return MathSkip::TextEm(v);
        }
        if self.eat("ex") {
            // cmr10 x-height is 0.430555 em.
            return MathSkip::TextEm(v * 0.430555);
        }
        for (unit, scale) in UNITS {
            if self.eat(unit) {
                return MathSkip::Pt(v * scale);
            }
        }
        MathSkip::Pt(v)
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
        self.env_depth = self.env_depth.saturating_add(1);
        let node = self.parse_matrix_rows(env_name, delim, env_kind, col_alignments);
        self.env_depth -= 1;
        node
    }

    fn parse_matrix_rows(
        &mut self,
        env_name: Option<&str>,
        delim: MatrixDelim,
        env_kind: MatrixEnvKind,
        col_alignments: Vec<ColAlign>,
    ) -> MathNode<'a> {
        let mut rows = Vec::with_capacity(4);
        // amsmath numbers every row of `align`, `alignat` and `gather` unless `\notag`.
        let numbered_env = matches!(env_name, Some("align" | "alignat" | "gather"));
        let display_env =
            numbered_env || matches!(env_name, Some("align*" | "alignat*" | "gather*" | "multline" | "multline*"));
        let mut notag_mask = 0u64;
        let mut row_skips: Vec<f32> = Vec::new();

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
            let outer_notag = std::mem::replace(&mut self.notag_seen, false);
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
                self.split_in_cell = false;
                let mut cell = self.parse_cell_expression_opt(env_name);
                // amsmath `\rendsplit@` under `\ifinalign@` (the `align` family; centertags):
                // a `split` alone in a cell spans it and the next.
                let mut split_tail = None;
                if std::mem::take(&mut self.split_in_cell)
                    && col_idx.is_multiple_of(2)
                    && matches!(env_name, Some("align" | "align*" | "alignat" | "alignat*" | "flalign" | "flalign*"))
                {
                    // A `\tag` after it stays with the row: it goes on the second cell.
                    let (inner, tag) = match std::mem::replace(&mut cell, MathNode::Row(Vec::new())) {
                        MathNode::Tagged { formula, tag, is_left, starred } => (*formula, Some((tag, is_left, starred))),
                        other => (other, None),
                    };
                    let retag = |n: MathNode<'a>, tag: Option<(Box<MathNode<'a>>, bool, bool)>| match tag {
                        Some((tag, is_left, starred)) => MathNode::Tagged { formula: Box::new(n), tag, is_left, starred },
                        None => n,
                    };
                    let lone = match inner {
                        MathNode::Row(mut v) if v.len() == 1 && matches!(v[0], MathNode::Matrix(_)) => v.pop().ok_or(MathNode::Row(Vec::new())),
                        m @ MathNode::Matrix(_) => Ok(m),
                        other => Err(other),
                    };
                    match lone {
                        Ok(m) => {
                            cell = MathNode::SplitLead(Box::new(m.clone()));
                            split_tail = Some(retag(MathNode::SplitTail(Box::new(m)), tag));
                        }
                        Err(other) => cell = retag(other, tag),
                    }
                }

                // AMS Parity: prepend empty Ord atom `{}` for operator spacing:
                // 1. In `aligned` odd columns (col 1, 3, 5...), LaTeX uses `${}#$`: the empty
                //    Ord spaces whatever atom starts the cell (Rel, Bin, Op, Inner).
                // 2. Every `multline` line is `$\displaystyle{}#$` (`\multline@`): a leading
                //    Op gets a thin space, a Bin stays binary.
                let needs_phantom_ord = match env_kind {
                    MatrixEnvKind::Aligned | MatrixEnvKind::AlignedAt => col_idx % 2 == 1,
                    MatrixEnvKind::Multline => true,
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

                // `\hdotsfor` is a `\multicolumn`: the columns it spans hold empty cells.
                let span = match &cell {
                    MathNode::HDotsFor { cols, .. } => *cols,
                    MathNode::Row(v) => match v.as_slice() {
                        [MathNode::HDotsFor { cols, .. }] => *cols,
                        _ => 1,
                    },
                    _ => 1,
                };
                row.push(cell);
                col_idx += 1;
                if let Some(tail) = split_tail {
                    row.push(tail);
                    col_idx += 1;
                }
                for _ in 1..span {
                    row.push(MathNode::Row(Vec::new()));
                    col_idx += 1;
                }
                self.skip_ws();
                if self.eat("&") {
                    continue;
                }
                if self.eat("\\\\") {
                    // amsmath `\math@cr`: `\\*` forbids a break; `[dim]` adds
                    // `\noalign{\vskip dim}` below the row.
                    self.eat("*");
                    let rest = self.remaining().trim_start();
                    if let Some(arg) = rest.strip_prefix('[') {
                        if let Some(end) = arg.find(']') {
                            if let Some(dim) = crate::doc_config::parse_dimension_pt(arg[..end].trim()) {
                                row_skips.resize(rows.len() + 1, 0.0);
                                row_skips[rows.len()] = dim;
                            }
                            self.pos = self.src.len() - arg.len() + end + 1;
                        }
                    }
                } else if !self.eat("\\crcr") {
                    self.eat("\\cr");
                }
                break;
            }
            let row_notag = std::mem::replace(&mut self.notag_seen, outer_notag);
            if !row.is_empty() {
                if row_notag && rows.len() < 64 {
                    notag_mask |= 1 << rows.len();
                }
                rows.push(row);
            }
        }

        MathNode::Matrix(Box::new(MatrixData {
            rows,
            delim,
            env_kind,
            col_alignments: col_alignments.into_boxed_slice(),
            numbered_env,
            display_env,
            notag_mask,
            // Inside another alignment's cell (`\ifinner`) amsmath's `split` is
            // `\split@aligned`: an `aligned`.
            split: env_name == Some("split") && self.env_depth == 1,
            extras: (!row_skips.is_empty() || self.multline_gap != 10.0)
                .then(|| Box::new(MatrixExtras { row_skips, multline_gap: self.multline_gap })),
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

/// [`parse_math_str`] with the document's `\definecolor` names in scope.
pub fn parse_math_str_with_colors<'a>(
    src: &'a str,
    colors: &[(&'a str, PackedColor)],
    params: &crate::math::MathParams,
) -> MathNode<'a> {
    let mut parser = MathParser::new(src);
    parser.colors.extend_from_slice(colors);
    parser.delim_params = params.delim;
    parser.multline_gap = params.multline_gap;
    parser.parse_expression(None)
}

/// Recursively transforms ASCII letters into canonical Unicode Mathematical Sans-Serif symbols.
/// In a text font, adjacent ordinary letters form the font's f-ligatures (TeX §752), so runs
/// spelling ff, fi, fl, ffi or ffl become one word that the layout shapes.
fn join_ligature_letters<'a>(node: MathNode<'a>) -> MathNode<'a> {
    let MathNode::Row(items) = node else { return node };
    let letter = |n: &MathNode| match n {
        MathNode::Variable(c) => Some(*c),
        _ => None,
    };
    let mut out = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        let w = &crate::math::math_ast::MATH_LIGATURE_WORDS;
        let word: Option<&'static str> = match (letter(&items[i]), items.get(i + 1).and_then(letter), items.get(i + 2).and_then(letter)) {
            (Some('f'), Some('f'), Some('i')) => Some(w[0]),
            (Some('f'), Some('f'), Some('l')) => Some(w[1]),
            (Some('f'), Some('f'), _) => Some(w[2]),
            (Some('f'), Some('i'), _) => Some(w[3]),
            (Some('f'), Some('l'), _) => Some(w[4]),
            _ => None,
        };
        match word {
            Some(w) => {
                out.push(MathNode::Symbol { kind: MathAtomKind::Ord, face: Face::Roman, glyph: w, code: None, mathcode: 0 });
                i += w.len();
            }
            None => {
                out.push(items[i].clone());
                i += 1;
            }
        }
    }
    MathNode::Row(out)
}

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

/// Atom class of a TeX math code (`\\mathchar` class digit; class 7 is a variable-family Ord).
#[inline]
fn mathcode_class_kind(mathcode: u16) -> MathAtomKind {
    match mathcode >> 12 {
        1 => MathAtomKind::Op,
        2 => MathAtomKind::Bin,
        3 => MathAtomKind::Rel,
        4 => MathAtomKind::Open,
        5 => MathAtomKind::Close,
        6 => MathAtomKind::Punct,
        _ => MathAtomKind::Ord,
    }
}

/// Composite math commands from `fontmath.ltx` and `amsmath.sty`, restated with the
/// primitives this parser implements (`#1` is the single argument). Sorted by name.
static MATH_MACROS: [(&str, &str); 29] = [
    ("Longleftarrow", r"\Leftarrow\joinrel\Relbar"),
    ("Longleftrightarrow", r"\Leftarrow\joinrel\Rightarrow"),
    ("Longrightarrow", r"\Relbar\joinrel\Rightarrow"),
    ("Relbar", r"\mathrel{=}"),
    ("bmod", r"\nonscript\mskip-\medmuskip\mkern5mu\mathbin{\mathrm{mod}}\mkern5mu\nonscript\mskip-\medmuskip"),
    ("colon", r"\mskip2mu\mathpunct{}\nonscript\mkern-\thinmuskip\mathord{:}\mskip6mu plus1mu"),
    ("hookleftarrow", r"\leftarrow\joinrel\rhook"),
    ("hookrightarrow", r"\lhook\joinrel\rightarrow"),
    ("iff", r"\;\Longleftrightarrow\;"),
    ("impliedby", r"\;\Longleftarrow\;"),
    ("implies", r"\;\Longrightarrow\;"),
    ("joinrel", r"\mathrel{\mkern-3mu}"),
    ("lVert", r"\mathopen{\|}"),
    ("leadsto", r"\rightsquigarrow"),
    ("longleftarrow", r"\leftarrow\joinrel\relbar"),
    ("longleftrightarrow", r"\leftarrow\joinrel\rightarrow"),
    ("longmapsto", r"\mapstochar\longrightarrow"),
    ("longrightarrow", r"\relbar\joinrel\rightarrow"),
    ("lvert", r"\mathopen{|}"),
    ("mapsto", r"\mapstochar\rightarrow"),
    ("mathstrut", r"\vphantom("),
    ("mod", r"\mathchoice{\mkern18mu}{\mkern12mu}{\mkern12mu}{\mkern12mu}\mathord{\mathrm{mod}}\,\,#1"),
    ("ne", r"\not="),
    ("neq", r"\not="),
    ("pmod", r"\mathchoice{\mkern18mu}{\mkern8mu}{\mkern8mu}{\mkern8mu}(\mathord{\mathrm{mod}}\mkern6mu#1)"),
    ("pod", r"\mathchoice{\mkern18mu}{\mkern8mu}{\mkern8mu}{\mkern8mu}(#1)"),
    ("rVert", r"\mathclose{\|}"),
    ("relbar", r"\mathrel{\smash{-}}"),
    ("rvert", r"\mathclose{|}"),
];

#[inline]
fn lookup_math_macro(cmd: &str) -> Option<&'static str> {
    MATH_MACROS
        .binary_search_by(|(n, _)| n.as_bytes().cmp(cmd.as_bytes()))
        .ok()
        .map(|i| MATH_MACROS[i].1)
}

/// What follows a dots command, as far as amsmath's spacing rules care.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DotsNext {
    Comma,
    SemiOrPeriod,
    Closing,
    BinRel,
    Other,
}

/// amsmath's `\genfrac` (and `\binom`, `\dbinom`, `\tbinom` built on it) is
/// `{\begingroup..\endgroup\@@atopwithdelims..}`: the braces make the Inner fraction an Ord.
fn genfrac_ord(node: MathNode<'_>) -> MathNode<'_> {
    MathNode::Coerce {
        kind: MathAtomKind::Ord,
        content: Box::new(node),
    }
}

/// The words of `\text{...}` source in roman, separated by interword spaces; a space
/// after a sentence end (space factor 2000 or more) gets the extra space.
/// The words of a `\text` argument: groups, the NFSS font switches (`\normalfont`,
/// `\ttfamily`, `\bfseries`, `\itshape`, and `\texttt`-style commands on the next group),
/// `\char`, escaped specials, and interword spaces by the space factor. Each run of one
/// face is a `StyleChange`, so its spaces are that font's.
fn text_words<'s>(raw: &'s str) -> MathNode<'s> {
    fn push<'s>(runs: &mut Vec<(Face, Vec<MathNode<'s>>)>, face: Face, node: MathNode<'s>) {
        match runs.last_mut() {
            Some((f, nodes)) if *f == face => nodes.push(node),
            _ => runs.push((face, vec![node])),
        }
    }
    let bytes = raw.as_bytes();
    let mut runs: Vec<(Face, Vec<MathNode<'s>>)> = Vec::new();
    let word = |glyph| MathNode::Symbol { kind: MathAtomKind::Ord, face: Face::Roman, glyph, code: None, mathcode: 0 };
    let (mut face, mut stack, mut pending) = (Face::Roman, Vec::new(), None);
    let mut sf = 1000u16;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                stack.push(face);
                face = pending.take().unwrap_or(face);
                i += 1;
            }
            b'}' => {
                face = stack.pop().unwrap_or(Face::Roman);
                i += 1;
            }
            b if b.is_ascii_whitespace() => {
                push(&mut runs, face, MathNode::Space(if sf >= 2000 { MathSkip::TextSpaceExtra } else { MathSkip::TextSpace }));
                sf = 1000;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            b'\\' => {
                let name_start = i + 1;
                let mut end = name_start;
                while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
                    end += 1;
                }
                if end == name_start {
                    // A control symbol: `\ ` is a space, `\%` and the like print the character.
                    let Some(c) = raw[name_start..].chars().next() else { break };
                    i = name_start + c.len_utf8();
                    if c.is_ascii_whitespace() {
                        push(&mut runs, face, MathNode::Space(MathSkip::TextSpace));
                        sf = 1000;
                    } else if c.is_ascii_punctuation() && !matches!(c, ',' | ';' | ':' | '!' | '/') {
                        push(&mut runs, face, word(&raw[name_start..i]));
                        sf = 1000;
                    }
                    continue;
                }
                let name = &raw[name_start..end];
                i = end;
                if name == "char" {
                    let digits = raw[i..].trim_start();
                    i = bytes.len() - digits.len();
                    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
                    if let Ok(code) = digits[..n].parse::<u8>() {
                        if let Some(glyph) = ascii_glyph(code) {
                            push(&mut runs, face, word(glyph));
                            sf = 1000;
                        }
                    }
                    i += n;
                } else {
                    let switch = match name {
                        "normalfont" | "rmfamily" | "upshape" | "mdseries" | "textrm" | "textup" | "textnormal" | "textmd" => Some(Face::Roman),
                        "ttfamily" | "texttt" => Some(Face::Mono),
                        "bfseries" | "textbf" => Some(Face::Bold),
                        "itshape" | "textit" | "em" | "emph" => Some(Face::Italic),
                        "slshape" | "textsl" => Some(Face::Slanted),
                        _ => None,
                    };
                    match switch {
                        Some(f) if name.starts_with("text") || name == "emph" => pending = Some(f),
                        Some(f) => face = f,
                        None => {}
                    }
                }
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
            _ => {
                let w_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'{' | b'}' | b'\\') {
                    sf = crate::layout::layout_hz::update_space_factor(sf, bytes[i], false);
                    i += 1;
                }
                push(&mut runs, face, word(&raw[w_start..i]));
            }
        }
    }
    let mut parts: Vec<MathNode<'s>> = runs
        .into_iter()
        .map(|(face, mut nodes)| {
            for n in &mut nodes {
                if let MathNode::Symbol { face: f, .. } = n {
                    *f = face;
                }
            }
            let content = if nodes.len() == 1 { nodes.pop().unwrap() } else { MathNode::Row(nodes) };
            MathNode::StyleChange { face, content: Box::new(content) }
        })
        .collect();
    match parts.len() {
        0 => MathNode::StyleChange { face: Face::Roman, content: Box::new(MathNode::Row(Vec::new())) },
        1 => parts.pop().unwrap(),
        _ => MathNode::Row(parts),
    }
}

/// The printable ASCII character `code` as a static one-character string.
fn ascii_glyph(code: u8) -> Option<&'static str> {
    const ASCII: &str = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
    (32..127).contains(&code).then(|| &ASCII[(code - 32) as usize..(code - 31) as usize])
}

/// The byte offset of the first `$` in `s` not escaped as `\$`.
fn unescaped_dollar(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    (0..b.len()).find(|&i| b[i] == b'$' && (i == 0 || b[i - 1] != b'\\'))
}

#[cfg(test)]
mod tests {
    use super::MATH_MACROS;

    #[test]
    fn math_macros_are_sorted_for_binary_search() {
        for pair in MATH_MACROS.windows(2) {
            assert!(pair[0].0.as_bytes() < pair[1].0.as_bytes(), "{} must sort before {}", pair[0].0, pair[1].0);
        }
    }
}
