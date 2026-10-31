//! The incremental pre-scan of a watch session (`memorycastle/journal/2026-10-31-incremental-prescan.md`):
//! a save scans again only the part of the source around the edit, with the same outputs as a
//! scan of the whole source.
//!
//! A scanner ([`super::scan_from`]) run in a session takes marks as it goes: at a step's
//! start, its whole state, and how far into the text the steps before it read (`reach`, the
//! scanner's lookahead included). On the next save the common prefix `p` and suffix `q` of
//! the old and new source are found; the scan starts from the last mark whose reach is at
//! most `p` (every byte read before it is unchanged), and, once past the edit, at each byte
//! where the old run had a mark, compares its state with the old one there (positions moved
//! by the edit's length): equal, the rest of the old run is taken (early cutoff, as the
//! native job's checkpoints do). Text the scanner reads as a whole (`\numberwithin` anywhere)
//! is compared first; a difference scans from the start.
//!
//! Three scanners take marks: the blocker scan, [`super::well_formed`] and
//! [`super::predict`] (whose output is a log of writes, so the old run's tail is its log's
//! tail). A one-shot run takes none. The searches of the whole body for control words
//! (`predict`'s definitions and `\setlist`s, `\ref`, `\cite`, `\begin{thebibliography}`) go
//! through the list of its backslashes, which a save moves around the edit ([`moved`]). `RT_PRESCAN_CHECK=1` runs each whole scan too and stops
//! with both outputs on any difference.

/// A run's marks, in byte order (structure of arrays: the search reads `reach` and `at`
/// alone), and while a run is going, the old run's marks past the resume point.
pub(super) struct Marks<S> {
    at: Vec<usize>,
    reach: Vec<usize>,
    state: Vec<S>,
    /// Bytes between two marks while the scan is not at the old run's marks.
    spacing: usize,
    tail: Option<Tail<S>>,
    /// Where the run was cut off, and where it started (for `RT_PRESCAN_CHECK`'s report).
    pub(super) cut: Option<usize>,
    pub(super) start: usize,
    /// The old run's state where the run was cut off.
    cut_old: Option<S>,
}

/// A scanner's state at a mark.
pub(super) trait State: Clone + PartialEq {
    /// A mark of the old run taken after a cut off at a mark of state `old` (the old run's)
    /// and `new` (this run's, equal to it): what is counted from the start of the run (the
    /// outputs made so far) moved from the one to the other.
    fn adopt(&mut self, _old: &Self, _new: &Self) {}
}

impl State for super::ScanState {}

impl State for super::WellState {}

impl State for super::PredictState {
    fn adopt(&mut self, old: &Self, new: &Self) {
        for k in 0..3 {
            self.logs[k] = self.logs[k] - old.logs[k] + new.logs[k];
        }
    }
}

/// The old run's marks after the resume point, the edit, and the old run's result.
struct Tail<S> {
    at: Vec<usize>,
    reach: Vec<usize>,
    state: Vec<S>,
    /// The next of them not yet passed.
    next: usize,
    /// The new text's length less the old's.
    delta: isize,
    /// Where the unchanged suffix starts in the new text.
    from: usize,
    /// The old run's result (a byte of the old text).
    end: usize,
}

impl<S: State> Marks<S> {
    pub(super) fn new(spacing: usize) -> Marks<S> {
        Marks { at: Vec::new(), reach: Vec::new(), state: Vec::new(), spacing, tail: None, cut: None, start: 0, cut_old: None }
    }

    /// The first byte at or after which a mark is due, for a run starting at `i`.
    pub(super) fn first(&self, i: usize) -> usize {
        self.next_after(i)
    }

    fn next_after(&self, i: usize) -> usize {
        let mut next = i + self.spacing;
        if let Some(t) = &self.tail {
            // The old run's next mark in the unchanged suffix, where states can be compared.
            if let Some(k) = (t.next..t.at.len()).map(|k| t.at[k].wrapping_add_signed(t.delta)).find(|&a| a > i && a >= t.from) {
                next = next.min(k);
            }
        }
        next
    }

