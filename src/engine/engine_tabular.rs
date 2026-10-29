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
use super::super::engine_node::RUNNING;
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
}

/// An open `tabularx`: its body, and `\TX@endtabularx`'s registers between trials.
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

/// An `array` column specification as source text, `\quad` as [`QUAD`]; `None` for any
/// other control sequence.
pub(super) fn array_spec(s: &str) -> Option<Vec<u8>> {
    let s = s.replace("\\quad", "\x01");
    (!s.contains('\\')).then(|| s.into_bytes())
}

/// A preamble: the steps of all columns, and per column where its ⟨u⟩ starts, where `#`
/// is (its ⟨v⟩ starts) and where its ⟨v⟩ ends.
#[derive(Debug, PartialEq)]
pub(super) struct Preamble {
    pub(super) ops: Vec<Cell>,
    pub(super) cols: Vec<[u16; 3]>,
}

/// An open `tabular`: its preamble, the number of open lists when it began (its `\\` is
/// the innermost while no list opened since), and the glue of a pending `\\[...]` whose
/// space is not positive (`\@yargarraycr`'s `\noalign`).
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
    mkpream_widths(spec, math, &[], &[])
}

/// [`mkpream_pkg`] with `p` columns and declarations, in a `tabular`: a `p{k}` takes the
/// width `widths[k]` (array.sty's `\@testpach`: class 10 at the `p`, whose `\@classx` places
/// the column space, then its argument as class 0 with `\@chnum` 4); a `{tk}` after `>`,
/// `<`, `!` or `@` is the token list `toks[k]` (`\save@decl`, `\@classv`).
pub(super) fn mkpream_widths(spec: &[u8], math: bool, widths: &[i32], toks: &[Span]) -> Option<Preamble> {
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
            list.push(Ok(Cell::PStart(w, true)));
            list.extend(before.take().map(|t| Ok(Cell::Toks(t))));
            list.extend([Ok(Cell::Ignore), Err(SHARP), Ok(Cell::Unskip), Err(AFTER), Ok(Cell::PEnd(true)), Ok(Cell::Hfil), Ok(Cell::RowStrut)]);
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
                if num != 1 {
                    list.push(Ok(Cell::Hfil));
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
pub(super) fn spec_ok(spec: &str, array: bool, x: bool) -> bool {
    spec_cols(spec, array, x).is_some()
}

/// [`spec_ok`]'s preamble's number of columns.
pub(super) fn spec_cols(spec: &str, array: bool, x: bool) -> Option<usize> {
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
    if array { mkpream_widths(&out, false, &w, &t) } else { mkpream_kernel(&out, false, &w, &t) }.map(|p| p.cols.len())
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
        let pre = self.spec_widths(&spec, pkg).and_then(|(s, w, t)| if pkg { mkpream_widths(&s, false, &w, &t) } else { mkpream_kernel(&s, false, &w, &t) });
        let Some(pre) = pre else { return self.not_native("tabular preamble") };
        let Some(doc) = self.doc.as_mut() else { return };
        let (lists, pboxes) = (doc.lists.len(), doc.pbox.len());
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), strut: 0, mc: None, math: false, pkg, pcell: (false, false), resume: (false, 0, 0, 0), pboxes });
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
        let Some(pre) = (if pkg { mkpream_pkg(spec, true) } else { mkpream_array(spec) }) else { return self.not_native("array preamble") };
        let Some(doc) = self.doc.as_mut() else { return };
        let (lists, pboxes) = (doc.lists.len(), doc.pbox.len());
        doc.tabs.push(Tab { pre, lists, skip: Glue::default(), strut: 0, mc: None, math: true, pkg, pcell: (false, false), resume: (false, 0, 0, 0), pboxes });
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
        // `\ialign`'s `\everycr{}`: an enclosing `align`'s does not reach the rows.
        {
            let reg = param!("everycr");
            let empty = self.e.toks.store(&[]);
            self.e.eqtb.set_toks(reg, empty, false);
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
            // `\@finalstrut`'s `\vrule` in vertical mode starts a paragraph (§1090), whose
            // `\everypar` runs first: the rest of the steps wait for it.
            if let (Cell::PEnd(pkg), false) = (op, self.mode == Mode::Horizontal) {
                self.unskip();
                // The kernel's `\@finalstrut` has no `\lastnodetype` test.
                if !pkg || self.last_node_type() > -1 {
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
        let pre = if math { spec_bytes(&spec).and_then(|s| if pkg { mkpream_pkg(&s, true) } else { mkpream_in(&s, true) }) } else { self.spec_widths(&spec, pkg).and_then(|(s, w, t)| if pkg { mkpream_widths(&s, false, &w, &t) } else { mkpream_kernel(&s, false, &w, &t) }) };
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
            Cell::Ignore => self.ignore_spaces(),
            Cell::Unskip => self.unskip(),
            Cell::HUnskip => {
                if self.mode == Mode::Horizontal {
                    self.unskip();
                }
            }
            Cell::Toks(_) => {}
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
        self.build_rule([RUNNING, get(self, BT_THIS_WIDTH), 0]);
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
        use super::super::engine_node::{Kind, Node};
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
        let (mut bytes, mut widths, mut toks, mut xs) = (Vec::with_capacity(spec.len()), Vec::new(), Vec::new(), Vec::new());
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
        let math = self.doc.as_mut().and_then(|d| d.tabs.pop()).is_some_and(|t| t.math);
        if !math {
            self.after_math();
            self.package();
            self.end_group(SEMI_SIMPLE_GROUP);
        }
    }
}

/// §247 `math_surround_code`.
const MATH_SURROUND: u32 = 1;

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
        let p = mkpream_widths(b">{t0}l<{t1}!{t2}c", false, &[], &[a, b, c]).expect("declarations");
        assert_eq!(p.ops[..8], [Strut, Acol, OneSp, Open, Toks(a), Ignore, HUnskip, Toks(b)]);
        assert_eq!(p.ops[8..13], [Close, RowStrut, Hfil, Acol, Open]);
        assert_eq!(p.ops[13], Toks(c));
        assert_eq!(mkpream_widths(b"<{t0}c", false, &[], &[a]), None);
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
}
