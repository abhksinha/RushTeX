//! The macro processor: tex.web's `get_x_token` (§380), `expand` (§366) and the part of
//! `main_control` (§1030) and `prefixed_command` (§1211) that macro code needs, in our own
//! form. Tokens the engine does not act on (characters, and control sequences it has no
//! meaning for) go to an output list, which the native pipeline typesets.

use super::engine_catcode::Cat;
use super::engine_eqtb::{Reg, Region};
use super::engine_input::Input;
use super::engine_names::END_LINE_CHAR;
use super::engine_prim::{Meaning, Prim, LONG, OUTER, PROTECTED};
use super::engine_token::{Token, DONT_EXPAND};
use super::Engine;

/// The most macro expansions in one run before it stops as runaway recursion.
const MAX_EXPANSIONS: u32 = 100_000_000;
/// The deepest token-list stack allowed (TeX's `stack_size`).
const MAX_DEPTH: usize = 5000;

/// Whether a token of meaning `m` expands (§366: a command code above `max_command`,
/// undefined names included).
pub(super) fn expandable(m: Meaning) -> bool {
    match m {
        Meaning::Macro(_) | Meaning::Undefined => true,
        Meaning::Prim(p) => p.expandable(),
        Meaning::Native(i) => super::engine_native::native_expands(i),
        _ => false,
    }
}

/// The file resolver a run reads through ([`Machine::read_file`]).
pub type FileReader<'e> = Box<dyn Fn(&str) -> Option<Vec<u8>> + 'e>;

/// §269 `simple_group` and `semi_simple_group`.
const SIMPLE_GROUP: u8 = 1;
pub(super) const SEMI_SIMPLE_GROUP: u8 = 14;
/// §236 `\globaldefs`.
const GLOBAL_DEFS: u32 = 43;

/// The macro processor over one source text.
pub struct Machine<'e, 'a> {
    /// The native LaTeX commands' state (`engine_kernel`), when `RT_NATIVE` or `RT_STATE` is set.
    pub(super) kernel: Option<Box<super::engine_kernel::Kernel>>,
    pub e: &'e mut Engine,
    pub(super) input: Input<'a>,
    /// Unexpandable tokens, in order, for the native pipeline.
    pub out: Vec<Token>,
    /// Reused buffers: a macro's arguments while they are scanned, its parameter text, a
    /// definition's text.
    pub(super) collect: Vec<Token>,
    pub(super) pattern: Vec<Token>,
    pub(super) def: Vec<Token>,
    /// §211 the semantic nest: the enclosing lists while a box is built (empty at the
    /// outer level, whose lists are the native pipeline's), the build buffer holding every
    /// open list back to back, where the current one starts, its `prev_depth` or
    /// `space_factor`, the open boxes, and `\badness` (§646 `last_badness`).
    pub(super) nest: Vec<super::engine_node::Level>,
    pub(super) cur: Vec<super::engine_node::Node>,
    pub(super) start: u32,
    pub(super) aux: i32,
    /// §213 `prev_graf`: lines of the paragraph so far on the current vertical list.
    pub(super) prev_graf: i32,
    pub(super) frames: Vec<super::engine_box::BoxFrame>,
    pub(super) last_badness: i32,
    /// §1271 `\aftergroup` tokens with the group level they belong to, in saved order.
    after_group: Vec<(u16, Token)>,
    /// The last commands the main loop ran, newest at `recent_at - 1`, and whether errors
    /// list them (`trace`): a diagnostic, cheap enough to keep on.
    recent: [Token; 16],
    recent_at: usize,
    pub trace: bool,
    /// §770 the alignments in progress, innermost last, and §305 `scanner_status =
    /// aligning` while a preamble is read.
    pub(super) aligns: Vec<super::engine_align::Align>,
    pub(super) aligning: bool,
    /// §1032 `cancel_boundary`: `\noboundary` came just before a character.
    pub(super) cancel_boundary: bool,
    /// The page builder (§980).
    pub(super) page: super::engine_page::Page,
    /// The box numbers of the open `\insert`s (255 for `\vadjust`), innermost last.
    pub(super) insert_numbers: Vec<u8>,
    /// §647 `adjust_tail`: while open, `hpack` moves insertions, marks and `\vadjust`
    /// material here; `adjust_out` holds what the last packaged box gave up.
    pub(super) adjust: Option<Vec<super::engine_node::Node>>,
    pub(super) adjust_out: Option<Vec<super::engine_node::Node>>,
    /// §1370: while a `\write` text expands, `mode` is 0, so the list being built has no
    /// last node, `\prevgraf` is 0 and `\prevdepth`, `\spacefactor` are improper.
    pub(super) mode_zero: bool,
    /// §1091 `clang`: the language of the paragraph's text so far.
    pub(super) clang: i32,
    /// §1117: the lists of the discretionaries being built: which comes next, and the
    /// pre-break and post-break lists so far.
    pub(super) disc_saved: Vec<super::engine_chars::DiscSaved>,
    /// `\pdfinfo` and `\pdfcatalog` text.
    pub(super) pdf_doc: super::engine_pdfdoc::PdfDoc,
    /// The last `\pdfmatch`: the string searched, its subexpressions (`None` when it did not
    /// match), for `\pdflastmatch`.
    pub(super) last_match: (Vec<u8>, Option<super::engine_regex::Captures>),
    /// Which fonts are virtual, checked when first shipped.
    pub(super) vf: super::engine_vf::VfTable,
    /// pdfTeX's colour stacks.
    pub(super) colors: super::engine_color::ColorStacks,
    /// e-TeX's `delim_ptr` of the current math list (see `Level::delim`).
    pub(super) delim: i32,
    /// §814 `just_box`: the last line `post_line_break` made.
    pub(super) just_box: u32,
    /// The noads of the formulas being built (`engine_mlist`), and §1139's `saved` words
    /// of the math groups: where a subformula goes, `\mathchoice`'s count, `\leqno`.
    pub(super) noads: Vec<super::engine_mlist::Noad>,
    pub(super) math_saved: Vec<u32>,
    /// §703 `cur_style`, `cur_size`, `cur_mu` while a formula becomes a list.
    pub(super) math: super::engine_mlist::MathState,
    /// Whether the engine typesets the outer lists itself (the main vertical list and its
    /// paragraphs, §1030), rather than passing them to the native pipeline as tokens.
    pub typeset: bool,
    /// pdfTeX's `\ifincsname`: how many `\csname`s are being read.
    pub(super) in_csname: u32,
    /// The name `\csname` and `\ifcsname` collect, reused so neither allocates per call.
    pub(super) name_buf: String,
    expansions: u32,
    /// Error messages, in TeX's words; the run continues past each one.
    pub errors: Vec<String>,
    /// The run stopped (runaway recursion or stack overflow).
    pub(super) halted: bool,
    /// §76 `error_count`: errors since the last paragraph ended; at 100 the run stops (§82).
    pub(super) error_count: u8,
    /// §489 the condition stack: each open conditional's limit, innermost last.
    pub(super) conds: Vec<u8>,
    /// §211 the mode `\ifvmode`, `\ifhmode` and `\ifmmode` test, set by whoever runs the
    /// machine (the native pipeline owns the lists); `inner` is `\ifinner`.
    pub mode: Mode,
    pub inner: bool,
    /// §1269 `after_token`.
    pub(super) after_assignment: Option<Token>,
    /// Reads a file (`\input`, `\openin`, `\font`) by the name as written, as bytes; `None`
    /// when there is none. The engine has no file system of its own (it builds for WASM
    /// too). Text is read as pdfTeX reads it: each byte is one character code 0–255.
    pub read_file: Option<FileReader<'e>>,
    /// §532 `\jobname`.
    pub job_name: String,
    /// §480 the sixteen `\read` streams and §1342 the sixteen `\write` streams; closed
    /// `\openout` files, in the order they closed; the log (`\message`, `\write` to the
    /// terminal); §73 the interaction mode (0 batch … 3 errorstop).
    pub(super) reads: [Option<super::engine_io::ReadStream>; 16],
    pub(super) writes: [Option<super::engine_io::WriteStream>; 16],
    pub closed: Vec<super::engine_io::WriteStream>,
    pub log: String,
    pub interaction: u8,
    /// §269 the type of each open group: 1 for a brace group, 14 for `\begingroup`.
    pub(super) groups: Vec<u8>,
    /// The runaway guard: the most expansions this run may make.
    pub max_expansions: u32,
    /// Whether the token `get_token` last read was marked by `\noexpand`.
    pub(super) dont_expand: bool,
    /// The main loop's `lig_stack`, kept between words so no word allocates one.
    pub(super) lig_buf: Vec<super::engine_chars::Item>,
    /// The token-arena length at which `engine_gc` next collects, and its reused buffer of
    /// live ranges.
    pub(super) collect_at: u32,
    pub(super) collect_buf: Vec<(u32, u32, u32)>,
    /// The node-arena length at which `engine_gc` next collects nodes, and its tables.
    pub(super) dead_buf: Vec<u32>,
    /// `RT_PROFILE`: one bit per control sequence whose macro was expanded.
    #[cfg(all(test, feature = "oracle"))]
    pub(super) used_cs: Vec<u64>,
}

