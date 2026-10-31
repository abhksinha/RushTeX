//! Macro definition and macro calls: tex.web §473–§479 `scan_toks` and §389–§400
//! `macro_call`, in our own form. A macro's text is its parameter text, `end_match` and its
//! body as one list in the engine's token arena; arguments are gathered in a reused buffer
//! and handed to the input stack's parameter stack.

use super::engine_catcode::Cat;
use super::engine_prim::{Macro, LONG};
use super::engine_run::Machine;
use super::engine_token::{Span, Token};

/// A `match` or `end_match` token: the end of a delimiter in the parameter text.
#[inline]
fn is_match_like(t: Token) -> bool {
    t.cs_id().is_none() && matches!(t.cat(), Some(Cat::Ignored | Cat::Comment))
}

#[inline]
fn cat_of(t: Token) -> Option<Cat> {
    if t.cs_id().is_some() {
        None
    } else {
        t.cat()
    }
}

impl Machine<'_, '_> {
    /// §473 `scan_toks(true, expand)`: a macro's parameter text and body, after the name.
    /// With `expand` (`\edef`, `\xdef`), the body is expanded as it is read (§478), except
    /// `\protected` macros and tokens after `\noexpand`. Returns the stored text.
    pub(super) fn scan_macro_text(&mut self, expand: bool) -> Option<Span> {
        let mut def = std::mem::take(&mut self.def);
        def.clear();
        // §474: the parameter text, up to the body's left brace.
        let mut params = 0u8;
        let mut hash_brace = None;
        loop {
            let Some(t) = self.get_token() else {
                self.def = def;
                return None;
            };
            match cat_of(t) {
                Some(Cat::BeginGroup) => break,
                Some(Cat::EndGroup) => {
                    self.error("Missing { inserted");
                    self.def = def;
                    return None;
                }
                Some(Cat::Param) => {
                    // §476: `#{` ends the parameter text and puts `{` after the body;
                    // `#k` must count up from 1.
                    let Some(u) = self.get_token() else { break };
                    if cat_of(u) == Some(Cat::BeginGroup) {
                        // The brace is the last delimiter and is put back after the body.
                        hash_brace = Some(u);
                        def.push(u);
                        break;
                    }
                    let want = char::from(b'1' + params);
                    if params >= 9 {
                        self.error("You already have nine parameters");
                    } else if u.chr() != want || cat_of(u).is_none() {
                        self.error("Parameters must be numbered consecutively");
                        self.input.back(u);
                    }
                    params += 1;
                    def.push(Token::match_param(t.chr()));
                }
                _ => def.push(t),
            }
        }
        def.push(Token::END_MATCH);
        // §477: the body, to the matching right brace.
        let mut depth = 1u32;
        loop {
            let t = if expand {
                match self.get_x_for_text(&mut def) {
                    Some(t) => t,
                    None if self.input_ended() => break,
                    None => continue,
                }
            } else {
                match self.get_token() {
                    Some(t) => t,
                    None => break,
                }
            };
            match cat_of(t) {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Some(Cat::Param) => {
                    // §479: `##` is one parameter character; `#k` is parameter k.
                    let u = if expand { self.get_x_token(true).map(|x| x.t) } else { self.get_token() };
                    let Some(u) = u else { break };
                    if cat_of(u) == Some(Cat::Param) {
                        def.push(u);
                        continue;
                    }
                    let k = u.chr().to_digit(10).filter(|&k| cat_of(u).is_some() && k >= 1 && k <= params as u32);
                    match k {
                        Some(k) => def.push(Token::out_param(k as u8)),
                        None => {
                            self.error("Illegal parameter number in definition");
                            self.input.back(u);
                            def.push(t);
                        }
                    }
                    continue;
                }
                _ => {}
            }
            def.push(t);
        }
        if let Some(b) = hash_brace {
            def.push(b);
        }
        let span = self.e.toks.store(&def);
        self.def = def;
        Some(span)
    }

    /// §478: the next token of an expanded text (`\edef`, `\message`, `\write`): macros and
    /// expandable primitives are expanded, except `\protected` macros and after
    /// `\noexpand`; `\the` (and e-TeX's `\unexpanded`) put their tokens straight into
    /// `out` without expanding them further, and `None` is returned for that step.
    pub(super) fn get_x_for_text(&mut self, out: &mut Vec<Token>) -> Option<Token> {
        use super::engine_prim::{Meaning, Prim, PROTECTED};
        // §477 `scan_toks` with `xpand`: one expansion at a time, so a `\the` or
        // `\unexpanded` that an expansion produces is caught here too.
        loop {
            let t = self.get_token()?;
            match self.meaning(t) {
                Meaning::Prim(Prim::The) => {
                    self.the_toks(out);
                    return None;
                }
                Meaning::Prim(Prim::Unexpanded) => {
                    self.unexpanded(out);
                    return None;
                }
                Meaning::Macro(mac) if mac.flags & PROTECTED == 0 => self.macro_call(t, mac),
                Meaning::Prim(Prim::NoExpand) => return self.get_token(),
                Meaning::Prim(p) if p.expandable() => self.expand(p, t),
                Meaning::Native(i) if super::engine_native::native_expands(i) => self.expand_native(i),
                _ => return Some(t),
            }
        }
    }

