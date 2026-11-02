//! A document's run resumed after its preamble (`--watch`, progress I1 on the engine path;
//! `2026-10-31-engine-preamble-snapshot.md`). The first run of a document pauses where the
//! preamble ends ([`Machine::stop_at_document`]: at `\document`'s last token, before any byte of
//! the body is read), and the whole job is copied there: the engine (`Engine` is `Clone`) and
//! every part of the machine that is not scratch, its input stack included. The next save
//! starts from that copy and reads on over the new text from the same byte, when what the
//! preamble read is unchanged: the document's bytes up to there, every file it asked for (found
//! or not, by content hash), the files the run before wrote (the `.aux` `\document` reads), the
//! job's minute when the preamble read the date (`\time`, `\day`, `\month`, `\year`), and its
//! name. A second copy is taken earlier, where `\document` is about to read the `.aux`, so a
//! run whose `.aux` changed (a label, a section, a citation) resumes there and reads the new
//! `.aux` and the rest of `\document` (`2026-10-31-engine-aux-snapshot.md`).
//!
//! The copies also go to disk ([`Kept::store`], [`Kept::load`]), so a one-shot `rushtex`
//! run of a document compiled before starts after its preamble too
//! (`2026-11-02-engine-disk-snapshot.md`): one file per document in the cache folder, holding
//! each copy whole in `engine_snapshot`'s byte form, read back and compared with the copy
//! before it is kept, and refused unless the same executable and the same format wrote it.

pub use super::engine_checkpoint::{file_hash, hash_stamped, Stamped};
use super::engine_checkpoint::MachineState;
use super::engine_io::WriteStream;
use super::engine_run::{FileReader, Machine};
use super::engine_snapshot::{Codec, Reader, Writer};
use super::Meaning;
use super::Engine;
use crate::backend::backend_hash::ContentHasher;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

impl Machine<'_, '_> {
    /// The machine's state, owned (the native job's checkpoint state, `MachineState`); `None`
    /// while a file `\input` opened cannot be read again by its name (`\scantokens`, a file
    /// this run wrote) or a native job's state is bound (`kernel`, `doc`), which are not
    /// resumed.
    pub fn save(&self) -> Option<MachineState> {
        if self.kernel.is_some() || self.doc.is_some() {
            return None;
        }
        self.save_state(true)
    }

    /// A machine at the state `s` on engine `e` (a copy of the one saved with it), reading on
    /// over `src` from where the saved one stopped reading, and over `opened` in each file
    /// `\input` open at the copy ([`Snapshot::open`] reads them), with `read` for its files.
    pub fn resume<'e, 'a>(e: &'e mut Engine, src: &'a str, s: MachineState, opened: Vec<String>, read: FileReader<'e>) -> Machine<'e, 'a> {
        let mut m = Machine::new(e, src);
        Machine::load_state(&mut m, s, src);
        for (i, t) in opened.into_iter().enumerate() {
            m.input.refill(i + 1, t);
        }
        m.read_file = Some(read);
        m
    }
}

/// The hash of the files a run starts with (what the run before wrote).
pub fn prior_hash(prior: &[WriteStream]) -> u128 {
    let mut h = ContentHasher::new();
    for w in prior {
        h.bytes(w.name.as_bytes());
        h.bytes(w.text.as_bytes());
    }
    h.u64(prior.len() as u64);
    h.finish()
}

/// Whether `prior` (the files the run before wrote) answers a request for `name`: the match
/// `Machine::open_bytes` makes against the files a run wrote and closed.
pub fn prior_answers(prior: &[WriteStream], name: &str) -> bool {
    let local = name.strip_prefix("./").unwrap_or(name);
    prior.iter().any(|w| {
        let n = w.name.strip_prefix("./").unwrap_or(&w.name);
        n == local || n.strip_suffix(".tex") == Some(local) || local.strip_suffix(".tex") == Some(n)
    })
}

/// A copy's engine and machine: in memory as taken, or as bytes read from disk, decoded
/// when the copy is resumed.
pub enum Body {
    Live(Box<(Engine, MachineState)>),
    /// The file's bytes, the copy's range in them and that range's hash.
    Disk(std::rc::Rc<[u8]>, std::ops::Range<usize>, u128),
}

