//! `tabular` on the GDL path: latex.ltx's `\tabular`, `\@tabular`, `\@array`, `\@mkpream`,
//! `\@tabularcr`, `\hline` and `\endtabular`, lowered onto the engine's alignment.
//!
//! LaTeX builds a preamble of TeX text (`\@mkpream` with `\@tabclassz` and kin, through
//! `\edef`) and runs `\halign` on it, so every entry reads its templates back as tokens.
//! Here the preamble is the same steps as data: [`mkpream`] runs `\@mkpream`'s class
//! automaton over the column specification once and gives each column its ⟨u⟩ and ⟨v⟩
//! steps ([`Cell`]). The alignment is made with a constructed preamble
//! (`init_align_with`) whose templates are one native each (`\rt@tabu`, `\rt@tabv`),
//! which run the steps of the entry's column. The rest of `\@array` and `\endtabular` are
//! builder calls.

use super::super::engine_catcode::Cat;
use super::super::engine_align::NO_ALIGN_GROUP;
use super::super::engine_box::{fil_glue, Ctx, HBOX_GROUP, VBOX_GROUP, VTOP_GROUP};
use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{Kind, Node, RUNNING};
use super::super::engine_noad::VCENTER_GROUP;
use super::super::engine_pack::{BASELINE_SKIP, LINE_SKIP};
use super::super::engine_prim::{Meaning, Prim};
use super::super::engine_run::{Machine, Mode, X, SEMI_SIMPLE_GROUP, SIMPLE_GROUP};
use super::super::engine_token::Token;
use super::super::engine_value::MAX_DIMEN;
use super::super::END_TEMPLATE_ID;
use crate::engine::Span;
use super::{ARSTRUT_BOX, CLINE, CLINE_CMD, HDOTSFOR, HLINE, MC_BODY, MC_TAIL, MULTICOLUMN, R_ARRAYCOLSEP, R_ARRAYRULEWIDTH, R_DOUBLERULESEP, R_TABCOLSEP, TAB_END, TAB_PAR, TAB_SKIP, TAB_U, TAB_V};

/// One step of an entry's templates, as `\@mkpream` writes them for `tabular`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Cell {
    /// `\@arstrut`: `\unhcopy\@arstrutbox` (the first column's ⟨u⟩ starts with it).
    Strut,
    /// `\@acol`: `\hskip\tabcolsep`.
    Acol,
    /// `\@arrayrule`: `\hskip -.5\arrayrulewidth \vrule \@width\arrayrulewidth
    /// \hskip -.5\arrayrulewidth`.
    Rule,
    /// `\@classi` after a rule: `\hskip\doublerulesep`.
    DoubleSep,
    /// `\@classii` after a rule: `\hskip .5\arrayrulewidth`.
    HalfRule,
    /// The entry's own group, `{` and `}`.
    Open,
    Close,
    Hfil,
    /// `\hskip1sp`.
    OneSp,
    Ignore,
    Unskip,
    /// `array`'s `$` opening an entry's formula (`$\relax#`), or an `@{...}`'s.
    MathOn,
    /// The `$` closing it.
    MathOff,
    /// `\@arrayacol`: `\hskip\arraycolsep`.
    ArrayAcol,
    /// `\quad` (`\hskip1em`) in an `@{\quad}`, inside its formula.
    Quad,
    /// array.sty's `\@arrayrule`, `\vline`: `\vrule\@width\arrayrulewidth`.
    VLine,
    /// array.sty's `\do@row@strut`: the strut a `\\[x]` left for the entry's end, once.
    RowStrut,
    /// array.sty's `p{w}` entry start, `\vtop \@startpbox{w}`: `\bgroup \color@begingroup
    /// \setlength\hsize{w} \@arrayparboxrestore \everypar{\vrule \@height \ht\@arstrutbox
    /// \@width \z@ \everypar{}}` (the width in scaled points).
    PStart(i32, bool),
    /// `\@endpbox`: `\@finalstrut\@arstrutbox \par \color@endgroup \egroup` (its `\hfil`
    /// is a step of its own). The kernel's `\@startpbox` and `\@endpbox` (`false`) have no
    /// colour group and no `\everypar`.
    PEnd(bool),
    /// array.sty's `\textonly@unskip`: `\ifhmode\unskip\fi`.
    HUnskip,
    /// The tokens of a `>{...}`, `<{...}`, `!{...}` or `@{...}`, read in place: the steps
    /// after them wait for them.
    Toks(Span),
    /// siunitx's `S[table-format=a.b]` entry start (`>{\siunitx_cell_begin:w}`, the format
    /// `10a + b`): `\__siunitx_table_collect_begin:w` takes the template's `\ignorespaces`
    /// and collects the entry's tokens, spaces outside braces dropped, to its end.
    SiBegin(u8),
    /// Its end (`<{\siunitx_cell_end:}`): the collected entry printed, as text or as a number
    /// in the format's boxes ([`Machine::si_cell_end`]).
    SiEnd(u8),
    /// colortbl's `\@classz` entry start: `\setbox\z@\hbox\bgroup\bgroup \CT@everycr{}`.
    CtOpen,
    /// Its end: `\egroup\egroup`, the row's colour (`\begingroup \CT@row@color \CT@do@color
    /// \endgroup`: the colour pushed, `\kern-\tabcolsep \leaders\vrule\hskip(\wd\z@ +
    /// 2\tabcolsep) plus 1fill \kern-\tabcolsep \hskip-\wd\z@ plus -1fill`, the pop), then
    /// `\vrule\@height\ht\z@ \@width\z@` (`\minrowclearance` 0) and `\unhbox\z@`.
    CtClose,
    /// colortbl's `\hfill`.
    Hfill,
    /// colortbl's `\hskip\stretch{.5}` (`0pt plus .5fill`).
    HalfFill,
    /// colortbl's `\kern\z@`.
    Kern0,
}

/// An open `longtable` (longtable.sty 4.24), one page long: its body (read again after a
/// trial when it has a head), the chunk being built, and `\LT@save@row`.
#[derive(PartialEq, Clone)]
pub(super) struct Lt {
    /// The tokens after the specification to `\end{longtable}`.
    body: Vec<Token>,
    /// `[pos]{spec}`, for the pass after the trial.
    head_args: Vec<Token>,
    /// The first pass, which only measures the chunks (`\LT@save@row` from a blank row, as
    /// the run with no `.aux` does); its row is the one the next run reads.
    trial: bool,
    /// `\c@LT@chunks`.
    chunk: i32,
    /// `\LT@save@row`: per column, the chunk whose width it is and the width.
    row: Vec<(i32, i32)>,
    /// `\LT@head`.
    head: Option<u32>,
    /// `\LTleft` and `\LTright`.
    left: Glue,
    right: Glue,
    /// What the chunk being ended is: `\endhead` (false) or the table's end.
    end: bool,
    /// `\output{\LT@output}` is in force.
    pub(super) output: bool,
    /// The number of open `tabular`s whose innermost is the longtable's own.
    depth: usize,
}

/// An open `tabularx`: its body, and `\TX@endtabularx`'s registers between trials.
#[derive(PartialEq, Clone)]
pub(super) struct Tx {
    /// `\TX@target`, `\TX@col@width`, `\TX@old@table`, `\TX@old@col`.
    target: i32,
    cw: i32,
    old_table: i32,
    old_col: i32,
    /// `\TX@cols`, counted in the first trial (`count`) at each `X` the preambles rewrite.
    cols: i32,
    count: bool,
    /// The tokens after the width to `\end{tabularx}`: `[pos]{spec}` and the rows.
    body: Vec<Token>,
}

/// `\quad` in an `array` column specification, as one byte.
pub(super) const QUAD: u8 = 1;

/// siunitx's `S[table-format=a.b]` in a column specification, as the byte `SI_COLUMN +
/// 10a + b`.
pub(super) const SI_COLUMN: u8 = 0x80;

/// An `S` column's `[table-format=a.b]` at the start of `s` (spaces around the key and
/// value allowed, as l3keys trims them): the format `10a + b` and the bytes taken; `None` for
/// any other option or none (`table-alignment-mode = marker`).
pub(super) fn si_format(s: &str) -> Option<(u8, usize)> {
    let r = s.strip_prefix('[')?;
    let k = r.find(']')?;
    let (key, val) = r[..k].split_once('=')?;
    let v = val.trim().as_bytes();
    (key.trim() == "table-format" && v.len() == 3 && v[0].is_ascii_digit() && v[1] == b'.' && v[2].is_ascii_digit() && v[0] > b'0').then(|| ((v[0] - b'0') * 10 + v[2] - b'0', k + 2))
}

/// An `array` column specification as source text, `\quad` as [`QUAD`]; `None` for any
/// other control sequence.
pub(super) fn array_spec(s: &str) -> Option<Vec<u8>> {
    let s = s.replace("\\quad", "\x01");
    (!s.contains('\\')).then(|| s.into_bytes())
}

/// A preamble: the steps of all columns, and per column where its ⟨u⟩ starts, where `#`
/// is (its ⟨v⟩ starts) and where its ⟨v⟩ ends.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Preamble {
    pub(super) ops: Vec<Cell>,
    pub(super) cols: Vec<[u16; 3]>,
}

/// An open `tabular`: its preamble, the number of open lists when it began (its `\\` is
/// the innermost while no list opened since), and the glue of a pending `\\[...]` whose
/// space is not positive (`\@yargarraycr`'s `\noalign`).
#[derive(PartialEq, Clone)]
pub(super) struct Tab {
    pub(super) pre: Preamble,
    pub(super) lists: usize,
    skip: Glue,
    /// The depth of `\@arraycr`'s strut rule, for `\rt@arstrut`.
    pub(super) strut: i32,
    /// The one-column preamble of the `\multicolumn` being read.
    mc: Option<Preamble>,
    /// An `array` (math entries, `\@arraycr`), not a `tabular`.
    pub(super) math: bool,
    /// Built by array.sty's `\@mkpream` ([`mkpream_pkg`]); `strut` is then the depth of the
    /// pending `\do@row@strut`, 0 for none.
    pub(super) pkg: bool,
    /// `\if@nobreak` and `\if@noskipsec` outside the open `p` entry.
    pcell: (bool, bool),
    /// The `\parbox`es and minipages open around the tabular: in one opened inside it, `\\`
    /// is `\@normalcr`.
    pboxes: usize,
    /// Where an entry's steps stopped, for tokens to be read or (`Cell::PEnd`) a paragraph to
    /// start: the `\multicolumn`'s preamble or the tabular's, the next step, the column's
    /// end, and the `PEnd` to finish first (0 none, 1 the kernel's, 2 array.sty's).
    resume: (bool, u16, u16, u8),
    /// The tokens a siunitx `S` entry collected ([`Cell::SiBegin`]).
    si: Vec<Token>,
    /// colortbl's `\CT@start`: `\CT@row@color` before the table, which `\CT@end` puts back.
    ct_save: Option<String>,
}

/// One item of a column specification as `\@tfor` reads it: a character, or a braced
/// group's contents.
#[derive(Clone, Copy, PartialEq)]
enum Item<'a> {
    Ch(u8),
    Group(&'a [u8]),
}

/// The items of `s` (spaces skipped, as undelimited arguments skip them); `None` for an
/// unbalanced group.
fn items(s: &[u8]) -> Option<Vec<Item<'_>>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'{' => {
                let mut depth = 0;
                let start = i + 1;
                loop {
                    match s.get(i)? {
                        b'{' => depth += 1,
                        b'}' if depth == 1 => break,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                out.push(Item::Group(&s[start..i]));
                i += 1;
            }
            b'}' => return None,
            c => {
                out.push(Item::Ch(c));
                i += 1;
            }
        }
    }
    Some(out)
}

/// `\@expast`: each `*{n}{cols}` replaced by `n` copies of `cols`, the first star first and
/// the result read again (`\@xexpast`), until none is left.
fn expast(spec: &[u8]) -> Option<Vec<u8>> {
    let mut s = spec.to_vec();
    for _ in 0..64 {
        let Some(star) = top_level_star(&s) else { return Some(s) };
        let rest = &s[star + 1..];
        let it = items(rest)?;
        let (count, body) = (it.first()?, it.get(1)?);
        let digits = match count {
            Item::Ch(c) => std::slice::from_ref(c),
            Item::Group(g) => g,
        };
        let digits = std::str::from_utf8(digits).ok()?.trim();
        let n: usize = digits.parse().ok().filter(|n| *n <= 256)?;
        let body: Vec<u8> = match body {
            Item::Ch(c) => vec![*c],
            Item::Group(g) => g.to_vec(),
        };
        // The bytes after the body: skip the two items in the text itself.
        let after = skip_items(rest, 2)?;
        let mut next = s[..star].to_vec();
        for _ in 0..n {
            next.extend_from_slice(&body);
        }
        next.extend_from_slice(&rest[after..]);
        if next.len() > 4096 {
            return None;
        }
        s = next;
    }
    None
}

/// What [`nc_rewrite`] needs to know of a token.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Nk {
    Open,
    Close,
    Space,
    /// `#`.
    Param,
    /// The digit after a `#`, or a number's.
    Digit(u8),
    Other,
}

/// A column type of array.sty's `\NC@list`: its letter, number of arguments and replacement
/// (`\NC@rewrite@<c>`); `None` for `*` (`\NC@rewrite@*`, which appends its copies to
/// `\@temptokena` without reading them again in that pass).
pub(super) type ColType<'a, T> = (T, usize, Option<&'a [T]>);

/// array.sty's rewriting of a column specification in `\@mkpream`: `\@whilesw\if@tempswa\fi
/// {\@tempswafalse\the\NC@list}`. Each `\NC@do<c>` reads the specification once
/// (`\NC@find@<c>`: the text to the next `c` outside braces, an argument whose outer braces a
/// lone group loses, then `\NC@rewrite@<c>` with its undelimited arguments, the replacement read
/// again), with `c` appended as the end; another pass of the whole list follows while one
/// rewrote. `None` for what would be a TeX error (an argument taken from the end marker) or a
/// runaway.
pub(super) fn nc_rewrite<T: Copy + PartialEq>(spec: &[T], types: &[ColType<'_, T>], kind: impl Fn(T) -> Nk) -> Option<Vec<T>> {
    // An undelimited argument at `s[i..]`: its tokens and the offset after it.
    let arg = |s: &[T], mut i: usize| -> Option<(std::ops::Range<usize>, usize)> {
        while kind(*s.get(i)?) == Nk::Space {
            i += 1;
        }
        if kind(s[i]) != Nk::Open {
            return (kind(s[i]) != Nk::Close).then_some((i..i + 1, i + 1));
        }
        let mut depth = 0;
        for (k, &t) in s.iter().enumerate().skip(i) {
            match kind(t) {
                Nk::Open => depth += 1,
                Nk::Close if depth == 1 => return Some((i + 1..k, k + 1)),
                Nk::Close => depth -= 1,
                _ => {}
            }
        }
        None
    };
    let mut cur = spec.to_vec();
    for _ in 0..64 {
        let mut changed = false;
        for &(c, nargs, body) in types {
            let mut out = Vec::with_capacity(cur.len());
            let mut rest = cur.clone();
            rest.push(c);
            loop {
                // `\NC@find@<c>`'s argument: to the first `c` at brace depth 0.
                let mut depth = 0i32;
                let k = rest.iter().position(|&t| {
                    match kind(t) {
                        Nk::Open => depth += 1,
                        Nk::Close => depth -= 1,
                        _ => {}
                    }
                    depth == 0 && t == c
                })?;
                let seg = &rest[..k];
                let lone = seg.len() >= 2 && kind(seg[0]) == Nk::Open && arg(seg, 0).is_some_and(|(_, e)| e == seg.len());
                out.extend_from_slice(if lone { &seg[1..seg.len() - 1] } else { seg });
                // `\NC@ifend`: the end marker read, `\relax` follows.
                if k + 1 == rest.len() {
                    break;
                }
                changed = true;
                let after = &rest[k + 1..];
                let mut at = 0;
                let mut args = Vec::with_capacity(nargs.max(2));
                for _ in 0..if body.is_some() { nargs } else { 2 } {
                    let (r, e) = arg(after, at)?;
                    // The end marker is not an argument.
                    if e >= after.len() {
                        return None;
                    }
                    args.push(after[r].to_vec());
                    at = e;
                }
                let tail = &after[at..];
                let next = match body {
                    // `\count@#1\relax`, then `#2` that many times onto `\@temptokena`.
                    None => {
                        let mut n = 0usize;
                        for &t in &args[0] {
                            match kind(t) {
                                Nk::Digit(d) => n = n * 10 + d as usize,
                                Nk::Space => {}
                                _ => return None,
                            }
                        }
                        if n > 256 {
                            return None;
                        }
                        for _ in 0..n {
                            out.extend_from_slice(&args[1]);
                        }
                        tail.to_vec()
                    }
                    // The replacement with `#n` its arguments (`##` a `#`), read again.
                    Some(b) => {
                        let mut v = Vec::with_capacity(b.len() + tail.len());
                        let mut j = 0;
                        while j < b.len() {
                            if kind(b[j]) == Nk::Param {
                                match b.get(j + 1).map(|&t| (t, kind(t))) {
                                    Some((_, Nk::Digit(d))) if d >= 1 && (d as usize) <= args.len() => v.extend_from_slice(&args[d as usize - 1]),
                                    Some((t, Nk::Param)) => v.push(t),
                                    _ => return None,
                                }
                                j += 2;
                                continue;
                            }
                            v.push(b[j]);
                            j += 1;
                        }
                        v.extend_from_slice(tail);
                        v
                    }
                };
                if out.len() + next.len() > 8192 {
                    return None;
                }
                rest = next;
            }
            cur = out;
        }
        if !changed {
            return Some(cur);
        }
    }
    None
}

/// A token of a column specification's source text, for [`nc_rewrite`] in the pre-scan.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum St<'a> {
    Cs(&'a str),
    Ch(u8),
}

/// The tokens of `s` (a control word, a control symbol, a character; a run of spaces one).
pub(super) fn st_tokens(s: &str) -> Option<Vec<St<'_>>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count().max(1);
                out.push(St::Cs(s.get(i + 1..i + 1 + n)?));
                i += 1 + n;
                // Spaces after a control word are skipped.
                if b[i - 1].is_ascii_alphabetic() {
                    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
                        i += 1;
                    }
                }
            }
            c if c.is_ascii_whitespace() => {
                if out.last() != Some(&St::Ch(b' ')) {
                    out.push(St::Ch(b' '));
                }
                i += 1;
            }
            b'%' => i = s[i..].find('\n').map_or(b.len(), |n| i + n + 1),
            c if c.is_ascii() => {
                out.push(St::Ch(c));
                i += 1;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// [`nc_rewrite`]'s view of a source token.
pub(super) fn st_kind(t: St) -> Nk {
    match t {
        St::Ch(b'{') => Nk::Open,
        St::Ch(b'}') => Nk::Close,
        St::Ch(b' ') => Nk::Space,
        St::Ch(b'#') => Nk::Param,
        St::Ch(c) if c.is_ascii_digit() => Nk::Digit(c - b'0'),
        _ => Nk::Other,
    }
}

/// [`nc_rewrite`]'s view of a token.
fn tok_kind(t: Token) -> Nk {
    match t.cat() {
        Some(Cat::BeginGroup) => Nk::Open,
        Some(Cat::EndGroup) => Nk::Close,
        Some(Cat::Space) => Nk::Space,
        Some(Cat::Param) => Nk::Param,
        Some(Cat::Other) if t.chr().is_ascii_digit() => Nk::Digit(t.chr() as u8 - b'0'),
        _ => Nk::Other,
    }
}

/// Source tokens as text again, a space after each control word.
pub(super) fn st_text(t: &[St]) -> String {
    let mut s = String::new();
    for x in t {
        match x {
            St::Cs(n) => {
                s.push('\\');
                s.push_str(n);
                if n.bytes().all(|c| c.is_ascii_alphabetic()) {
                    s.push(' ');
                }
            }
            St::Ch(c) => s.push(*c as char),
        }
    }
    s
}

/// The position of the first `*` outside braces.
fn top_level_star(s: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    for (k, &c) in s.iter().enumerate() {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'*' if depth == 0 => return Some(k),
            _ => {}
        }
    }
    None
}

