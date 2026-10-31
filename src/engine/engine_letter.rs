//! letter.cls on the GDL path (`2026-10-31-native-letter-class.md`): the sender's fields,
//! the `letter` environment, `\opening` and `\closing`.
//!
//! The class's parameters (`\parskip` 0.7em, `\parindent` 0, its `\@listI`, page geometry,
//! `\raggedbottom`) are the class table (`rushtex-gen class --class letter`). What is code
//! is here: letter.cls's macros as tokens of commands the native job has, with the
//! document's own fields spliced in unchanged, as `\opening` expands them. `\@texttop` is in
//! the output routine (`output_column`).
//!
//! Not native yet (each declines): `\opening` without an `\address` (the `firstpage` page
//! style, `\fromlocation` and `\telephonenum` in its foot), a second `letter` (its local
//! `\c@page\@ne`), `twoside`, `\cc`, `\encl`, `\ps`, `\makelabels`, and text between
//! `\closing` and `\end{letter}` (`\stopbreaks`' `\par`, `\\` and `\vspace`).

use super::super::engine_catcode::Cat;
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::{NEW_PAGE, Op};

/// The sender's fields: `\name`, `\signature`, `\address`, `\location`, `\telephone`
/// (`\fromname`, `\fromsig`, `\fromaddress`, `\fromlocation`, `\telephonenum`).
pub(super) const NAME: usize = 0;
pub(super) const SIGNATURE: usize = 1;
pub(super) const ADDRESS: usize = 2;
pub(super) const LOCATION: usize = 3;
pub(super) const TELEPHONE: usize = 4;

#[derive(PartialEq, Clone, Default)]
pub(super) struct Letter {
    from: [Vec<Token>; 5],
    /// `\toname` (with its `\leavevmode\ignorespaces`) and `\toaddress`, from `\@processto`.
    to_name: Vec<Token>,
    to_address: Vec<Token>,
    /// `\longindentation` (`.5\textwidth`) and `\indentedwidth`, set when the class loads,
    /// before any package changes `\textwidth`; `\medskipamount` (`=\parskip`, 0.7em).
    long_indent: i32,
    indented_width: i32,
    medskip: i32,
}

impl Letter {
    /// The class's state at `\documentclass`: `textwidth` and `\parskip` are the table's.
    pub(super) fn new(textwidth: i32, parskip: i32) -> Letter {
        // `\longindentation=.5\textwidth`: §455's `xn_over_d(v, 32768, 65536)`, rounded down.
        let long_indent = (textwidth as i64 * 32768 / 65536) as i32;
        Letter { long_indent, indented_width: textwidth - long_indent, medskip: parskip, ..Letter::default() }
    }
}

/// The tokens of `list` after its first `\\` at brace depth zero, and whether there is one.
fn after_cr(list: &[Token], cr: Token) -> Option<&[Token]> {
    let mut depth = 0i32;
    for (k, t) in list.iter().enumerate() {
        match t.cat() {
            Some(Cat::BeginGroup) => depth += 1,
            Some(Cat::EndGroup) => depth -= 1,
            _ if depth == 0 && *t == cr => return Some(&list[k + 1..]),
            _ => {}
        }
    }
    None
}

/// A delimited argument's value (§392): one group alone loses its braces.
fn strip_group(list: &[Token]) -> &[Token] {
    let (Some(first), Some(last)) = (list.first(), list.last()) else { return list };
    if list.len() < 2 || first.cat() != Some(Cat::BeginGroup) || last.cat() != Some(Cat::EndGroup) {
        return list;
    }
    let mut depth = 0;
    for (k, t) in list.iter().enumerate() {
        match t.cat() {
            Some(Cat::BeginGroup) => depth += 1,
            Some(Cat::EndGroup) => {
                depth -= 1;
                if depth == 0 && k + 1 < list.len() {
                    return list;
                }
            }
            _ => {}
        }
    }
    &list[1..list.len() - 1]
}

impl Machine<'_, '_> {
    fn letter_mut(&mut self) -> Option<&mut Letter> {
        self.doc.as_mut().and_then(|d| d.letter.as_deref_mut())
    }