/// A run paused and copied, and what it read before the pause: it is resumed only when all
/// of that reads the same. Two pauses (`engine_job::run_kept`): where the preamble ends
/// (`\document`'s last `\ignorespaces`, after the `.aux` and every hook), and earlier, where
/// `\document` is about to read the `.aux` (copied only from a run that started with no
/// files, so it serves every run whose prior files answer none of its requests).
pub struct Snapshot {
    pub body: Body,
    /// The document's text up to the next unread line at the pause.
    pub read_to: String,
    /// Each file the run asked for before the pause, once, with the hash of what it got.
    pub files: Vec<(String, u128)>,
    /// [`prior_hash`] of the files the run started with; `None` for the copy taken before the
    /// `.aux` is read, which no prior file had answered.
    pub prior: Option<u128>,
    /// The job's date in minutes (Unix seconds / 60) when the run used it after `\everyjob`
    /// before the pause (`Eqtb::watch`), or [`Redate`] is not exact for its format; `None`
    /// when the copy serves any date, the resumed run given its own ([`Redate::apply`]).
    pub minute: Option<i64>,
    /// The job's name.
    pub job_name: String,
}

impl Snapshot {
    /// Whether a run of `src` named `job_name` at minute `minute`, starting with files `prior`
    /// (of hash `ph`) and finding files whose [`file_hash`] is `hash_of` theirs, reads before
    /// the pause exactly what this one did, so it would reach the same state. For the copy
    /// before the `.aux` (`prior: None`) every file it asked for reached the disk, as the run
    /// that made it started with no files: a prior file of one of those names would have
    /// answered instead (`Machine::open_bytes` looks there first), so none may.
    pub fn fits(&self, src: &str, job_name: &str, minute: i64, prior: &[WriteStream], ph: u128, hash_of: &dyn Fn(&str) -> u128) -> bool {
        let prior_ok = match self.prior {
            Some(h) => h == ph,
            None => !self.files.iter().any(|(name, _)| prior_answers(prior, name)),
        };
        prior_ok && self.minute.is_none_or(|m| m == minute) && self.job_name == job_name && src.as_bytes().starts_with(self.read_to.as_bytes()) && self.files.iter().all(|(name, h)| hash_of(name) == *h)
    }

