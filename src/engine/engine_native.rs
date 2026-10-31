//! The long tail of e-TeX's and pdfTeX's `last_item` and `convert` primitives (pdftex.web
//! §416, §468, and the e-TeX and pdfTeX parts), dispatched by name from the generated
//! `PRIMITIVES` table rather than given `Prim` variants: they are rare, and keeping them out of
//! the `Prim` enum keeps the hot dispatch small.
//!
//! Quantities that belong to typesetting the engine does not do (the last node, badness,
//! PDF objects) read as TeX reads them on an empty list: zero, or -1 for `\lastnodetype`.
//! Nothing here depends on the clock or the environment, so a run is reproducible.

use super::engine_node::Kind;
use super::engine_run::Machine;
use super::engine_tables::{Cmd, PRIMITIVES};
use super::engine_token::Token;
use super::engine_value::{str_toks, Value};

/// pdfTeX's `\pdftexversion` and `\pdftexrevision`, the version whose behaviour the engine
/// follows.
pub const PDFTEX_VERSION: i32 = 140;
pub const PDFTEX_REVISION: &str = "29";

/// Whether native primitive `i` expands (a `convert` command).
pub fn native_expands(i: u16) -> bool {
    PRIMITIVES[i as usize].1 == Cmd::Convert
}

impl Machine<'_, '_> {
    /// The value of `last_item` primitive `i`, or `None` when it is not one.
    pub(super) fn native_value(&mut self, i: u16) -> Option<Value> {
        let (name, cmd) = PRIMITIVES[i as usize];
        if cmd == Cmd::AssignFontInt {
            return self.micro_value(name);
        }
        if cmd != Cmd::LastItem {
            return None;
        }
        Some(match name {
            "pdftexversion" => Value::Int(PDFTEX_VERSION),
            "inputlineno" => {
                self.line_reads += 1;
                Value::Int(self.input.line() as i32)
            }
            "currentgrouplevel" => Value::Int(self.e.eqtb.level() as i32 - 1),
            "currentgrouptype" => Value::Int(self.groups.last().copied().unwrap_or(0) as i32),
            "currentiflevel" => Value::Int(self.conds.len() as i32),
            "lastnodetype" => Value::Int(self.last_node_type()),
            // §424: the last node of the list being built, if it is of that kind.
            // §424: on an empty main vertical list, the page builder's last ones.
            "lastskip" => Value::Glue(match self.last_item(Kind::Glue) {
                Some(n) => self.e.nodes.glue(&n),
                None => self.page_last_glue().unwrap_or_default(),
            }),
            "lastkern" => Value::Dimen(self.last_item(Kind::Kern).or_else(|| self.page_last(Kind::Kern)).map_or(0, |n| n.a)),
            "lastpenalty" => Value::Int(self.last_item(Kind::Penalty).or_else(|| self.page_last(Kind::Penalty)).map_or(0, |n| n.a)),
            "badness" => Value::Int(self.last_badness),
            "pdflastxpos" => Value::Int(self.pdf_doc.last_pos.0),
            "pdflastypos" => Value::Int(self.pdf_doc.last_pos.1),
            "pdflastobj" => Value::Int(self.pdf_doc.last_obj),
            "pdflastlink" => Value::Int(self.pdf_doc.last_link),
            "pdflastximage" => Value::Int(self.pdf_doc.last_ximage),
            "pdflastximagepages" => Value::Int(self.pdf_doc.last_ximage_pages),
            "pdflastxform" => Value::Int(self.pdf_doc.last_xform),
            "fontcharwd" | "fontcharht" | "fontchardp" | "fontcharic" => Value::Dimen(self.font_char_dimen(name)),
            "gluestretch" | "glueshrink" => {
                let g = self.scan_glue(false);
                Value::Dimen(if name == "gluestretch" { g.stretch } else { g.shrink })
            }
            "gluestretchorder" | "glueshrinkorder" => {
                let g = self.scan_glue(false);
                Value::Int(if name == "gluestretchorder" { g.stretch_order } else { g.shrink_order } as i32)
            }
            // e-TeX's conversions between glue and mu glue keep the numbers.
            "mutoglue" => Value::Glue(self.scan_glue(true)),
            "gluetomu" => Value::Mu(self.scan_glue(false)),
            // e-TeX: entry n of `\parshape` (the last line repeats); `\parshapedimen 2n-1`
            // is line n's indent, `2n` its length.
            "parshapelength" | "parshapeindent" | "parshapedimen" => {
                let n = self.scan_int();
                let shape = self.e.eqtb.par_shape();
                if shape.is_empty() || n <= 0 {
                    return Some(Value::Dimen(0));
                }
                let (line, k) = match name {
                    "parshapedimen" => ((n + 1) / 2, if n % 2 == 1 { 0 } else { 1 }),
                    "parshapeindent" => (n, 0),
                    _ => (n, 1),
                };
                Value::Dimen(shape[(line as usize).min(shape.len()) - 1][k])
            }
            // badness, lastpenalty, the current if's type and branch, the PDF object
            // numbers, the random seed, the shell escape (never enabled), elapsed time.
            _ => Value::Int(0),
        })
    }

    /// `\fontcharwd<font><char>` and its kin: zero when the character is absent (e-TeX).
    fn font_char_dimen(&mut self, name: &str) -> i32 {
        let f = self.scan_font_ident() as usize;
        let c = self.scan_int();
        let font = &self.e.fonts.list[f];
        let Some([wd, ht, dp, ic]) = u8::try_from(c).ok().and_then(|c| font.dims(c, &self.e.fonts.info)) else { return 0 };
        match name {
            "fontcharwd" => wd,
            "fontcharht" => ht,
            "fontchardp" => dp,
            _ => ic,
        }
    }