/// §211: vertical, horizontal or math mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Vertical,
    Horizontal,
    Math,
}

/// A token with the meaning it had when read (§297 `cur_cmd`/`cur_chr`).
#[derive(Clone, Copy, Debug)]
pub(super) struct X {
    pub t: Token,
    pub m: Meaning,
}

impl<'e, 'a> Machine<'e, 'a> {
    pub fn new(e: &'e mut Engine, src: &'a str) -> Self {
        Machine { e, input: Input::new(src), out: Vec::new(), collect: Vec::new(), pattern: Vec::new(), def: Vec::new(), nest: Vec::new(), cur: Vec::new(), start: 0, aux: 0, prev_graf: 0, frames: Vec::new(), last_badness: 0, after_group: Vec::new(), recent: [Token(0); 16], recent_at: 0, trace: false, aligns: Vec::new(), aligning: false, cancel_boundary: false, page: Default::default(), insert_numbers: Vec::new(), adjust: None, adjust_out: None, mode_zero: false, clang: 0, disc_saved: Vec::new(), pdf_doc: Default::default(), last_match: Default::default(), vf: Default::default(), colors: Default::default(), delim: -1, just_box: 0, noads: Vec::new(), math_saved: Vec::new(), math: Default::default(), typeset: false, in_csname: 0, name_buf: String::new(), expansions: 0, errors: Vec::new(), halted: false, error_count: 0, conds: Vec::new(), mode: Mode::Vertical, inner: false, after_assignment: None, read_file: None, job_name: String::from("texput"), reads: Default::default(), writes: Default::default(), closed: Vec::new(), log: String::new(), interaction: 3, groups: Vec::new(), max_expansions: MAX_EXPANSIONS, dont_expand: false, lig_buf: Vec::new(), collect_at: super::engine_gc::MIN_COLLECT, collect_buf: Vec::new(), dead_buf: Vec::new(), kernel: None, #[cfg(all(test, feature = "oracle"))] used_cs: Vec::new() }
    }

    /// Records an error with TeX's `l.<line>` context of the innermost file.
    pub(super) fn error(&mut self, msg: impl Into<String>) {
        let mut msg = msg.into();
        if self.trace {
            msg.push_str("\n      after:");
            for k in 0..16 {
                let t = self.recent[(self.recent_at + k) % 16];
                if t.0 != 0 {
                    msg.push(' ');
                    msg.push_str(&self.name_of(t));
                }
            }
        }
        if self.trace {
            msg.push_str(&format!("\n      align_state {}, groups {:?}", self.input.align_state, self.groups));
        }
        self.errors.push(format!("l.{} [{}] {msg}", self.input.line(), self.input.file_depth()));
        self.error_count += 1;
        if self.error_count == 100 {
            self.errors.push(String::from("(That makes 100 errors; please try again.)"));
            self.halted = true;
        }
    }

    /// §365 `get_token`: the next token, unexpanded.
    pub(super) fn get_token(&mut self) -> Option<Token> {
        if self.halted {
            return None;
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: END_LINE_CHAR });
        loop {
            let Engine { eqtb, cs, toks, .. } = &mut *self.e;
            let t = self.input.next(toks, eqtb, cs, elc)?;
            self.dont_expand = t.0 & DONT_EXPAND != 0;
            let t = Token(t.0 & !DONT_EXPAND);
            // §342: an alignment entry ends at an alignment character or `\cr` at its
            // own brace level.
            if self.input.align_state == 0 {
                if let Some(code) = self.align_code(t) {
                    self.insert_v_template(code);
                    if self.halted {
                        return None;
                    }
                    continue;
                }
            }
            return Some(t);
        }
    }

    /// The meaning of the token `get_token` just read: §358, an expandable token marked by
    /// `\noexpand` means [`Meaning::DontExpand`].
    pub(super) fn meaning_read(&self, t: Token) -> Meaning {
        let m = self.meaning(t);
        if self.dont_expand && expandable(m) {
            Meaning::DontExpand
        } else {
            m
        }
    }

    /// Reads a token again, with its `\noexpand` mark if it had one.
    pub(super) fn back_read(&mut self, t: Token) {
        self.input.back(if self.dont_expand { t.dont_expand() } else { t });
    }

    /// The meaning of `t` now: a control sequence's or active character's equivalent (a
    /// primitive by name while never assigned); a character token means itself.
    pub(super) fn meaning(&self, t: Token) -> Meaning {
        let id = match t.cs_id() {
            Some(id) => id,
            None if t.cat() == Some(Cat::Active) => match t.chr() as u32 {
                c @ 0..=255 => c,
                _ => match self.e.cs.get(active_key(t.chr(), &mut [0; 5])) {
                    Some(id) => id,
                    None => return Meaning::Undefined,
                },
            },
            None => return Meaning::Char(t),
        };
        // Primitives were entered at startup (`Engine::default`), so a name never assigned
        // is undefined.
        match self.e.eqtb.meaning(id) {
            Meaning::Unset => Meaning::Undefined,
            m => m,
        }
    }

    /// §372: the characters up to `\endcsname`, expanded, in the reused name buffer (the
    /// caller hands it back to `name_buf`; a nested `\csname` takes a fresh one meanwhile).
    pub(super) fn scan_cs_name(&mut self) -> String {
        let mut name = std::mem::take(&mut self.name_buf);
        name.clear();
        self.in_csname += 1;
        while let Some(x) = self.get_x_token(false) {
            if x.m == Meaning::Prim(Prim::EndCsname) {
                break;
            }
            match x.t.cs_id() {
                None => name.push(x.t.chr()),
                Some(_) => {
                    self.error("Missing \\endcsname inserted");
                    self.input.back(x.t);
                    break;
                }
            }
        }
        self.in_csname -= 1;
        name
    }