/// The offset in `s` just past its first `n` items.
fn skip_items(s: &[u8], n: usize) -> Option<usize> {
    let mut i = 0;
    for _ in 0..n {
        while s.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
            i += 1;
        }
        if *s.get(i)? == b'{' {
            let mut depth = 0;
            loop {
                match s.get(i)? {
                    b'{' => depth += 1,
                    b'}' if depth == 1 => break,
                    b'}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
        }
        i += 1;
    }
    Some(i)
}

/// `\@mkpream` for `array` (`\@arrayclassz`, `\@arrayclassiv`, `\@arrayacol`): each entry
/// `\hfil$\relax#$\hfil` (`l` without the first `\hfil`, `r` without the second), the
/// column space `\arraycolsep`, and an `@{...}` set as a formula, `$$` or `$\quad$`.
pub(super) fn mkpream_array(spec: &[u8]) -> Option<Preamble> {
    mkpream_in(spec, true)
}

/// array.sty's `\@mkpream` for a `tabular` (its `\@testpach` classes: 0 a column, 1 `|`, 7
/// `@`, 5 `@`'s argument; `\@lastchclass` starts at 4): `\@classx` before a column, each
/// entry `\hfil\hskip1sp\d@llarbegin\insert@column\d@llarend\do@row@strut\hfil` (`l`
/// without the first `\hfil`, `r` without the last), `|` as `\vline` after `\@classvi`'s
/// space, an empty `@{}` an empty group. In an `array` (`math`) `\col@sep` is `\arraycolsep`
/// and `\d@llarbegin`/`\d@llarend` are `$`: the entry `$\ignorespaces#\relax$` (array.sty's
/// `\textonly@unskip` does nothing in math), an `@{}` the formula `$\relax$`, `@{\quad}` `$\quad
/// \relax$`. `None` beyond `c`, `l`, `r`, `|`, `@{}` and an array's `@{\quad}`.
pub(super) fn mkpream_pkg(spec: &[u8], math: bool) -> Option<Preamble> {
    mkpream_widths(spec, math, &[], &[], false)
}

/// [`mkpream_pkg`] with `p` columns and declarations, in a `tabular`: a `p{k}` takes the
/// width `widths[k]` (array.sty's `\@testpach`: class 10 at the `p`, whose `\@classx` places
/// the column space, then its argument as class 0 with `\@chnum` 4); a `{tk}` after `>`,
/// `<`, `!` or `@` is the token list `toks[k]` (`\save@decl`, `\@classv`). With colortbl
/// (`ct`, colortbl.sty's `\@classz`), each entry inside `\setbox\z@\hbox{{...}}`
/// ([`Cell::CtOpen`], [`Cell::CtClose`]): `c` as `\hskip\stretch{.5}\kern\z@ ...
/// \hskip\stretch{.5}`, `l` ending `\hfill`, `r` starting `\hfill\kern\z@`, no `\hskip1sp`;
/// `p` as array.sty's. Not in formulas or `S` columns.
pub(super) fn mkpream_widths(spec: &[u8], math: bool, widths: &[i32], toks: &[Span], ct: bool) -> Option<Preamble> {
    if ct && math {
        return None;
    }
    const AMP: u8 = 0;
    const SHARP: u8 = 1;
    // Where a column's `<{...}` tokens go (`\the@toks\the\count@`).
    const AFTER: u8 = 2;
    let tok = |item: Item| match item {
        Item::Group([b't', k @ ..]) => toks.get(std::str::from_utf8(k).ok()?.parse::<usize>().ok()?).copied(),
        _ => None,
    };
    // A `>{...}` waiting for its column.
    let mut before: Option<Span> = None;
    let spec = expast(spec)?;
    let mut list: Vec<Result<Cell, u8>> = vec![Ok(Cell::Strut)];
    let mut first_amp = true;
    let mut last = 4u8;
    let acol = if math { Cell::ArrayAcol } else { Cell::Acol };
    // `\@addamp`.
    fn add_amp(list: &mut Vec<Result<Cell, u8>>, first: &mut bool) {
        if !std::mem::replace(first, false) {
            list.push(Err(AMP));
        }
    }
    for item in items(&spec)? {
        // `p`'s argument: `\vtop \@startpbox{w}\insert@pcolumn \@endpbox \do@row@strut`,
        // `\insert@pcolumn` being `\relax \ignorespaces \@sharp \unskip \relax`.
        if last == 10 {
            let Item::Group(g) = item else { return None };
            let w = *widths.get(std::str::from_utf8(g).ok()?.parse::<usize>().ok()?)?;
            if ct {
                list.push(Ok(Cell::CtOpen));
            }
            list.push(Ok(Cell::PStart(w, true)));
            list.extend(before.take().map(|t| Ok(Cell::Toks(t))));
            list.extend([Ok(Cell::Ignore), Err(SHARP), Ok(Cell::Unskip), Err(AFTER), Ok(Cell::PEnd(true)), Ok(Cell::Hfil), Ok(Cell::RowStrut)]);
            if ct {
                list.push(Ok(Cell::CtClose));
            }
            last = 0;
            continue;
        }
        match last {
            // `>`'s argument (`\save@decl`), for the next column.
            9 => {
                before = Some(tok(item)?);
                last = 3;
                continue;
            }
            // `<`'s (`\@classii`), for the column before.
            8 => {
                let t = tok(item)?;
                let at = list.iter().rposition(|c| *c == Err(AFTER))?;
                list[at] = Ok(Cell::Toks(t));
                last = 2;
                continue;
            }
            // `!`'s or `@`'s (`\@classv`): `\d@llarbegin\the@toks\relax\d@llarend`.
            6 | 7 if !math && tok(item).is_some() => {
                list.extend([Ok(Cell::Open), Ok(Cell::Toks(tok(item)?)), Ok(Cell::Close)]);
                last = if last == 6 { 1 } else { 5 };
                continue;
            }
            _ => {}
        }
        let (class, num) = if last == 7 {
            (5, 0)
        } else {
            let Item::Ch(c) = item else { return None };
            match c {
                b'c' => (0, 0),
                b'l' => (0, 1),
                b'r' => (0, 2),
                b'p' if !math => (10, 4),
                b'|' => (1, 0),
                b'@' => (7, 0),
                b'>' if !math && before.is_none() => (9, 0),
                b'<' if !math && last == 0 => (8, 0),
                b'!' if !math => (6, 0),
                // siunitx's `S[table-format=a.b]` as one byte (`SI_COLUMN + 10a + b`):
                // `> {...\siunitx_cell_begin:w} c < {\siunitx_cell_end:}`, no `>` of its own.
                c if c >= SI_COLUMN && !math && before.is_none() => (0, c),
                _ => return None,
            }
        };
        match class {
            0 | 9 | 10 => {
                // `\@classx`.
                match last {
                    0 | 2 => {
                        list.push(Ok(acol));
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(acol));
                    }
                    1 => {
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(acol));
                    }
                    4 => {
                        list.push(Ok(acol));
                        first_amp = false;
                    }
                    5 => add_amp(&mut list, &mut first_amp),
                    3 => {}
                    _ => return None,
                }
                if class != 0 {
                    last = class;
                    continue;
                }
                if ct {
                    if num >= SI_COLUMN {
                        return None;
                    }
                    list.push(Ok(Cell::CtOpen));
                    match num {
                        0 => list.extend([Ok(Cell::HalfFill), Ok(Cell::Kern0)]),
                        2 => list.extend([Ok(Cell::Hfill), Ok(Cell::Kern0)]),
                        _ => {}
                    }
                    list.push(Ok(Cell::Open));
                    list.extend(before.take().map(|t| Ok(Cell::Toks(t))));
                    list.extend([Ok(Cell::Ignore), Err(SHARP), Ok(Cell::HUnskip), Err(AFTER), Ok(Cell::Close), Ok(Cell::RowStrut)]);
                    match num {
                        0 => list.push(Ok(Cell::HalfFill)),
                        1 => list.push(Ok(Cell::Hfill)),
                        _ => {}
                    }
                    list.push(Ok(Cell::CtClose));
                    last = 0;
                    continue;
                }
                if num != 1 {
                    list.push(Ok(Cell::Hfil));
                }
                if num >= SI_COLUMN {
                    // The collection takes `\ignorespaces` and, in the ⟨v⟩, `\textonly@unskip`.
                    let f = num - SI_COLUMN;
                    list.extend([Ok(Cell::OneSp), Ok(Cell::Open), Ok(Cell::SiBegin(f)), Err(SHARP), Ok(Cell::SiEnd(f)), Ok(Cell::Close), Ok(Cell::RowStrut), Ok(Cell::Hfil)]);
                    last = 0;
                    continue;
                }
                if math {
                    list.extend([Ok(Cell::OneSp), Ok(Cell::MathOn), Ok(Cell::Ignore), Err(SHARP), Ok(Cell::MathOff), Ok(Cell::RowStrut)]);
                } else {
                    list.extend([Ok(Cell::OneSp), Ok(Cell::Open)]);
                    list.extend(before.take().map(|t| Ok(Cell::Toks(t))));
                    list.extend([Ok(Cell::Ignore), Err(SHARP), Ok(Cell::HUnskip), Err(AFTER), Ok(Cell::Close), Ok(Cell::RowStrut)]);
                }
                if num != 2 {
                    list.push(Ok(Cell::Hfil));
                }
            }
            // `\@classvi`, then `\@arrayrule` (`!` its argument instead).
            1 | 6 => {
                match last {
                    0 | 2 => list.push(Ok(acol)),
                    1 => list.push(Ok(Cell::DoubleSep)),
                    3 => return None,
                    _ => {}
                }
                if class == 6 {
                    last = 6;
                    continue;
                }
                list.push(Ok(Cell::VLine));
            }
            8 => {
                last = 8;
                continue;
            }
            // `\@classv`: `\d@llarbegin\the@toks\relax\d@llarend`, nothing for `@{}`.
            5 if item == Item::Group(&[]) && !math => {}
            5 if item == Item::Group(&[]) => list.extend([Ok(Cell::MathOn), Ok(Cell::MathOff)]),
            5 if math && item == Item::Group(&[QUAD]) => list.extend([Ok(Cell::MathOn), Ok(Cell::Quad), Ok(Cell::MathOff)]),
            7 => {}
            _ => return None,
        }
        last = class;
    }
    match last {
        0 | 2 => list.push(Ok(acol)),
        1 | 5 => {}
        _ => return None,
    }
    list.retain(|c| *c != Err(AFTER));
    split_cols(list, AMP, SHARP)
}

/// Whether the pre-scan takes a `tabular` specification: the kernel's [`mkpream_kernel`], or
/// with array.sty [`mkpream_widths`], each `p{...}` standing for a width the native job reads
/// (`spec_widths`; a width that is not one dimension declines there).
pub(super) fn spec_ok(spec: &str, array: bool, x: bool, si: bool, types: &[(u8, usize, &str)]) -> bool {
    spec_cols(spec, array, x, si, types).is_some()
}

