//! Checkpoints of the native job for incremental recompile (`--watch`): the job's whole state
//! at the top of the main loop after a page is output, owned apart from the machine's borrows
//! (the engine, the source text, the file resolver), so a later compile of an edited source
//! can start from the last checkpoint before the first changed byte instead of from the
//! class (`memorycastle/journal/2026-10-31-incremental-snapshots.md`).
//!
//! A checkpoint is the engine (its big arrays copy-on-write, `engine_chunk`) and
//! [`MachineState`]: every field of [`Machine`] but the borrows and the scratch buffers that
//! are empty between commands. The base file's lexer is kept without its text; a resumed run
//! points it at the new text at the same offset.

use super::engine_input::Input;
use super::engine_run::{Machine, Mode};
use super::engine_token::Token;
use super::Engine;

/// What [`Machine`] owns besides its borrows and scratch, cloned whole: a native job's
/// checkpoint (`Snap`) and, with no `kernel` or `doc` bound, an engine run's pause in
/// `--watch` (`engine_resume`).
#[derive(Clone, PartialEq)]
pub struct MachineState {
    kernel: Option<Box<super::engine_kernel::Kernel>>,
    pub(super) doc: Option<Box<super::engine_doc::Doc>>,
    input: Input<'static>,
    out: Vec<Token>,
    nest: Vec<super::engine_node::Level>,
    cur: Vec<super::engine_node::Node>,
    start: u32,
    aux: i32,
    prev_graf: i32,
    frames: Vec<super::engine_box::BoxFrame>,
    last_badness: i32,
    after_group: Vec<(u16, Token)>,
    recent: [Token; 16],
    recent_at: usize,
    trace: bool,
    aligns: Vec<super::engine_align::Align>,
    aligning: bool,
    cancel_boundary: bool,
    pub(super) page: super::engine_page::Page,
    insert_numbers: Vec<u8>,
    adjust: Option<Vec<super::engine_node::Node>>,
    adjust_out: Option<Vec<super::engine_node::Node>>,
    mode_zero: bool,
    clang: i32,
    disc_saved: Vec<super::engine_chars::DiscSaved>,
    pdf_doc: super::engine_pdfdoc::PdfDoc,
    last_match: (Vec<u8>, Option<super::engine_regex::Captures>),
    vf: super::engine_vf::VfTable,
    colors: super::engine_color::ColorStacks,
    delim: i32,
    just_box: u32,
    noads: Vec<super::engine_mlist::Noad>,
    math_saved: Vec<u32>,
    math: super::engine_mlist::MathState,
    typeset: bool,
    in_csname: u32,
    expansions: u32,
    errors: Vec<String>,
    halted: bool,
    error_count: u8,
    box_warnings: [u32; 2],
    conds: Vec<u8>,
    mode: Mode,
    inner: bool,
    after_assignment: Option<Token>,
    job_name: String,
    reads: [Option<super::engine_io::ReadStream>; 16],
    writes: [Option<super::engine_io::WriteStream>; 16],
    closed: Vec<super::engine_io::WriteStream>,
    log: String,
    interaction: u8,
    groups: Vec<u8>,
    max_expansions: u32,
    dont_expand: bool,
    text_run: Option<(Vec<u8>, usize)>,
    words: super::engine_words::Words,
    collect_at: u32,
    run_index: u32,
    line_reads: u32,
}