    /// The equivalent's id for a control sequence or active character `t` (a definable
    /// token, §1215 `get_r_token`), interning an active character's key.
    pub(super) fn key(&mut self, t: Token) -> Option<u32> {
        match t.cs_id() {
            Some(id) => Some(id),
            None if t.cat() == Some(Cat::Active) => Some(match t.chr() as u32 {
                c @ 0..=255 => c,
                _ => self.e.cs.intern(active_key(t.chr(), &mut [0; 5])),
            }),
            None => None,
        }
    }

    /// §1215 `get_r_token`: the next token that is not a space, as a definable name; an
    /// error (and `None`) for anything else, which is read again.
    pub(super) fn get_r_token(&mut self) -> Option<u32> {
        loop {
            let t = self.get_token()?;
            if t.cs_id().is_none() && t.cat() == Some(Cat::Space) {
                continue;
            }
            return match self.key(t) {
                Some(id) => Some(id),
                None => {
                    self.error("Missing control sequence inserted");
                    self.input.back(t);
                    None
                }
            };
        }
    }

    /// §380 `get_x_token`: the next token after expansion. With `protect`, `\protected`
    /// macros are not expanded (e-TeX, inside `\edef`). A token after `\noexpand` comes back
    /// unexpanded, meaning [`Meaning::DontExpand`].
    pub(super) fn get_x_token(&mut self, protect: bool) -> Option<X> {
        loop {
            let t = self.get_token()?;
            let m = self.meaning_read(t);
            match m {
                Meaning::Macro(mac) if !(protect && mac.flags & PROTECTED != 0) => self.macro_call(t, mac),
                Meaning::Prim(p) if p.expandable() => self.expand(p, t),
                Meaning::Native(i) if super::engine_native::native_expands(i) => self.expand_native(i),
                _ => return Some(X { t, m }),
            }
        }
    }

    /// Counts one expansion; `false` (and the run halts) past the limits.
    pub(super) fn count_expansion(&mut self) -> bool {
        self.expansions += 1;
        if self.expansions > self.max_expansions || self.input.depth() > MAX_DEPTH {
            self.error("TeX capacity exceeded (runaway recursion)");
            self.halted = true;
            return false;
        }
        true
    }

    /// §366 `expand` for the expandable primitives other than `\noexpand`; `t` is the
    /// token that meant `p`.
    pub(super) fn expand(&mut self, p: Prim, t: Token) {
        if !self.count_expansion() {
            return;
        }
        if p.is_if() {
            return self.conditional(p, false);
        }
        if let Some(code) = p.fi_or_else() {
            return self.fi_or_else(t, code);
        }
        match p {
            Prim::Unless => self.unless(),
            Prim::TopMark | Prim::FirstMark | Prim::BotMark | Prim::SplitFirstMark | Prim::SplitBotMark | Prim::TopMarks | Prim::FirstMarks | Prim::BotMarks | Prim::SplitFirstMarks | Prim::SplitBotMarks => self.insert_mark(p),
            Prim::Input => self.start_input(),
            Prim::ScanTokens => self.pseudo_start(),
            Prim::Meaning => {
                let Some(t) = self.get_token() else { return };
                let mut s = String::new();
                self.print_meaning(t, &mut s);
                let mut list = Vec::new();
                super::engine_value::str_toks(&s, &mut list);
                self.back_list(&list);
            }
            Prim::Unexpanded => {
                let mut list = Vec::new();
                self.unexpanded(&mut list);
                self.back_list(&list);
            }
            Prim::FontName => {
                let s = self.font_name();
                let mut list = Vec::new();
                super::engine_value::str_toks(&s, &mut list);
                self.back_list(&list);
            }
            Prim::Detokenize | Prim::Expanded | Prim::ETeXRevision | Prim::PdfStrcmp => {
                let mut list = Vec::new();
                self.etex_convert(p, &mut list);
                self.back_list(&list);
            }
            Prim::EndInput => self.input.end_input(),
            Prim::JobName => {
                let list: Vec<Token> = self.job_name.chars().map(|c| Token::char(if c == ' ' { Cat::Space } else { Cat::Other }, c)).collect();
                self.back_list(&list);
            }
            Prim::Number | Prim::RomanNumeral | Prim::The => self.convert(p),
            // §367: the next token goes back marked, so its next reading does not expand it.
            Prim::NoExpand => {
                let Some(u) = self.get_token() else { return };
                let named = u.cs_id().is_some() || u.cat() == Some(Cat::Active);
                self.input.back(if named { u.dont_expand() } else { u });
            }
            // §368: expand the token after the next one, then read the next one.
            Prim::ExpandAfter => {
                let Some(t) = self.get_token() else { return };
                let Some(u) = self.get_token() else {
                    self.input.back(t);
                    return;
                };
                match self.meaning_read(u) {
                    Meaning::Macro(mac) => self.macro_call(u, mac),
                    Meaning::Prim(q) if q.expandable() => self.expand(q, u),
                    Meaning::Native(i) if super::engine_native::native_expands(i) => self.expand_native(i),
                    _ => self.back_read(u),
                }
                self.input.back(t);
            }
            // §372: `\csname ... \endcsname`; an undefined name becomes `\relax`.
            Prim::Csname => {
                let name = self.scan_cs_name();
                let id = self.e.cs.intern(&name);
                self.name_buf = name;
                if self.meaning(Token::cs(id)) == Meaning::Undefined {
                    self.e.eqtb.set_meaning(id, Meaning::Prim(Prim::Relax), false);
                }
                self.input.back(Token::cs(id));
            }
            // §1373 `\string`: a name as characters of catcode 12 (space 10), with `\`
            // as the escape character.
            Prim::String => {
                let Some(t) = self.get_token() else { return };
                let mut chars = String::new();
                // §470 and §263 `sprint_cs`: the escape character (if `\\escapechar` is a
                // character) and the name, with no space after it.
                match t.cs_id() {
                    Some(id) => {
                        let ctx = self.show_ctx();
                        match self.e.cs.name(id) {
                            "" => {
                                ctx.esc("csname", &mut chars);
                                ctx.esc("endcsname", &mut chars);
                            }
                            name => ctx.esc(name, &mut chars),
                        }
                    }
                    None => chars.push(t.chr()),
                }
                self.def.clear();
                self.def.extend(chars.chars().map(|c| Token::char(if c == ' ' { Cat::Space } else { Cat::Other }, c)));
                let list = std::mem::take(&mut self.def);
                self.back_list(&list);
                self.def = list;
            }
            _ => {}
        }
    }