/// [`spec_ok`]'s preamble's number of columns. `types` are the document's `\newcolumntype`s
/// (letter, arguments, replacement), rewritten first as array.sty does ([`nc_rewrite`]).
pub(super) fn spec_cols(spec: &str, array: bool, x: bool, si: bool, types: &[(u8, usize, &str)]) -> Option<usize> {
    // siunitx replaces a document's `S` (`\__siunitx_declare_column:Nnn`).
    if si && types.iter().any(|t| t.0 == b'S') {
        return None;
    }
    let rewritten;
    let spec = if array && !types.is_empty() {
        let bodies = types.iter().map(|t| st_tokens(t.2)).collect::<Option<Vec<_>>>()?;
        let mut list = vec![(St::Ch(b'*'), 2, None)];
        list.extend(types.iter().zip(&bodies).map(|(t, b)| (St::Ch(t.0), t.1, Some(&b[..]))));
        rewritten = st_text(&nc_rewrite(&st_tokens(spec)?, &list, st_kind)?);
        &rewritten[..]
    } else {
        spec
    };
    // `\NC@ecs`: an `\extracolsep` becomes tabskip glue, which the steps do not set.
    if spec.contains("extracolsep") {
        return None;
    }
    let (b, mut out) = (spec.as_bytes(), Vec::with_capacity(spec.len()));
    let (mut n, mut m) = (0, 0);
    let mut i = 0;
    while i < b.len() {
        if b.get(i + 1) == Some(&b'{') && matches!(b[i], b'p' | b'>' | b'<' | b'!' | b'@') {
            let k = super::balanced(&spec[i + 2..])?;
            out.push(b[i]);
            if b[i] == b'p' {
                out.extend(format!("{{{n}}}").bytes());
                n += 1;
            } else if k == 0 {
                out.extend(b"{}");
            } else {
                out.extend(format!("{{t{m}}}").bytes());
                m += 1;
            }
            i += 2 + k + 1;
            continue;
        }
        if b[i] == b'\\' {
            return None;
        }
        if b[i] == b'S' && si {
            let (f, k) = si_format(&spec[i + 1..])?;
            out.push(SI_COLUMN + f);
            i += 1 + k;
            continue;
        }
        // tabularx's `X`, in and around its tables.
        if b[i] == b'X' && x {
            out.extend(format!("p{{{n}}}").bytes());
            n += 1;
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    let (w, t) = (vec![0; n], vec![Span::default(); m]);
    if array { mkpream_widths(&out, false, &w, &t, false) } else { mkpream_kernel(&out, false, &w, &t) }.map(|p| p.cols.len())
}

/// A step list with `&` (`amp`) and `#` (`sharp`) marks cut into columns, each one `#`.
fn split_cols(list: Vec<Result<Cell, u8>>, amp: u8, sharp: u8) -> Option<Preamble> {
    let mut pre = Preamble { ops: Vec::with_capacity(list.len()), cols: Vec::new() };
    let mut col = [0u16, u16::MAX, 0];
    for step in list {
        let at = pre.ops.len() as u16;
        match step {
            Ok(c) => pre.ops.push(c),
            Err(m) if m == sharp && col[1] == u16::MAX => col[1] = at,
            Err(m) if m == amp && col[1] != u16::MAX => {
                col[2] = at;
                pre.cols.push(col);
                col = [at, u16::MAX, 0];
            }
            Err(_) => return None,
        }
    }
    if col[1] == u16::MAX {
        return None;
    }
    col[2] = pre.ops.len() as u16;
    pre.cols.push(col);
    Some(pre)
}

/// `\@mkpream` for `tabular` (`\@tabclassz`, `\@classi`, `\@classii`, `\@tabclassiv`) or
/// `array` (`math`): the columns of `spec`, or `None` for what the native path does not take:
/// an `@{...}` that is not empty, an unknown character (`\@preamerr`), or no column.
fn mkpream_in(spec: &[u8], math: bool) -> Option<Preamble> {
    mkpream_kernel(spec, math, &[], &[])
}

/// [`mkpream_in`] with `p` columns (class 3, `\@classiii`'s spaces as a column's), each
/// argument `p{k}` (class 5, `\@classv`) the width `widths[k]`: `\@startpbox{w}\ignorespaces
/// \@sharp\@endpbox`.
pub(super) fn mkpream_kernel(spec: &[u8], math: bool, widths: &[i32], toks: &[Span]) -> Option<Preamble> {
    const AMP: u8 = 0;
    const SHARP: u8 = 1;
    let spec = expast(spec)?;
    // The preamble as a step list with `&` and `#` marks (`None`), `\@firstamptrue`,
    // `\@lastchclass6`.
    let mut list: Vec<Result<Cell, u8>> = vec![Ok(Cell::Strut)];
    let mut first_amp = true;
    let mut last = 6u8;
    let acol = if math { Cell::ArrayAcol } else { Cell::Acol };
    // `\@addamp`.
    fn add_amp(list: &mut Vec<Result<Cell, u8>>, first: &mut bool) {
        if !std::mem::replace(first, false) {
            list.push(Err(AMP));
        }
    }
    for item in items(&spec)? {
        // `\@testpach`: the class of the item, and for a column its alignment.
        let ch = match item {
            Item::Ch(c) => Some(c),
            Item::Group([c]) => Some(*c),
            Item::Group(_) => None,
        };
        if last == 3 {
            let Item::Group(g) = item else { return None };
            let w = *widths.get(std::str::from_utf8(g).ok()?.parse::<usize>().ok()?)?;
            list.extend([Ok(Cell::PStart(w, false)), Ok(Cell::Ignore), Err(SHARP), Ok(Cell::PEnd(false)), Ok(Cell::Hfil)]);
            last = 5;
            continue;
        }
        let (class, num) = if last == 2 {
            (4, 0)
        } else {
            match ch? {
                b'c' => (0, 0),
                b'l' => (0, 1),
                b'r' => (0, 2),
                b'p' if !math => (3, 0),
                b'|' => (1, 0),
                b'@' => (2, 0),
                _ => return None,
            }
        };
        match class {
            0 | 3 => {
                match last {
                    0 | 5 => {
                        list.push(Ok(acol));
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(acol));
                    }
                    1 => {
                        add_amp(&mut list, &mut first_amp);
                        list.push(Ok(acol));
                    }
                    4 => add_amp(&mut list, &mut first_amp),
                    6 => {
                        first_amp = false;
                        list.push(Ok(acol));
                    }
                    _ => {}
                }
                if class == 3 {
                    last = 3;
                    continue;
                }
                if math {
                    if num != 1 {
                        list.push(Ok(Cell::Hfil));
                    }
                    list.extend([Ok(Cell::MathOn), Err(SHARP), Ok(Cell::MathOff)]);
                    if num != 2 {
                        list.push(Ok(Cell::Hfil));
                    }
                    last = class;
                    continue;
                }
                // `{\hfil\hskip1sp\ignorespaces\@sharp\unskip\hfil}` for `c`; `l` has no
                // `\hfil` before, `r` none after.
                list.push(Ok(Cell::Open));
                if num != 1 {
                    list.push(Ok(Cell::Hfil));
                }
                list.extend([Ok(Cell::OneSp), Ok(Cell::Ignore), Err(SHARP), Ok(Cell::Unskip)]);
                if num != 2 {
                    list.push(Ok(Cell::Hfil));
                }
                list.push(Ok(Cell::Close));
            }
            1 => match last {
                0 | 5 => list.extend([Ok(acol), Ok(Cell::Rule)]),
                1 => list.extend([Ok(Cell::DoubleSep), Ok(Cell::Rule)]),
                4 | 6 => list.push(Ok(Cell::Rule)),
                _ => {}
            },
            2 => {
                if last == 1 {
                    list.push(Ok(Cell::HalfRule));
                }
            }
            // `@{...}`: only an empty one adds nothing to a `tabular`'s preamble; an
            // `array`'s is a formula, `$$` or `$\quad$`.
            4 if item == Item::Group(&[]) && !math => {}
            // `\@tabclassiv`: the tokens themselves.
            4 if !math => {
                let Item::Group([b't', k @ ..]) = item else { return None };
                list.push(Ok(Cell::Toks(*toks.get(std::str::from_utf8(k).ok()?.parse::<usize>().ok()?)?)));
            }
            4 if item == Item::Group(&[]) => list.extend([Ok(Cell::MathOn), Ok(Cell::MathOff)]),
            4 if math && item == Item::Group(&[QUAD]) => list.extend([Ok(Cell::MathOn), Ok(Cell::Quad), Ok(Cell::MathOff)]),
            _ => return None,
        }
        last = class;
    }
    match last {
        0 | 5 => list.push(Ok(acol)),
        1 | 4 => {}
        _ => return None,
    }
    split_cols(list, AMP, SHARP)
}

/// The characters of a column specification read as an argument; `None` when it holds a
/// control sequence.
fn spec_bytes(spec: &[Token]) -> Option<Vec<u8>> {
    spec.iter().map(|t| if t.cs_id().is_some() { None } else { u8::try_from(t.chr() as u32).ok() }).collect()
}

/// `.5\arrayrulewidth` as TeX scans it (§455: `xn_over_d(v, 32768, 65536)`, rounding toward
/// zero).
fn half(w: i32) -> i32 {
    (w as i64 * 32768 / 65536) as i32
}

impl Machine<'_, '_> {
    /// `\begin{tabular}`'s `\tabular` after `\begin`'s group: `\leavevmode \hbox \bgroup $`,
    /// then (after `\everymath`) [`Self::tabular_array`].
    pub(super) fn tabular_open(&mut self) {
        if self.mode == Mode::Math {
            return self.not_native("tabular in math");
        }
        if self.leave_vmode() {
            return;
        }
        let pos = if self.next_is('[') { self.bracket_arg().first().map(|t| t.chr()) } else { None };
        let mut spec = Vec::new();
        self.read_arg(&mut spec);
        let pkg = self.array_pkg();
        let ct = self.colortbl();
        let pre = self.spec_widths(&spec, pkg).and_then(|(s, w, t)| if pkg { mkpream_widths(&s, false, &w, &t, ct) } else { mkpream_kernel(&s, false, &w, &t) });
        let Some(pre) = pre else { return self.not_native("tabular preamble") };
        let Some(doc) = self.doc.as_mut() else { return };
        let (lists, pboxes) = (doc.lists.len(), doc.pbox.len());
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), strut: 0, mc: None, math: false, pkg, pcell: (false, false), resume: (false, 0, 0, 0), pboxes, si: Vec::new(), ct_save: doc.ct_row.clone() });
        let group = match pos {
            Some('t') => VTOP_GROUP,
            Some('b') => VBOX_GROUP,
            _ => VCENTER_GROUP,
        };
        doc.tab_group = group;
        self.open_box(HBOX_GROUP, Ctx::Shift(0), false, 0, false);
        // `$`: `\everymath` runs before the rest of `\@tabular`.
        if let Some(doc) = self.doc.as_mut() {
            doc.steps.push(vec![super::Op::Call(|m| m.tabular_array())]);
        }
        let step = self.doc_id(super::STEP);
        self.input.back(step);
        self.enter_text_math();
    }

    /// `\begin{array}`'s `\array` after `\begin`'s group, in a formula: `[pos]{spec}`, then
    /// [`Self::tabular_array`]. amsmath's `cases` and matrices open it with their own
    /// specification.
    pub(super) fn array_open(&mut self) {
        if self.mode != Mode::Math {
            return self.not_native("array outside a formula");
        }
        let pos = if self.next_is('[') { self.bracket_arg().first().map(|t| t.chr()) } else { None };
        let mut spec = Vec::new();
        self.read_arg(&mut spec);
        let quad = self.named_id(named!("quad"));
        let bytes: Option<Vec<u8>> = spec
            .iter()
            .map(|t| match t.cs_id() {
                Some(c) if c == quad => Some(QUAD),
                Some(_) => None,
                None => u8::try_from(t.chr() as u32).ok(),
            })
            .collect();
        let Some(bytes) = bytes else { return self.not_native("array preamble") };
        self.array_open_spec(pos, &bytes);
    }

    /// The `array` of specification `spec` (`\quad` as [`QUAD`]) at position `pos`.
    pub(super) fn array_open_spec(&mut self, pos: Option<char>, spec: &[u8]) {
        let pkg = self.array_pkg();
        // colortbl's `\@classz` in a formula: not native yet.
        if self.colortbl() {
            return self.not_native("colortbl: an array");
        }
        let Some(pre) = (if pkg { mkpream_pkg(spec, true) } else { mkpream_array(spec) }) else { return self.not_native("array preamble") };
        let Some(doc) = self.doc.as_mut() else { return };
        let (lists, pboxes) = (doc.lists.len(), doc.pbox.len());
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), strut: 0, mc: None, math: true, pkg, pcell: (false, false), resume: (false, 0, 0, 0), pboxes, si: Vec::new(), ct_save: doc.ct_row.clone() });
        doc.tab_group = match pos {
            Some('t') => VTOP_GROUP,
            Some('b') => VBOX_GROUP,
            _ => VCENTER_GROUP,
        };
        self.tabular_array();
    }

    /// amsmath's `\hskip -\arraycolsep` on either side of a matrix.
    pub(super) fn matrix_skip(&mut self) {
        let w = self.rw(R_ARRAYCOLSEP);
        self.build_glue(Glue { width: -w, ..Glue::default() }, true);
    }

    /// `\m@th`, then `\@array`: the `\vtop`, `\vbox` or `\vcenter`, `\@arstrutbox`, no
    /// `\lineskip` or `\baselineskip`, and `\ialign` with the preamble.
    fn tabular_array(&mut self) {
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: MATH_SURROUND }, 0, false);
        let group = self.doc.as_ref().map_or(VCENTER_GROUP, |d| d.tab_group);
        self.open_box(group, Ctx::Shift(0), false, 0, false);
        // `\setbox\@arstrutbox\hbox{\vrule \@height\arraystretch\ht\strutbox
        // \@depth\arraystretch\dp\strutbox \@width\z@}`.
        let Some(stretch) = self.array_stretch() else { return self.not_native("an \\arraystretch that is not a number") };
        let (ht, dp) = (stretch(self.strut_height()), stretch(self.strut_depth()));
        self.push_nest(Mode::Horizontal, true, 1000);
        self.build_rule([0, ht, dp]);
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.box_end(Ctx::Set { n: ARSTRUT_BOX, global: false }, Some(idx));
        for slot in [LINE_SKIP, BASELINE_SKIP] {
            self.e.eqtb.set_glue(Reg { region: Region::Glue, slot }, Glue::default(), false);
        }
        // `\def\par{\ifnum\currentgrouptype=6 \else\@@par\fi}`, and `\let\\\@tabularcr`
        // (`\\`'s own native, which finds the tabular innermost).
        if let Some((par, nl)) = self.doc.as_ref().map(|d| (d.par_id, d.newline_id)) {
            self.e.eqtb.set_meaning(par, Meaning::Doc(TAB_PAR as u16), false);
            self.e.eqtb.set_meaning(nl, Meaning::Doc(super::NEWLINE as u16), false);
        }
        // `\ialign`'s `\everycr{}`: an enclosing `align`'s does not reach the rows. Under
        // colortbl `\ialign` sets colortbl's own `\everycr` register, and the primitive is
        // `\CT@everycr`: `\noalign{..}` stepping `\rownum` and choosing the row's colour
        // (`ct_cr`), empty in a tabular nested in an entry (`\CT@everycr{}` there).
        {
            let reg = param!("everycr");
            let outer = self.doc.as_ref().is_some_and(|d| d.tabs.len() == 1 && !d.tabs[0].math);
            let list = if self.colortbl() && outer {
                let (open, close) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'));
                vec![self.named_cs(named!("noalign")), open, self.doc_id(doc!("rt@ctcr")), close]
            } else {
                Vec::new()
            };
            let ct = !list.is_empty();
            let span = self.e.toks.store(&list);
            self.e.eqtb.set_toks(reg, span, false);
            // §785: `\everycr` also follows the preamble's `\cr` (`init_align_cols` does not
            // insert it): its `\noalign` adds nothing, so its step runs here.
            if ct {
                self.ct_cr();
            }
        }
        let n = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(0, |t| t.pre.cols.len());
        let u = [self.doc_id(TAB_U)];
        let v = [self.doc_id(TAB_V), Token::cs(END_TEMPLATE_ID)];
        let (u, v) = (self.e.toks.store(&u), self.e.toks.store(&v));
        // `tabular*`'s `\halign to`.
        match self.doc.as_mut().and_then(|d| d.tab_to.take()) {
            Some(w) => self.init_align_cols(&vec![(u, v); n], vec![Glue::default(); n + 1], true, w),
            None => self.init_align_with(n, u, v),
        }
    }

    /// `\arraystretch` as the factor of `\arraystretch\ht\strutbox`: 1 unless the document
    /// redefined it, else its digits read as §448–§455 read a decimal before an internal
    /// dimension (`nx_plus_y(n, v, xn_over_d(v, f, 65536))`, the sign last); `None` for a
    /// body that is not a plain decimal.
    fn array_stretch(&self) -> Option<impl Fn(i32) -> i32> {
        self.macro_factor(self.doc.as_ref()?.stretch_id, (false, 1, 0))
    }

    /// `\rt@tabu` and `\rt@tabv`: the ⟨u⟩ or ⟨v⟩ steps of the entry's column.
    pub(super) fn tabular_template(&mut self, v: bool) {
        let k = self.align_column();
        self.cell_steps(false, k, v);
    }

    /// The ⟨u⟩ (`v` false) or ⟨v⟩ steps of column `k` of the tabular's preamble, or of the
    /// `\multicolumn`'s (`mc`). The steps are read in place, one byte each.
    fn cell_steps(&mut self, mc: bool, k: usize, v: bool) {
        fn pre<'m>(m: &'m Machine<'_, '_>, mc: bool) -> Option<&'m Preamble> {
            m.doc.as_ref().and_then(|d| d.tabs.last()).and_then(|t| if mc { t.mc.as_ref() } else { Some(&t.pre) })
        }
        let Some([a, b]) = pre(self, mc).and_then(|p| p.cols.get(k)).map(|c| if v { [c[1], c[2]] } else { [c[0], c[1]] }) else { return };
        self.run_cells(mc, a, b);
    }

    /// Steps `a..b` of the `\multicolumn`'s preamble (`mc`) or the tabular's, until one
    /// waits for tokens or a paragraph ([`Self::cell_resume`] goes on).
    fn run_cells(&mut self, mc: bool, a: u16, b: u16) {
        fn pre<'m>(m: &'m Machine<'_, '_>, mc: bool) -> Option<&'m Preamble> {
            m.doc.as_ref().and_then(|d| d.tabs.last()).and_then(|t| if mc { t.mc.as_ref() } else { Some(&t.pre) })
        }
        for i in a as usize..b as usize {
            let Some(op) = pre(self, mc).map(|p| p.ops[i]) else { return };
            if let Cell::Toks(span) = op {
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.resume = (mc, i as u16 + 1, b, 0);
                }
                if let Some(d) = self.doc.as_mut() {
                    d.steps.push(vec![super::Op::Call(|m| m.cell_resume())]);
                }
                let mut list = self.e.toks.get(span).to_vec();
                list.push(self.doc_id(super::STEP));
                return self.back_list(&list);
            }
            if let Cell::SiEnd(f) = op {
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.resume = (mc, i as u16 + 1, b, 0);
                }
                if self.si_cell_end(f) {
                    return;
                }
                continue;
            }
            // `\@finalstrut`'s `\vrule` in vertical mode starts a paragraph (§1090), whose
            // `\everypar` runs first: the rest of the steps wait for it.
            if let (Cell::PEnd(pkg), false) = (op, self.mode == Mode::Horizontal) {
                self.unskip();
                // The kernel's `\@finalstrut` has no `\lastnodetype` test (also in
                // `\LT@endpbox` without array.sty).
                if !pkg || self.last_node_type() > -1 || !self.array_pkg() && self.in_longtable() {
                    let g = self.glue_param(BASELINE_SKIP);
                    self.build_glue(Glue { width: -g.width, stretch: -g.stretch, shrink: -g.shrink, ..g }, false);
                }
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.resume = (mc, i as u16 + 1, b, 1 + pkg as u8);
                }
                return self.run_ops(vec![super::Op::Call(|m| m.new_graf(true)), super::Op::Call(|m| m.cell_resume())]);
            }
            if let Cell::PEnd(pkg) = op {
                self.unskip();
                self.build_penalty(10000);
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.resume = (mc, i as u16 + 1, b, 0);
                }
                if self.pend_close(pkg) {
                    return;
                }
                continue;
            }
            self.cell_step(op);
        }
    }

    /// The rest of a `p` entry's [`Cell::PEnd`] whose paragraph had to start first, and the
    /// steps after it.
    fn cell_resume(&mut self) {
        let Some((mc, i, b, pend)) = self.doc.as_ref().and_then(|d| d.tabs.last()).map(|t| t.resume) else { return };
        if pend > 0 && self.pend_close(pend == 2) {
            return;
        }
        self.run_cells(mc, i, b);
    }

    pub(in crate::engine) fn is_hdotsfor(&self, m: Meaning) -> bool {
        m == Meaning::Doc(HDOTSFOR as u16)
    }

    pub(in crate::engine) fn is_multicolumn(&self, m: Meaning) -> bool {
        (m == Meaning::Doc(MULTICOLUMN as u16) || m == Meaning::Doc(HDOTSFOR as u16)) && self.in_tabular()
    }

    /// `\multicolumn{n}{spec}{text}` as an entry's first token, after its `\omit`:
    /// `\@multispan`'s `n - 1` times `\span\omit`, then (`\rt@mcbody`) `\begingroup
    /// \@arstrut \@preamble` with `text` at the `#`, and (`\rt@mctail`) the rest of the
    /// preamble, `\hbox{}\endgroup\ignorespaces`. amsmath's `\hdotsfor` (`hdots`) is
    /// `\multicolumn{n}c{dots}` ([`Machine::hdots_for`]).
    pub(in crate::engine) fn multicolumn(&mut self, hdots: bool) {
        let (mut n, mut spec, mut text) = (Vec::new(), Vec::new(), Vec::new());
        if hdots {
            let Some((k, dots)) = self.hdots_for() else { return };
            n.extend(k.chars().map(|c| Token::char(Cat::Other, c)));
            spec.push(Token::char(Cat::Letter, 'c'));
            text = dots;
        } else {
            self.read_arg(&mut n);
            self.read_arg(&mut spec);
            self.read_arg(&mut text);
        }
        let n = spec_bytes(&n).and_then(|b| std::str::from_utf8(&b).ok()?.trim().parse::<usize>().ok()).filter(|n| (1..=256).contains(n));
        // In an `array` the kernel's `\multicolumn` builds its column as `array` does.
        let (math, pkg) = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or((false, false), |t| (t.math, t.pkg));
        let ct = self.colortbl();
        let pre = if math { spec_bytes(&spec).and_then(|s| if pkg { mkpream_pkg(&s, true) } else { mkpream_in(&s, true) }) } else { self.spec_widths(&spec, pkg).and_then(|(s, w, t)| if pkg { mkpream_widths(&s, false, &w, &t, ct) } else { mkpream_kernel(&s, false, &w, &t) }) };
        let pre = pre.filter(|p| p.cols.len() == 1);
        let (Some(n), Some(pre)) = (n, pre) else { return self.not_native("a \\multicolumn the native path does not take") };
        let Some(doc) = self.doc.as_mut() else { return };
        if let Some(t) = doc.tabs.last_mut() {
            t.mc = Some(pre);
        }
        let (span, omit) = (Token::cs(doc.span_id), Token::cs(doc.omit_id));
        let mut list = Vec::with_capacity(2 * n + text.len() + 2);
        for _ in 1..n {
            list.extend([span, omit]);
        }
        list.push(self.doc_id(MC_BODY));
        list.extend_from_slice(&text);
        list.push(self.doc_id(MC_TAIL));
        self.back_list(&list);
    }

    /// `\rt@mcbody` and `\rt@mctail`.
    pub(super) fn multicolumn_part(&mut self, tail: bool) {
        if !tail {
            self.begin_group(SEMI_SIMPLE_GROUP);
            return self.cell_steps(true, 0, false);
        }
        // The rest waits for the steps' tokens, if any.
        if let Some(d) = self.doc.as_mut() {
            d.steps.push(vec![super::Op::Call(|m| m.multicolumn_end())]);
        }
        let step = self.doc_id(super::STEP);
        self.input.back(step);
        self.cell_steps(true, 0, true);
    }

    /// `\hbox{}\endgroup\ignorespaces` after a `\multicolumn`'s ⟨v⟩ steps.
    fn multicolumn_end(&mut self) {
        self.build_hbox(|_| {});
        self.end_group(SEMI_SIMPLE_GROUP);
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.mc = None;
        }
        self.ignore_spaces();
    }

    /// `\cline{i-j}` (`\@cline`) at a row's start: an omitted entry spanning columns 1 to
    /// `i - 1` when `i > 1`, then one spanning `i` to `j` holding `\leaders\hrule
    /// \@height\arrayrulewidth\hfill`, `\cr`, and `\noalign{\vskip-\arrayrulewidth}`.
    fn cline(&mut self) -> Option<bool> {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let range = spec_bytes(&arg).and_then(|b| {
            let s = String::from_utf8(b).ok()?;
            let (i, j) = s.split_once('-')?;
            Some((i.trim().parse::<usize>().ok()?, j.trim().parse::<usize>().ok()?))
        });
        let Some((i, j)) = range.filter(|&(i, j)| i >= 1 && j >= i && j <= 256) else {
            self.not_native("a \\cline the native path does not take");
            return Some(false);
        };
        let w = self.rw(R_ARRAYRULEWIDTH);
        let doc = self.doc.as_mut()?;
        let (span, omit, cr) = (Token::cs(doc.span_id), Token::cs(doc.omit_id), Token::cs(doc.cr_id));
        if let Some(t) = doc.tabs.last_mut() {
            t.skip = Glue { width: -w, ..Glue::default() };
        }
        let mut list = Vec::with_capacity(2 * j + 4);
        list.push(omit);
        if i > 1 {
            for _ in 2..i {
                list.extend([span, omit]);
            }
            list.extend([Token::char(super::super::engine_catcode::Cat::AlignTab, '&'), omit]);
        }
        for _ in i..j {
            list.extend([span, omit]);
        }
        list.extend([self.doc_id(CLINE), cr, self.doc_id(TAB_SKIP)]);
        self.back_list(&list);
        Some(true)
    }

    /// `\cline`'s `\leaders\hrule\@height\arrayrulewidth\hfill` (rule leaders are glue of
    /// subtype 110 holding the rule's index, as `scan_box` makes them).
    pub(super) fn cline_rule(&mut self) {
        let w = self.rw(R_ARRAYRULEWIDTH);
        let r = self.e.nodes.rule_node([RUNNING, w, 0]);
        let mut n = self.e.nodes.glue_node(fil_glue(Prim::Hfill), 110);
        n.b = r.a;
        self.cur.push(n);
    }

    fn cell_step(&mut self, op: Cell) {
        let hskip = |m: &mut Self, w: i32| m.build_glue(Glue { width: w, ..Glue::default() }, true);
        match op {
            Cell::Strut => {
                if let Some(b) = self.e.eqtb.box_reg(ARSTRUT_BOX) {
                    let list = self.e.nodes.boxes[b as usize].list;
                    let mut cur = std::mem::take(&mut self.cur);
                    self.e.nodes.copy_into(list, &mut cur);
                    self.cur = cur;
                }
            }
            Cell::Acol => hskip(self, self.rw(R_TABCOLSEP)),
            Cell::Rule => {
                let w = self.rw(R_ARRAYRULEWIDTH);
                hskip(self, -half(w));
                self.build_rule([w, RUNNING, RUNNING]);
                hskip(self, -half(w));
            }
            Cell::DoubleSep => hskip(self, self.rw(R_DOUBLERULESEP)),
            Cell::HalfRule => hskip(self, half(self.rw(R_ARRAYRULEWIDTH))),
            Cell::Open => self.begin_group(SIMPLE_GROUP),
            Cell::Close => self.end_group(SIMPLE_GROUP),
            Cell::Hfil => {
                let n = self.e.nodes.glue_node(fil_glue(Prim::Hfil), 0);
                self.cur.push(n);
            }
            Cell::OneSp => hskip(self, 1),
            Cell::Hfill => {
                let n = self.e.nodes.glue_node(fil_glue(Prim::Hfill), 0);
                self.cur.push(n);
            }
            Cell::HalfFill => {
                let n = self.e.nodes.glue_node(Glue { stretch: 32768, stretch_order: 2, ..Glue::default() }, 0);
                self.cur.push(n);
            }
            Cell::Kern0 => self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) }),
            Cell::CtOpen => {
                self.open_box(HBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
                self.begin_group(SIMPLE_GROUP);
            }
            Cell::CtClose => self.ct_close(),
            Cell::Ignore => self.ignore_spaces(),
            Cell::Unskip => self.unskip(),
            Cell::HUnskip => {
                if self.mode == Mode::Horizontal {
                    self.unskip();
                }
            }
            Cell::Toks(_) | Cell::SiEnd(_) => {}
            Cell::SiBegin(_) => self.si_collect(),
            Cell::MathOn => self.enter_text_math(),
            Cell::MathOff => self.after_math(),
            Cell::ArrayAcol => hskip(self, self.rw(R_ARRAYCOLSEP)),
            Cell::Quad => self.build_quad(),
            Cell::VLine => self.build_rule([self.rw(R_ARRAYRULEWIDTH), RUNNING, RUNNING]),
            Cell::RowStrut => {
                let d = self.doc.as_mut().and_then(|d| d.tabs.last_mut()).map_or(0, |t| std::mem::take(&mut t.strut));
                if d > 0 {
                    self.build_rule([0, RUNNING, d]);
                }
            }
            Cell::PStart(w, pkg) => {
                self.open_box(VTOP_GROUP, Ctx::Shift(0), false, 0, false);
                if pkg && self.colored() {
                    self.begin_group(SEMI_SIMPLE_GROUP);
                }
                self.set_dimen_named(param!("hsize"), w);
                // `\let\if@nobreak\iffalse \let\if@noskipsec\iffalse`, local to the entry.
                if let Some(d) = self.doc.as_mut() {
                    let saved = (std::mem::take(&mut d.nobreak), std::mem::take(&mut d.noskipsec));
                    if let Some(t) = d.tabs.last_mut() {
                        t.pcell = saved;
                    }
                }
                // `\@arrayparboxrestore` leaves `\\` as it is: the tabular's.
                self.par_box_restore();
                if pkg {
                    self.set_everypar(super::P_CELL);
                }
            }
            // `\@finalstrut`: `\unskip`, then `\nobreak` in a paragraph, or
            // `\vskip-\baselineskip` after a node (`\lastnodetype > -1`); then `\vrule
            // \@width\z@ \@height\z@ \@depth\dp#1`, which in vertical mode starts a paragraph
            // (§1090). [`Self::run_cells`] runs it, as the steps after it may wait.
            Cell::PEnd(_) => {}
        }
    }

    /// [`Cell::CtClose`]: the entry's box `\z@` closed; with a row colour (`\rowcolors`),
    /// `\begingroup\color{c}` (its push), the leaders and glue of `\CT@@do@color` around
    /// the entry (`\@tempdimb`, `\@tempdimc` are `\col@sep`), `\endgroup` and its
    /// `\reset@color` (the pop); then the zero-width rule as tall as the entry and the entry.
    fn ct_close(&mut self) {
        self.end_group(SIMPLE_GROUP);
        self.package();
        let Some(b) = self.e.eqtb.box_reg(0) else { return };
        let r = self.e.nodes.boxes[b as usize];
        let color = self.doc.as_ref().and_then(|d| d.ct_row.clone());
        if let Some(spec) = color {
            let Some(lit) = self.color_literal(None, &spec, false) else { return self.not_native("a colour the native job does not know") };
            self.begin_group(SEMI_SIMPLE_GROUP);
            let Some(doc) = self.doc.as_mut() else { return };
            let i = match doc.colors.iter().position(|c| *c == lit) {
                Some(i) => i,
                None => {
                    doc.colors.push(lit.clone());
                    doc.colors.len() - 1
                }
            };
            self.e.eqtb.set_word(super::CURRENT_COLOR, i as i32 + 1, false);
            self.color_stack(Some(&lit));
            let sep = self.rw(R_TABCOLSEP);
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -sep) });
            // `\vrule` with no size: 0.4pt wide, its height and depth the row's.
            let rule = self.e.nodes.rule_node([26214, RUNNING, RUNNING]);
            let mut n = self.e.nodes.glue_node(Glue { width: r.width + 2 * sep, stretch: 65536, stretch_order: 2, ..Glue::default() }, 110);
            n.b = rule.a;
            self.cur.push(n);
            self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, -sep) });
            let n = self.e.nodes.glue_node(Glue { width: -r.width, stretch: -65536, stretch_order: 2, ..Glue::default() }, 0);
            self.cur.push(n);
            self.end_group(SEMI_SIMPLE_GROUP);
            self.color_stack(None);
        }
        self.build_rule([0, r.height, RUNNING]);
        self.cur.extend_from_slice(self.e.nodes.get(r.list));
        self.e.eqtb.replace_box(0, None);
        self.e.nodes.free_block(r.list);
        self.e.nodes.free_box(b);
    }

    /// colortbl's `\CT@everycr` after each `\cr` of an outer tabular: with `\rowcolors
    /// {first}{odd}{even}` in force, `\global\advance\rownum\@ne` and the row's colour
    /// (none before `first`, then odd or even rows'; an empty colour none); without, none
    /// (`\global\let\CT@row@color\relax`).
    pub(super) fn ct_cr(&mut self) {
        let k = self.e.eqtb.word(super::ROWCOLORS);
        let Some(doc) = self.doc.as_mut() else { return };
        if k <= 0 {
            doc.ct_row = None;
            return;
        }
        let Some((first, odd, even)) = doc.ct_specs.get(k as usize - 1).cloned() else { return };
        doc.ct_num += 1;
        doc.ct_row = if doc.ct_num < first { None } else if doc.ct_num % 2 != 0 { odd } else { even };
    }

    /// colortbl's `\rowcolors{first}{odd}{even}`: `\global\rownum\z@`, the colours for the
    /// `\CT@everycr` it sets (local), then `\ignorespaces`. `\rowcolors*` and the optional
    /// commands are not native yet.
    pub(super) fn row_colors(&mut self) {
        if self.next_is('*') || self.next_is('[') {
            return self.not_native("\\rowcolors* or with commands");
        }
        let mut a = [Vec::new(), Vec::new(), Vec::new()];
        for x in &mut a {
            self.read_arg(x);
        }
        let [Some(first), Some(odd), Some(even)] = a.map(|x| x.iter().map(|t| t.cs_id().is_none().then(|| t.chr())).collect::<Option<String>>()) else {
            return self.not_native("\\rowcolors with commands");
        };
        let Ok(first) = first.trim().parse::<i32>() else { return self.not_native("\\rowcolors with a first row that is not a number") };
        let color = |c: String| (!c.trim().is_empty()).then(|| c.trim().to_string());
        let Some(doc) = self.doc.as_mut() else { return };
        doc.ct_specs.push((first, color(odd), color(even)));
        doc.ct_num = 0;
        let k = doc.ct_specs.len() as i32;
        self.e.eqtb.set_word(super::ROWCOLORS, k, false);
        self.ignore_spaces();
    }

    /// `\@finalstrut`'s rule in the entry's paragraph, then `\par \color@endgroup \egroup`.
    /// `true` when the box waits for the colour group's `\aftergroup` tokens: the steps after
    /// it then go on from `Tab::resume`.
    fn pend_close(&mut self, pkg: bool) -> bool {
        let dp = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].depth);
        self.build_rule([0, 0, dp]);
        self.end_graf();
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.resume.3 = 0;
        }
        if pkg && self.colored() {
            self.after_color_group(|m| {
                m.pend_box();
                m.cell_resume();
            });
            return true;
        }
        self.pend_box();
        false
    }

    /// The `p` entry's `\egroup`, outside its paragraph's groups.
    fn pend_box(&mut self) {
        if let Some(d) = self.doc.as_mut() {
            if let Some(t) = d.tabs.last() {
                (d.nobreak, d.noskipsec) = t.pcell;
            }
        }
        self.package();
    }

    /// [`Cell::SiBegin`]: `\__siunitx_table_collect_loop:` (siunitx.sty:5264), the entry's
    /// tokens up to its end, a group whole and spaces outside groups dropped
    /// (`\peek_remove_spaces:n`). The end is the ⟨v⟩ template, which an `&` or `\cr` inserts
    /// (§342), or `\\` (`\tabularnewline`), which runs before the entry is printed
    /// (`\__siunitx_table_collect_pre_cr:`). Outside groups only characters are taken: a
    /// control sequence there declines.
    fn si_collect(&mut self) {
        let Some(nl) = self.doc.as_ref().map(|d| Token::cs(d.newline_id)) else { return };
        let v = self.doc_id(TAB_V);
        let mut toks = Vec::new();
        let mut depth = 0usize;
        loop {
            let Some(t) = self.get_token() else { return };
            if depth == 0 {
                if t == v || t == nl {
                    self.input.back(t);
                    break;
                }
                match (t.cs_id(), t.cat()) {
                    (None, Some(Cat::Space)) => continue,
                    (None, Some(Cat::Letter | Cat::Other | Cat::BeginGroup)) => {}
                    _ => return self.not_native("an S entry the native path does not take"),
                }
            }
            match (t.cs_id(), t.cat()) {
                (None, Some(Cat::BeginGroup)) => depth += 1,
                (None, Some(Cat::EndGroup)) => depth -= 1,
                _ => {}
            }
            toks.push(t);
        }
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.si = toks;
        }
    }

    /// [`Cell::SiEnd`]: `\__siunitx_table_collect_end:` with format `10a + b`
    /// (`table-alignment-mode = format`, `table-number-alignment` and
    /// `table-text-alignment = center`). A lone group or nothing is text
    /// (`\__siunitx_table_print_text:n`): `\hskip 0pt plus .5fill \kern0pt`, the group's
    /// tokens, the same skip again (`\__siunitx_table_align_center:n`); `true` as the steps
    /// after wait for those tokens. A number (`\__siunitx_table_print_format:nnn`) is four
    /// boxes between those skips: the text before (none, `\hbox to 0pt{\hfil}`), the integer
    /// part right-aligned in the width of `a` eights (`\hbox to W{\hfil\hfil$int$}`, from
    /// `\__siunitx_table_print_format_auxi:w` and `auxii:w`), the decimal part at its own width
    /// (`\hbox{$.dec$}`, empty with none), and the space the model's `.` and `b` eights leave
    /// (`\hbox to c{\hskip c\hfil}`, `table-align-text-after`). Signs, exponents and
    /// uncertainties, and a part longer than its format, decline.
    fn si_cell_end(&mut self, f: u8) -> bool {
        let toks = self.doc.as_mut().and_then(|d| d.tabs.last_mut()).map(|t| std::mem::take(&mut t.si)).unwrap_or_default();
        let lone = toks.len() >= 2 && toks[0].cat() == Some(Cat::BeginGroup) && toks[0].cs_id().is_none() && {
            let mut depth = 0i32;
            toks.iter().position(|t| {
                match (t.cs_id(), t.cat()) {
                    (None, Some(Cat::BeginGroup)) => depth += 1,
                    (None, Some(Cat::EndGroup)) => depth -= 1,
                    _ => {}
                }
                depth == 0
            }) == Some(toks.len() - 1)
        };
        let half = Glue { width: 0, stretch: 32768, shrink: 0, stretch_order: 2, shrink_order: 0 };
        if lone || toks.is_empty() {
            self.si_skip(half);
            if toks.is_empty() {
                self.si_skip(half);
                return false;
            }
            if let Some(d) = self.doc.as_mut() {
                d.steps.push(vec![super::Op::Call(|m| {
                    let half = Glue { width: 0, stretch: 32768, shrink: 0, stretch_order: 2, shrink_order: 0 };
                    m.si_skip(half);
                    m.cell_resume();
                })]);
            }
            let mut list = toks[1..toks.len() - 1].to_vec();
            list.push(self.doc_id(super::STEP));
            self.back_list(&list);
            return true;
        }
        let (a, b) = ((f / 10) as usize, (f % 10) as usize);
        let s: Option<Vec<u8>> = toks.iter().map(|t| (t.cs_id().is_none() && t.cat() == Some(Cat::Other)).then(|| t.chr() as u8)).collect();
        let parts = s.as_deref().and_then(super::siunitx::number).filter(|(i, d)| i.len() <= a && d.len() <= b).map(|(i, d)| (i.to_vec(), d.to_vec()));
        let Some((int, dec)) = parts else {
            self.not_native("an S entry the native path does not take");
            return true;
        };
        let eights = [b'8'; 9];
        let wi = self.si_width(&eights[..a], &[]);
        let wd = if b == 0 { Some(0) } else { self.si_width(&[], &eights[..b]) };
        let (Some(wi), Some(wd)) = (wi, wd) else {
            self.not_native("an S entry the native path does not take");
            return true;
        };
        self.si_skip(half);
        let fil = fil_glue(Prim::Hfil);
        self.build_hbox_to(0, |m| m.build_glue(fil, true));
        let mut ok = Some(());
        self.build_hbox_to(wi, |m| {
            m.build_glue(fil, true);
            m.build_glue(fil, true);
            ok = m.si_formula(&int, &[]);
        });
        let wdec = self.si_math_box(&[], &dec).and_then(|_| self.cur.last().map(|n| self.e.nodes.boxes[n.a as usize].width));
        let (Some(()), Some(wdec)) = (ok, wdec) else {
            self.not_native("an S entry the native path does not take");
            return true;
        };
        let carry = wd - wdec;
        self.build_hbox_to(carry, |m| {
            m.build_glue(Glue { width: carry, ..Glue::default() }, true);
            m.build_glue(fil, true);
        });
        self.si_skip(half);
        false
    }

    /// `\__siunitx_table_skip:n`: `\hskip g \kern\z@`.
    fn si_skip(&mut self, g: Glue) {
        self.build_glue(g, true);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
    }

    /// Whether array.sty is loaded: its `\@mkpream` builds the tables.
    fn array_pkg(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"array"))
    }

    /// Whether `\\` is the innermost `tabular`'s `\@tabularcr`.
    pub(super) fn in_tabular(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.tabs.last().is_some_and(|t| t.lists == d.lists.len() && t.pboxes == d.pbox.len()))
    }

    /// `\@tabularcr`: `{\ifnum0=`}\fi\@ifstar\@xtabularcr\@xtabularcr`, the brace hiding a
    /// following `&` from the alignment while `*` and `[` are looked for; then `\cr`, after
    /// `\@xargarraycr`'s strut for a positive space, or before `\@yargarraycr`'s
    /// `\noalign{\@vspace@calcify{#1}}` for another.
    pub(super) fn tabular_cr(&mut self) {
        // array.sty's `\@arraycr` is `\@tabularcr`: its entries' `$` are in the templates.
        if self.doc.as_ref().and_then(|d| d.tabs.last()).is_some_and(|t| t.math && !t.pkg) {
            return self.array_cr();
        }
        self.input.align_state += 1;
        let _ = self.next_is('*');
        let arg = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        self.input.align_state -= 1;
        let cr = Token::cs(self.doc.as_ref().map_or(0, |d| d.cr_id));
        let Some(arg) = arg else { return self.input.back(cr) };
        // `\ifdim #1>\z@`.
        let relax = self.named_cs(named!("relax"));
        let mut list = arg.clone();
        list.push(relax);
        self.back_list(&list);
        let d = self.scan_dimen(false).0;
        if self.get_token() != Some(relax) {
            return self.not_native("\\\\[...] that is not a dimension");
        }
        if d > 0 {
            // `\unskip`, then `\@tempdima #1 \advance\@tempdima \dp\@arstrutbox \vrule
            // \@height\z@ \@depth\@tempdima \@width\z@`; array.sty's `\@xargarraycr` leaves
            // the rule (with no `\@height`) to `\do@row@strut`.
            self.unskip();
            let dp = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].depth);
            match self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                Some(t) if t.pkg => t.strut = d + dp,
                _ => self.build_rule([0, 0, d + dp]),
            }
            return self.input.back(cr);
        }
        let Some(g) = self.glue_arg(&arg) else { return self.not_native("\\\\[...] that is not glue") };
        if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
            t.skip = g;
        }
        let list = [cr, self.doc_id(TAB_SKIP)];
        self.back_list(&list);
    }

    /// `\@arraycr`: `$` ends the entry's formula, then `{\ifnum0=`}\fi\@ifstar\@xarraycr
    /// \@xarraycr` as in a `tabular`, and `${}` before `\cr` (the last entry ends with an
    /// empty formula holding an empty group); `\\[x]` adds `\@xargarraycr`'s strut inside
    /// that formula for a positive space, `\@yargarraycr`'s `\noalign` after `\cr` for
    /// another.
    fn array_cr(&mut self) {
        if self.mode == Mode::Horizontal && self.in_tabular() {
            // After a `\multicolumn` the entry is text: the `$` opens a formula that holds
            // the empty group, `$` closes it, and an empty group is left: `${}${}\cr`.
            self.input.align_state += 1;
            let _ = self.next_is('*');
            let arg = self.next_is('[');
            self.input.align_state -= 1;
            if arg {
                return self.not_native("\\\\[...] after a \\multicolumn in an array");
            }
            let (open, close, shift) = (Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}'), Token::char(Cat::MathShift, '$'));
            let cr = Token::cs(self.doc.as_ref().map_or(0, |d| d.cr_id));
            return self.back_list(&[shift, open, close, shift, open, close, cr]);
        }
        if self.mode != Mode::Math {
            return self.not_native("\\\\ in an array outside its entry's formula");
        }
        self.after_math();
        self.input.align_state += 1;
        let _ = self.next_is('*');
        let arg = if self.next_is('[') { Some(self.bracket_arg()) } else { None };
        self.input.align_state -= 1;
        let cr = Token::cs(self.doc.as_ref().map_or(0, |d| d.cr_id));
        let mut tail = Vec::new();
        if let Some(arg) = arg {
            // `\ifdim #1>\z@`.
            let relax = self.named_cs(named!("relax"));
            let mut list = arg.clone();
            list.push(relax);
            self.back_list(&list);
            let d = self.scan_dimen(false).0;
            if self.get_token() != Some(relax) {
                return self.not_native("\\\\[...] that is not a dimension");
            }
            if d > 0 {
                // `\@tempdima #1 \advance\@tempdima \dp\@arstrutbox \vrule \@height\z@
                // \@depth\@tempdima \@width\z@`.
                let dp = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].depth);
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.strut = d + dp;
                }
                tail.extend([self.doc_id(doc!("rt@arstrut")), cr]);
            } else {
                let Some(g) = self.glue_arg(&arg) else { return self.not_native("\\\\[...] that is not glue") };
                if let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.last_mut()) {
                    t.skip = g;
                }
                tail.extend([cr, self.doc_id(TAB_SKIP)]);
            }
        } else {
            tail.push(cr);
        }
        let mut list = vec![Token::char(Cat::MathShift, '$'), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')];
        list.extend(tail);
        self.back_list(&list);
    }

    /// What the next entry's first token (`x`, §785) expands to in LaTeX when it is a native:
    /// `Some(true)` to look again, `Some(false)` when done, `None` for a row's first token.
    pub(in crate::engine) fn align_peek_native(&mut self, x: X) -> Option<bool> {
        // amsmath's `align`: its `\everycr` and its end.
        match x.m {
            Meaning::Doc(i) if i as usize == super::AL_ALIGN_CR => {
                self.align_every_cr();
                return Some(false);
            }
            Meaning::Doc(i) if i as usize == super::AL_END => {
                self.align_finish();
                return Some(false);
            }
            Meaning::Doc(i) if i as usize == doc!("rt@adgap") => {
                self.aligned_gap();
                return Some(false);
            }
            Meaning::Doc(i) if i as usize == doc!("rt@adend") => {
                self.aligned_finish();
                return Some(false);
            }
            Meaning::Doc(i) if i as usize == doc!("rt@algap") => {
                self.align_gap();
                return Some(false);
            }
            Meaning::Doc(i) if i as usize == doc!("rt@eqgap") => {
                self.eqnarray_gap();
                return Some(false);
            }
            _ => {}
        }
        let doc = self.doc.as_ref()?;
        // `\end{aligned}` after a trailing `\\`: `\end` expands at the row's start, and its
        // `\crcr` ends nothing.
        if self.in_aligned() && x.m == Meaning::Doc(super::END as u16) {
            self.end_env();
            return Some(true);
        }
        doc.tabs.last()?;
        if x.t == Token::cs(doc.par_id) {
            // `\def\par{\ifnum\currentgrouptype=6 \else\@@par\fi}`: between rows, nothing.
            return Some(true);
        }
        let Meaning::Doc(i) = x.m else { return None };
        match i as usize {
            super::END => {
                self.end_env();
                Some(true)
            }
            // tabularx's `\endtabular` expands at a row's start as `\end{tabular}` does.
            super::END_TABULAR => {
                self.tabular_end();
                Some(true)
            }
            HLINE => {
                self.hline();
                Some(false)
            }
            k if k == super::BT_RULES[5] => self.cmidrule(),
            k if super::BT_RULES.contains(&k) => {
                self.bt_rule(super::BT_RULES.iter().position(|&r| r == k).unwrap_or(0));
                Some(false)
            }
            super::BT_MORE => {
                // `\noalign{\relax}`.
                self.begin_group(NO_ALIGN_GROUP);
                self.normal_paragraph();
                self.end_no_align();
                Some(false)
            }
            super::BT_CMID_END => {
                self.cmid_end();
                Some(false)
            }
            CLINE_CMD => self.cline(),
            // longtable: `\endhead` and the table's end at a row's start, the save row, the
            // chunk's end; booktabs' `\@BLTrule`'s last `\noalign`.
            k if k == doc!("rt@endhead") || k == doc!("rt@ltend") => {
                self.lt_chunk_mark(k == doc!("rt@ltend"));
                Some(true)
            }
            k if k == doc!("rt@ltchunk") => {
                self.lt_save_row();
                Some(true)
            }
            k if k == doc!("rt@ltechunk") => {
                self.lt_echunk();
                Some(false)
            }
            k if k == doc!("rt@btltend") => {
                self.bt_lt_end();
                Some(false)
            }
            TAB_SKIP => {
                let g = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(Glue::default(), |t| t.skip);
                self.begin_group(NO_ALIGN_GROUP);
                self.normal_paragraph();
                self.build_glue(g, false);
                self.end_no_align();
                Some(false)
            }
            TAB_END => {
                self.tabular_close();
                Some(false)
            }
            _ => None,
        }
    }

    /// `\hline`: `\noalign{\ifnum0=`}\fi\hrule \@height\arrayrulewidth \futurelet
    /// \reserved@a\@xhline`, and for a second `\hline` next `\vskip\doublerulesep
    /// \vskip-\arrayrulewidth`, then the `\noalign`'s `}`.
    fn hline(&mut self) {
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let w = self.rw(R_ARRAYRULEWIDTH);
        self.build_rule([RUNNING, w, 0]);
        if let Some(t) = self.get_token() {
            let again = self.meaning(t) == Meaning::Doc(HLINE as u16);
            self.input.back(t);
            if again {
                // array.sty's `\@xhline` has no `\vskip-\arrayrulewidth`.
                let sep = self.rw(R_DOUBLERULESEP);
                self.build_glue(Glue { width: sep, ..Glue::default() }, true);
                if !self.array_pkg() {
                    self.build_glue(Glue { width: -w, ..Glue::default() }, true);
                }
            }
        }
        self.end_no_align();
    }

    /// booktabs' rules (`kind` 0 to 4: `\toprule`, `\midrule`, `\bottomrule`,
    /// `\specialrule{w}{above}{below}`, `\addlinespace[w]`), each `\noalign{\ifnum0=`}\fi`:
    /// `\@aboverulesep` (local), `\@belowrulesep` and `\@thisruleclass` (global), the width
    /// `[w]` or the default, then `\@BTrule`: `\vskip\@aboverulesep` for class 2 or after a
    /// row, `\vskip\doublerulesep` after another rule, `\hrule\@height\@thisrulewidth`, and
    /// `\@BTendrule`. `\addlinespace` is only `\@belowrulesep`, class 2 and `\@BTendrule`.
    fn bt_rule(&mut self, kind: usize) {
        let Some(r) = self.doc.as_ref().and_then(|d| d.booktabs) else { return };
        use super::{BT_ABOVE, BT_ABOVE_TOP, BT_ADD_SPACE, BT_AT_ABOVE, BT_AT_BELOW, BT_BELOW, BT_BELOW_BOTTOM, BT_HEAVY, BT_LIGHT, BT_LAST_CLASS, BT_THIS_CLASS, BT_THIS_WIDTH};
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let get = |m: &Self, i: usize| m.e.eqtb.word(r[i]);
        match kind {
            0..=2 => {
                let (above, below, width) = [(BT_ABOVE_TOP, BT_BELOW, BT_HEAVY), (BT_ABOVE, BT_BELOW, BT_LIGHT), (BT_ABOVE, BT_BELOW_BOTTOM, BT_HEAVY)][kind];
                self.e.eqtb.set_word(r[BT_AT_ABOVE], get(self, above), false);
                self.e.eqtb.set_word(r[BT_AT_BELOW], get(self, below), true);
                self.e.eqtb.set_word(r[BT_THIS_CLASS], 1, true);
                let w = if self.next_is('[') {
                    let arg = self.bracket_arg();
                    self.dimen_arg(arg)
                } else {
                    get(self, width)
                };
                self.e.eqtb.set_word(r[BT_THIS_WIDTH], w, true);
            }
            3 => {
                let (mut w, mut above, mut below) = (Vec::new(), Vec::new(), Vec::new());
                self.read_arg(&mut w);
                self.read_arg(&mut above);
                self.read_arg(&mut below);
                let v = self.dimen_arg(above);
                self.e.eqtb.set_word(r[BT_AT_ABOVE], v, false);
                let v = self.dimen_arg(below);
                self.e.eqtb.set_word(r[BT_AT_BELOW], v, true);
                self.e.eqtb.set_word(r[BT_THIS_CLASS], 2, true);
                let v = self.dimen_arg(w);
                self.e.eqtb.set_word(r[BT_THIS_WIDTH], v, true);
            }
            _ => {
                // `\@ifnextchar[` passes spaces; `\@addspace`'s `\futurelet` sees the next token.
                let bracket = self.next_is('[');
                let w = if bracket {
                    let arg = self.bracket_arg();
                    self.dimen_arg(arg)
                } else {
                    get(self, BT_ADD_SPACE)
                };
                self.e.eqtb.set_word(r[BT_AT_BELOW], w, true);
                self.e.eqtb.set_word(r[BT_THIS_CLASS], 2, true);
                return self.bt_end(r, !bracket);
            }
        }
        // In a longtable (`\hline` is `\LT@hline`): `\nobreak`, and `\@BLTrule` for the rule.
        let lt = self.in_longtable();
        if lt {
            self.build_penalty(10000);
        }
        let (class, last) = (get(self, BT_THIS_CLASS), get(self, BT_LAST_CLASS));
        let skip = if class == 2 || last == 0 {
            Some(get(self, BT_AT_ABOVE))
        } else if last == 1 {
            Some(self.rw(R_DOUBLERULESEP))
        } else {
            None
        };
        if let Some(v) = skip {
            self.build_glue(Glue { width: v, ..Glue::default() }, true);
        }
        if lt {
            return self.bt_lt_rule();
        }
        self.build_rule([RUNNING, get(self, BT_THIS_WIDTH), 0]);
        self.bt_end(r, true);
    }

    /// `\@BLTrule` (no trim): `\@setrulekerning{}`, `\@cmidlb\LT@cols`, the `\noalign`'s end,
    /// `\@cmidruleb` (a row `\multispan\LT@cols\unskip\hskip\z@{\leaders\hrule\@height
    /// \@thisrulewidth\hfill\kern\z@}\hskip\z@\cr`), then (`\rt@btltend`) `\noalign{..
    /// \futurenonspacelet\@tempa\@BTendrule`.
    fn bt_lt_rule(&mut self) {
        if self.next_is('(') {
            return self.not_native("a longtable rule with a trim");
        }
        let n = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(1, |t| t.pre.cols.len());
        let Some(doc) = self.doc.as_ref() else { return };
        let (span, omit, cr) = (Token::cs(doc.span_id), Token::cs(doc.omit_id), Token::cs(doc.cr_id));
        let hskip = self.named_cs(named!("hskip"));
        let zero = [Token::char(Cat::Other, '0'), Token::char(Cat::Letter, 's'), Token::char(Cat::Letter, 'p')];
        let mut list = vec![omit];
        for _ in 1..n {
            list.extend([span, omit]);
        }
        list.push(hskip);
        list.extend(zero);
        list.extend([self.doc_id(doc!("rt@cmidleaders")), hskip]);
        list.extend(zero);
        list.extend([cr, self.doc_id(doc!("rt@btltend"))]);
        self.back_list(&list);
        self.end_no_align();
    }

    /// `\rt@btltend` at a row's start: `\noalign{\ifnum0=`}\fi\futurenonspacelet\@tempa
    /// \@BTendrule`.
    fn bt_lt_end(&mut self) {
        let Some(r) = self.doc.as_ref().and_then(|d| d.booktabs) else { return };
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        self.bt_end(r, true);
    }

    /// `\@BTendrule` after `\futurenonspacelet\@tempa` (`skip`: the spaces before the next
    /// token are passed) or `\futurelet\@tempa`: `\@lastruleclass` the rule's class when
    /// another booktabs rule comes next, else 0; then `\vskip\@belowrulesep` unless 1, and
    /// the `\noalign`'s end.
    fn bt_end(&mut self, r: [Reg; 17], skip: bool) {
        use super::{BT_AT_BELOW, BT_LAST_CLASS, BT_THIS_CLASS};
        let next = if skip { self.next_unblank() } else { self.get_token() };
        let rule = next.is_some_and(|t| matches!(self.meaning(t), Meaning::Doc(i) if super::BT_RULES.contains(&(i as usize))));
        if let Some(t) = next {
            self.input.back(t);
        }
        let last = if rule { self.e.eqtb.word(r[BT_THIS_CLASS]) } else { 0 };
        self.e.eqtb.set_word(r[BT_LAST_CLASS], last, true);
        if last != 1 {
            let v = self.e.eqtb.word(r[BT_AT_BELOW]);
            self.build_glue(Glue { width: v, ..Glue::default() }, true);
        }
        self.end_no_align();
    }

    /// `\cmidrule[w](trim){a-b}`: in its `\noalign`, `\@thisrulewidth` (default
    /// `\cmidrulewidth`), the trim's kerns (`l`, `r`: `\cmidrulekern`), `\vskip\aboverulesep`
    /// after a row; then a row `\multispan{a-1}&\multispan{b-a+1}\hskip<l>{\leaders\hrule
    /// \@height\@thisrulewidth\hfill\kern\z@}\hskip<r>\cr` (`\@cmidrulea`, `\@cmidruleb`
    /// for `a` = 1; the `\unskip` meets an empty entry), and `\@xcmidrule`'s `\noalign`.
    fn cmidrule(&mut self) -> Option<bool> {
        use super::{BT_ABOVE, BT_CMID_KERN, BT_CMID_WIDTH, BT_LAST_CLASS, BT_THIS_WIDTH};
        let r = self.doc.as_ref()?.booktabs?;
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let w = if self.next_is('[') {
            let arg = self.bracket_arg();
            self.dimen_arg(arg)
        } else {
            self.e.eqtb.word(r[BT_CMID_WIDTH])
        };
        let (mut kl, mut kr) = (0, 0);
        if self.next_is('(') {
            let kern = self.e.eqtb.word(r[BT_CMID_KERN]);
            while let Some(t) = self.get_token() {
                match (t.cs_id(), t.chr()) {
                    (None, ')') => break,
                    (None, 'l') => kl = kern,
                    (None, 'r') => kr = kern,
                    _ => {
                        self.not_native("a \\cmidrule trim other than l and r");
                        return Some(false);
                    }
                }
            }
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let range = spec_bytes(&arg).and_then(|b| {
            let s = String::from_utf8(b).ok()?;
            let (i, j) = s.split_once('-')?;
            Some((i.trim().parse::<usize>().ok()?, j.trim().parse::<usize>().ok()?))
        });
        let Some((i, j)) = range.filter(|&(i, j)| i >= 1 && j >= i && j <= 256) else {
            self.not_native("a \\cmidrule the native path does not take");
            return Some(false);
        };
        self.e.eqtb.set_word(r[BT_THIS_WIDTH], w, true);
        if self.e.eqtb.word(r[BT_LAST_CLASS]) == 0 {
            let v = self.e.eqtb.word(r[BT_ABOVE]);
            self.build_glue(Glue { width: v, ..Glue::default() }, true);
        }
        let doc = self.doc.as_ref()?;
        let (span, omit, cr) = (Token::cs(doc.span_id), Token::cs(doc.omit_id), Token::cs(doc.cr_id));
        let hskip = self.named_cs(named!("hskip"));
        let sp = |v: i32| v.to_string().chars().map(|c| Token::char(Cat::Other, c)).chain("sp".chars().map(|c| Token::char(Cat::Letter, c))).collect::<Vec<_>>();
        let mut list = Vec::with_capacity(2 * j + 16);
        if i > 1 {
            list.push(omit);
            for _ in 2..i {
                list.extend([span, omit]);
            }
            list.push(Token::char(Cat::AlignTab, '&'));
        }
        list.push(omit);
        for _ in i..j {
            list.extend([span, omit]);
        }
        list.push(hskip);
        list.extend(sp(kl));
        list.extend([self.doc_id(doc!("rt@cmidleaders")), hskip]);
        list.extend(sp(kr));
        list.extend([cr, self.doc_id(super::BT_CMID_END)]);
        self.back_list(&list);
        self.end_no_align();
        Some(false)
    }

    /// `{\leaders\hrule\@height\@thisrulewidth\hfill\kern\z@}` (the leaders as `cline_rule`
    /// makes them).
    pub(super) fn cmid_leaders(&mut self) {
        let Some(r) = self.doc.as_ref().and_then(|d| d.booktabs) else { return };
        let w = self.e.eqtb.word(r[super::BT_THIS_WIDTH]);
        let rule = self.e.nodes.rule_node([RUNNING, w, 0]);
        let mut n = self.e.nodes.glue_node(fil_glue(Prim::Hfill), 110);
        n.b = rule.a;
        self.cur.push(n);
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) });
    }

    /// `\@xcmidrule`: after another `\cmidrule`, `\vskip-\@thisrulewidth`; before
    /// `\morecmidrules`, `\vskip\cmidrulesep`; both leave `\@lastruleclass` 1. Otherwise
    /// `\vskip\belowrulesep` and 0.
    fn cmid_end(&mut self) {
        use super::{BT_BELOW, BT_CMID_SEP, BT_LAST_CLASS, BT_THIS_WIDTH};
        let Some(r) = self.doc.as_ref().and_then(|d| d.booktabs) else { return };
        self.begin_group(NO_ALIGN_GROUP);
        self.normal_paragraph();
        let next = self.next_unblank();
        let m = next.map(|t| self.meaning(t));
        if let Some(t) = next {
            self.input.back(t);
        }
        let (v, last) = match m {
            Some(Meaning::Doc(i)) if i as usize == super::BT_RULES[5] => (-self.e.eqtb.word(r[BT_THIS_WIDTH]), 1),
            Some(Meaning::Doc(i)) if i as usize == super::BT_MORE => (self.e.eqtb.word(r[BT_CMID_SEP]), 1),
            _ => (self.e.eqtb.word(r[BT_BELOW]), 0),
        };
        self.build_glue(Glue { width: v, ..Glue::default() }, true);
        self.e.eqtb.set_word(r[BT_LAST_CLASS], last, true);
        self.end_no_align();
    }

    /// A dimension argument read as `#1\relax` would be.
    /// A column specification as bytes, each `p{w}` as `p{k}` with `w` (`\setlength\hsize{w}`,
    /// read here in the font of the table's start, as the entries read it) the `k`th width;
    /// `None` for another control sequence or a width that is not one dimension.
    fn spec_widths(&mut self, spec: &[Token], array: bool) -> Option<(Vec<u8>, Vec<i32>, Vec<Span>)> {
        let rewritten;
        let spec = match self.doc.as_ref().filter(|d| array && !d.coltypes.is_empty()) {
            Some(doc) => {
                let mut list = vec![(Token::char(Cat::Other, '*'), 2, None)];
                list.extend(doc.coltypes.iter().map(|t| (t.0, t.1, Some(&t.2[..]))));
                rewritten = nc_rewrite(spec, &list, tok_kind)?;
                &rewritten[..]
            }
            None => spec,
        };
        let (mut bytes,mut widths, mut toks, mut xs) = (Vec::with_capacity(spec.len()), Vec::new(), Vec::new(), Vec::new());
        let relax = self.named_cs(named!("relax"));
        let mut i = 0;
        while i < spec.len() {
            let t = spec[i];
            i += 1;
            let c = if t.cs_id().is_none() { t.chr() } else { '\\' };
            // tabularx's `X`: `p{\TX@col@width}` (`\NC@rewrite@X`).
            if c == 'X' {
                let cw = self.doc.as_ref().and_then(|d| d.tx.as_ref())?.cw;
                bytes.extend(format!("p{{{}}}", widths.len()).bytes());
                xs.push(widths.len());
                widths.push(cw);
                continue;
            }
            // siunitx's `S[table-format=a.b]`.
            if c == 'S' && self.doc.as_ref().is_some_and(|d| d.packages.contains(&"siunitx")) {
                if self.doc.as_ref().is_some_and(|d| d.coltypes.iter().any(|t| t.0.cs_id().is_none() && t.0.chr() == 'S')) {
                    return None;
                }
                let k = spec[i..].iter().position(|t| t.cs_id().is_none() && t.chr() == ']')?;
                let text: Option<String> = spec[i..=i + k].iter().map(|t| t.cs_id().is_none().then(|| t.chr())).collect();
                let (f, _) = si_format(&text?)?;
                bytes.push(SI_COLUMN + f);
                i += k + 1;
                continue;
            }
            if matches!(c, 'p' | '>' | '<' | '!' | '@') && spec.get(i).is_some_and(|n| n.cat() == Some(Cat::BeginGroup)) {
                let mut depth = 0;
                let start = i + 1;
                while i < spec.len() {
                    match spec[i].cat() {
                        Some(Cat::BeginGroup) => depth += 1,
                        Some(Cat::EndGroup) => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                }
                let mut arg = spec[start..i - 1].to_vec();
                bytes.push(c as u8);
                if c != 'p' {
                    if arg.is_empty() {
                        bytes.extend(b"{}");
                        continue;
                    }
                    // array.sty's templates have `\relax` after these but `>`'s.
                    if array && c != '>' {
                        arg.push(relax);
                    }
                    bytes.extend(format!("{{t{}}}", toks.len()).bytes());
                    toks.push(self.e.toks.store(&arg));
                    continue;
                }
                arg.push(relax);
                self.back_list(&arg);
                let (w, _) = self.scan_dimen(false);
                if self.get_token() != Some(relax) {
                    return None;
                }
                bytes.extend(format!("{{{}}}", widths.len()).bytes());
                widths.push(w);
                continue;
            }
            if t.cs_id().is_some() {
                return None;
            }
            bytes.push(u8::try_from(t.chr() as u32).ok()?);
        }
        // The first trial's `\global\advance\TX@cols\@ne` at each `X`, after `*` repeats it.
        if !xs.is_empty() {
            let n = expast(&bytes).map_or(0, |e| xs.iter().map(|k| e.windows(format!("p{{{k}}}").len()).filter(|w| *w == format!("p{{{k}}}").as_bytes()).count()).sum::<usize>());
            if let Some(tx) = self.doc.as_mut().and_then(|d| d.tx.as_mut()).filter(|t| t.count) {
                tx.cols += n as i32;
            }
        }
        Some((bytes, widths, toks))
    }

    /// array.sty's `\newcolumntype{c}[n]{text}` (preamble only): `\NC@find@<c>` and
    /// `\NC@rewrite@<c>` as an entry of `Doc.coltypes`, which [`nc_rewrite`] reads; a letter
    /// defined again keeps its place in `\NC@list`.
    pub(super) fn new_column_type(&mut self) {
        let mut c = Vec::new();
        self.read_arg(&mut c);
        let k = if self.next_is('[') { Self::plain_text(&self.bracket_arg()).and_then(|s| s.trim().parse::<usize>().ok()).filter(|k| *k <= 9) } else { Some(0) };
        let mut body = Vec::new();
        self.read_arg(&mut body);
        let (Some(k), [c]) = (k, &c[..]) else { return self.not_native("a \\newcolumntype the pre-scan does not take") };
        let Some(doc) = self.doc.as_mut().filter(|d| !d.body && c.cs_id().is_none()) else { return self.not_native("\\newcolumntype in the body") };
        match doc.coltypes.iter_mut().find(|t| t.0 == *c) {
            Some(t) => *t = (*c, k, body),
            None => doc.coltypes.push((*c, k, body)),
        }
    }

    /// `\begin{longtable}` inside `\begin`'s group: `\longtable`'s `\par` and `\@nobreakfalse`,
    /// then the body to `\end{longtable}` (as tabularx reads it). A body with a head
    /// (`\endhead`) makes two chunks whose widths `\LT@save@row` evens out from the `.aux`
    /// file's row: a trial pass measures them as the first run does, then the table is set
    /// with that row. Declined: what would count, label or write twice, a caption, footnotes,
    /// page breaks asked for, `\hline` (`\LT@hline`), `\kill`, the other heads and feet, `\\*`,
    /// 200 rows (`\LTchunksize`), and a table that does not end on the page it starts on
    /// (`lt_output`).
    pub(super) fn longtable_begin(&mut self) {
        // colortbl's row colours in a longtable (each chunk's `\LT@bchunk` runs the
        // preamble's `\cr` again): not native yet.
        if self.e.eqtb.word(super::ROWCOLORS) > 0 || self.doc.as_ref().is_some_and(|d| d.ct_row.is_some()) {
            return self.not_native("colortbl: row colours in a longtable");
        }
        self.par();
        if self.doc.as_ref().is_some_and(|d| d.noskipsec) {
            return self.not_native("a longtable after a run-in heading");
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.nobreak = false;
        }
        let mut head_args = Vec::new();
        if self.next_is('[') {
            head_args.push(Token::char(Cat::Other, '['));
            head_args.extend(self.bracket_arg());
            head_args.push(Token::char(Cat::Other, ']'));
        }
        let mut spec = Vec::new();
        self.read_arg(&mut spec);
        head_args.push(Token::char(Cat::BeginGroup, '{'));
        head_args.extend(spec);
        head_args.push(Token::char(Cat::EndGroup, '}'));
        let end = Token::cs(self.e.cs.intern("end"));
        let mut body = Vec::new();
        loop {
            let Some(t) = self.get_token() else { return self.not_native("a longtable with no end") };
            if t != end {
                body.push(t);
                continue;
            }
            let mut name = Vec::new();
            self.read_arg(&mut name);
            if name.iter().map(|t| t.chr()).collect::<String>() == "longtable" && name.iter().all(|t| t.cs_id().is_none()) {
                break;
            }
            body.push(end);
            body.push(Token::char(Cat::BeginGroup, '{'));
            body.extend(name);
            body.push(Token::char(Cat::EndGroup, '}'));
        }
        const NOT: [&str; 26] = ["footnote", "footnotemark", "footnotetext", "label", "refstepcounter", "stepcounter", "addtocounter", "setcounter", "caption", "write", "marginpar", "index", "longtable", "newcounter", "kill", "newpage", "pagebreak", "nopagebreak", "hline", "endfirsthead", "endfoot", "endlastfoot", "clearpage", "LTleft", "LTright", "cline"];
        let Some(nl) = self.doc.as_ref().map(|d| Token::cs(d.newline_id)) else { return };
        let (mut rows, mut heads) = (0, 0);
        for (k, t) in body.iter().enumerate() {
            let Some(id) = t.cs_id() else { continue };
            let name = self.e.cs.name(id);
            if NOT.contains(&name) {
                return self.not_native("a longtable with what the native path does not take");
            }
            if name == "endhead" {
                heads += 1;
            }
            if *t == nl || name == "tabularnewline" {
                rows += 1;
                if body.get(k + 1).is_some_and(|n| n.cs_id().is_none() && n.chr() == '*') {
                    return self.not_native("\\\\* in a longtable");
                }
            }
        }
        if heads > 1 || rows >= 199 {
            return self.not_native("a longtable of more than two chunks");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.lt = Some(Box::new(Lt { body, head_args, trial: heads > 0, chunk: 0, row: Vec::new(), head: None, left: Glue::default(), right: Glue::default(), end: false, output: false, depth: 0 }));
        self.lt_pass();
    }

    /// One pass over the table: `\longtable`'s `\begingroup`, `\LT@array` (the table counter,
    /// `\LTleft` and `\LTright` by the position, `\\`, `\par`, the strut box, the preamble),
    /// `\LT@bchunk`, the body and the table's end.
    fn lt_pass(&mut self) {
        let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) else { return };
        let (args, body, trial) = (lt.head_args.clone(), lt.body.clone(), lt.trial);
        self.begin_group(SEMI_SIMPLE_GROUP);
        // `\@kernel@refstepcounter{\LTcaptype}`: once, in the pass that stays.
        if !trial {
            let r = super::R_FIGURE + 1;
            let n = self.rw(r) + 1;
            self.set_rw(r, n, true);
            let prefix = self.doc.as_ref().map_or(String::new(), |d| d.the_prefix[1].clone());
            self.store_current_label(&format!("{prefix}{n}"));
            // hyperref's patch of `\LT@array`: `\hyper@makecurrent{table}`.
            if self.hyper() {
                // report: `\theHtable` is `\theHchapter.\the\value{table}` (`float_href`).
                let name = self.float_href(2, n);
                self.set_current_href(&name);
            }
        }
        let (pos, spec) = match args.first().map(|t| t.chr()) {
            Some('[') if args.get(2).is_some_and(|t| t.chr() == ']') => (args[1].chr(), &args[4..args.len() - 1]),
            Some('[') => return self.not_native("a longtable position of more than one letter"),
            _ => ('x', &args[1..args.len() - 1]),
        };
        let fill = Glue { stretch: 65536, stretch_order: 2, ..Glue::default() };
        let (left, right) = match pos {
            'l' => (Glue::default(), fill),
            'r' => (fill, Glue::default()),
            _ => (fill, fill),
        };
        let pkg = self.array_pkg();
        let spec = spec.to_vec();
        let ct = self.colortbl();
        let pre = self.spec_widths(&spec, pkg).and_then(|(s, w, t)| if pkg { mkpream_widths(&s, false, &w, &t, ct) } else { mkpream_kernel(&s, false, &w, &t) });
        let Some(mut pre) = pre else { return self.not_native("longtable preamble") };
        // Without array.sty, `\@startpbox` is `\vtop\LT@startpbox` and `\@endpbox` is
        // `\LT@endpbox`: array.sty's colour group and `\everypar`.
        for op in pre.ops.iter_mut() {
            match op {
                Cell::PStart(_, p) | Cell::PEnd(p) => *p = true,
                _ => {}
            }
        }
        let n = pre.cols.len();
        // `\let\\\LT@tabularcr`, `\let\tabularnewline\\`, `\let\par\@empty` (nothing in an
        // entry; a `p` entry's `\@arrayparboxrestore` brings `\@@par` back).
        if let Some((par, nl)) = self.doc.as_ref().map(|d| (d.par_id, d.newline_id)) {
            self.e.eqtb.set_meaning(par, Meaning::Doc(TAB_PAR as u16), false);
            self.e.eqtb.set_meaning(nl, Meaning::Doc(super::NEWLINE as u16), false);
        }
        // `\setbox\@arstrutbox\hbox{\vrule \@height\arraystretch\@tempdima \@depth
        // \arraystretch\dp\strutbox \@width\z@}`.
        let Some(stretch) = self.array_stretch() else { return self.not_native("an \\arraystretch that is not a number") };
        let (ht, dp) = (stretch(self.strut_height()), stretch(self.strut_depth()));
        self.push_nest(Mode::Horizontal, true, 1000);
        self.build_rule([0, ht, dp]);
        let start = self.start as usize;
        let b = self.hpack(start, 0, false);
        self.pop_nest();
        let idx = self.e.nodes.add_box(b);
        self.box_end(Ctx::Set { n: ARSTRUT_BOX, global: false }, Some(idx));
        // `\m@th`, `\everycr{..}` (only tagging), `\lineskip\z@\baselineskip\z@`.
        self.e.eqtb.set_word(Reg { region: Region::Dimen, slot: MATH_SURROUND }, 0, false);
        let reg = param!("everycr");
        let empty = self.e.toks.store(&[]);
        self.e.eqtb.set_toks(reg, empty, false);
        for slot in [LINE_SKIP, BASELINE_SKIP] {
            self.e.eqtb.set_glue(Reg { region: Region::Glue, slot }, Glue::default(), false);
        }
        let Some(doc) = self.doc.as_mut() else { return };
        let (lists, pboxes) = (doc.lists.len(), doc.pbox.len());
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), strut: 0, mc: None, math: false, pkg, pcell: (false, false), resume: (false, 0, 0, 0), pboxes, si: Vec::new(), ct_save: doc.ct_row.clone() });
        let depth = doc.tabs.len();
        let Some(lt) = doc.lt.as_mut() else { return };
        (lt.left, lt.right, lt.depth, lt.chunk, lt.head) = (left, right, depth, 0, None);
        // `\LT@make@row`: the row the `.aux` file gave, blank in the trial.
        if lt.row.len() != n {
            lt.row = vec![(1, 0); n];
        }
        let mut list = body;
        list.push(self.doc_id(doc!("rt@ltend")));
        self.back_list(&list);
        self.lt_bchunk();
    }

    /// `\LT@bchunk`: `\c@LT@chunks` up, `\setbox\z@\vbox\bgroup \tabskip\LTleft \halign
    /// to\hsize\bgroup \tabskip\z@ \@arstrut \@preamble \tabskip\LTright \cr`.
    fn lt_bchunk(&mut self) {
        let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) else { return };
        lt.chunk += 1;
        let (left, right) = (lt.left, lt.right);
        self.open_box(VBOX_GROUP, Ctx::Set { n: 0, global: false }, false, 0, false);
        let n = self.doc.as_ref().and_then(|d| d.tabs.last()).map_or(0, |t| t.pre.cols.len());
        let u = [self.doc_id(TAB_U)];
        let v = [self.doc_id(TAB_V), Token::cs(END_TEMPLATE_ID)];
        let (u, v) = (self.e.toks.store(&u), self.e.toks.store(&v));
        let mut skips = vec![Glue::default(); n + 1];
        (skips[0], skips[n]) = (left, right);
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        self.init_align_cols(&vec![(u, v); n], skips, true, hsize);
    }

    /// Whether the innermost `tabular` is a longtable's.
    fn in_longtable(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.lt.as_ref().is_some_and(|l| l.depth == d.tabs.len()))
    }

    /// `\endhead` (`end` false) or the table's end: `\crcr`, then (`\rt@ltchunk`) the chunk's
    /// end.
    pub(super) fn lt_chunk_mark(&mut self, end: bool) {
        if !self.in_longtable() {
            return self.not_native("\\endhead outside a longtable");
        }
        if let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) {
            lt.end = end;
        }
        let crcr = Token::cs(self.doc.as_ref().map_or(0, |d| d.crcr_id));
        let list = [crcr, self.doc_id(doc!("rt@ltchunk"))];
        self.back_list(&list);
    }

    /// `\LT@echunk`'s `\LT@save@row\cr`: per column `\LT@entry{chunk}{width}`, `\omit` with
    /// `\kern width` unless the width is this chunk's own; then (`\rt@ltechunk`) the rest.
    fn lt_save_row(&mut self) {
        let Some(doc) = self.doc.as_ref() else { return };
        let Some(lt) = doc.lt.as_ref() else { return };
        let (omit, cr) = (Token::cs(doc.omit_id), Token::cs(doc.cr_id));
        let kern = self.named_cs(named!("kern"));
        let mut list = Vec::with_capacity(lt.row.len() * 8 + 2);
        for (j, &(k, w)) in lt.row.iter().enumerate() {
            if j > 0 {
                list.push(Token::char(Cat::AlignTab, '&'));
            }
            list.push(omit);
            if k != lt.chunk {
                list.push(kern);
                list.extend(w.to_string().chars().map(|c| Token::char(Cat::Other, c)).chain("sp".chars().map(|c| Token::char(Cat::Letter, c))));
                list.push(self.named_cs(named!("relax")));
            }
        }
        list.extend([cr, self.doc_id(doc!("rt@ltechunk"))]);
        self.back_list(&list);
    }

    /// The rest of `\LT@echunk`: the alignment's `\egroup`, `\global\setbox\LT@gbox\lastbox
    /// \unskip \egroup`; then `\LT@get@widths` (`\LT@max@sel`: a width that changed takes
    /// this chunk's number); then for `\endhead` `\setbox\LT@head\box\z@` and the next chunk,
    /// for the end `\LT@start` and the rest of `\endlongtable`.
    fn lt_echunk(&mut self) {
        self.fin_align();
        let start = self.start as usize;
        let mut widths = Vec::new();
        if self.cur.len() > start && self.cur.last().is_some_and(|n| n.kind == super::super::engine_node::Kind::Box) {
            if let Some(n) = self.cur.pop() {
                let r = self.e.nodes.boxes[n.a as usize];
                for c in self.e.nodes.get(r.list) {
                    if c.kind == super::super::engine_node::Kind::Box {
                        widths.push(self.e.nodes.boxes[c.a as usize].width);
                    }
                }
                self.e.nodes.flush_node(n);
            }
        }
        self.unskip();
        self.package();
        let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) else { return };
        if widths.len() == lt.row.len() {
            for (e, w) in lt.row.iter_mut().zip(widths) {
                if e.1 != w {
                    *e = (lt.chunk, w);
                }
            }
        }
        if !lt.end {
            let b = self.e.eqtb.box_reg(0);
            self.e.eqtb.replace_box(0, None);
            if let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) {
                lt.head = b;
            }
            return self.lt_bchunk();
        }
        if lt.trial {
            // The trial's chunks are dropped; the table again with the row it measured.
            lt.trial = false;
            if let Some(h) = lt.head.take() {
                self.e.nodes.flush_box(h);
            }
            if let Some(d) = self.doc.as_mut() {
                d.tabs.pop();
            }
            self.end_group(SEMI_SIMPLE_GROUP);
            return self.lt_pass();
        }
        if let Some(d) = self.doc.as_mut() {
            d.tabs.pop();
        }
        self.lt_start();
    }

    /// Runs `then` after a step that may start the output routine (a penalty on the main
    /// vertical list): the routine's tokens come first.
    fn lt_then(&mut self, then: fn(&mut Machine)) {
        if let Some(d) = self.doc.as_mut() {
            d.steps.push(vec![super::Op::Call(then)]);
        }
        let step = self.doc_id(super::STEP);
        self.input.back(step);
    }

    /// `\LT@start`: `\endgraf\penalty\z@\vskip\LTpre\endgraf`, ...
    fn lt_start(&mut self) {
        self.par();
        self.lt_then(|m| {
            let pre = LT_SKIP;
            m.vskip(pre);
            m.par();
            // `\ifdim\pagetotal<\pagegoal\else \kern(\pageshrink+1sp) \penalty9999 \endgraf
            // \kern-(\pageshrink+1sp) \fi`.
            if m.page_dimen(1) >= m.page_dimen(0) {
                let k = m.page_dimen(6) + 1;
                m.cur.push(super::super::engine_node::Node::new(super::super::engine_node::Kind::Kern, k));
                m.lt_then(|m| {
                    m.par();
                    let k = m.page_dimen(6) + 1;
                    m.cur.push(super::super::engine_node::Node::new(super::super::engine_node::Kind::Kern, -k));
                    m.lt_fit();
                });
                return m.vpenalty(9999);
            }
            m.lt_fit();
        });
        self.vpenalty(0);
    }

    /// The rest of `\LT@start`'s test: `\pagetotal`, the head, and the first row (`\vsplit` of
    /// a copy of the chunk to `\ht\@arstrutbox`, `\vbox{\unvbox\tw@}`, at least the strut) on
    /// the page, else `\vfil\break`; else `\pageshrink\z@` when positive.
    fn lt_fit(&mut self) {
        let mut d = self.page_dimen(1);
        if let Some(h) = self.doc.as_ref().and_then(|d| d.lt.as_ref()).and_then(|l| l.head) {
            let b = self.e.nodes.boxes[h as usize];
            d += b.height + b.depth;
        }
        let Some(b0) = self.e.eqtb.box_reg(0) else { return self.not_native("an empty longtable") };
        let (sh, sd) = self.e.eqtb.box_reg(ARSTRUT_BOX).map_or((0, 0), |b| (self.e.nodes.boxes[b as usize].height, self.e.nodes.boxes[b as usize].depth));
        let copy = self.e.nodes.copy_box(b0);
        let old2 = self.e.eqtb.box_reg(2);
        self.e.eqtb.replace_box(2, Some(copy));
        let top = self.vsplit(2, sh);
        let (th, td) = match top {
            Some(t) => {
                let r = self.e.nodes.boxes[t as usize];
                let list = self.e.nodes.get(r.list).to_vec();
                let packed = self.vpack_list(&list, 0, false, MAX_DIMEN);
                self.e.nodes.free_block(packed.list);
                self.e.nodes.flush_box(t);
                (packed.height, packed.depth)
            }
            None => (0, 0),
        };
        if let Some(rest) = self.e.eqtb.box_reg(2) {
            self.e.nodes.flush_box(rest);
        }
        self.e.eqtb.replace_box(2, old2);
        d += sh.max(th) + sd.max(td);
        d -= self.page_dimen(0);
        if d > 0 {
            let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
            self.vskip(fil);
            self.lt_then(|m| m.lt_head());
            return self.vpenalty(-10000);
        }
        if self.page_dimen(6) > 0 {
            self.page.so_far[6] = 0;
        }
        self.lt_head();
    }

    /// The end of `\LT@start` (`\@colroom\@colht`, `\MakeLinkTarget{table}`'s `\special{}`,
    /// the head and `\nobreak`, `\output{\LT@output}`), then `\endlongtable`'s `\unvbox\z@`,
    /// `\endgraf\penalty-\LT@end@pen`.
    fn lt_head(&mut self) {
        let colht = self.fl().colht;
        self.fl().colroom = colht;
        // hyperref's patch of `\LT@start`: `\Hy@raisedlink{\hyper@anchorstart{table.N}
        // \hyper@anchorend}` in its place.
        if self.hyper() {
            let n = self.rw(super::R_FIGURE + 1);
            let name = self.float_href(2, n);
            self.vertical_anchor(name);
        } else {
            self.cur.push(super::super::engine_node::Node { kind: super::super::engine_node::Kind::Whatsit, sub: super::super::engine_whatsit::SPECIAL, font: 0, a: 0, b: 0 });
        }
        let head = self.doc.as_ref().and_then(|d| d.lt.as_ref()).and_then(|l| l.head);
        if let Some(h) = head {
            let c = self.e.nodes.copy_box(h);
            self.append_to_vlist(c);
            self.maybe_build_page();
        }
        self.vpenalty(10000);
        if let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.as_mut()) {
            lt.output = true;
        }
        if let Some(b) = self.e.eqtb.box_reg(0) {
            self.e.eqtb.replace_box(0, None);
            let r = self.e.nodes.boxes[b as usize];
            self.cur.extend_from_slice(self.e.nodes.get(r.list));
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(b);
        }
        self.par();
        self.lt_then(|m| m.lt_end());
        self.vpenalty(-30000);
    }

    /// `\LT@output` at `\outputpenalty` `-\LT@end@pen`: `\setbox\z@\vbox{\unvbox\@cclv}`,
    /// `\unvbox\z@\box\LT@foot` (no foot here). Any other output inside the table is a page
    /// break in it: not native.
    pub(super) fn lt_output(&mut self, p: i32) {
        if p != -30000 {
            return self.not_native("a longtable across a page break");
        }
        if let Some(b) = self.e.eqtb.box_reg(255) {
            self.e.eqtb.replace_box(255, None);
            let r = self.e.nodes.boxes[b as usize];
            self.cur.extend_from_slice(self.e.nodes.get(r.list));
            self.e.nodes.free_block(r.list);
            self.e.nodes.free_box(b);
        }
    }

    /// The rest of `\endlongtable`: `\endgroup` (the output routine and the table's settings
    /// back), `\global\@mparbottom\z@`, `\endgraf\penalty\z@\addvspace\LTpost`, and
    /// `\end`'s `\endgroup`. A footnote waiting in `\footins` (`\insert\footins{}`) is not
    /// native.
    fn lt_end(&mut self) {
        let Some(lt) = self.doc.as_mut().and_then(|d| d.lt.take()) else { return };
        if let Some(h) = lt.head {
            self.e.nodes.flush_box(h);
        }
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.e.eqtb.box_reg(super::FOOTINS).is_some() {
            return self.not_native("a footnote on a longtable's page");
        }
        self.par();
        self.lt_then(|m| {
            let post = LT_SKIP;
            m.add_vspace(post);
            m.end_group(SEMI_SIMPLE_GROUP);
        });
        self.vpenalty(0);
    }

    /// `\begin{tabularx}{width}`: `\setlength\TX@target{width}`, the body to
    /// `\end{tabularx}` (`\TX@get@body`, `\TX@find@end`), then `\TX@endtabularx`'s first
    /// trial. A body whose second reading would count, label or write twice goes to the
    /// engine: `\TX@ckpt` and the trial's `\write` are not followed.
    pub(super) fn tabularx_begin(&mut self) {
        if self.doc.as_ref().is_some_and(|d| d.tx.is_some()) {
            return self.not_native("a tabularx in a tabularx");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let target = self.dimen_arg(arg);
        let end = Token::cs(self.e.cs.intern("end"));
        let mut body = Vec::new();
        loop {
            let Some(t) = self.get_token() else { return self.not_native("a tabularx with no end") };
            if t != end {
                body.push(t);
                continue;
            }
            let mut name = Vec::new();
            self.read_arg(&mut name);
            if name.iter().map(|t| t.chr()).collect::<String>() == "tabularx" && name.iter().all(|t| t.cs_id().is_none()) {
                break;
            }
            body.push(end);
            body.push(Token::char(Cat::BeginGroup, '{'));
            body.extend(name);
            body.push(Token::char(Cat::EndGroup, '}'));
        }
        const TWICE: [&str; 14] = ["footnote", "footnotemark", "footnotetext", "label", "refstepcounter", "stepcounter", "addtocounter", "setcounter", "caption", "write", "marginpar", "index", "tabularx", "newcounter"];
        if body.iter().any(|t| t.cs_id().is_some_and(|id| TWICE.contains(&self.e.cs.name(id)))) {
            return self.not_native("a tabularx body that counts, labels or writes");
        }
        let Some(doc) = self.doc.as_mut() else { return };
        doc.tx = Some(Box::new(Tx { target, cw: target, old_table: MAX_DIMEN, old_col: 0, cols: 1, count: true, body }));
        self.tx_trial();
    }

    /// `\TX@trial`: `\setbox\@tempboxa\hbox{\tabular body\endtabular}` with the column
    /// width of the moment, then [`Self::tx_after`].
    fn tx_trial(&mut self) {
        let Some(body) = self.doc.as_mut().and_then(|d| d.tx.as_mut()).map(|t| std::mem::take(&mut t.body)) else { return };
        self.open_box(HBOX_GROUP, Ctx::Set { n: super::TEMP_BOX, global: false }, false, 0, false);
        // The group `tabular`'s end closes, in place of `\end`'s.
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.tx_run(body, |m| m.tx_after());
    }

    /// `\tabular` (a `tabular*` when `tab_to` is set), the body and `\endtabular`, then `then`.
    fn tx_run(&mut self, body: Vec<Token>, then: fn(&mut Machine)) {
        let mut list = Vec::with_capacity(body.len() + 3);
        list.push(self.doc_id(super::TABULAR));
        list.extend_from_slice(&body);
        list.push(self.doc_id(super::END_TABULAR));
        list.push(self.doc_id(super::STEP));
        let Some(doc) = self.doc.as_mut() else { return };
        if let Some(tx) = doc.tx.as_mut() {
            tx.body = body;
        }
        doc.steps.push(vec![super::Op::Call(then)]);
        self.back_list(&list);
    }

    /// The trial's box closed, `\TX@arith`: another trial with the narrowed column width, or
    /// the table.
    fn tx_after(&mut self) {
        self.package();
        let mut w = self.e.eqtb.box_reg(super::TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let em = self.dimen_of("1em");
        let Some(doc) = self.doc.as_mut() else { return };
        let delta = doc.tx_delta;
        let Some(tx) = doc.tx.as_mut() else { return };
        tx.count = false;
        if tx.cw > MAX_DIMEN / tx.cols {
            w = MAX_DIMEN;
        }
        let again = if tx.old_table == w {
            tx.cw = tx.old_col;
            false
        } else {
            let mut d = w - tx.target;
            if d < delta {
                false
            } else {
                if tx.cols > 1 {
                    tx.cols -= 1;
                }
                d = d / tx.cols - tx.cw;
                if d > 0 {
                    // `X Columns too narrow (table too wide)`: `\TX@error@width`.
                    tx.cw = em;
                    false
                } else {
                    (tx.old_col, tx.old_table, tx.cw) = (tx.cw, w, -d);
                    true
                }
            }
        };
        if again {
            return self.tx_trial();
        }
        // `{\csname tabular*\endcsname\TX@target body\csname endtabular*\endcsname}`.
        let target = tx.target;
        let body = std::mem::take(&mut tx.body);
        doc.tab_to = Some(target);
        self.begin_group(SIMPLE_GROUP);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.tx_run(body, |m| {
            m.end_group(SIMPLE_GROUP);
            if let Some(doc) = m.doc.as_mut() {
                doc.tx = None;
            }
            // `\end{tabularx}`'s `\endgroup`.
            m.end_group(SEMI_SIMPLE_GROUP);
        });
    }

    pub(super) fn dimen_arg(&mut self, mut arg: Vec<Token>) -> i32 {
        let relax = self.named_cs(named!("relax"));
        arg.push(relax);
        self.back_list(&arg);
        let (v, _) = self.scan_dimen(false);
        if let Some(t) = self.get_token().filter(|&t| t != relax) {
            self.input.back(t);
        }
        v
    }

    /// `\end{tabular}`: `\endtabular` (`\crcr\egroup\egroup $\egroup`), then `\end`'s
    /// `\endgroup`, the `\crcr` ending an entry left open.
    pub(super) fn tabular_end(&mut self) {
        let crcr = Token::cs(self.doc.as_ref().map_or(0, |d| d.crcr_id));
        let list = [crcr, self.doc_id(TAB_END)];
        self.back_list(&list);
    }

    /// After the alignment's last row: its `}`, the box's, the formula's `$`, the `\hbox`'s,
    /// and `\end`'s `\endgroup`.
    /// An `array`'s ends only its alignment and box: what follows `\endarray` (amsmath's
    /// `\right.`, a matrix's `\hskip-\arraycolsep`) and `\end`'s `\endgroup` are the
    /// tokens after it.
    fn tabular_close(&mut self) {
        self.fin_align();
        self.package();
        let Some(t) = self.doc.as_mut().and_then(|d| d.tabs.pop()) else { return };
        let math = t.math;
        // `\endarray`'s `\CT@end` (colortbl).
        if let Some(d) = self.doc.as_mut() {
            d.ct_row = t.ct_save;
        }
        if !math {
            self.after_math();
            self.package();
            self.end_group(SEMI_SIMPLE_GROUP);
        }
    }
}

/// §247 `math_surround_code`.
const MATH_SURROUND: u32 = 1;

/// longtable's `\LTpre` and `\LTpost`, `\bigskipamount` at the load: `12pt plus4pt minus4pt`
/// (`size1x.clo`; the pre-scan refuses the names, so they keep it).
const LT_SKIP: Glue = Glue { width: 12 << 16, stretch: 4 << 16, shrink: 4 << 16, stretch_order: 0, shrink_order: 0 };

#[cfg(test)]
mod tests {
    use super::Cell::*;
    use super::*;

    fn cols(spec: &str) -> Option<Vec<(Vec<Cell>, Vec<Cell>)>> {
        let p = mkpream_kernel(spec.as_bytes(), false, &[7], &[])?;
        Some(p.cols.iter().map(|c| (p.ops[c[0] as usize..c[1] as usize].to_vec(), p.ops[c[1] as usize..c[2] as usize].to_vec())).collect())
    }

    #[test]
    fn mkpream_follows_the_class_automaton() {
        // `c`: `\@arstrut\hskip\tabcolsep{\hfil\hskip1sp\ignorespaces # \unskip\hfil}\hskip\tabcolsep`.
        assert_eq!(cols("c"), Some(vec![(vec![Strut, Acol, Open, Hfil, OneSp, Ignore], vec![Unskip, Hfil, Close, Acol])]));
        // `|l|r|`: rules, and a column after a rule has its own `\@acol`.
        assert_eq!(
            cols("|l|r|"),
            Some(vec![(vec![Strut, Rule, Acol, Open, OneSp, Ignore], vec![Unskip, Hfil, Close, Acol, Rule]), (vec![Acol, Open, Hfil, OneSp, Ignore], vec![Unskip, Close, Acol, Rule])])
        );
        // `||`: `\doublerulesep` between; `@{}`: no `\tabcolsep` on that side.
        assert_eq!(cols("@{}c||c@{}").map(|c| c.len()), Some(2));
        assert_eq!(cols("@{}c@{}"), Some(vec![(vec![Strut, Open, Hfil, OneSp, Ignore], vec![Unskip, Hfil, Close])]));
        assert!(cols("c||c").unwrap()[0].1.contains(&DoubleSep));
        // `*{3}{c}` is `ccc`; `*{2}{|c}|` is `|c|c|`.
        assert_eq!(cols("*{3}{c}"), cols("ccc"));
        assert_eq!(cols("*{2}{|c}|"), cols("|c|c|"));
        // `p{0}` (width 7sp): `\@classiii`'s spaces, `\@startpbox{w}\ignorespaces#\@endpbox`.
        assert_eq!(cols("lp{0}"), Some(vec![(vec![Strut, Acol, Open, OneSp, Ignore], vec![Unskip, Hfil, Close, Acol]), (vec![Acol, PStart(7, false), Ignore], vec![PEnd(false), Hfil, Acol])]));
        assert_eq!(cols("{c} c"), cols("cc"));
        // Not native: `p{...}`, a non-empty `@{...}`, an unknown column, no column.
        assert_eq!(cols("p{2cm}"), None);
        assert_eq!(cols("@{:}c"), None);
        assert_eq!(cols("x"), None);
        assert_eq!(cols("|"), None);
    }

    #[test]
    fn mkpream_pkg_follows_array_sty() {
        let cols = |spec: &str| {
            let p = mkpream_pkg(spec.as_bytes(), false)?;
            Some(p.cols.iter().map(|c| (p.ops[c[0] as usize..c[1] as usize].to_vec(), p.ops[c[1] as usize..c[2] as usize].to_vec())).collect::<Vec<_>>())
        };
        // `|l|r|`: `\vline` with no half-rule spaces; a column after a rule has one `\@acol`.
        assert_eq!(
            cols("|l|r|"),
            Some(vec![(vec![Strut, VLine, Acol, OneSp, Open, Ignore], vec![HUnskip, Close, RowStrut, Hfil, Acol, VLine]), (vec![Acol, Hfil, OneSp, Open, Ignore], vec![HUnskip, Close, RowStrut, Acol, VLine])])
        );
        assert!(cols("c||c").unwrap()[0].1.contains(&DoubleSep));
        assert_eq!(cols("@{}c@{}"), Some(vec![(vec![Strut, Hfil, OneSp, Open, Ignore], vec![HUnskip, Close, RowStrut, Hfil])]));
        assert_eq!(cols("*{2}{c}"), cols("cc"));
        assert_eq!(cols(">{x}c"), None);
        assert_eq!(cols("p{1cm}"), None);
        // Declarations: `>{t0}` before the entry's `\ignorespaces`, `<{t1}` after its
        // `\textonly@unskip`, `!{t2}` a group in place of a rule.
        let (a, b, c) = (Span { start: 1, end: 2 }, Span { start: 2, end: 3 }, Span { start: 3, end: 4 });
        let p = mkpream_widths(b">{t0}l<{t1}!{t2}c", false, &[], &[a, b, c], false).expect("declarations");
        assert_eq!(p.ops[..8], [Strut, Acol, OneSp, Open, Toks(a), Ignore, HUnskip, Toks(b)]);
        assert_eq!(p.ops[8..13], [Close, RowStrut, Hfil, Acol, Open]);
        assert_eq!(p.ops[13], Toks(c));
        assert_eq!(mkpream_widths(b"<{t0}c", false, &[], &[a], false), None);
        assert_eq!(cols("|"), None);
        // In an `array`: `$` entries, `\arraycolsep`, `@{\quad}` a formula.
        let p = mkpream_pkg(b"@{}l@{\x01}l@{}", true).expect("cases' preamble");
        assert_eq!(p.ops[p.cols[0][0] as usize..p.cols[0][1] as usize], [Strut, MathOn, MathOff, OneSp, MathOn, Ignore]);
        assert!(p.ops.contains(&Quad) && !p.ops.contains(&Acol));
    }

    #[test]
    fn mkpream_array_sets_math_entries() {
        let cols = |spec: &[u8]| {
            let p = mkpream_array(spec)?;
            Some(p.cols.iter().map(|c| (p.ops[c[0] as usize..c[1] as usize].to_vec(), p.ops[c[1] as usize..c[2] as usize].to_vec())).collect::<Vec<_>>())
        };
        // `c`: `\@arstrut\hskip\arraycolsep\hfil$\relax#$\hfil\hskip\arraycolsep`.
        assert_eq!(cols(b"c"), Some(vec![(vec![Strut, ArrayAcol, Hfil, MathOn], vec![MathOff, Hfil, ArrayAcol])]));
        // amsmath's `cases`: `@{}l@{\quad}l@{}`, each `@{...}` a formula.
        let quad = [b'@', b'{', b'}', b'l', b'@', b'{', QUAD, b'}', b'l', b'@', b'{', b'}'];
        assert_eq!(
            cols(&quad),
            Some(vec![(vec![Strut, MathOn, MathOff, MathOn], vec![MathOff, Hfil, MathOn, Quad, MathOff]), (vec![MathOn], vec![MathOff, Hfil, MathOn, MathOff])])
        );
        assert_eq!(array_spec("@{}l@{\\quad}l@{}").as_deref(), Some(&quad[..]));
        assert_eq!(array_spec("@{\\,}c"), None);
        // A matrix's `*{10}{c}`; `r` has no `\hfil` after its formula.
        assert_eq!(cols(b"*{10}{c}").map(|c| c.len()), Some(10));
        assert_eq!(cols(b"r").unwrap()[0].1, vec![MathOff, ArrayAcol]);
    }

    #[test]
    fn nc_rewrite_follows_nc_list() {
        let rw = |spec: &str, types: &[(u8, usize, &str)]| {
            let bodies: Vec<_> = types.iter().map(|t| st_tokens(t.2).unwrap()).collect();
            let mut list = vec![(St::Ch(b'*'), 2, None)];
            list.extend(types.iter().zip(&bodies).map(|(t, b)| (St::Ch(t.0), t.1, Some(&b[..]))));
            nc_rewrite(&st_tokens(spec).unwrap(), &list, st_kind).map(|t| st_text(&t))
        };
        let p = (b'P', 1, r">{\raggedright\arraybackslash}p{#1}");
        assert_eq!(rw(r"@{}P{.3\textwidth}P{2cm}@{}", &[p]).as_deref(), Some(r"@{}>{\raggedright \arraybackslash }p{.3\textwidth }>{\raggedright \arraybackslash }p{2cm}@{}"));
        // A type defined after the one it uses, and `*` in a replacement: further passes.
        assert_eq!(rw("QZ{2}{r}", &[(b'Q', 0, "Z{1}{c}|"), (b'Z', 2, "*{#1}{#2}")]).as_deref(), Some("c|rr"));
        // Braces of a group the letter is in are not searched; a lone group loses its braces.
        assert_eq!(rw(">{Q}cQ", &[(b'Q', 0, "l")]).as_deref(), Some(">{Q}cl"));
        assert_eq!(rw("{c}Q", &[(b'Q', 0, "l")]).as_deref(), Some("cl"));
        // An argument taken from the end marker is TeX's runaway.
        assert_eq!(rw("cP", &[p]), None);
        assert_eq!(spec_cols("@{}P{1cm}Y@{}", true, true, false, &[p, (b'Y', 0, r">{\raggedleft\arraybackslash}X")]), Some(2));
    }
}