impl Machine<'_, '_> {
    /// The machine's state without its borrows, at the top of the main loop with only the
    /// document's file open (`None` otherwise: a file `\input` opened is read from an offset
    /// of its own). The scratch buffers are empty there and are not kept.
    pub(super) fn save_state(&self) -> Option<MachineState> {
        // Every field is named, so a new field must be placed here (kept or scratch).
        let Machine { kernel, doc, e: _, input, out, collect: _, pattern: _, break_pool: _, def: _, nest, cur, start, aux, prev_graf, frames, last_badness, after_group, recent, recent_at, trace, aligns, aligning, cancel_boundary, page, insert_numbers, adjust, adjust_out, mode_zero, clang, disc_saved, pdf_doc, last_match, vf, colors, delim, just_box, noads, math_saved, math, typeset, in_csname, name_buf: _, expansions, errors, halted, error_count, conds, mode, inner, after_assignment, read_file: _, job_name, reads, writes, closed, log, interaction, groups, max_expansions, dont_expand, lig_buf: _, text_run, words, collect_at, collect_buf: _, dead_buf: _, #[cfg(all(test, feature = "oracle"))] used_cs: _, #[cfg(feature = "mprof")] mprof: _, run_index, checkpoints: _, pdf_cache: _, stop_at_document: _, paused: _, box_warnings, stop_at_aux: _, line_reads } = self;
        let input = input.detach()?;
        Some(MachineState { kernel: kernel.clone(), doc: doc.clone(), input, out: out.clone(), nest: nest.clone(), cur: cur.clone(), start: *start, aux: *aux, prev_graf: *prev_graf, frames: frames.clone(), last_badness: *last_badness, after_group: after_group.clone(), recent: *recent, recent_at: *recent_at, trace: *trace, aligns: aligns.clone(), aligning: *aligning, cancel_boundary: *cancel_boundary, page: page.clone(), insert_numbers: insert_numbers.clone(), adjust: adjust.clone(), adjust_out: adjust_out.clone(), mode_zero: *mode_zero, clang: *clang, disc_saved: disc_saved.clone(), pdf_doc: pdf_doc.clone(), last_match: last_match.clone(), vf: vf.clone(), colors: colors.clone(), delim: *delim, just_box: *just_box, noads: noads.clone(), math_saved: math_saved.clone(), math: *math, typeset: *typeset, in_csname: *in_csname, expansions: *expansions, errors: errors.clone(), halted: *halted, error_count: *error_count, box_warnings: *box_warnings, conds: conds.clone(), mode: *mode, inner: *inner, after_assignment: *after_assignment, job_name: job_name.clone(), reads: reads.clone(), writes: writes.clone(), closed: closed.clone(), log: log.clone(), interaction: *interaction, groups: groups.clone(), max_expansions: *max_expansions, dont_expand: *dont_expand, text_run: text_run.clone(), words: words.clone(), collect_at: *collect_at, run_index: *run_index, line_reads: *line_reads })
    }

    /// Puts `s` back as this machine's state, its document file read from `text` at the
    /// offset it had.
    pub(super) fn load_state<'b>(m: &mut Machine<'_, 'b>, s: MachineState, text: &'b str) {
        let MachineState { kernel, doc, input, out, nest, cur, start, aux, prev_graf, frames, last_badness, after_group, recent, recent_at, trace, aligns, aligning, cancel_boundary, page, insert_numbers, adjust, adjust_out, mode_zero, clang, disc_saved, pdf_doc, last_match, vf, colors, delim, just_box, noads, math_saved, math, typeset, in_csname, expansions, errors, halted, error_count, box_warnings, conds, mode, inner, after_assignment, job_name, reads, writes, closed, log, interaction, groups, max_expansions, dont_expand, text_run, words, collect_at, run_index, line_reads } = s;
        m.kernel = kernel;
        m.doc = doc;
        m.input = input.over(text);
        m.out = out;
        m.nest = nest;
        m.cur = cur;
        m.start = start;
        m.aux = aux;
        m.prev_graf = prev_graf;
        m.frames = frames;
        m.last_badness = last_badness;
        m.after_group = after_group;
        m.recent = recent;
        m.recent_at = recent_at;
        m.trace = trace;
        m.aligns = aligns;
        m.aligning = aligning;
        m.cancel_boundary = cancel_boundary;
        m.page = page;
        m.insert_numbers = insert_numbers;
        m.adjust = adjust;
        m.adjust_out = adjust_out;
        m.mode_zero = mode_zero;
        m.clang = clang;
        m.disc_saved = disc_saved;
        m.pdf_doc = pdf_doc;
        m.last_match = last_match;
        m.vf = vf;
        m.colors = colors;
        m.delim = delim;
        m.just_box = just_box;
        m.noads = noads;
        m.math_saved = math_saved;
        m.math = math;
        m.typeset = typeset;
        m.in_csname = in_csname;
        m.expansions = expansions;
        m.errors = errors;
        m.halted = halted;
        m.error_count = error_count;
        m.box_warnings = box_warnings;
        m.conds = conds;
        m.mode = mode;
        m.inner = inner;
        m.after_assignment = after_assignment;
        m.job_name = job_name;
        m.reads = reads;
        m.writes = writes;
        m.closed = closed;
        m.log = log;
        m.interaction = interaction;
        m.groups = groups;
        m.max_expansions = max_expansions;
        m.dont_expand = dont_expand;
        m.text_run = text_run;
        m.words = words;
        m.collect_at = collect_at;
        m.run_index = run_index;
        m.line_reads = line_reads;
    }
}