    /// Expands `convert` primitive `i` into `out`.
    pub(super) fn native_convert(&mut self, i: u16, out: &mut Vec<Token>) {
        let name = PRIMITIVES[i as usize].0;
        match name {
            "pdftexrevision" => str_toks(PDFTEX_REVISION, out),
            // `\pdffilesize{<file>}`: the size in bytes, empty when there is no such file.
            "pdffilesize" => {
                let file = self.general_text_string();
                if let Some(bytes) = self.open_bytes(&file) {
                    str_toks(&bytes.len().to_string(), out);
                }
            }
            "pdfescapestring" | "pdfescapename" | "pdfescapehex" | "pdfunescapehex" => {
                let s = self.general_text_string();
                str_toks(&escape(name, &s), out);
            }
            "pdfmdfivesum" => {
                let file = self.scan_keyword("file");
                let s = self.general_text_string();
                let bytes = if file { self.open_bytes(&s) } else { Some(latin1(&s)) };
                if let Some(b) = bytes {
                    let mut hex = String::with_capacity(32);
                    for byte in super::engine_md5::md5(&b) {
                        hex.push_str(&format!("{byte:02X}"));
                    }
                    str_toks(&hex, out);
                }
            }
            "pdfcolorstackinit" => self.color_stack_init(out),
            // pdfTeX: the margin kern at either end of an `\hbox` register.
            "leftmarginkern" | "rightmarginkern" => {
                let n = self.scan_box_num();
                let list = self.e.eqtb.box_reg(n).map(|b| self.e.nodes.boxes[b as usize]).filter(|b| !b.vertical);
                let Some(b) = list else { return self.error("pdfTeX error (marginkern): a non-empty hbox expected") };
                let w = self.margin_kern_of(self.e.nodes.get(b.list), name == "rightmarginkern");
                // No margin kern prints as `0pt`, not `print_scaled`'s `0.0pt`.
                let mut s = String::new();
                match w {
                    Some(w) => super::engine_value::print_scaled(w, &mut s),
                    None => s.push('0'),
                }
                s.push_str("pt");
                str_toks(&s, out);
            }
            // utils.c `matchstrings`: `1` matched, `0` not, `-1` a bad pattern; at most
            // `subcount` (default 10) subexpressions are kept for `\pdflastmatch`.
            "pdfmatch" => {
                let icase = self.scan_keyword("icase");
                let subcount = if self.scan_keyword("subcount") { self.scan_int() } else { -1 };
                let pattern = latin1(&self.general_text_string());
                let subject = latin1(&self.general_text_string());
                let r = match super::engine_regex::Regex::new(&pattern, icase) {
                    None => "-1",
                    Some(re) => {
                        let keep = if subcount < 0 { 10 } else { subcount as usize };
                        let caps = re.exec(&subject).map(|mut c| {
                            c.resize(keep, None);
                            c
                        });
                        let r = if caps.is_some() { "1" } else { "0" };
                        self.last_match = (subject, caps);
                        r
                    }
                };
                str_toks(r, out);
            }
            // utils.c `getmatch`: `pos->text` of subexpression n, or `-1->`.
            "pdflastmatch" => {
                let mut n = self.scan_int();
                if n < 0 {
                    self.error("Bad match number");
                    n = 0;
                }
                let (s, caps) = &self.last_match;
                let found = caps.as_ref().and_then(|c| c.get(n as usize).copied().flatten());
                let text = match found {
                    Some((a, b)) => format!("{a}->{}", s[a..b].iter().map(|&c| char::from(c)).collect::<String>()),
                    None => "-1->".to_string(),
                };
                str_toks(&text, out);
            }
            // Dates, file dumps, matches, and the rest: empty, deterministic.
            _ => {
                if matches!(name, "pdffilemoddate" | "pdffiledump") {
                    self.general_text_string();
                }
            }
        }
    }

    /// A general text, expanded, as the string pdfTeX makes of it.
    pub(super) fn general_text_string(&mut self) -> String {
        let span = self.scan_braced_text(true);
        let mut s = String::new();
        self.show(self.e.toks.get(span), &mut s);
        s
    }
}

/// The string's characters as bytes (pdfTeX strings are 8-bit).
fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

/// `\pdfescapestring`, `\pdfescapename`, `\pdfescapehex` and `\pdfunescapehex` (pdftex's
/// `escapestring`, `escapename`, `escapehex`, `unescapehex` in utils.c).
fn escape(kind: &str, s: &str) -> String {
    let mut out = String::new();
    match kind {
        "pdfescapestring" => {
            for b in latin1(s) {
                match b {
                    b'(' | b')' | b'\\' => {
                        out.push('\\');
                        out.push(b as char);
                    }
                    0x21..=0x7E => out.push(b as char),
                    _ => out.push_str(&format!("\\{b:03o}")),
                }
            }
        }
        "pdfescapename" => {
            for b in latin1(s) {
                match b {
                    0 => {}
                    b'#' | b'%' | b'(' | b')' | b'/' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' => out.push_str(&format!("#{b:02X}")),
                    0x21..=0x7E => out.push(b as char),
                    _ => out.push_str(&format!("#{b:02X}")),
                }
            }
        }
        "pdfescapehex" => {
            for b in latin1(s) {
                out.push_str(&format!("{b:02X}"));
            }
        }
        _ => {
            let digits: Vec<u8> = s.chars().filter_map(|c| c.to_digit(16)).map(|d| d as u8).collect();
            for pair in digits.chunks(2) {
                let b = pair[0] << 4 | pair.get(1).copied().unwrap_or(0);
                out.push(b as char);
            }
        }
    }
    out
}