    /// Whether the input is exhausted (no more tokens anywhere).
    pub(super) fn input_ended(&mut self) -> bool {
        match self.get_token() {
            Some(t) => {
                self.back_read(t);
                false
            }
            None => true,
        }
    }

    /// e-TeX `\unexpanded<general text>`: the text's tokens, unexpanded, into `out`.
    pub(super) fn unexpanded(&mut self, out: &mut Vec<Token>) {
        let span = self.scan_braced_text(false);
        out.extend_from_slice(self.e.toks.get(span));
    }

    /// §473 `scan_toks(false, expand)` after a left brace that may still be ahead: a general
    /// text (no parameters) to its matching right brace, stored in the token arena.
    pub(super) fn scan_braced_text(&mut self, expand: bool) -> Span {
        match self.next_non_blank_non_relax() {
            Some(x) if x.t.cs_id().is_none() && x.t.cat() == Some(Cat::BeginGroup) => {}
            Some(x) => {
                self.error("Missing { inserted");
                self.input.back(x.t);
            }
            None => return Span::default(),
        }
        self.scan_general_text(expand)
    }

    /// The general text after its left brace, to the matching right brace.
    pub(super) fn scan_general_text(&mut self, expand: bool) -> Span {
        let mut buf = std::mem::take(&mut self.def);
        buf.clear();
        let mut depth = 1u32;
        loop {
            let t = if expand {
                match self.get_x_for_text(&mut buf) {
                    Some(t) => t,
                    None if self.input_ended() => break,
                    None => continue,
                }
            } else {
                match self.get_token() {
                    Some(t) => t,
                    None => break,
                }
            };
            match cat_of(t) {
                Some(Cat::BeginGroup) => depth += 1,
                Some(Cat::EndGroup) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            buf.push(t);
        }
        let span = self.e.toks.store(&buf);
        self.def = buf;
        span
    }

    /// §389 `macro_call`: reads the arguments of `name` (meaning `mac`) and feeds its body.
    pub(super) fn macro_call(&mut self, name: Token, mac: Macro) {
        if !self.count_expansion() {
            return;
        }
        #[cfg(all(test, feature = "oracle"))]
        if let Some(id) = name.cs_id() {
            let w = (id / 64) as usize;
            if self.used_cs.len() <= w {
                self.used_cs.resize(w + 1, 0);
            }
            self.used_cs[w] |= 1 << (id % 64);
        }
        let mut pattern = std::mem::take(&mut self.pattern);
        pattern.clear();
        let text = self.e.toks.get(mac.text);
        let Some(e) = text.iter().position(|&t| t == Token::END_MATCH) else {
            self.pattern = pattern;
            return;
        };
        let body = Span { start: mac.text.start + e as u32 + 1, end: mac.text.end };
        if e == 0 {
            self.pattern = pattern;
            self.input.trim();
            self.input.begin_macro(body, 0, u32::MAX);
            #[cfg(feature = "mprof")]
            if let Some(id) = name.cs_id() {
                self.mprof_body(id, body.end);
            }
            return;
        }
        pattern.extend_from_slice(&text[..=e]);
        let mut args = std::mem::take(&mut self.collect);
        args.clear();
        let mut bounds: [(u32, u32); 9] = [(0, 0); 9];
        let ok = self.scan_arguments(name, mac.flags & LONG != 0, &pattern, &mut args, &mut bounds);
        if let Some(n) = ok {
            // §390: drop depleted lists first, then move the arguments to the parameter
            // stack and feed the body.
            self.input.trim();
            let mark = self.input.scratch().len() as u32;
            self.input.scratch().extend_from_slice(&args);
            for &(a, b) in &bounds[..n] {
                self.input.push_param(Span { start: mark + a, end: mark + b });
            }
            self.input.begin_macro(body, n as u32, mark);
            #[cfg(feature = "mprof")]
            if let Some(id) = name.cs_id() {
                self.mprof_body(id, body.end);
            }
        }
        self.pattern = pattern;
        self.collect = args;
    }

    /// §391–§400: matches `pattern` (ending in `end_match`) against the input, storing each
    /// argument in `args` and its bounds in `bounds`. Returns the number of arguments, or
    /// `None` after an error (the call is dropped, as TeX drops it).
    fn scan_arguments(&mut self, name: Token, long: bool, pattern: &[Token], args: &mut Vec<Token>, bounds: &mut [(u32, u32); 9]) -> Option<usize> {
        let par = Some(Token::cs(self.e.cs.par()));
        let space = Token::char(Cat::Space, ' ');
        let mut n = 0usize;
        let mut r = 0usize;
        loop {
            // `s`: where the delimiter of the current parameter starts; `None` while the
            // literal tokens before the first parameter are matched.
            let s = if cat_of(pattern[r]) == Some(Cat::Ignored) {
                r += 1;
                Some(r)
            } else {
                None
            };
            let start = args.len();
            let mut m = 0u32;
            'scan: loop {
                let Some(t) = self.get_token() else {
                    self.error(format!("File ended while scanning use of {}", self.show_one(name)));
                    return None;
                };
                if t == pattern[r] {
                    // §394: the delimiter advances; done when it is fully matched.
                    r += 1;
                    if is_match_like(pattern[r]) {
                        // §394: a delimiter ending in `#{` has its brace read twice, here
                        // and again from the macro body; only one reading counts.
                        if cat_of(t) == Some(Cat::BeginGroup) {
                            self.input.align_state -= 1;
                        }
                        break 'scan;
                    }
                    continue;
                }
                // §397: contribute the partly matched delimiter, keeping the longest
                // suffix that is still a prefix of it.
                if s != Some(r) {
                    let Some(s) = s else {
                        self.error(format!("Use of {} doesn't match its definition", self.show_one(name)));
                        return None;
                    };
                    let mut ti = s;
                    let mut resumed = false;
                    while ti != r {
                        args.push(pattern[ti]);
                        m += 1;
                        let (mut u, mut v) = (ti + 1, s);
                        loop {
                            if u == r {
                                if t == pattern[v] {
                                    r = v + 1;
                                    resumed = true;
                                }
                                break;
                            }
                            if pattern[u] != pattern[v] {
                                break;
                            }
                            u += 1;
                            v += 1;
                        }
                        if resumed {
                            break;
                        }
                        ti += 1;
                    }
                    if resumed {
                        continue 'scan;
                    }
                    r = s;
                }
                if Some(t) == par && !long {
                    // §396: a non-`\long` macro's argument may not hold `\par`.
                    self.error(format!("Paragraph ended before {} was complete", self.show_one(name)));
                    self.input.back(t);
                    return None;
                }
                match cat_of(t) {
                    Some(Cat::BeginGroup) => {
                        // §399: a whole group.
                        let mut depth = 1u32;
                        args.push(t);
                        loop {
                            let Some(u) = self.get_token() else {
                                self.error(format!("File ended while scanning use of {}", self.show_one(name)));
                                return None;
                            };
                            if Some(u) == par && !long {
                                self.error(format!("Paragraph ended before {} was complete", self.show_one(name)));
                                self.input.back(u);
                                return None;
                            }
                            args.push(u);
                            match cat_of(u) {
                                Some(Cat::BeginGroup) => depth += 1,
                                Some(Cat::EndGroup) => {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Cat::EndGroup) => {
                        self.error(format!("Argument of {} has an extra }}", self.show_one(name)));
                        continue;
                    }
                    _ => {
                        // §393: spaces before an undelimited argument are skipped.
                        if t == space && is_match_like(pattern[r]) {
                            continue;
                        }
                        args.push(t);
                    }
                }
                m += 1;
                if is_match_like(pattern[r]) {
                    break;
                }
            }
            if s.is_some() {
                // §400: one braced group alone loses its braces.
                let (mut a, mut b) = (start, args.len());
                if m == 1 && b - a >= 2 && cat_of(args[a]) == Some(Cat::BeginGroup) && cat_of(args[b - 1]) == Some(Cat::EndGroup) {
                    a += 1;
                    b -= 1;
                }
                bounds[n] = (a as u32, b as u32);
                n += 1;
            }
            if pattern[r] == Token::END_MATCH {
                return Some(n);
            }
        }
    }

    fn show_one(&self, t: Token) -> String {
        self.name_of(t)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Engine;
    use super::*;

    fn run(src: &str) -> (String, Vec<String>) {
        let mut e = Engine::default();
        let mut m = Machine::new(&mut e, src);
        m.run();
        (m.out_text(), m.errors.clone())
    }

    fn ok(src: &str) -> String {
        let (out, errors) = run(src);
        assert!(errors.is_empty(), "{errors:?}");
        out
    }

    #[test]
    fn plain_and_undelimited_parameters() {
        assert_eq!(ok("\\def\\a{xy}\\a\\a"), "xyxy");
        assert_eq!(ok("\\def\\b#1#2{[#2#1]}\\b x {yz}"), "[yzx] ");
        // A braced argument keeps inner braces, loses the outer pair.
        assert_eq!(ok("\\def\\c#1{(#1)}\\c{{a}}"), "({a}) ");
    }

    #[test]
    fn delimited_parameters() {
        assert_eq!(ok("\\def\\a#1.{<#1>}\\a xy.z"), "<xy>z ");
        // §397 partial match: delimiter `ab`, input `aab`.
        assert_eq!(ok("\\def\\a#1ab{<#1>}\\a aab"), "<a> ");
        // A group alone loses its braces; with more around it, it keeps them.
        assert_eq!(ok("\\def\\a#1.{<#1>}\\a{x}.\\a{x}y."), "<x><{x}y> ");
        // Literal tokens before the first parameter.
        assert_eq!(ok("\\def\\a(#1){#1}\\a(q)"), "q ");
        // `#{`: the parameter ends at a left brace, which stays.
        assert_eq!(ok("\\def\\a#1#{[#1]}\\a xy{z}"), "[xy]{z} ");
    }

    #[test]
    fn hash_hash_and_nested_definitions() {
        assert_eq!(ok("\\def\\a#1{\\def\\b##1{#1##1}}\\a x\\b y"), "xy ");
    }

    #[test]
    fn grouping_and_global() {
        assert_eq!(ok("{\\def\\a{1}}\\a"), "{}\\a ");
        assert_eq!(ok("{\\gdef\\a{1}}\\a"), "{}1");
        assert_eq!(ok("\\begingroup\\global\\def\\a{2}\\endgroup\\a"), "\\begingroup \\endgroup 2");
    }

    #[test]
    fn let_and_edef() {
        assert_eq!(ok("\\def\\a{x}\\let\\b=\\a\\def\\a{y}\\b\\a"), "xy");
        assert_eq!(ok("\\def\\a{x}\\edef\\b{\\a\\noexpand\\a}\\def\\a{y}\\b"), "xy");
        assert_eq!(ok("\\let\\x=a\\x"), "a");
    }

    #[test]
    fn expandafter_csname_string() {
        assert_eq!(ok("\\def\\a{b}\\def\\b{c}\\expandafter\\def\\csname q\\a\\endcsname{z}\\qb"), "z");
        assert_eq!(ok("\\def\\a#1{<#1>}\\def\\b{xy}\\expandafter\\a\\b"), "<x>y");
        assert_eq!(ok("\\string\\foo"), "\\foo");
    }

    #[test]
    fn catcodes_apply_at_definition() {
        // The body is tokenized when defined: `\@x` stays one name after `@` is other again.
        assert_eq!(ok("\\catcode`\\@=11 \\def\\@x{X}\\def\\a{\\@x}\\catcode`\\@=12 \\a"), "X");
    }

    #[test]
    fn tail_recursion_uses_no_stack() {
        // `\a` calls itself last 3000 times; depleted bodies are dropped before each call
        // (§390), so the stack stays below the limit.
        let mut src = String::from("\\def\\a#1{#1\\a}\\def\\stop#1{}\\a ");
        src.push_str(&"x".repeat(3000));
        src.push_str("\\stop");
        let (out, errors) = run(&src);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(out.len(), 3000);
    }

    #[test]
    fn runaway_recursion_stops() {
        let (_, errors) = run("\\def\\a{\\a\\a}\\a");
        assert!(errors.iter().any(|e| e.contains("capacity")), "{errors:?}");
    }

    #[test]
    fn misuse_is_reported() {
        let (_, errors) = run("\\def\\a(#1){#1}\\a x");
        assert!(errors[0].contains("doesn't match its definition"));
        let (_, errors) = run("\\def\\a#1{#1}\\a\\par");
        assert!(errors[0].contains("Paragraph ended"));
        let (_, errors) = run("\\def\\a#1{#1}\\a{x\\par}");
        assert!(errors[0].contains("Paragraph ended"));
        let (out, errors) = run("\\long\\def\\a#1{#1}\\a{x\\par}");
        assert!(errors.is_empty() && out == "x\\par  ", "{out} {errors:?}");
    }

    #[test]
    fn active_characters_are_definable() {
        assert_eq!(ok("\\catcode`\\~=13 \\def~{X}a~b"), "aXb ");
    }

    /// `cargo test --release throughput -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn throughput() {
        let mut src = String::from("\\def\\b#1#2{#2#1}\\def\\a#1{\\b#1x\\a}\\def\\stop#1{}\\a ");
        src.push_str(&"y".repeat(300_000));
        src.push_str("\\stop");
        let t = std::time::Instant::now();
        let (out, errors) = run(&src);
        let dt = t.elapsed();
        assert!(errors.is_empty(), "{errors:?}");
        eprintln!("600k macro calls: {dt:?}, {} output bytes", out.len());
    }

    #[test]
    fn meaning_types_stay_small() {
        assert!(std::mem::size_of::<super::super::Meaning>() <= 16);
    }
}