    /// A mark at byte `i`: the state there and how far the steps before it read. `Ok` with
    /// the next byte a mark is due at, or `Err` with the run's result when the old run's tail
    /// is taken from here.
    pub(super) fn mark(&mut self, i: usize, reach: usize, st: S) -> Result<usize, usize> {
        if let Some(t) = &mut self.tail {
            if i >= t.from {
                while t.next < t.at.len() && t.at[t.next].wrapping_add_signed(t.delta) < i {
                    t.next += 1;
                }
                let k = t.next;
                if k < t.at.len() && t.at[k].wrapping_add_signed(t.delta) == i && t.state[k] == st {
                    // The same state at the same place in the same text: the rest is the old
                    // run's, its marks moved by the edit.
                    let t = self.tail.take().expect("a tail");
                    let old = t.state[k].clone();
                    for (j, mut s) in t.state.into_iter().enumerate().skip(k) {
                        s.adopt(&old, &st);
                        self.at.push(t.at[j].wrapping_add_signed(t.delta));
                        self.reach.push(reach.max(t.reach[j].wrapping_add_signed(t.delta)));
                        self.state.push(s);
                    }
                    self.cut = Some(i);
                    self.cut_old = Some(old);
                    return Err(t.end.wrapping_add_signed(t.delta));
                }
            }
        }
        self.at.push(i);
        self.reach.push(reach);
        self.state.push(st);
        Ok(self.next_after(i))
    }
}

/// What the edit from `old` to `new` left alone: the common prefix's length `p` and the
/// common suffix's `q`, with `p + q` at most either length.
pub(super) fn common(old: &[u8], new: &[u8]) -> (usize, usize) {
    // 64 bytes at a time (a `memcmp` each), then byte by byte.
    const STEP: usize = 64;
    let n = old.len().min(new.len());
    let mut p = 0;
    while p + STEP <= n && old[p..p + STEP] == new[p..p + STEP] {
        p += STEP;
    }
    p += old[p..n].iter().zip(&new[p..n]).take_while(|(a, b)| a == b).count();
    let (o, w) = (&old[p..], &new[p..]);
    let room = n - p;
    let mut q = 0;
    while q + STEP <= room && o[o.len() - q - STEP..o.len() - q] == w[w.len() - q - STEP..w.len() - q] {
        q += STEP;
    }
    q += o[..o.len() - q].iter().rev().zip(w[..w.len() - q].iter().rev()).take(room - q).take_while(|(a, b)| a == b).count();
    (p, q)
}

/// A finished run: its marks and its result (`None` for a run that did not reach its end
/// cleanly, whose tail is never taken).
pub(super) struct Run<S> {
    at: Vec<usize>,
    reach: Vec<usize>,
    state: Vec<S>,
    end: Option<usize>,
}

impl<S: State> Run<S> {
    pub(super) fn finish(marks: Marks<S>, end: Option<usize>) -> Run<S> {
        Run { at: marks.at, reach: marks.reach, state: marks.state, end }
    }

    /// The marks for a run over a text edited from the old one (`p`, `q` as [`common`],
    /// lengths `old_len` and `new_len`): those up to the resume point kept, the rest the
    /// tail; with the mark to start from (its state and byte), if any.
    pub(super) fn resume(mut self, spacing: usize, p: usize, q: usize, old_len: usize, new_len: usize) -> (Marks<S>, Option<(S, usize)>) {
        // `reach` grows with `at`: the last mark whose reads are all in the unchanged prefix.
        let k = self.reach.partition_point(|&r| r <= p);
        let start = k.checked_sub(1).map(|k| (self.state[k].clone(), self.at[k]));
        let tail = self.end.map(|end| Tail { at: self.at.split_off(k), reach: self.reach.split_off(k), state: self.state.split_off(k), next: 0, delta: new_len as isize - old_len as isize, from: new_len - q, end });
        self.at.truncate(k);
        self.reach.truncate(k);
        self.state.truncate(k);
        let marks = Marks { at: self.at, reach: self.reach, state: self.state, spacing, tail, cut: None, start: start.as_ref().map_or(0, |s| s.1), cut_old: None };
        (marks, start)
    }
}

/// Bytes between two marks: about 60 marks over a 120 KB source; a save scans about twice
/// this around the edit.
const SPACING: usize = 2048;

/// What a watch session keeps of the last pre-scan: the source it read, what the scanners
/// read of it as a whole, and each scanner's run.
#[derive(Default)]
pub struct Cache {
    src: String,
    /// The source names `\numberwithin` (`scan_from` reads it from anywhere).
    numberwithin: bool,
    scan: Option<Run<super::ScanState>>,
    /// The body [`super::well_formed`] last read, and its run.
    well: Option<(String, Run<super::WellState>)>,
    /// The body last read for backslashes, and where each is ([`super::backslashes`]).
    body: String,
    slashes: Vec<usize>,
    predict: Option<PredictRun>,
}

