//! §173–§198 `show_box`: a box and everything in it, one node per line, as TeX prints it
//! for `\showbox` and `\tracingoutput`. The engine's boxes then diff line by line against
//! pdfTeX's log, which locates any difference to the node.

use super::engine_node::{BoxRec, Kind, Node, Sign, MARGIN_LEFT, MARGIN_RIGHT, RUNNING};
use super::engine_run::Machine;
use super::engine_value::{print_scaled, print_spec};

/// §225 the glue parameters, by `sub - 1` of a glue node (§189).
const GLUE_NAMES: [&str; 18] = [
    "lineskip", "baselineskip", "parskip", "abovedisplayskip", "belowdisplayskip", "abovedisplayshortskip", "belowdisplayshortskip", "leftskip", "rightskip", "topskip", "splittopskip", "tabskip", "spaceskip", "xspaceskip", "parfillskip", "thinmuskip", "medmuskip", "thickmuskip",
];

impl Machine<'_, '_> {
    /// `show_box(p)` of box record `b`, with TeX's `.` per nesting level.
    pub fn show_box(&self, b: u32, out: &mut String) {
        self.show_box_rec(&self.e.nodes.boxes[b as usize], 0, out);
    }

    fn line(depth: usize, out: &mut String) {
        if !out.is_empty() {
            out.push('\n');
        }
        for _ in 0..depth {
            out.push('.');
        }
    }

    /// §184: the box line, then its list one level deeper.
    fn show_box_rec(&self, b: &BoxRec, depth: usize, out: &mut String) {
        Self::line(depth, out);
        out.push_str(if b.vertical { "\\vbox(" } else { "\\hbox(" });
        print_scaled(b.height, out);
        out.push('+');
        print_scaled(b.depth, out);
        out.push_str(")x");
        print_scaled(b.width, out);
        // §186: the glue setting, unless it is zero.
        if b.glue_sign != Sign::Normal && b.glue_set != 0.0 {
            out.push_str(", glue set ");
            if b.glue_sign == Sign::Shrinking {
                out.push_str("- ");
            }
            // Beyond 20000 only the bound is printed (`print_glue(20000*unity, ...)`).
            if b.glue_set > 20000.0 {
                out.push('>');
                print_glue(20000 * 65536, b.glue_order, "", out);
            } else {
                let g = (65536.0 * b.glue_set).round() as i32;
                print_glue(g, b.glue_order, "", out);
            }
        }
        if b.shift != 0 {
            out.push_str(", shifted ");
            print_scaled(b.shift, out);
        }
        for n in self.e.nodes.get(b.list) {
            self.show_node(n, depth + 1, out);
        }
    }

    /// A character, or a ligature made from `from`.
    fn show_char(&self, n: &Node, from: &[u8], depth: usize, out: &mut String) {
        Self::line(depth, out);
        self.font_ident(n.font, out);
        out.push(' ');
        print_ascii(n.a as u8, out);
        if n.kind == Kind::Lig {
            out.push_str(" (ligature ");
            for &c in from {
                print_ascii(c, out);
            }
            out.push(')');
        }
    }

    /// Word-store items as the nodes they stand for: ligatures read the store's
    /// characters, and a hyphenated form's discretionary its texts there (as §195).
    fn show_word_items(&self, items: &[Node], depth: usize, out: &mut String) {
        let words = &self.e.nodes.words;
        for m in items {
            match m.kind {
                Kind::Char | Kind::Lig => self.show_char(m, words.lig_from(m), depth, out),
                Kind::Disc if m.a >= 0 => {
                    Self::line(depth, out);
                    out.push_str("\\discretionary");
                    if m.sub > 0 {
                        out.push_str(" replacing ");
                        out.push_str(&m.sub.to_string());
                    }
                    let [pre, post] = words.discs[m.a as usize];
                    self.show_word_items(&words.items[pre.0 as usize..pre.1 as usize], depth + 1, out);
                    for p in &words.items[post.0 as usize..post.1 as usize] {
                        Self::line(depth + 1, out);
                        out.push('|');
                        let mut s = String::new();
                        self.show_word_items(std::slice::from_ref(p), 0, &mut s);
                        out.push_str(s.trim_start_matches('\n'));
                    }
                }
                _ => self.show_node(m, depth, out),
            }
        }
    }