    /// The copy's engine and machine state, each a copy of the kept ones, at the date `now`
    /// when the copy serves any date ([`Redate::apply`]), unwatched; `None` when the bytes on
    /// disk do not decode (or no longer hash as written), so the run is made whole.
    ///
    /// With them, the text of each file `\input` open at the copy, read again by its name
    /// through `read` (its bytes are in the copy's key, [`Snapshot::files`], as every file the
    /// run asked for is); `None` when one can no longer be read.
    pub fn open(&self, now: i64, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<(Engine, MachineState, Vec<String>)> {
        let (mut e, m) = match &self.body {
            Body::Live(b) => (b.0.clone(), b.1.clone()),
            Body::Disk(bytes, range, hash) => {
                let b = bytes.get(range.clone())?;
                if content_hash(b) != *hash {
                    return None;
                }
                decode_body(b)?
            }
        };
        e.eqtb.watch.stop();
        if self.minute.is_none() && !Redate::apply(&mut e, now) {
            return None;
        }
        let mut opened = Vec::new();
        for o in m.input.opened() {
            opened.push(read(o?)?.iter().map(|&b| char::from(b)).collect::<String>());
        }
        Some((e, m, opened))
    }
}

/// What a `--watch` session keeps of its engine runs between compiles, and a one-shot run
/// reads from and writes to disk ([`Kept::load`], [`Kept::store`]).
#[derive(Default)]
pub struct Kept {
    /// The last compile's runs, each copied where its preamble ended.
    pub runs: Vec<Snapshot>,
    /// A run copied where `\document` is about to read the `.aux`.
    pub aux: Option<Snapshot>,
    /// Each file a copy's key names, with where it was found, that file's stamp and its hash
    /// ([`hash_stamped`]).
    pub stamps: std::cell::RefCell<std::collections::HashMap<String, Stamped>>,
    /// The hash of the stamps as read from disk ([`Kept::load`]), so a store that changes
    /// no copy writes only when they changed.
    pub stamps_read: Option<u128>,
}

impl Kept {
    /// Drops the copies (the format changed); the files' stamps stay true.
    pub fn clear(&mut self) {
        self.runs.clear();
        self.aux = None;
    }
}

/// The five constants l3sys's `\everyjob` makes of the job's date (expl3-code.tex,
/// `\__sys_everyjob:n`: `\int_const:Nn \c_sys_minute_int { \time mod 60 }`, the hour
/// `\time div 60`, then `\day`, `\month`, `\year`), in that order.
const DATE_NAMES: [&str; 5] = ["c_sys_minute_int", "c_sys_hour_int", "c_sys_day_int", "c_sys_month_int", "c_sys_year_int"];

/// A copy given another date. Every LaTeX job reads the date in `\everyjob`, before the
/// document's first byte; a copy is keyed on its minute only when something after that used
/// it too (`Eqtb::watch`). Else a run resumed from it is given its own date: `\time`, `\day`,
/// `\month`, `\year` set as a run made whole sets them, and the five constants `\everyjob`
/// made redefined with the new values ([`Redate::apply`]). That this is all `\everyjob` does
/// with the date is not assumed: [`Redate::probe`] proves it for the format on each compile
/// that copies a run.
pub struct Redate;

impl Redate {
    /// The control sequences of [`DATE_NAMES`] (`u32::MAX` for one the format lacks).
    pub fn ids(e: &Engine) -> [u32; 5] {
        DATE_NAMES.map(|n| e.cs.get(n).unwrap_or(u32::MAX))
    }

    /// Whether the five constants are as `\int_const` leaves them (`\chardef` or `\mathchardef`,
    /// global), so [`Redate::apply`] can give them other values.
    pub fn ready(e: &Engine) -> bool {
        Redate::ids(e).iter().filter(|&&id| id != u32::MAX).all(|&id| matches!(e.eqtb.meaning_quiet(id), Meaning::CharDef(_) | Meaning::MathCharDef(_)) && e.eqtb.meaning_level(id) == super::engine_eqtb::LEVEL_ONE)
    }

    /// Puts the date `now` in `e`: the four parameters (`Machine::fix_date_and_time`), then
    /// each constant with its new value, defined as `\int_const` defines it (l3int
    /// `\__int_const:nnN`: `\global\mathchardef` for 0 to 32767 on pdfTeX, or `\chardef`).
    /// `false`, `e` unchanged, when a constant is not one [`Redate::ready`] takes or its new
    /// value does not fit its kind.
    pub fn apply(e: &mut Engine, now: i64) -> bool {
        if !Redate::ready(e) {
            return false;
        }
        let (y, mo, d) = super::engine_run::civil_from_days(now.div_euclid(86_400));
        let time = (now.rem_euclid(86_400) / 60) as u32;
        let vals = [time % 60, time / 60, d, mo, y as u32];
        let ids = Redate::ids(e);
        let new: Vec<(u32, Meaning)> = ids
            .iter()
            .zip(vals)
            .filter(|(&id, _)| id != u32::MAX)
            .filter_map(|(&id, v)| match e.eqtb.meaning_quiet(id) {
                Meaning::CharDef(_) if v <= 255 => Some((id, Meaning::CharDef(v))),
                Meaning::MathCharDef(_) if v <= 0x7FFF => Some((id, Meaning::MathCharDef(v))),
                _ => None,
            })
            .collect();
        if new.len() != ids.iter().filter(|&&id| id != u32::MAX).count() {
            return false;
        }
        for (slot, v) in [(20, time as i32), (21, d as i32), (22, mo as i32), (23, y as i32)] {
            e.eqtb.set_word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot }, v, true);
        }
        for (id, m) in new {
            e.eqtb.set_meaning(id, m, true);
        }
        true
    }

    /// Whether [`Redate::apply`] is exact for format engine `base` and job `job_name`: two
    /// runs of `\everyjob` alone (an empty document), at `now` and at a date where all five
    /// values differ, reach the same state once the first is given the second's date, but
    /// for what [`MachineState::same_but_counters`] leaves out and the dead token lists.
    pub fn probe(base: &Engine, job_name: &str, now: i64) -> bool {
        let job = |secs: i64| -> Option<(Engine, MachineState)> {
            let mut e = base.clone();
            let s = {
                let mut m = Machine::new(&mut e, "");
                m.job_name = job_name.to_string();
                m.typeset = true;
                m.fix_date_and_time(secs);
                m.start_job();
                m.run();
                // `\everyjob`'s expansions leave token lists in the arena that nothing refers
                // to, the date's digits among them: compared once collected, as dead tokens
                // stay unread in the run itself.
                m.collect_tokens();
                m.save_state(false)?
            };
            Some((e, s))
        };
        // A year and a month and a day, an hour and a minute later.
        let later = now + (366 + 31 + 1) * 86_400 + 3_600 + 60;
        let (Some((mut e0, s0)), Some((e1, s1))) = (job(now), job(later)) else { return false };
        Redate::apply(&mut e0, later) && e0.same_on_disk(&e1) && s0.same_but_counters(&s1)
    }
}