/// The last [`super::predict`] run: the body it read, what it read before its walk, its
/// marks and its outputs.
struct PredictRun {
    text: String,
    globals: Option<super::PredictGlobals>,
    run: Run<super::PredictState>,
    log: super::PredictLog,
}

/// A session's [`super::predict_from`] going: its marks, and the old run once its globals
/// are found equal.
pub(super) struct Predicting {
    pub(super) marks: Marks<super::PredictState>,
    old: Option<PredictRun>,
    /// The edit (as [`common`]) and the new body's length.
    edit: (usize, usize, usize),
    globals: Option<super::PredictGlobals>,
    /// The old run's outputs, for the part before the mark started from and after a cut off.
    old_log: Option<super::PredictLog>,
}

impl Predicting {
    /// The walk's start, with what was read before it: from the old run's last mark the
    /// edit left valid, its state, byte and outputs so far, when the globals are as they were.
    pub(super) fn start(&mut self, globals: super::PredictGlobals) -> Option<(super::PredictState, usize, super::PredictLog)> {
        let same = self.old.as_ref().is_some_and(|o| o.globals.as_ref() == Some(&globals));
        self.globals = Some(globals);
        let old = self.old.take().filter(|_| same)?;
        let (p, q, len) = self.edit;
        let (marks, start) = old.run.resume(SPACING, p, q, old.text.len(), len);
        self.marks = marks;
        let start = start.map(|(st, at)| {
            let l = &old.log;
            let log = super::PredictLog { labels: l.labels[..st.logs[0]].to_vec(), hrefs: l.hrefs[..st.logs[1]].to_vec(), toc: l.toc[..st.logs[2]].to_vec(), complete: false };
            (st, at, log)
        });
        self.old_log = Some(old.log);
        start
    }

    /// After a cut off: the old run's outputs from the mark where it was met on.
    pub(super) fn tail(&mut self, out: &mut super::PredictLog) {
        let (Some(st), Some(l)) = (self.marks.cut_old.as_ref(), self.old_log.as_ref()) else { return };
        out.labels.extend_from_slice(&l.labels[st.logs[0]..]);
        out.hrefs.extend_from_slice(&l.hrefs[st.logs[1]..]);
        out.toc.extend_from_slice(&l.toc[st.logs[2]..]);
        out.complete = l.complete;
    }
}

/// The backslashes of `new` from those of the old text (`old`, `old_len` bytes) and the edit
/// ([`common`]: `p` bytes alike at the start, `q` at the end): the old ones before the edit,
/// a walk of the edited bytes, the old ones after it moved by the change of length.
fn moved(old: &[usize], new: &str, p: usize, q: usize, old_len: usize) -> Vec<usize> {
    let b = &new.as_bytes()[..new.len() - q];
    let (keep, tail) = (old.partition_point(|&k| k < p), old.partition_point(|&k| k < old_len - q));
    let mut out = Vec::with_capacity(keep + old.len() - tail + 16);
    out.extend_from_slice(&old[..keep]);
    let mut at = p;
    loop {
        at = super::byte_end(b, at, b'\\');
        if at == b.len() {
            break;
        }
        out.push(at);
        at += 1;
    }
    out.extend(old[tail..].iter().map(|&k| k + new.len() - old_len));
    out
}

/// `RT_PRESCAN_CHECK=1`: each incremental result is compared with a whole scan's, and a
/// difference panics with the edit.
fn checking() -> bool {
    std::env::var_os("RT_PRESCAN_CHECK").is_some()
}