    fn show_node(&self, n: &Node, depth: usize, out: &mut String) {
        let nodes = &self.e.nodes;
        match n.kind {
            Kind::Box => self.show_box_rec(&nodes.boxes[n.a as usize], depth, out),
            Kind::Char | Kind::Lig => self.show_char(n, nodes.lig_from(n), depth, out),
            // A word shows as the nodes it stands for.
            Kind::Word => self.show_word_items(nodes.word_items(n), depth, out),
            Kind::Rule => {
                Self::line(depth, out);
                let [w, h, d] = nodes.rules[n.a as usize];
                out.push_str("\\rule(");
                rule_dimen(h, out);
                out.push('+');
                rule_dimen(d, out);
                out.push_str(")x");
                rule_dimen(w, out);
            }
            // §189
            Kind::Glue => {
                Self::line(depth, out);
                let g = nodes.glue(n);
                if n.sub >= 100 {
                    out.push_str(match n.sub % 10 {
                        1 => "\\cleaders ",
                        2 => "\\xleaders ",
                        _ => "\\leaders ",
                    });
                    print_spec(g, "", out);
                    if n.sub >= 110 {
                        let [w, h, d] = nodes.rules[n.b as usize];
                        Self::line(depth + 1, out);
                        out.push_str("\\rule(");
                        rule_dimen(h, out);
                        out.push('+');
                        rule_dimen(d, out);
                        out.push_str(")x");
                        rule_dimen(w, out);
                    } else {
                        self.show_box_rec(&nodes.boxes[n.b as usize], depth + 1, out);
                    }
                    return;
                }
                out.push_str("\\glue");
                match n.sub {
                    1..=18 => {
                        out.push_str("(\\");
                        out.push_str(GLUE_NAMES[n.sub as usize - 1]);
                        out.push(')');
                    }
                    98 => out.push_str("(\\nonscript)"),
                    _ => {}
                }
                out.push(' ');
                print_spec(g, if n.sub == 99 { "mu" } else { "" }, out);
            }
            // §191
            Kind::Kern => {
                Self::line(depth, out);
                match n.sub {
                    1 => {
                        out.push_str("\\kern ");
                        print_scaled(n.a, out);
                    }
                    2 => {
                        out.push_str("\\kern ");
                        print_scaled(n.a, out);
                        out.push_str(" (for accent)");
                    }
                    MARGIN_LEFT | MARGIN_RIGHT => {
                        out.push_str("\\kern");
                        print_scaled(n.a, out);
                        out.push_str(if n.sub == MARGIN_LEFT { " (left margin)" } else { " (right margin)" });
                    }
                    99 => {
                        out.push_str("\\mkern");
                        print_scaled(n.a, out);
                        out.push_str("mu");
                    }
                    _ => {
                        out.push_str("\\kern");
                        print_scaled(n.a, out);
                    }
                }
            }
            Kind::Math => {
                Self::line(depth, out);
                out.push_str(if n.sub == 0 { "\\mathon" } else { "\\mathoff" });
                if n.a != 0 {
                    out.push_str(", surrounded ");
                    print_scaled(n.a, out);
                }
            }
            Kind::Penalty => {
                Self::line(depth, out);
                out.push_str("\\penalty ");
                out.push_str(&n.a.to_string());
            }
            Kind::Disc => {
                Self::line(depth, out);
                out.push_str("\\discretionary");
                if n.sub > 0 {
                    out.push_str(" replacing ");
                    out.push_str(&n.sub.to_string());
                }
                if n.a >= 0 {
                    let [pre, post] = nodes.discs[n.a as usize];
                    for p in nodes.get(pre) {
                        self.show_node(p, depth + 1, out);
                    }
                    for p in nodes.get(post) {
                        Self::line(depth + 1, out);
                        out.push('|');
                        let mut s = String::new();
                        self.show_node(p, 0, &mut s);
                        out.push_str(s.trim_start_matches('\n'));
                    }
                }
            }
            Kind::Mark => {
                Self::line(depth, out);
                out.push_str("\\mark");
            }
            Kind::Ins => {
                Self::line(depth, out);
                let r = &nodes.inserts[n.a as usize];
                out.push_str(&format!("\\insert{}, natural size ", r.n));
                print_scaled(r.height, out);
                out.push_str("; split(");
                print_spec(r.split_top, "", out);
                out.push(',');
                print_scaled(r.depth, out);
                out.push_str(&format!("); float cost {}", r.float_cost));
                for p in nodes.get(r.list) {
                    self.show_node(p, depth + 1, out);
                }
            }
            Kind::Adjust => {
                Self::line(depth, out);
                out.push_str("\\vadjust");
                for p in nodes.get((n.a as u32, n.b as u32)) {
                    self.show_node(p, depth + 1, out);
                }
            }
            // pdfTeX's `pdf_refximage_node` and `pdf_refxform_node`: `\pdfrefximage(h+d)xw`.
            Kind::Whatsit if super::engine_whatsit::sized(n.sub) => {
                Self::line(depth, out);
                let [w, h, d] = nodes.rules[n.a as usize];
                out.push_str(if n.sub == super::engine_whatsit::XFORM { "\\pdfrefxform(" } else { "\\pdfrefximage(" });
                print_scaled(h, out);
                out.push('+');
                print_scaled(d, out);
                out.push_str(")x");
                print_scaled(w, out);
            }
            Kind::Whatsit => {
                Self::line(depth, out);
                // A pgf path literal is a `\pdfliteral` to TeX.
                let sub = if n.sub == super::engine_whatsit::PGF_POINT { super::engine_whatsit::LITERAL } else { n.sub };
                out.push_str(&format!("\\whatsit{sub}"));
            }
            Kind::Noad => {}
        }
    }