/// One checkpoint: where the document's text had been read to (the base lexer's next
/// unread line: every byte before it was consumed), how many files the job had read from
/// disk, and the state.
#[derive(Clone)]
pub(super) struct Snap {
    pub(super) mark: usize,
    pub(super) reads: usize,
    /// How many pages had been shipped (`Page::pdf`, not kept here), and output.
    pub(super) pdf_len: usize,
    pub(super) outputs: u32,
    pub(super) engine: Engine,
    pub(super) state: MachineState,
    /// The node arena's lineage: a state continues every earlier state of its lineage place
    /// for place (a run continues its resume point, and a [`Splice::Patch`] writes the new
    /// run's places into the old run's final state). A run from the class is lineage 0; only
    /// a [`Splice::Graft`] breaks the chain, starting a new one ([`new_arena`]).
    pub(super) arena: u64,
}

/// A new arena lineage (`Snap::arena`).
pub(super) fn new_arena() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A run's checkpoints, taken when a watch session asks for them.
#[derive(Default)]
pub(super) struct Checkpoints {
    /// A checkpoint is due: take one at the next main-loop top.
    pub(super) due: bool,
    /// Pages output so far, and up to which of them every page gets a checkpoint (else every
    /// [`EVERY`]-th): a run resumed after an edit takes them densely just past it, where
    /// the next edit most likely falls.
    pub(super) outputs: u32,
    pub(super) dense_until: u32,
    /// The token collector's threshold while a checkpoint is due (see [`Self::page_out`]).
    pub(super) collect_at: u32,
    pub(super) snaps: Vec<Snap>,
    /// The files the run read from disk so far, with their bytes' hashes (`Snap::reads`).
    pub(super) log: std::rc::Rc<std::cell::RefCell<Vec<(String, u128)>>>,
    /// The pages the run shipped, kept at its end for a later run resumed from one of its
    /// checkpoints (or, going in, the pages of the run the checkpoints came from).
    pub(super) pages: Vec<super::engine_ship::ShippedPage>,
    /// For the early cutoff, from the last run: its checkpoints past the resume point, its
    /// text, the files it read, its final state (at the end of the body) and pages; how many
    /// pages were kept (`Doc::deferred`) at the resume point; and where this run met it.
    pub(super) old: Vec<Snap>,
    pub(super) old_text: String,
    pub(super) old_log: Vec<(String, u128)>,
    pub(super) old_fin: Option<Snap>,
    pub(super) old_pages: Vec<super::engine_ship::ShippedPage>,
    pub(super) resumed_pages: usize,
    pub(super) cut: Option<Cut>,
    /// The page after which the run was cut off, if it was.
    pub(super) cut_page: Option<u32>,
    /// This run's final state, at the end of the body.
    pub(super) fin: Option<Snap>,
    /// The lineage of this run's arena (`Snap::arena`): its resume point's, or 0 from the class.
    pub(super) arena: u64,
    /// A graft cannot hold for the rest of this run ([`converged`]).
    pub(super) graft_off: bool,
}