    /// `\name`, `\signature`, `\address`, `\location`, `\telephone`: `\def\from..{#1}`.
    pub(super) fn letter_field(&mut self, k: usize) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        match self.letter_mut() {
            Some(l) => l.from[k] = arg,
            None => self.not_native("a letter command outside the letter class"),
        }
    }

    /// `\begin{letter}{#1}`: `\begingroup`, then `\newpage` and the rest (`letter_start`).
    /// `\@processto{\leavevmode\ignorespaces #1}`: `\@xproc` takes `\toname` up to the
    /// first `\\` and `\toaddress` after it; when that is not empty, `\@yproc` takes
    /// `\toaddress` again without the `\\` `\@processto` appended.
    pub(super) fn letter_begin(&mut self) {
        self.begin_group(SEMI_SIMPLE_GROUP);
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let cr = self.named_cs(named!("\\"));
        let mut name = vec![self.named_cs(named!("indent")), self.named_cs(named!("ignorespaces"))];
        let address = match after_cr(&arg, cr) {
            Some(rest) => {
                name.extend_from_slice(&arg[..arg.len() - rest.len() - 1]);
                strip_group(rest).to_vec()
            }
            None => {
                name.extend_from_slice(&arg);
                Vec::new()
            }
        };
        let Some(doc) = self.doc.as_mut() else { return };
        doc.endpe = false;
        match doc.letter.as_deref_mut() {
            Some(l) => (l.to_name, l.to_address) = (name, address),
            None => return self.not_native("the letter environment outside the letter class"),
        }
        let t = [self.doc_id(NEW_PAGE), self.doc_id(doc!("rt@letter"))];
        self.back_list(&t);
    }

    /// After `\newpage`: `\c@page\@ne` (local; native only where it is already 1),
    /// `\c@footnote\z@`, `\interlinepenalty=200`.
    pub(super) fn letter_start(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.page != 1 {
            return self.not_native("a letter after the first page");
        }
        doc.footnote = 0;
        self.e.eqtb.set_word(param!("interlinepenalty"), 200, false);
    }

    /// `\endletter`: `\stopletter\@@par\pagebreak\@@par` (`\pagebreak` in vertical mode is
    /// `\penalty-\@M`); the `.aux` write of `\@mlabel` is `\immediate`, no node. Then
    /// `\end`'s `\endgroup`.
    pub(super) fn letter_end(&mut self) {
        let mut t = vec![self.named_cs(named!("par")), self.named_cs(named!("penalty"))];
        t.extend(self.tex_tokens("-10000 "));
        t.extend([self.named_cs(named!("par")), self.named_cs(named!("endgroup"))]);
        self.back_list(&t);
    }

    /// `\opening{#1}` with an `\address`: `\thispagestyle{empty}`, `{\raggedleft
    /// \begin{tabular}{l@{}}\ignorespaces \fromaddress \\*[2\parskip] \@date \end{tabular}
    /// \par}`, `\vspace{2\parskip}`, `{\raggedright \toname \\\toaddress \par}`,
    /// `\vspace{2\parskip}`, `#1\par\nobreak`. `2\parskip` is twice its width (§455).
    pub(super) fn opening(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(doc) = self.doc.as_ref() else { return };
        let date = if doc.dated { doc.stored[2].clone() } else { vec![self.named_cs(named!("today"))] };
        let Some(l) = doc.letter.as_deref() else { return self.not_native("\\opening outside the letter class") };
        if l.from[ADDRESS].is_empty() {
            return self.not_native("\\opening without \\address (the firstpage style)");
        }
        let (address, name, to) = (l.from[ADDRESS].clone(), l.to_name.clone(), l.to_address.clone());
        let skip = 2 * self.glue_param(super::super::engine_pack::PAR_SKIP).width;
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.doc_id(doc!("thispagestyle"))];
        t.extend(self.tex_tokens("{empty}"));
        t.extend([open, self.doc_id(doc!("raggedleft")), self.named_cs(named!("begin"))]);
        t.extend(self.tex_tokens("{tabular}{l@{}}"));
        t.push(self.named_cs(named!("ignorespaces")));
        t.extend(address);
        t.push(self.named_cs(named!("\\")));
        t.extend(self.tex_tokens(&format!("*[{skip}sp]")));
        t.extend(date);
        t.push(self.named_cs(named!("end")));
        t.extend(self.tex_tokens("{tabular}"));
        t.extend([self.named_cs(named!("par")), close, self.doc_id(doc!("vspace"))]);
        t.extend(self.tex_tokens(&format!("{{{skip}sp}}")));
        t.extend([open, self.doc_id(doc!("raggedright"))]);
        t.extend(name);
        t.push(self.named_cs(named!("\\")));
        t.extend(to);
        t.extend([self.named_cs(named!("par")), close, self.doc_id(doc!("vspace"))]);
        t.extend(self.tex_tokens(&format!("{{{skip}sp}}")));
        t.extend(arg);
        t.extend([self.named_cs(named!("par")), self.doc_id(doc!("nobreak"))]);
        self.back_list(&t);
    }

    /// `\closing{#1}`: `\par\nobreak\vspace{\parskip}\stopbreaks\noindent`, with an
    /// `\address` `\hspace*{\longindentation}`, then `\parbox{\indentedwidth}{\raggedright
    /// \ignorespaces #1\\[6\medskipamount]` the signature (`\fromsig`, else `\fromname`)
    /// `\strut}\par`, whose `\par` is `\stopbreaks`' `\@@par\nobreak`.
    pub(super) fn closing(&mut self) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(l) = self.doc.as_ref().and_then(|d| d.letter.as_deref()) else { return self.not_native("\\closing outside the letter class") };
        let sig = if l.from[SIGNATURE].is_empty() { l.from[NAME].clone() } else { l.from[SIGNATURE].clone() };
        let indent = (!l.from[ADDRESS].is_empty()).then_some(l.long_indent);
        let (width, medskip) = (l.indented_width, l.medskip);
        let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
        let mut t = vec![self.named_cs(named!("par")), self.doc_id(doc!("nobreak")), self.doc_id(doc!("vspace"))];
        // `\parskip` as the primitive's glue, scanned by `\vspace`.
        t.extend(self.tex_tokens("{\\parskip}"));
        t.extend([self.doc_id(doc!("rt@stopbreaks")), self.named_cs(named!("noindent"))]);
        if let Some(w) = indent {
            t.push(self.doc_id(doc!("hspace")));
            t.extend(self.tex_tokens(&format!("*{{{w}sp}}")));
        }
        t.push(self.doc_id(doc!("parbox")));
        t.extend(self.tex_tokens(&format!("{{{width}sp}}")));
        t.extend([open, self.doc_id(doc!("raggedright")), self.named_cs(named!("ignorespaces"))]);
        t.extend(arg);
        t.push(self.named_cs(named!("\\")));
        t.extend(self.tex_tokens(&format!("[{}sp]", 6 * medskip)));
        t.extend(sig);
        t.extend([self.doc_id(doc!("strut")), close, self.named_cs(named!("par")), self.doc_id(doc!("nobreak")), self.doc_id(doc!("rt@closingend"))]);
        self.back_list(&t);
    }

    /// `\stopbreaks`: `\interlinepenalty\@M` (its `\par`, `\\` and `\vspace` are seen only by
    /// text after `\closing`'s box, which `closing_end` refuses).
    pub(super) fn stop_breaks(&mut self) {
        self.e.eqtb.set_word(param!("interlinepenalty"), 10000, false);
    }

    /// What may follow `\closing` before `\end{letter}`: spaces, and blank lines, each a
    /// `\par` that `\stopbreaks` made `\@@par\nobreak` (in vertical mode, `\penalty\@M`).
    pub(super) fn closing_end(&mut self) {
        let par = self.named_cs(named!("par"));
        let mut ops = Vec::new();
        while let Some(t) = self.get_token() {
            if t.cat() == Some(Cat::Space) {
                continue;
            }
            if t == par && self.mode == Mode::Vertical {
                ops.push(Op::Penalty(10000));
                continue;
            }
            self.input.back(t);
            if t != self.named_cs(named!("end")) {
                return self.not_native("text after \\closing");
            }
            break;
        }
        if !ops.is_empty() {
            self.run_ops(ops);
        }
    }
}