impl Cache {
    /// [`super::scan`] of `src`, from the last run's marks where the edit allows.
    pub(super) fn scan<'a>(&mut self, src: &'a str) -> Result<&'a str, &'a str> {
        let numberwithin = src.contains("\\numberwithin");
        let old = self.scan.take().filter(|_| self.numberwithin == numberwithin);
        // The same source again: the same body.
        if let Some(end) = old.as_ref().filter(|_| self.src == src).and_then(|r| r.end) {
            self.scan = old;
            return Ok(&src[..end]);
        }
        let (p, q) = common(self.src.as_bytes(), src.as_bytes());
        let (mut marks, start) = match old {
            Some(run) => run.resume(SPACING, p, q, self.src.len(), src.len()),
            None => (Marks::new(SPACING), None),
        };
        let out = super::scan_from(src, &mut |_| false, start.as_ref().map(|(s, at)| (s, *at)), Some(&mut marks));
        if checking() {
            let full = super::scan(src);
            if full.map(str::len) != out.map(str::len) {
                panic!("RT_PRESCAN_CHECK: the incremental scan gave {:?}, a whole scan {:?} (edit: prefix {p}, suffix {q}, lengths {} to {}; started at {}, cut off at {:?})", out.map(str::len), full.map(str::len), self.src.len(), src.len(), marks.start, marks.cut);
            }
        }
        self.scan = Some(Run::finish(marks, out.ok().map(str::len)));
        self.numberwithin = numberwithin;
        self.src.clear();
        self.src.push_str(src);
        out
    }

    /// [`super::well_formed`] of `body`, from the last run's marks where the edit allows.
    pub(super) fn well_formed(&mut self, body: &str) -> Result<(), &'static str> {
        let old = self.well.take();
        // The same body again, found well formed.
        if old.as_ref().is_some_and(|(t, r)| t == body && r.end.is_some()) {
            self.well = old;
            return Ok(());
        }
        let mut text = String::new();
        let (mut marks, start, edit) = match old {
            Some((t, run)) => {
                let (p, q) = common(t.as_bytes(), body.as_bytes());
                let (marks, start) = run.resume(SPACING, p, q, t.len(), body.len());
                text = t;
                (marks, start, (p, q))
            }
            None => (Marks::new(SPACING), None, (0, 0)),
        };
        let out = super::well_formed_from(body, start.as_ref().map(|(s, at)| (s, *at)), Some(&mut marks));
        if checking() {
            let full = super::well_formed(body);
            if full != out {
                panic!("RT_PRESCAN_CHECK: the incremental well-formed check gave {out:?}, a whole one {full:?} (edit: prefix {}, suffix {}; started at {}, cut off at {:?})", edit.0, edit.1, marks.start, marks.cut);
            }
        }
        text.clear();
        text.push_str(body);
        self.well = Some((text, Run::finish(marks, out.ok().map(|()| 0))));
        out
    }

    /// [`super::backslashes`] of `body`, moved from the last body's ([`moved`]).
    pub(super) fn slashes(&mut self, body: &str) -> Vec<usize> {
        if self.body != body {
            let (p, q) = common(self.body.as_bytes(), body.as_bytes());
            self.slashes = moved(&self.slashes, body, p, q, self.body.len());
            if checking() && self.slashes != super::backslashes(body) {
                panic!("RT_PRESCAN_CHECK: the moved backslashes differ from a whole walk's (edit: prefix {p}, suffix {q}, lengths {} to {})", self.body.len(), body.len());
            }
            self.body.clear();
            self.body.push_str(body);
        }
        self.slashes.clone()
    }

    /// [`super::predict`] of `body`, from the last run's marks where the edit allows.
    pub(super) fn predict(&mut self, body: &str, slashes: &[usize]) -> super::Predicted {
        let old = self.predict.take();
        // The same body again: the same outputs.
        if let Some(o) = old.as_ref().filter(|o| o.text == body) {
            let log = o.log.clone();
            self.predict = old;
            return log.predicted();
        }
        let (p, q) = old.as_ref().map_or((0, 0), |o| common(o.text.as_bytes(), body.as_bytes()));
        let mut s = Predicting { marks: Marks::new(SPACING), old, edit: (p, q, body.len()), globals: None, old_log: None };
        let log = super::predict_from(body, slashes, Some(&mut s));
        if checking() {
            let full = super::predict_from(body, &super::backslashes(body), None);
            if full != log {
                panic!("RT_PRESCAN_CHECK: the incremental prediction differs from a whole one (labels {} and {}, hrefs {} and {}, contents {} and {}, complete {} and {}; edit: prefix {p}, suffix {q}; started at {}, cut off at {:?})", log.labels.len(), full.labels.len(), log.hrefs.len(), full.hrefs.len(), log.toc.len(), full.toc.len(), log.complete, full.complete, s.marks.start, s.marks.cut);
            }
        }
        self.predict = Some(PredictRun { text: body.to_string(), globals: s.globals, run: Run::finish(s.marks, Some(0)), log: log.clone() });
        log.predicted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_backslashes_match_a_whole_walk() {
        let old = "a\\b \\c{d}\\e\n\n\\f \\g";
        for new in ["a\\b \\c{dd}\\e\n\n\\f \\g", "a\\b \\c{\\x}\\e\n\n\\f \\g", "\\a\\b", "", "a\\b \\c{d}\\e\n\n\\f \\g\\", "x"] {
            let (p, q) = common(old.as_bytes(), new.as_bytes());
            assert_eq!(moved(&super::super::backslashes(old), new, p, q, old.len()), super::super::backslashes(new), "{new:?}");
        }
    }
}