/// Pages between checkpoints outside the dense stretch, so taking them stays a small part
/// of a run (measured: `memorycastle/journal/2026-10-31-incremental-snapshots.md`).
pub(super) const EVERY: u32 = 4;

impl Checkpoints {
    /// A page was output (`output_page`). A checkpoint due zeroes the token collector's
    /// threshold `collect_at` (kept here), so the main loop's test between commands calls
    /// [`Machine::checkpoint_top`]: no test of its own on the hot path.
    pub(super) fn page_out(&mut self, collect_at: &mut u32) {
        self.outputs += 1;
        if (self.outputs <= self.dense_until || self.outputs.is_multiple_of(EVERY)) && !self.due {
            self.due = true;
            self.collect_at = std::mem::replace(collect_at, 0);
        }
    }
}

/// The 128-bit hash of a file as read: its bytes, or that there was none.
pub fn file_hash(bytes: Option<&[u8]>) -> u128 {
    let mut h = crate::backend::backend_hash::ContentHasher::new();
    match bytes {
        Some(b) => {
            h.u64(1);
            h.bytes(b);
        }
        None => h.u64(0),
    }
    h.finish()
}

/// A file as last hashed: where it was found, its stamp then, and the hash of its bytes.
pub struct Stamped {
    path: std::path::PathBuf,
    stamp: Stamp,
    hash: u128,
}

/// What changes whenever a file's bytes do: device, inode, size, modification and change
/// times (ns). The change time is set by the kernel on every write, rename or `chmod` and
/// cannot be set back, so a file whose bytes changed has a new stamp, unless it changed
/// within the time stamps' granularity of the moment it was hashed ([`hash_stamped`]).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: i128,
    ctime: i128,
}

/// How close to the moment a file was hashed its times may be and its hash still be kept:
/// 2 s covers the coarsest granularity of a time stamp in use (FAT; HFS+ 1 s, APFS and ext4
/// 1 ns).
const RACY_NS: i128 = 2_000_000_000;

#[cfg(unix)]
fn stamp_of(path: &std::path::Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    let ns = |s: i64, n: i64| s as i128 * 1_000_000_000 + n as i128;
    Some(Stamp { dev: m.dev(), ino: m.ino(), len: m.len(), mtime: ns(m.mtime(), m.mtime_nsec()), ctime: ns(m.ctime(), m.ctime_nsec()) })
}

#[cfg(not(unix))]
fn stamp_of(_: &std::path::Path) -> Option<Stamp> {
    None
}