/// A disk copy's first bytes, then its layout: bumped when what is written changes in a way
/// the executable's stamp would not show (it always does: the stamp is in the key).
const MAGIC: &[u8; 4] = b"RTXS";
const LAYOUT: u32 = 3;

fn content_hash(b: &[u8]) -> u128 {
    let mut h = ContentHasher::new();
    h.bytes(b);
    h.finish()
}

/// An engine and machine state as bytes (`engine_snapshot`'s varints).
fn encode_body(e: &Engine, m: &MachineState) -> Vec<u8> {
    let mut w = Writer::default();
    e.put(&mut w);
    m.put(&mut w);
    w.bytes
}

fn decode_body(b: &[u8]) -> Option<(Engine, MachineState)> {
    let mut r = Reader::new(b);
    let e = Engine::take(&mut r)?;
    let m = MachineState::take(&mut r)?;
    r.at_end().then_some((e, m))
}

/// What a disk copy is valid for beyond its own key: the executable that wrote it (its path
/// and stamp: a rebuilt binary may lay its state out differently) and the format it started
/// from (path and stamp). `None` when either cannot be stamped: nothing is then kept on disk.
pub fn disk_key(fmt: &Path) -> Option<u128> {
    let exe = std::env::current_exe().ok()?;
    let mut h = ContentHasher::new();
    for p in [exe.as_path(), fmt] {
        // Not the ctime: macOS changes it on a binary's first runs (it adds the
        // `com.apple.provenance` attribute), which made the first copies of a build miss.
        let m = std::fs::metadata(p).ok()?;
        let mtime = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
        let mut w = Writer::default();
        p.to_path_buf().put(&mut w);
        (m.len(), mtime).put(&mut w);
        h.bytes(&w.bytes);
    }
    h.u32(LAYOUT);
    Some(h.finish())
}

/// The file in `dir` that holds the copies of document `input` run as `job_name`.
pub fn disk_path(dir: &Path, input: &Path, job_name: &str) -> PathBuf {
    let full = std::fs::canonicalize(input).unwrap_or_else(|_| input.to_path_buf());
    let mut h = ContentHasher::new();
    h.bytes(full.as_os_str().as_encoded_bytes());
    h.bytes(job_name.as_bytes());
    dir.join(format!("{:032x}.rtsnap", h.finish()))
}

impl Kept {
    /// The stamps as written, in name order (a map's order is not fixed).
    fn stamps_bytes(&self) -> Vec<u8> {
        let kept = self.stamps.borrow();
        let mut stamps: Vec<(&String, &Stamped)> = kept.iter().collect();
        stamps.sort_by(|a, b| a.0.cmp(b.0));
        let mut w = Writer::default();
        stamps.len().put(&mut w);
        for (name, s) in stamps {
            name.put(&mut w);
            s.put(&mut w);
        }
        w.bytes
    }