    /// §1337: a run that starts from a format reads `\everyjob` first.
    /// §241 `fix_date_and_time` from a Unix time in seconds, read as UTC (web2c reads
    /// `SOURCE_DATE_EPOCH` so when `FORCE_SOURCE_DATE` is 1): `\time` in minutes since
    /// midnight, `\day`, `\month`, `\year`.
    pub fn fix_date_and_time(&mut self, secs: i64) {
        let (y, m, d) = crate::expand::civil_from_days(secs.div_euclid(86_400));
        let minutes = secs.rem_euclid(86_400) / 60;
        for (slot, v) in [(20, minutes as i32), (21, d as i32), (22, m as i32), (23, y as i32)] {
            self.e.eqtb.set_word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot }, v, true);
        }
    }

    pub fn start_job(&mut self) {
        self.bind_kernel();
        let Some(reg) = super::engine_names::param("everyjob") else { return };
        let span = self.e.eqtb.toks(reg);
        if !span.is_empty() {
            let list = self.e.toks.get(span).to_vec();
            self.back_list(&list);
        }
    }

    /// §323 `back_list`: `list` is read next, in order.
    pub(super) fn back_list(&mut self, list: &[Token]) {
        self.input.trim();
        let start = self.input.scratch().len() as u32;
        self.input.scratch().extend_from_slice(list);
        self.input.back_scratch(start);
    }

    /// Runs the source to its end; the output is in `out`.
    pub fn run(&mut self) {
        while let Some(x) = self.get_x_token(false) {
            self.command(x, 0);
            // Between commands no Rust frame holds a token span: the arena can be collected.
            self.maybe_collect();
        }
        // §1378: the job ends with every `\openout` file closed.
        for k in 0..16 {
            if let Some(w) = self.writes[k].take() {
                self.closed.push(w);
            }
        }
        if self.kernel.is_some() {
            self.kernel_finish();
        }
    }

    /// §1030: one command of the main loop; `prefix` holds `\global` and its kin read so far.
    pub(super) fn command(&mut self, x: X, prefix: u8) {
        self.recent[self.recent_at % 16] = x.t;
        self.recent_at = (self.recent_at + 1) % 16;
        const GLOBAL: u8 = 8;
        // §1214: a positive `\globaldefs` makes every assignment global, a negative one none.
        let global_defs = self.e.eqtb.word(Reg { region: Region::Int, slot: GLOBAL_DEFS });
        let global = if global_defs != 0 { global_defs > 0 } else { prefix & GLOBAL != 0 };
        let p = match x.m {
            Meaning::Prim(Prim::Count | Prim::Dimen | Prim::Skip | Prim::MuSkip | Prim::Toks) => {
                if let Some(reg) = self.assignment_target(x) {
                    self.assign_register(reg, global);
                    self.after_assignment();
                }
                return;
            }
            Meaning::Prim(p) => p,
            Meaning::Font(f) => return self.select_font(f, global),
            // §358: a `\noexpand`ed token acts as `\relax`, which does nothing.
            Meaning::DontExpand => return,
            Meaning::Reg(reg) => {
                self.assign_register(reg, global);
                return self.after_assignment();
            }
            Meaning::Char(c) if x.t.cs_id().is_some() || x.t.cat() == Some(Cat::Active) => {
                // §1221: `\let\x=a` makes `\x` act as the character.
                return self.char_token(c);
            }
            // §1090 `char_given`: the character, for the native pipeline.
            // §1154 `math_given`; outside a formula it needs one (§1046).
            Meaning::MathCharDef(n) if self.building() => {
                if self.mode == Mode::Math {
                    return self.set_math_char(n, 0);
                }
                return self.insert_dollar_sign(x.t);
            }
            // pdfTeX's protrusion and expansion codes and `\pdffontexpand`.
            Meaning::Kernel(i) => return self.kernel_command(i, x.t),
            Meaning::Native(i) if self.micro_command(super::engine_tables::PRIMITIVES[i as usize].0) => return,
            // pdfTeX's `\pdfxform` and `\pdfrefxform`.
            Meaning::Native(i) if self.xform_command(super::engine_tables::PRIMITIVES[i as usize].0, false) => return,
            // pdfTeX's `\quitvmode` (`start_par`, 2): `\indent` in vertical mode, nothing in
            // horizontal and math modes (no `indent_in_hmode`).
            Meaning::Native(i) if self.building() && super::engine_tables::PRIMITIVES[i as usize].0 == "quitvmode" => {
                if self.mode == Mode::Vertical {
                    self.new_graf(true);
                }
                return;
            }
            Meaning::Native(i) if self.building() && super::engine_tables::PRIMITIVES[i as usize].0 == "pdfsavepos" => return self.save_pos(),
            Meaning::CharDef(n) => {
                let c = char::from_u32(n).unwrap_or('\0');
                let cat = if c.is_ascii_alphabetic() { Cat::Letter } else { Cat::Other };
                return self.char_token(Token::char(cat, c));
            }
            _ => return self.char_token(x.t),
        };
        if self.building() && self.math_command(x, p) {
            return;
        }
        if self.box_command(x, p, global) {
            // §1279: `\afterassignment`'s token follows `\setbox`'s `{` and `\wd n=...`.
            if matches!(p, Prim::SetBox | Prim::Wd | Prim::Ht | Prim::Dp) {
                self.after_assignment();
            }
            return;
        }
        match p {
            // §1271 `save_for_after`: outside every group it does nothing.
            // §1045: spaces after it are skipped, expanding as it goes.
            Prim::IgnoreSpaces if self.building() => {
                if let Some(n) = self.next_non_blank() {
                    self.input.back(n.t);
                }
            }
            // §1248 `\parshape n i1 l1 ... in ln`.
            Prim::ParShape => {
                self.scan_optional_equals();
                let n = self.scan_int();
                let lines: Vec<[i32; 2]> = (0..n.max(0)).map(|_| [self.scan_dimen(false).0, self.scan_dimen(false).0]).collect();
                self.e.eqtb.set_par_shape(lines, global);
                self.after_assignment();
            }
            // §1130: `\halign` in a vertical list; in a paragraph it ends the paragraph
            // first (§1094 `head_for_vmode`).
            Prim::Halign if self.building() => match self.mode {
                Mode::Vertical => self.init_align(),
                // §1145: in a display, as its whole contents.
                Mode::Math if !self.inner && self.groups.last() == Some(&super::engine_noad::MATH_SHIFT_GROUP) => self.init_align(),
                Mode::Math if !self.inner => self.off_save(x.t),
                Mode::Horizontal if !self.inner => {
                    self.input.back(x.t);
                    let par = Token::cs(self.e.cs.intern("par"));
                    self.input.back(par);
                }
                _ => self.unsupported(x.t),
            },
            Prim::Span | Prim::Cr | Prim::Crcr if self.building() => self.align_error(x.t),
            // §1129
            Prim::Omit | Prim::NoAlign if self.building() => {
                let name = self.name_of(x.t);
                self.error(format!("Misplaced {name}"));
            }
            Prim::EndTemplate if self.building() => self.do_endv(),
            // §1252
            Prim::Patterns | Prim::Hyphenation => self.hyph_data(p),
            // §1348
            Prim::PdfGlyphToUnicode => self.pdf_glyph_to_unicode(),
            Prim::PdfInfo | Prim::PdfCatalog | Prim::PdfObj | Prim::PdfRefObj | Prim::PdfOutline => self.pdf_doc_command(p),
            Prim::PdfDest | Prim::PdfStartLink | Prim::PdfEndLink if self.building() => self.pdf_doc_command(p),
            Prim::PdfXImage => self.pdf_ximage(false),
            Prim::PdfRefXImage => self.pdf_refximage(),
            Prim::Special | Prim::SetLanguage | Prim::PdfLiteral | Prim::PdfColorStack | Prim::PdfSetMatrix | Prim::PdfSave | Prim::PdfRestore if self.building() => self.whatsit_command(p),
            Prim::Insert | Prim::Vadjust | Prim::Mark | Prim::Marks | Prim::End | Prim::Dump if self.building() => {
                if !self.page_command(x.t, x.m) {
                    self.unsupported(x.t);
                }
            }
            // §1245, §1246
            Prim::PageGoal | Prim::PageTotal | Prim::PageStretch | Prim::PageFilStretch | Prim::PageFillStretch | Prim::PageFilllStretch | Prim::PageShrink | Prim::PageDepth => {
                self.scan_optional_equals();
                let v = self.scan_dimen(false).0;
                self.page.so_far[p as usize - Prim::PageGoal as usize] = v;
                self.after_assignment();
            }
            Prim::DeadCycles | Prim::InsertPenalties => {
                self.scan_optional_equals();
                let v = self.scan_int();
                if p == Prim::DeadCycles {
                    self.page.dead_cycles = v;
                } else {
                    self.page.insert_penalties = v;
                }
                self.after_assignment();
            }
            // §1090: in a vertical list it starts a paragraph; §1045 in a horizontal one.
            Prim::NoBoundary if self.building() => match self.mode {
                Mode::Vertical => {
                    self.input.back(x.t);
                    self.new_graf(true);
                }
                Mode::Horizontal => self.no_boundary(),
                Mode::Math => {}
            },
            Prim::AfterGroup => {
                if let Some(t) = self.get_token() {
                    let level = self.e.eqtb.level();
                    if level > 1 {
                        self.after_group.push((level, t));
                    }
                }
            }
            Prim::TextFont | Prim::ScriptFont | Prim::ScriptScriptFont => {
                self.def_family(p, global);
                self.after_assignment();
            }
            // §1242 `alter_aux`: only in the list it belongs to.
            // §1244: `\prevgraf` of the innermost vertical list; never negative.
            Prim::PrevGraf => {
                self.scan_optional_equals();
                let v = self.scan_int();
                if v < 0 {
                    self.error(format!("Bad \\prevgraf ({v})"));
                } else {
                    self.set_prev_graf_of_vlist(v);
                }
                self.after_assignment();
            }
            Prim::SpaceFactor | Prim::PrevDepth => {
                self.scan_optional_equals();
                let horizontal = p == Prim::SpaceFactor;
                let v = if horizontal { self.scan_int() } else { self.scan_dimen(false).0 };
                if horizontal && !(1..=32767).contains(&v) {
                    self.error(format!("Bad space factor ({v})"));
                } else if self.building() && (self.mode == Mode::Horizontal) == horizontal {
                    self.aux = v;
                }
                self.after_assignment();
            }
            Prim::Global | Prim::Long | Prim::Outer | Prim::Protected => {
                // §1211: prefixes, then the command they apply to.
                let bit = match p {
                    Prim::Global => GLOBAL,
                    Prim::Long => LONG,
                    Prim::Outer => OUTER,
                    _ => PROTECTED,
                };
                let next = loop {
                    match self.get_x_token(false) {
                        Some(n) if n.t == Token::char(Cat::Space, ' ') || n.m == Meaning::Prim(Prim::Relax) => continue,
                        n => break n,
                    }
                };
                match next {
                    Some(n) => self.command(n, prefix | bit),
                    None => self.error("You can't use a prefix with the end of the file"),
                }
            }
            Prim::Def | Prim::Gdef | Prim::Edef | Prim::Xdef => {
                let global = if global_defs != 0 { global_defs > 0 } else { global || matches!(p, Prim::Gdef | Prim::Xdef) };
                let expand = matches!(p, Prim::Edef | Prim::Xdef);
                let Some(id) = self.get_r_token() else { return };
                if let Some(text) = self.scan_macro_text(expand) {
                    let m = super::engine_prim::Macro { text, flags: prefix & (LONG | OUTER | PROTECTED) };
                    self.e.eqtb.set_meaning(id, Meaning::Macro(m), global);
                }
                self.after_assignment();
            }
            Prim::Let => {
                // §1221: `\let<cs>[ ][=][ ]<token>`.
                let Some(id) = self.get_r_token() else { return };
                let space = Token::char(Cat::Space, ' ');
                let mut u = self.get_token();
                while u == Some(space) {
                    u = self.get_token();
                }
                if u == Some(Token::char(Cat::Other, '=')) {
                    u = self.get_token();
                    if u == Some(space) {
                        u = self.get_token();
                    }
                }
                let Some(u) = u else { return };
                let m = self.meaning(u);
                self.e.eqtb.set_meaning(id, m, global);
                self.after_assignment();
            }
            Prim::FutureLet => {
                self.future_let(global);
                self.after_assignment();
            }
            Prim::CharDef | Prim::MathCharDef | Prim::CountDef | Prim::DimenDef | Prim::SkipDef | Prim::MuSkipDef | Prim::ToksDef => {
                self.shorthand_def(p, global);
                self.after_assignment();
            }
            Prim::Advance | Prim::Multiply | Prim::Divide => {
                self.register_command(p, global);
                self.after_assignment();
            }
            Prim::AfterAssignment => self.after_assignment = self.get_token(),
            Prim::Catcode | Prim::LcCode | Prim::UcCode | Prim::SfCode | Prim::MathCode | Prim::DelCode => {
                self.def_code(p, global);
                self.after_assignment();
            }
            Prim::Lowercase | Prim::Uppercase => self.case_shift(p == Prim::Uppercase),
            Prim::Font => {
                self.new_font(global);
                self.after_assignment();
            }
            Prim::NullFont => self.select_font(0, global),
            Prim::FontDimen => {
                self.assign_font_dimen();
                self.after_assignment();
            }
            Prim::HyphenChar | Prim::SkewChar => {
                self.assign_font_int(p == Prim::SkewChar);
                self.after_assignment();
            }
            Prim::OpenIn | Prim::CloseIn => self.open_in(p == Prim::CloseIn),
            Prim::Read | Prim::ReadLine => {
                self.read_to(p == Prim::ReadLine, global);
                self.after_assignment();
            }
            Prim::Immediate => {
                // §1375: `\immediate` before `\write`, `\openout` or `\closeout`.
                if let Some(n) = self.get_x_token(false) {
                    match n.m {
                        Meaning::Prim(q @ (Prim::Write | Prim::OpenOut | Prim::CloseOut)) => self.extension(q, true),
                        // pdfTeX: `\immediate\pdfobj` writes the object whether or not a page refers to it.
                        Meaning::Prim(Prim::PdfObj) => self.pdf_obj(true),
                        Meaning::Prim(Prim::PdfXImage) => self.pdf_ximage(true),
                        Meaning::Native(i) if self.xform_command(super::engine_tables::PRIMITIVES[i as usize].0, true) => {}
                        _ => self.input.back(n.t),
                    }
                }
            }
            Prim::Write | Prim::OpenOut | Prim::CloseOut => self.extension(p, false),
            Prim::Message | Prim::ErrMessage => self.message(p == Prim::ErrMessage),
            Prim::BatchMode => self.interaction = 0,
            Prim::NonstopMode => self.interaction = 1,
            Prim::ScrollMode => self.interaction = 2,
            Prim::ErrorstopMode => self.interaction = 3,
            Prim::BeginGroup => {
                self.begin_group(SEMI_SIMPLE_GROUP);
                if !self.building() {
                    self.out.push(x.t);
                }
            }
            Prim::EndGroup => {
                self.end_group(SEMI_SIMPLE_GROUP);
                if !self.building() {
                    self.out.push(x.t);
                }
            }
            // The `\relax` `insert_relax` put in has done its job; any other name meaning
            // `\relax` goes out as `\relax` itself, as the native pipeline knows no aliases.
            Prim::Relax if self.building() => {}
            Prim::Relax => {
                let frozen = x.t.cs_id().is_some_and(|id| self.e.cs.name(id).starts_with('\0'));
                if !frozen {
                    let relax = self.e.cs.intern("relax");
                    self.out.push(Token::cs(relax));
                }
            }
            // Inside a box, a typesetting primitive the engine lacks is reported.
            _ if self.building() => self.unsupported(x.t),
            // `\relax` and the rest go to the output: the native pipeline reads them.
            _ => self.out.push(x.t),
        }
    }

    pub(super) fn begin_group(&mut self, kind: u8) {
        self.groups.push(kind);
        if self.e.eqtb.begin_group().is_err() {
            self.error("TeX capacity exceeded, sorry [grouping levels]");
            self.halted = true;
        }
    }

    /// §1065 and §1069: closing a group of the kind it was opened as; a mismatch is an error
    /// and leaves the group open.
    pub(super) fn end_group(&mut self, kind: u8) {
        match self.groups.last() {
            Some(&k) if k == kind => {
                self.groups.pop();
                // §282: the group's `\aftergroup` tokens are read next, in the order saved.
                let level = self.e.eqtb.level();
                let keep = self.after_group.iter().rposition(|&(l, _)| l < level).map_or(0, |i| i + 1);
                let tokens: Vec<Token> = self.after_group.drain(keep..).map(|(_, t)| t).collect();
                self.e.eqtb.end_group();
                if !tokens.is_empty() {
                    self.back_list(&tokens);
                }
            }
            Some(_) if kind == SIMPLE_GROUP => self.error("Extra }, or forgotten \\endgroup"),
            None if kind == SIMPLE_GROUP => self.error("Too many }'s"),
            _ => self.error("Extra \\endgroup"),
        }
    }

    /// Expands `convert` native `i` (e-TeX's and pdfTeX's long tail).
    pub(super) fn expand_native(&mut self, i: u16) {
        if !self.count_expansion() {
            return;
        }
        let mut list = Vec::new();
        self.native_convert(i, &mut list);
        self.back_list(&list);
    }

    /// A character for the output; braces open and close a simple group (§1063, §1085).
    fn char_token(&mut self, t: Token) {
        if self.mode == Mode::Math && self.building() && self.math_char_token(t) {
            return;
        }
        match t.cat() {
            Some(Cat::BeginGroup) => self.begin_group(SIMPLE_GROUP),
            Some(Cat::EndGroup) if self.in_box_group() => return self.package(),
            Some(Cat::EndGroup) if self.groups.last() == Some(&super::engine_align::NO_ALIGN_GROUP) => return self.end_no_align(),
            Some(Cat::EndGroup) if self.groups.last() == Some(&super::engine_page::INSERT_GROUP) => return self.end_insert(),
            Some(Cat::EndGroup) if self.groups.last() == Some(&super::engine_chars::DISC_GROUP) => return self.build_discretionary(),
            Some(Cat::EndGroup) if self.groups.last() == Some(&super::engine_page::OUTPUT_GROUP) => return self.resume_page_builder(),
            // §1132: an entry's `}` too many: `\cr` is assumed first.
            Some(Cat::EndGroup) if self.groups.last() == Some(&super::engine_align::ALIGN_GROUP) => {
                self.input.back(t);
                self.input.back(Token::cs(self.e.cs.intern("cr")));
                return self.error("Missing \\cr inserted");
            }
            Some(Cat::AlignTab) if self.building() => return self.align_error(t),
            Some(Cat::EndGroup) => self.end_group(SIMPLE_GROUP),
            _ => {}
        }
        if !self.building() {
            return self.out.push(t);
        }
        match t.cat() {
            Some(Cat::Letter | Cat::Other) => match u8::try_from(t.chr() as u32) {
                Ok(c) => self.append_char(c),
                Err(_) => self.error(format!("Bad character code ({})", t.chr() as u32)),
            },
            Some(Cat::Space) => self.append_space(),
            Some(Cat::BeginGroup | Cat::EndGroup) => {}
            Some(Cat::MathShift | Cat::Superscript | Cat::Subscript) => self.math_shift_outside(t),
            _ => self.unsupported(t),
        }
    }

    /// Something the engine cannot build into a box yet (math, `\vadjust`, inserts ...):
    /// one error per kind, so the run shows what is missing.
    fn unsupported(&mut self, t: Token) {
        let what = match t.cs_id() {
            Some(id) => format!("\\{}", self.e.cs.name(id)),
            None => format!("{:?} character", t.cat()),
        };
        let msg = format!("Not in the engine's boxes yet: {what}");
        if !self.errors.iter().any(|e| e.ends_with(&msg)) {
            self.error(msg);
        }
    }

    /// §537 `start_input`: reads a file name (§526 `scan_file_name`, or a braced name) and
    /// opens the file, trying `.tex` after the name as written (§537).
    fn start_input(&mut self) {
        let mut name = String::new();
        let first = self.next_non_blank();
        match first {
            Some(x) if x.t.cs_id().is_none() && x.t.cat() == Some(Cat::BeginGroup) => {
                while let Some(x) = self.get_x_token(false) {
                    if x.t.cs_id().is_none() && x.t.cat() == Some(Cat::EndGroup) {
                        break;
                    }
                    if x.t.cs_id().is_none() {
                        name.push(x.t.chr());
                    }
                }
            }
            Some(mut x) => loop {
                // §526: characters up to a space (consumed) or a non-character (put back);
                // quotes only group.
                if x.t.cs_id().is_some() {
                    self.input.back(x.t);
                    break;
                }
                match x.t.chr() {
                    ' ' => break,
                    '"' => {}
                    c => name.push(c),
                }
                match self.get_x_token(false) {
                    Some(y) => x = y,
                    None => break,
                }
            },
            None => {}
        }
        match self.open_file(&name) {
            Some(text) => {
                self.input.trim();
                self.input.push_file(text);
            }
            None => self.error(format!("I can't find file `{name}'")),
        }
    }

    /// e-TeX's `pseudo_start` (`\scantokens`): the general text, shown as `\the` would
    /// show it, is read again as a file, its lines ending at `\newlinechar`.
    fn pseudo_start(&mut self) {
        let span = self.scan_braced_text(false);
        let mut s = String::new();
        self.show(self.e.toks.get(span), &mut s);
        let nl = self.e.eqtb.word(Reg { region: Region::Int, slot: super::engine_io::NEW_LINE_CHAR });
        if let Some(nl) = u32::try_from(nl).ok().and_then(char::from_u32) {
            s = s.replace(nl, "\n");
        }
        self.input.trim();
        self.input.push_file(s);
    }

    /// §1225 `frozen_relax`: a `\relax` no definition can change, for `insert_relax`.
    pub(super) fn frozen_relax(&mut self) -> Token {
        let id = self.e.cs.intern("\0relax");
        self.e.eqtb.set_meaning(id, Meaning::Prim(Prim::Relax), true);
        Token::cs(id)
    }

    /// §292 `show_token_list` with the current `\escapechar` and catcodes.
    pub(super) fn show(&self, list: &[Token], out: &mut String) {
        let ctx = self.show_ctx();
        super::engine_token::show(list, &self.e.cs, &ctx, out);
    }

    pub(super) fn show_ctx(&self) -> super::engine_token::ShowCtx<'_> {
        let escape = self.e.eqtb.word(Reg { region: Region::Int, slot: super::engine_names::ESCAPE_CHAR });
        super::engine_token::ShowCtx { cats: self.e.eqtb.cats(), escape, file: None }
    }

    /// A token as TeX prints it in a message.
    pub(super) fn name_of(&self, t: Token) -> String {
        let mut s = String::new();
        self.show(&[t], &mut s);
        s.trim_end().to_string()
    }

    /// The output shown as text (§294), for the native pipeline and for tests.
    pub fn out_text(&self) -> String {
        let mut s = String::new();
        self.show(&self.out, &mut s);
        s
    }
}