/// The [`file_hash`] of what `read` gives for `name`, read and hashed again only when the
/// file changed. `locate` says where `read` would find `name` (the search done again by
/// the files' presence alone, so a file dropped beside the document or in a directory
/// searched earlier is seen), or `None` when it cannot tell (not found on disk, a file
/// carried in the binary): then `name` is read. A file found where it was last time with
/// the same [`Stamp`] has the same bytes, so its hash is kept. A stamp within [`RACY_NS`]
/// of the moment the file was read is not kept (Git's racy-clean rule: a write in the same
/// tick after the read would leave the stamp as it was), nor one that moved during the read.
pub fn hash_stamped(stamps: &std::cell::RefCell<std::collections::HashMap<String, Stamped>>, name: &str, locate: &dyn Fn(&str) -> Option<std::path::PathBuf>, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> u128 {
    let mut stamps = stamps.borrow_mut();
    if let Some(path) = locate(name) {
        if let Some(before) = stamp_of(&path) {
            if let Some(s) = stamps.get(name).filter(|s| s.path == path && s.stamp == before) {
                return s.hash;
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i128);
            if let Ok(bytes) = std::fs::read(&path) {
                let hash = file_hash(Some(&bytes));
                let settled = before.mtime.max(before.ctime) < now - RACY_NS;
                if settled && stamp_of(&path) == Some(before) {
                    stamps.insert(name.to_string(), Stamped { path, stamp: before, hash });
                } else {
                    stamps.remove(name);
                }
                return hash;
            }
        }
    }
    stamps.remove(name);
    file_hash(read(name).as_deref())
}

impl Machine<'_, '_> {
    /// `maybe_collect` between commands with a checkpoint due: what it does there, then the
    /// checkpoint (when only the document's file is open; else at a later top).
    #[cold]
    pub(super) fn checkpoint_top(&mut self) {
        let Some(c) = self.checkpoints.as_mut() else { return };
        self.collect_at = c.collect_at;
        if self.e.toks.len() as u32 >= self.collect_at {
            self.collect_tokens();
        }
        if self.e.eqtb.has_dead_boxes() {
            self.flush_dead_boxes();
        }
        self.checkpoint();
        if let Some(c) = self.checkpoints.as_mut().filter(|c| c.due) {
            c.collect_at = std::mem::replace(&mut self.collect_at, 0);
        }
    }

    /// The run's state now, as a checkpoint; `None` while a file `\input` opened is read.
    /// The shipped pages are only appended to: a checkpoint keeps their number, and a run
    /// resumed from it takes them from the run that made them (`Checkpoints::pages`); the
    /// per-word ship cache refills.
    pub(super) fn snapshot(&mut self) -> Option<Snap> {
        let pdf = std::mem::take(&mut self.page.pdf);
        let word_ship = std::mem::take(&mut self.page.word_ship);
        let state = self.save_state();
        let pdf_len = pdf.len();
        self.page.pdf = pdf;
        self.page.word_ship = word_ship;
        let mut state = state?;
        let c = self.checkpoints.as_ref()?;
        // With a checkpoint due the collector's threshold is parked in `c` (`page_out`).
        if c.due {
            state.collect_at = c.collect_at;
        }
        let (reads, outputs) = (c.log.borrow().len(), c.outputs);
        Some(Snap { mark: self.input.base_read_to(), reads, pdf_len, outputs, engine: self.e.freeze(), state, arena: c.arena })
    }

    /// A checkpoint of the run at the top of the main loop, when only the document's file is
    /// open; in a resumed run, the early cutoff's test against the last run's checkpoint
    /// after as many pages ([`converged`]): when it holds, the run stops (`halted`) and the
    /// job takes the rest from the last run (`engine_doc::job`).
    fn checkpoint(&mut self) {
        let Some(snap) = self.snapshot() else { return };
        let text = self.input.base_text();
        let Some(c) = self.checkpoints.as_mut() else { return };
        c.due = false;
        if c.cut.is_none() {
            if let (Some(k), Some(fin)) = (c.old.iter().position(|o| o.outputs == snap.outputs), c.old_fin.as_ref()) {
                let log = c.log.borrow();
                let same_reads = snap.reads == c.old[k].reads && log[..] == c.old_log[..snap.reads.min(c.old_log.len())];
                drop(log);
                if same_reads {
                    if let Some(how) = converged(&snap, &mut c.old[k], fin, text, &c.old_text, c.resumed_pages, &mut c.graft_off) {
                        let lines = snap.state.input.line() as i32 - c.old[k].state.input.line() as i32;
                        c.cut = Some(Cut { at: k, how, delta: text.len() as isize - c.old_text.len() as isize, lines });
                        self.halted = true;
                    }
                }
            }
        }
        if let Some(c) = self.checkpoints.as_mut() {
            c.snaps.push(snap);
        }
    }
}

impl<'a> Machine<'_, 'a> {
    /// After a run cut off early ([`Checkpoints::cut`]): the state becomes the last run's
    /// final one with this run's pages since the resume point written in (`Doc::deferred`'s,
    /// kept for the contents list), its shipped pages, its text offset and expansion count.
    /// With the arenas laid out alike the last run's later checkpoints, given the same
    /// differences, become this run's.
    pub(super) fn splice(&mut self, text: &'a str) {
        let Some(mut c) = self.checkpoints.take() else { return };
        let (Some(cut), Some(fin), Some(new)) = (c.cut.take(), c.old_fin.take(), c.snaps.last()) else {
            self.checkpoints = Some(c);
            return;
        };
        let old = &c.old[cut.at];
        let extra = new.state.expansions as i64 - old.state.expansions as i64;
        let old_arena = old.arena;
        let pdf_len = new.pdf_len;
        c.cut_page = Some(new.outputs);
        let mut later = c.old.split_off(cut.at + 1);
        match cut.how {
            Splice::Patch(edits, words) => {
                let mut engine = fin.engine;
                // Only the later checkpoints of the cut one's lineage take the same places.
                let lineage = old_arena;
                later.truncate(later.iter().position(|o| o.arena != lineage).unwrap_or(later.len()));
                if !edits.is_empty() {
                    engine.nodes = engine.nodes.patched(&edits, &words);
                    for o in &mut later {
                        o.engine.nodes = o.engine.nodes.patched(&edits, &words);
                    }
                }
                *self.e = engine.thaw();
                c.arena = fin.arena;
            }
            // The last run's later checkpoints take this run's pages the same way (one that
            // cannot is dropped, with those after it).
            Splice::Graft(engine, pages) => {
                *self.e = engine.thaw();
                let words = new.engine.nodes.words.thaw();
                let resumed = c.resumed_pages;
                let lineage = new_arena();
                let mut kept = Vec::with_capacity(later.len());
                for mut o in later {
                    let at = o.state.doc.as_ref().and_then(|d| d.deferred.get(resumed..resumed + pages.len()));
                    let Some(e) = at.and_then(|at| grafted(&o.engine, at, new, &pages, &words)) else { break };
                    o.engine = e;
                    o.arena = lineage;
                    kept.push(o);
                }
                later = kept;
                c.arena = lineage;
            }
        }
        let mut state = fin.state;
        state.expansions = (state.expansions as i64 + extra).clamp(0, u32::MAX as i64) as u32;
        let shipped = std::mem::take(&mut self.page.pdf);
        Machine::load_state(self, state, text);
        self.input.shift(cut.delta, cut.lines);
        // The spliced state and the checkpoints adopted take this run's predictions.
        let arcs = new.state.doc.as_ref().map(|d| d.prediction_arcs());
        if let (Some(d), Some(a)) = (self.doc.as_mut(), arcs.clone()) {
            d.set_predictions(a);
        }
        // The pages shipped by the end of the body: this run's, then the last run's after them
        // (the rest, `finish_contents`'s, are shipped again from the spliced state).
        self.page.pdf = shipped.into_iter().take(pdf_len).chain(c.old_pages.drain(..).take(fin.pdf_len).skip(pdf_len)).collect();
        for mut o in later {
            o.mark = o.mark.wrapping_add_signed(cut.delta);
            o.state.input.shift(cut.delta, cut.lines);
            if let (Some(d), Some(a)) = (o.state.doc.as_mut(), arcs.clone()) {
                d.set_predictions(a);
            }
            o.state.expansions = (o.state.expansions as i64 + extra).clamp(0, u32::MAX as i64) as u32;
            c.snaps.push(o);
        }
        *c.log.borrow_mut() = std::mem::take(&mut c.old_log);
        c.outputs = fin.outputs;
        self.checkpoints = Some(c);
    }
}