    /// Stamps every file a copy's key names ([`hash_stamped`]), so the next run checks the
    /// keys by stamps alone instead of reading each file again.
    pub fn stamp_all(&self, locate: &dyn Fn(&str) -> Option<PathBuf>, read: &dyn Fn(&str) -> Option<Vec<u8>>) {
        for s in self.aux.iter().chain(&self.runs) {
            for (name, _) in &s.files {
                if !self.stamps.borrow().contains_key(name) {
                    hash_stamped(&self.stamps, name, locate, read);
                }
            }
        }
    }
}

fn put_head(s: &Snapshot, w: &mut Writer) {
    s.read_to.put(w);
    s.files.put(w);
    s.prior.put(w);
    s.minute.put(w);
    s.job_name.put(w);
}

/// The file in `dir` holding the format's engine encoded for `key` ([`disk_key`]): what every
/// document's copies are written as differences from ([`delta`]). One per binary and format.
fn base_path(dir: &Path, key: u128) -> PathBuf {
    dir.join(format!("{key:032x}.rtbase"))
}

const BASE_MAGIC: &[u8; 4] = b"RTXB";

/// The base's bytes when `path` holds them for `key` and they hash as written.
fn read_base(path: &Path, key: u128) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let mut r = Reader::new(&bytes);
    if r.take::<4>()? != *BASE_MAGIC || r.u32()? != LAYOUT || u128::take(&mut r)? != key {
        return None;
    }
    let hash = u128::take(&mut r)?;
    let body = &bytes[r.position()..];
    (content_hash(body) == hash).then(|| body.to_vec())
}

/// The base for `key`: read from `dir`, else made from the format at `fmt` (undumped and
/// encoded as a copy's engine is) and written there, through a file renamed over it.
fn base(dir: &Path, key: u128, fmt: &Path) -> Option<Vec<u8>> {
    let path = base_path(dir, key);
    if let Some(b) = read_base(&path, key) {
        return Some(b);
    }
    let e = Engine::undump(&std::fs::read(fmt).ok()?)?;
    let mut w = Writer::default();
    e.put(&mut w);
    let body = w.bytes;
    let mut out = Writer::default();
    out.bytes.extend_from_slice(BASE_MAGIC);
    LAYOUT.put(&mut out);
    key.put(&mut out);
    content_hash(&body).put(&mut out);
    out.bytes.extend_from_slice(&body);
    write_atomic(&path, &out.bytes).ok()?;
    Some(body)
}

/// Writes `bytes` to `path` through a file beside it renamed over it: a reader sees the old
/// file or the new one whole, never a torn one.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The window [`delta`] finds matches by: positions of the reference bytes are indexed every
/// `WINDOW` bytes, so any match of at least `2 * WINDOW - 1` bytes is found.
const WINDOW: usize = 32;
const ROLL: u64 = 0x0000_0100_0000_01b3;

fn window_hash(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |h, &x| h.wrapping_mul(ROLL).wrapping_add(u64::from(x) + 1))
}

/// A hasher passing on a [`window_hash`], already well mixed.
#[derive(Default)]
struct Passed(u64);