    /// §267 `print_font_and_char`'s font part: the control sequence that names it.
    fn font_ident(&self, f: u16, out: &mut String) {
        let font = &self.e.fonts.list[f as usize];
        out.push('\\');
        if font.ident == u32::MAX {
            out.push_str("FONT");
        } else {
            out.push_str(self.e.cs.name(font.ident));
        }
        // pdftex.web `print_font_identifier`: an expanded copy shows its ratio.
        if font.expand != 0 {
            out.push_str(if font.expand > 0 { " (+" } else { " (" });
            out.push_str(&font.expand.to_string());
            out.push(')');
        }
    }
}

/// §177 `print_glue` of `g` with its order, and `unit` for finite glue.
fn print_glue(g: i32, order: u8, unit: &str, out: &mut String) {
    print_scaled(g, out);
    if order > 0 {
        out.push_str("fil");
        for _ in 1..order {
            out.push('l');
        }
    } else {
        out.push_str(unit);
    }
}

/// §176 `print_rule_dimen`: `*` for a running dimension.
fn rule_dimen(d: i32, out: &mut String) {
    if d == RUNNING {
        out.push('*');
    } else {
        print_scaled(d, out);
    }
}

/// §49/§59: a character as TeX prints it to the log (`^^` for the unprintable ones).
fn print_ascii(c: u8, out: &mut String) {
    if (32..127).contains(&c) {
        out.push(c as char);
    } else if c < 64 {
        out.push_str("^^");
        out.push((c + 64) as char);
    } else if c == 127 {
        out.push_str("^^?");
    } else {
        out.push_str(&format!("^^{c:02x}"));
    }
}