/// The equivalent's key of active character `c`, built in `buf`: a name no tokenized
/// control sequence has (it starts with NUL, which the tokenizer ignores).
pub(super) fn active_key(c: char, buf: &mut [u8; 5]) -> &str {
    let n = c.encode_utf8(&mut buf[1..]).len();
    std::str::from_utf8(&buf[..1 + n]).unwrap_or("\0")
}

#[cfg(all(test, feature = "oracle"))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// `RT_FORMAT=latex.ltx RT_PRIMS=prims.txt cargo test --release format_smoke -- --ignored
    /// --nocapture`: runs a format source with files found by `kpsewhich`, and lists the
    /// errors and the TeX primitives that reached the output (the engine lacks them).
    #[test]
    #[ignore]
    fn format_smoke() {
        let Ok(path) = std::env::var("RT_FORMAT") else { return };
        // `RT_DIR` overrides the TeX tree, for probing a truncated copy of a file; then the
        // document's own directory and kpathsea's search (`engine_host`).
        let doc_dir = std::path::Path::new(&path).parent().filter(|d| !d.as_os_str().is_empty()).map(std::path::Path::to_path_buf);
        let host = crate::engine::engine_host::Kpse::new("pdflatex", Some(std::path::Path::new("target/parity/kpse-paths")), doc_dir).expect("a TeX installation");
        // `RT_FILES=1`: every TeX file the run found, as `FILE <path>` lines when the harness
        // ends (`scripts/texlive/pin.py` builds the pinned snapshot's file list from them).
        struct Files<'h>(&'h crate::engine::engine_host::Kpse);
        impl Drop for Files<'_> {
            fn drop(&mut self) {
                if std::env::var("RT_FILES").is_ok() {
                    for p in self.0.found_paths() {
                        eprintln!("FILE {}", p.display());
                    }
                }
            }
        }
        let _files = Files(&host);
        // `RT_NO_TFM_READS=1`: every font the run loads must come from the static table
        // (`data_tfm`, pillar 2); a `.tfm` read from a file is a failure.
        let no_tfm = std::env::var("RT_NO_TFM_READS").is_ok();
        let kpse_raw = |name: &str| -> Option<Vec<u8>> {

            if let Some(b) = std::env::var("RT_DIR").ok().and_then(|d| std::fs::read(format!("{d}/{name}")).ok()) {
                return Some(b);
            }
            let found = host.read(name);
            // A TFM looked for and not found is no failure: pdfTeX does not find it either.
            assert!(!(no_tfm && found.is_some() && name.ends_with(".tfm")), "RT_NO_TFM_READS: {name} was read from a file, not the font table");
            found
        };
        // Time spent finding and reading files, reported apart from the engine's own time.
        let lookups: std::rc::Rc<std::cell::Cell<(u32, std::time::Duration)>> = Default::default();
        let looked = lookups.clone();
        let kpse = move |name: &str| -> Option<Vec<u8>> {
            let t = std::time::Instant::now();
            let v = kpse_raw(name);
            let (n, d) = looked.get();
            looked.set((n + 1, d + t.elapsed()));
            v
        };
        let src: String = kpse(&path).unwrap_or_default().iter().map(|&b| char::from(b)).collect();
        // `RT_FMT_IN=<file>`: start from a dumped format, as `pdflatex doc.tex` does, instead
        // of IniTeX.
        let fmt_in = std::env::var("RT_FMT_IN").ok();
        let now = crate::engine::engine_host::run_date();
        // §532: the job is named after the first file read.
        let job_name = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("texput").to_string();
        let setup = |m: &mut Machine| {
            m.trace = std::env::var("RT_TRACE").is_ok();
            // The page dumps below read the shipped boxes after the run.
            m.page.keep_shipped = std::env::var("RT_BOXES").is_ok() || std::env::var("RT_PAGES").is_ok();
            if let Some(n) = std::env::var("RT_MAXEXP").ok().and_then(|v| v.parse().ok()) {
                m.max_expansions = n;
            }
            // `RT_PAGES=1`: the files each run reads back (what the run before it wrote), so
            // the file that keeps a document rerunning shows as the one whose length changes.
            if std::env::var("RT_PAGES").is_ok() {
                for w in &m.closed {
                    eprintln!("READS {} ({} bytes)", w.name, w.text.len());
                }
            }
        };
        // `RT_PROFILE=1`: how much of the format a document uses (the document-needs gate's
        // measure): format macros expanded, and their share of the format's macro tokens.
        let fmt_macros = std::cell::RefCell::new(Vec::new());
        let setup = |m: &mut Machine| {
            if fmt_macros.borrow().is_empty() {
                *fmt_macros.borrow_mut() = m.e.eqtb.macro_lengths();
            }
            setup(m)
        };
        let profile = |m: &Machine| {
            let fm = fmt_macros.borrow();
            let (mut n, mut t, mut un, mut ut) = (0u64, 0u64, 0u64, 0u64);
            for (id, &len) in fm.iter().enumerate() {
                if len == 0 {
                    continue;
                }
                n += 1;
                t += len as u64;
                if m.used_cs.get(id / 64).is_some_and(|w| w >> (id % 64) & 1 == 1) {
                    un += 1;
                    ut += len as u64;
                }
            }
            let all: u64 = m.used_cs.iter().map(|w| w.count_ones() as u64).sum();
            eprintln!("PROFILE format macros used {un} of {n} ({:.1}%), their tokens {ut} of {t} ({:.1}%); macros expanded in all {all}", 100.0 * un as f64 / n.max(1) as f64, 100.0 * ut as f64 / t.max(1) as f64);
        };
        let t = std::time::Instant::now();
        if let Some(f) = &fmt_in {
            let fmt = || std::fs::read(f).ok();
            let max = if std::env::var("RT_ONCE").is_ok() { 1 } else { crate::engine::engine_job::MAX_RUNS };
            let job = crate::engine::engine_job::run(&fmt, &src, &crate::engine::engine_job::Opts { job_name: &job_name, now, max_runs: max }, &kpse, &mut |m| setup(m), &mut |m| {
                // `RT_LEAKS=1`: model M's leak audit of the last run.
                if std::env::var("RT_LEAKS").is_ok() {
                    eprintln!("{}", m.node_leaks());
                }
                if std::env::var("RT_PROFILE").is_ok() {
                    profile(m);
                }
                report(m, &path, t.elapsed(), lookups.get())
            }).expect("a format of this build");
            eprintln!("{path}: {} runs", job.runs);
            // `RT_PDF=<file>`: the shipped pages as a PDF.
            if let Ok(out) = std::env::var("RT_PDF") {
                let _ = std::fs::write(out, &job.pdf);
            }
            return;
        }
        let mut e = Engine::default();
        e.eqtb.use_initex_catcodes();
        let mut m = Machine::new(&mut e, &src);
        m.read_file = Some(Box::new(&kpse));
        m.job_name = job_name;
        m.typeset = std::env::var("RT_TYPESET").is_ok();
        setup(&mut m);
        m.fix_date_and_time(now);
        m.run();
        report(&m, &path, t.elapsed(), lookups.get());
        if let Ok(path) = std::env::var("RT_NAMES") {
            let names: Vec<&str> = (0..m.e.cs.len() as u32).map(|id| m.e.cs.name(id)).filter(|n| !n.is_empty() && !n.starts_with('\0')).collect();
            let _ = std::fs::write(path, names.join("\n"));
        }
        // `RT_FMT=<file>`: dump the format there, time it, and check it reads back.
        if let Ok(fmt) = std::env::var("RT_FMT") {
            let (stored, t) = (m.e.toks.len(), std::time::Instant::now());
            drop(m);
            let bytes = e.dump().expect("dumped at top level");
            let dump_t = t.elapsed();
            let _ = std::fs::write(&fmt, &bytes);
            let t = std::time::Instant::now();
            let mut back = Engine::undump(&bytes).expect("the format reads back");
            let meanings = |e: &mut Engine| -> Vec<String> {
                let m = Machine::new(e, "");
                (0..m.e.cs.len() as u32)
                    .map(|id| {
                        let mut s = String::new();
                        m.print_meaning(Token::cs(id), &mut s);
                        s
                    })
                    .collect()
            };
            let load_t = t.elapsed();
            let differ = meanings(&mut e).iter().zip(meanings(&mut back)).filter(|(a, b)| **a != *b).count();
            eprintln!("format: {differ} meanings differ after loading");
            eprintln!("format: {} bytes, {} of {stored} tokens live, {} names; dump {dump_t:?}, load {load_t:?}", bytes.len(), back.toks.len(), back.cs.len());
            return;
        }
        if let Ok(out) = std::env::var("RT_PDF") {
            let _ = std::fs::write(out, m.pdf());
        }
    }

    /// `format_smoke`'s report on the last run: timing, errors, and the `RT_BOXES`,
    /// `RT_PAGES`, `RT_OUT` and `RT_PRIMS` dumps.
    fn report(m: &Machine, path: &str, dt: std::time::Duration, (n_files, file_t): (u32, std::time::Duration)) {
        // `RT_FONTS=1`: the TFM name of every font the run loaded, one `FONT` line each
        // (font popularity, the font table's order and tiers).
        if std::env::var("RT_FONTS").is_ok() {
            let mut names: Vec<&str> = m.e.fonts.list.iter().skip(1).map(|f| &*f.name).collect();
            names.sort_unstable();
            names.dedup();
            names.iter().for_each(|n| eprintln!("FONT {n}"));
        }
        // `RT_MEM=1`: the bytes each arena holds at the end of the run, to find what grows.
        if std::env::var("RT_MEM").is_ok() {
            let n = &m.e.nodes;
            let mb = |b: usize| b as f64 / 1048576.0;
            eprintln!(
                "MEM toks {:.1} MB, nodes {:.1} MB ({} nodes), boxes {:.1} MB, glues {:.1} MB, rules {:.1} MB, discs {:.1} MB, page pdf {:.1} MB, cur {} nodes, closed {:.1} MB",
                mb(m.e.toks.len() * 4),
                mb(n.list.capacity() * std::mem::size_of::<super::super::engine_node::Node>()),
                n.list.len(),
                mb(n.boxes.capacity() * std::mem::size_of::<super::super::engine_node::BoxRec>()),
                mb(n.glues.capacity() * std::mem::size_of::<super::super::Glue>()),
                mb(n.rules.capacity() * 12),
                mb(n.discs.capacity() * 16),
                mb(m.page.pdf.iter().map(|p| p.content.len()).sum::<usize>()),
                m.cur.len(),
                mb(m.closed.iter().map(|w| w.text.len()).sum::<usize>()),
            );
            let (sc, pa, fr) = m.input.capacities();
            eprintln!(
                "MEM scratch {:.1} MB, params {pa}, frames {fr}, out {:.1} MB, collect {:.1} MB, pattern {:.1} MB, def {:.1} MB, cur cap {:.1} MB, log {:.1} MB",
                mb(sc * 4),
                mb(m.out.capacity() * 4),
                mb(m.collect.capacity() * 4),
                mb(m.pattern.capacity() * 4),
                mb(m.def.capacity() * 4),
                mb(m.cur.capacity() * 12),
                mb(m.log.capacity())
            );
        }
        let prims: Vec<String> = std::env::var("RT_PRIMS").ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.lines().filter_map(|l| l.split(' ').next().map(String::from)).collect()).unwrap_or_default();
        let mut missing: HashMap<String, u32> = HashMap::new();
        for &t in &m.out {
            if let Meaning::Native(i) = m.meaning(t) {
                *missing.entry(crate::engine::engine_tables::PRIMITIVES[i as usize].0.to_string()).or_default() += 1;
            } else if let Some(id) = t.cs_id() {
                let name = m.e.cs.name(id).to_string();
                if prims.binary_search(&name).is_ok() {
                    *missing.entry(name).or_default() += 1;
                }
            }
        }
        let mut missing: Vec<_> = missing.into_iter().collect();
        missing.sort_by_key(|m| std::cmp::Reverse(m.1));
        eprintln!("{path}: engine {:?} ({n_files} file lookups took {file_t:?} more)", dt.saturating_sub(file_t));
        eprintln!("{path}: {} expansions, {} out tokens, {dt:?}, {} errors, {} names, {} stored tokens, {} pages, halted at line {}", m.expansions, m.out.len(), m.errors.len(), m.e.cs.len(), m.e.toks.len(), m.page.pages, m.input.line());
        for err in m.errors.iter().take(std::env::var("RT_ERRS").ok().and_then(|v| v.parse().ok()).unwrap_or(40)) {
            eprintln!("  {err}");
        }
        // `RT_BOXES=<file>`: every shipped page as `\showbox` prints it, for diffing against
        // pdfTeX's `\tracingoutput` log.
        if let Ok(out) = std::env::var("RT_BOXES") {
            let mut s = String::new();
            for (i, &b) in m.page.shipped.iter().enumerate() {
                s.push_str(&format!("PAGE {}\n", i + 1));
                let mut one = String::new();
                m.show_box(b, &mut one);
                s.push_str(&one);
                s.push('\n');
            }
            let _ = std::fs::write(out, s);
        }
        // `RT_PAGES=1`: each shipped page's first characters, to line pages up with pdfTeX's.
        if std::env::var("RT_PAGES").is_ok() {
            use crate::engine::engine_node::Kind;
            fn text(m: &Machine, b: u32, out: &mut String) {
                for n in m.e.nodes.get(m.e.nodes.boxes[b as usize].list) {
                    match n.kind {
                        Kind::Char => out.push(char::from(n.a as u8)),
                        Kind::Lig => out.extend(m.e.nodes.lig_from(n).iter().map(|&c| char::from(c))),
                        Kind::Glue if !out.ends_with(' ') => out.push(' '),
                        Kind::Box => text(m, n.a as u32, out),
                        _ => {}
                    }
                }
            }
            for (i, &b) in m.page.shipped.iter().enumerate() {
                let mut s = String::new();
                text(m, b, &mut s);
                let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
                let end: String = s.chars().rev().take(50).collect::<Vec<_>>().into_iter().rev().collect();
                eprintln!("PAGE {}: {} ... {}", i + 1, s.chars().take(60).collect::<String>(), end);
            }
        }
        if std::env::var("RT_OUT").is_ok() {
            eprintln!("OUT: {}\nLOG: {}", m.out_text(), m.log);
        }
        eprintln!("missing primitives ({}):", missing.len());
        for (n, c) in missing.iter().take(80) {
            eprint!("{n}:{c} ");
        }
        eprintln!();
    }
}