impl std::hash::Hasher for Passed {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("only u64 keys")
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

/// The reference bytes' windows, by [`window_hash`], to their first position.
#[derive(Default)]
struct Windows {
    at: HashMap<u64, u32, std::hash::BuildHasherDefault<Passed>>,
    next: usize,
}

impl Windows {
    /// Indexes the windows of `refs` not yet indexed (`refs` only grows).
    fn extend(&mut self, refs: &[u8]) {
        self.at.reserve(refs.len().saturating_sub(self.next) / WINDOW);
        while self.next + WINDOW <= refs.len() {
            self.at.entry(window_hash(&refs[self.next..self.next + WINDOW])).or_insert(self.next as u32);
            self.next += WINDOW;
        }
    }
}

/// `target` as runs of new bytes and copies from `refs` (rsync's matching: a rolling hash of
/// each window of `target` looked up in `refs`' indexed windows, a hit checked byte for byte
/// and grown both ways): `n`, then until `n` bytes are made, (literal length, literal bytes,
/// copy length, copy position). A copy's engine differs from the format's and from the
/// document's other copies in a few places (new macros, the token lists they hold, changed
/// parameters), so most of it is copies.
/// `windows` may index past the end of `refs` (a later copy's bytes): such a hit is passed over.
fn delta(refs: &[u8], windows: &Windows, target: &[u8]) -> Vec<u8> {
    let mut w = Writer::default();
    w.var(target.len() as u64);
    let out = ROLL.wrapping_pow(WINDOW as u32 - 1);
    let (mut lit, mut i) = (0, 0);
    let mut h = target.get(..WINDOW).map_or(0, window_hash);
    while i + WINDOW <= target.len() {
        if let Some(&at) = windows.at.get(&h) {
            let at = at as usize;
            if at + WINDOW <= refs.len() && refs[at..at + WINDOW] == target[i..i + WINDOW] {
                let (mut s, mut r) = (i, at);
                while s > lit && r > 0 && target[s - 1] == refs[r - 1] {
                    s -= 1;
                    r -= 1;
                }
                let (mut e, mut re) = (i + WINDOW, at + WINDOW);
                while e < target.len() && re < refs.len() && target[e] == refs[re] {
                    e += 1;
                    re += 1;
                }
                w.var((s - lit) as u64);
                w.bytes.extend_from_slice(&target[lit..s]);
                w.var((e - s) as u64);
                w.var(r as u64);
                (lit, i) = (e, e);
                h = target.get(i..i + WINDOW).map_or(0, window_hash);
                continue;
            }
        }
        if i + WINDOW < target.len() {
            h = h.wrapping_sub((u64::from(target[i]) + 1).wrapping_mul(out)).wrapping_mul(ROLL).wrapping_add(u64::from(target[i + WINDOW]) + 1);
        }
        i += 1;
    }
    if lit < target.len() {
        w.var((target.len() - lit) as u64);
        w.bytes.extend_from_slice(&target[lit..]);
        w.var(0);
        w.var(0);
    }
    w.bytes
}

/// [`delta`]'s bytes `d` made again from `refs`; `None` when they do not read whole.
fn undelta(refs: &[u8], d: &[u8]) -> Option<Vec<u8>> {
    let mut r = Reader::new(d);
    let n = usize::try_from(r.var()?).ok()?;
    let mut out = Vec::with_capacity(n.min(1 << 30));
    while out.len() < n {
        let l = usize::try_from(r.var()?).ok()?;
        out.extend_from_slice(r.slice(l)?);
        let (c, from) = (usize::try_from(r.var()?).ok()?, usize::try_from(r.var()?).ok()?);
        out.extend_from_slice(refs.get(from..from.checked_add(c)?)?);
    }
    (r.at_end() && out.len() == n).then_some(out)
}

impl Kept {
    /// The copies of `path`, when it was written for `key` ([`disk_key`]) by this layout,
    /// against the base its directory holds for `key` ([`base`]), and reads back whole; else
    /// none (a missing, older, foreign or damaged file runs cold). Each copy is made again
    /// from its differences here; its engine is decoded only when it fits ([`Snapshot::open`]).
    pub fn load(path: &Path, key: u128) -> Option<Kept> {
        let bytes = std::fs::read(path).ok()?;
        let mut r = Reader::new(&bytes);
        if r.take::<4>()? != *MAGIC || r.u32()? != LAYOUT || u128::take(&mut r)? != key {
            return None;
        }
        let base_hash = u128::take(&mut r)?;
        // The heads (and the stamps) are hashed whole; each body by its own hash when opened.
        let head_len = usize::take(&mut r)?;
        let head_at = r.position();
        let head = bytes.get(head_at..head_at.checked_add(head_len)?)?;
        let written = bytes.get(head_at + head_len..head_at + head_len + 16)?;
        if u128::from_le_bytes(written.try_into().ok()?) != content_hash(head) {
            return None;
        }
        let mut refs = read_base(&base_path(path.parent()?, key), key)?;
        if content_hash(&refs) != base_hash {
            return None;
        }
        let mut h = Reader::new(head);
        let stamps: Vec<(String, Stamped)> = Codec::take(&mut h)?;
        let stamps_read = Some(content_hash(&head[..h.position()]));
        let n = usize::take(&mut h)?;
        let mut delta_at = head_at + head_len + 16;
        let mut heads = Vec::new();
        for _ in 0..n {
            let aux = bool::take(&mut h)?;
            let (read_to, files, prior, minute, job_name) = (Codec::take(&mut h)?, Codec::take(&mut h)?, Codec::take(&mut h)?, Codec::take(&mut h)?, Codec::take(&mut h)?);
            let (len, hash) = (usize::take(&mut h)?, u128::take(&mut h)?);
            let d = bytes.get(delta_at..delta_at.checked_add(len)?)?;
            delta_at += len;
            let body = undelta(&refs, d)?;
            let range = refs.len()..refs.len() + body.len();
            refs.extend_from_slice(&body);
            heads.push((aux, range, hash, (read_to, files, prior, minute, job_name)));
        }
        if !h.at_end() || delta_at != bytes.len() {
            return None;
        }
        let all: std::rc::Rc<[u8]> = refs.into();
        let mut kept = Kept { stamps: std::cell::RefCell::new(stamps.into_iter().collect()), stamps_read, ..Default::default() };
        for (aux, range, hash, (read_to, files, prior, minute, job_name)) in heads {
            let s = Snapshot { body: Body::Disk(all.clone(), range, hash), read_to, files, prior, minute, job_name };
            if aux {
                kept.aux = Some(s);
            } else {
                kept.runs.push(s);
            }
        }
        Some(kept)
    }