/// Where a resumed run met the last run's state again ([`converged`]): the index of the last
/// run's checkpoint in `Checkpoints::old`, how its final state takes this run's pages since
/// the resume point, and how much longer the new text is.
pub(super) struct Cut {
    pub(super) at: usize,
    pub(super) how: Splice,
    pub(super) delta: isize,
    /// And by how many lines (`Lexer::line`).
    pub(super) lines: i32,
}

/// How the last run's final state takes this run's pages since the resume point.
pub(super) enum Splice {
    /// The arenas are laid out alike: the node places that differ, written in, with the
    /// word table (this run's) their word nodes name entries of.
    Patch(super::engine_node::NodeEdits, Box<super::engine_words::WordStore>),
    /// The final engine with the pages' trees (this run's page boxes given) copied into
    /// its arena under the last run's page boxes.
    Graft(Box<super::Engine>, Vec<u32>),
}

/// Whether the run at checkpoint `new` will do exactly what the last run did after its
/// checkpoint `old` (early cutoff), and how the last run's final state `fin` then takes
/// this run's pages. The rest of the text must be the same, and the state equal but for
/// what only the output reads: the text offset (by the edit's length), the expansion count
/// (the runaway guard's), the last commands run (a diagnostic), the word table's index (a
/// cache), the shipped pages (not in a checkpoint), and, with a contents list, the pages
/// kept since the resume point (`Doc::deferred` past `resumed`, which nothing reads before
/// `finish_contents`). In the node arenas, everything outside those pages' trees must be
/// in the same places and alike (a word node by what its entry stands for), or, when the
/// arenas are laid out alike, every differing place must be the pages'.
fn converged(new: &Snap, old: &mut Snap, fin: &Snap, new_text: &str, old_text: &str, resumed: usize, graft_off: &mut bool) -> Option<Splice> {
    // With no graft possible, a patch needs the three arenas of one lineage (else the
    // place-for-place comparison below would fail at length).
    if *graft_off && (fin.arena != old.arena || new.arena != old.arena) {
        return None;
    }
    if new.pdf_len != old.pdf_len || new_text.get(new.mark..)? != old_text.get(old.mark..)? || !new.engine.same_but_nodes(&old.engine) {
        return None;
    }
    let new_pages = new.state.doc.as_ref().map_or(&[][..], |d| d.deferred.get(resumed..).unwrap_or(&[]));
    let old_pages = old.state.doc.as_ref().map_or(Vec::new(), |d| d.deferred.get(resumed..).unwrap_or(&[]).to_vec());
    if new_pages.len() != old_pages.len() {
        return None;
    }
    let delta = new.mark as isize - old.mark as isize;
    // Lines moved too (a paragraph break): only when the last run read no line number after
    // this point (`\inputlineno`), so nothing taken from it depends on them.
    let lines = new.state.input.line() as i32 - old.state.input.line() as i32;
    if lines != 0 && fin.state.line_reads != old.state.line_reads {
        return None;
    }
    // Changed predictions: the last run's rest is the same under the new ones when all it
    // read of them in the whole run (its final state's reads) reads the same.
    let renamed = match (&new.state.doc, &old.state.doc, &fin.state.doc) {
        (Some(n), Some(o), Some(f)) if n.predictions_differ(o) => {
            if !f.reads_same(&o.predictions(), &n.predictions(), true) {
                return None;
            }
            true
        }
        _ => false,
    };
    let saved = (old.state.expansions, old.state.recent, old.state.recent_at, old.state.line_reads);
    (old.state.expansions, old.state.recent, old.state.recent_at, old.state.line_reads) = (new.state.expansions, new.state.recent, new.state.recent_at, new.state.line_reads);
    old.state.input.shift(delta, lines);
    let words = std::mem::replace(&mut old.state.words, new.state.words.clone());
    if let Some(d) = old.state.doc.as_mut() {
        d.deferred.truncate(resumed);
        d.deferred.extend_from_slice(new_pages);
    }
    // The current list's and page's nodes may sit at other places (compared as trees by
    // `Nodes::same_outside` below, and never written in place by a patch); §814
    // `just_box` is read only right after the line break that sets it, within one command,
    // so it is dead between commands.
    let kept = match (renamed, &new.state.doc, &mut old.state.doc) {
        (true, Some(n), Some(o)) => Some(o.set_predictions(n.prediction_arcs())),
        _ => None,
    };
    let cur = std::mem::replace(&mut old.state.cur, new.state.cur.clone());
    let page = std::mem::replace(&mut old.state.page.list, new.state.page.list.clone());
    let just_box = std::mem::replace(&mut old.state.just_box, new.state.just_box);
    let same = new.state == old.state;
    if let (Some(k), Some(o)) = (kept, old.state.doc.as_mut()) {
        o.set_predictions(k);
    }
    old.state.cur = cur;
    old.state.page.list = page;
    old.state.just_box = just_box;
    let renumbered = old.state.cur != new.state.cur || old.state.page.list != new.state.page.list;
    if let Some(d) = old.state.doc.as_mut() {
        d.deferred.truncate(resumed);
        d.deferred.extend_from_slice(&old_pages);
    }
    old.state.words = words;
    old.state.input.shift(-delta, -lines);
    (old.state.expansions, old.state.recent, old.state.recent_at, old.state.line_reads) = saved;
    if !same {
        return None;
    }
    let (a, b) = (&new.engine.nodes, &old.engine.nodes);
    // Written in place for place: only into a final state that continues `old`'s arena.
    if let Some(edits) = b.edits_to(a).filter(|_| fin.arena == old.arena && !renumbered) {
        if edits.is_empty() || (new_pages == old_pages && !new_pages.is_empty() && a.owns(new_pages, &edits) && b.owns(new_pages, &edits)) {
            return Some(Splice::Patch(edits, Box::new(a.words.clone())));
        }
    }
    // The pages' trees differ in size: what is outside them must be the same lists.
    let toc: Vec<u32> = new.state.doc.as_ref().map_or(Vec::new(), |d| d.toc_boxes.iter().map(|&(b, _)| b).collect());
    // A contents entry's page-number box inside the pages since the resume point stays
    // there at every later checkpoint: no graft this run (the test is costly).
    if *graft_off {
        return None;
    }
    if !toc.is_empty() && !a.outside_all(new_pages, &toc) {
        *graft_off = true;
        return None;
    }
    if new_pages.is_empty() || !a.same_outside(new_pages, b, &old_pages, &toc, (&a.words.thaw(), &b.words.thaw()), &[(&new.state.cur, &old.state.cur), (&new.state.page.list, &old.state.page.list)]) {
        return None;
    }
    let fin_pages = fin.state.doc.as_ref()?.deferred.get(resumed..resumed + new_pages.len())?;
    let e = grafted(&fin.engine, fin_pages, new, new_pages, &a.words.thaw())?;
    Some(Splice::Graft(Box::new(e), new_pages.to_vec()))
}

/// Frozen engine `target` with the trees of `new`'s page boxes `pages` copied into its arena
/// under its page boxes `at` (their old trees freed); `words` is `new`'s word table, live.
fn grafted(target: &Engine, at: &[u32], new: &Snap, pages: &[u32], words: &super::engine_words::WordStore) -> Option<Engine> {
    let (mut nodes, mut toks) = (target.nodes.thaw(), target.toks.thaw());
    let mut memo = std::collections::HashMap::new();
    for (&nb, &ob) in pages.iter().zip(at) {
        let r = nodes.graft_box(&new.engine.nodes, words, &new.engine.toks, &mut toks, &mut memo, nb)?;
        let l = nodes.boxes.at(ob as usize)?.list;
        nodes.flush_list(l);
        nodes.boxes.set(ob as usize, r);
    }
    Some(Engine { nodes: nodes.freeze(), toks: toks.freeze(), ..target.clone() })
}