    /// What [`Pending::finish`] writes to `path` for `key`: each copy taken in this compile
    /// encoded (the part that needs the engine, which stays on this thread), each read from
    /// disk with its bytes. `None` when there is nothing new: the file's time is then moved
    /// to now, for [`prune`]'s least recently used order.
    pub fn prepare(&self, path: &Path, key: u128, fmt: &Path) -> Option<Pending> {
        let all: Vec<(bool, &Snapshot)> = self.aux.iter().map(|s| (true, s)).chain(self.runs.iter().map(|s| (false, s))).collect();
        let stamps = self.stamps_bytes();
        let fresh = all.iter().any(|(_, s)| matches!(s.body, Body::Live(_))) || self.stamps_read != Some(content_hash(&stamps));
        if !fresh {
            let _ = std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            return None;
        }
        let t = std::time::Instant::now();
        let copies = all
            .into_iter()
            .map(|(aux, s)| {
                let mut head = Writer::default();
                aux.put(&mut head);
                put_head(s, &mut head);
                let (bytes, live) = match &s.body {
                    Body::Live(b) => (encode_body(&b.0, &b.1), true),
                    Body::Disk(bytes, range, _) => (bytes[range.clone()].to_vec(), false),
                };
                (head.bytes, bytes, live)
            })
            .collect();
        Some(Pending { path: path.to_path_buf(), key, fmt: fmt.to_path_buf(), stamps, copies, encoded: t.elapsed().as_secs_f64() * 1e3 })
    }
}

/// A copy file to write, with no engine in it (it may go to another thread).
pub struct Pending {
    path: PathBuf,
    key: u128,
    fmt: PathBuf,
    stamps: Vec<u8>,
    /// Each copy's head (aux flag and key), its encoded bytes, and whether it was taken in
    /// this compile (and so is checked before it is written).
    copies: Vec<(Vec<u8>, Vec<u8>, bool)>,
    encoded: f64,
}

impl Pending {
    /// Writes the file: each new copy kept only if its bytes decode and encode back the same
    /// (a gap in a [`Codec`] shows as a missing copy, never a wrong page), then each written
    /// as its [`delta`] from the base and the copies before it, through a file renamed over
    /// the old one. Then keeps `keep` files in its directory ([`prune`]).
    pub fn finish(self, keep: usize) {
        let said = std::env::var_os("RT_SNAP").is_some();
        let t = std::time::Instant::now();
        let Some(dir) = self.path.parent() else { return };
        let Some(base) = base(dir, self.key, &self.fmt) else { return };
        let based = t.elapsed().as_secs_f64() * 1e3;
        // Each copy is checked on its own thread while one more indexes the base and every
        // copy (indexed again without a copy that fails, which should not happen). Copy k
        // takes copies only from what comes before it (the base and copies 0..k), which the
        // reader has made again by then.
        let index = |copies: &[(Vec<u8>, Vec<u8>, bool)], mut refs: Vec<u8>| {
            let mut limits = Vec::new();
            for (_, b, _) in copies {
                limits.push(refs.len());
                refs.extend_from_slice(b);
            }
            let mut windows = Windows::default();
            windows.extend(&refs);
            (refs, limits, windows)
        };
        let base_hash = content_hash(&base);
        let (good, indexed): (Vec<bool>, _) = std::thread::scope(|sc| {
            let checks: Vec<_> = self.copies.iter().map(|(_, b, live)| sc.spawn(move || !live || decode_body(b).is_some_and(|(e, m)| encode_body(&e, &m) == *b))).collect();
            let indexed = index(&self.copies, base.clone());
            (checks.into_iter().map(|c| c.join().unwrap_or(false)).collect(), indexed)
        });
        let all_good = good.iter().all(|&g| g);
        let copies: Vec<_> = self.copies.into_iter().zip(good).filter(|(_, g)| *g).map(|(c, _)| c).collect();
        let (refs, limits, windows) = if all_good { indexed } else { index(&copies, base) };
        let checked = t.elapsed().as_secs_f64() * 1e3;
        let deltas: Vec<Vec<u8>> = std::thread::scope(|sc| {
            let (refs, windows) = (&refs, &windows);
            let jobs: Vec<_> = copies.iter().zip(&limits).map(|((_, b, _), &limit)| sc.spawn(move || delta(&refs[..limit], windows, b))).collect();
            jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect()
        });
        let mut head = Writer { bytes: self.stamps };
        copies.len().put(&mut head);
        let mut whole = 0;
        for ((h, b, _), d) in copies.iter().zip(&deltas) {
            head.bytes.extend_from_slice(h);
            d.len().put(&mut head);
            content_hash(b).put(&mut head);
            whole += b.len();
        }
        let mut w = Writer::default();
        w.bytes.extend_from_slice(MAGIC);
        LAYOUT.put(&mut w);
        self.key.put(&mut w);
        base_hash.put(&mut w);
        head.bytes.len().put(&mut w);
        w.bytes.extend_from_slice(&head.bytes);
        content_hash(&head.bytes).put(&mut w);
        for d in &deltas {
            w.bytes.extend_from_slice(d);
        }
        let differenced = t.elapsed().as_secs_f64() * 1e3;
        let _ = write_atomic(&self.path, &w.bytes);
        prune(dir, keep, self.key);
        if said {
            eprintln!("[SNAP] disk: {} copies ({:.1} MB encoded) in {:.1} MB; encoded in {:.2} ms here, then base {based:.2} ms, checked and indexed {:.2} ms, differenced {:.2} ms, written {:.2} ms", copies.len(), whole as f64 / 1e6, w.bytes.len() as f64 / 1e6, self.encoded, checked - based, differenced - checked, t.elapsed().as_secs_f64() * 1e3 - differenced);
        }
    }
}

/// Keeps the `keep` most recently used copy files in `dir` (by modification time), removing
/// the others, and the base for `key` alone ([`base`]); a file changed in the last 10 s stays
/// (another run may be reading or writing it), as does any file a failed `remove_file` leaves.
pub fn prune(dir: &Path, keep: usize, key: u128) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let current = base_path(dir, key);
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rtsnap" || x == "rtbase" || x.to_str().is_some_and(|x| x.starts_with("tmp"))))
        .filter(|p| *p != current)
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let now = std::time::SystemTime::now();
    let mut kept = 0;
    for (t, p) in files {
        let young = now.duration_since(t).map_or(true, |d| d.as_secs() < 10);
        let snap = p.extension().is_some_and(|x| x == "rtsnap");
        if young || (snap && kept < keep) {
            kept += usize::from(snap);
            continue;
        }
        let _ = std::fs::remove_file(&p);
    }
}
