//! A document's run resumed after its preamble (`--watch`, progress I1 on the engine path;
//! `2026-10-31-engine-preamble-snapshot.md`). The first run of a document pauses where the
//! preamble ends ([`Machine::stop_at_document`]: at `\document`'s last token, before any byte of
//! the body is read), and the whole job is copied there: the engine (`Engine` is `Clone`) and
//! every part of the machine that is not scratch, its input stack included. The next save
//! starts from that copy and reads on over the new text from the same byte, when what the
//! preamble read is unchanged: the document's bytes up to there, every file it asked for (found
//! or not, by content hash), the files the run before wrote (the `.aux` `\document` reads), the
//! job's minute (`\time`, `\day`, `\month`, `\year`) and its name. A second copy is taken
//! earlier, where `\document` is about to read the `.aux`, so a run whose `.aux` changed (a
//! label, a section, a citation) resumes there and reads the new `.aux` and the rest of
//! `\document` (`2026-10-31-engine-aux-snapshot.md`).

pub use super::engine_checkpoint::{file_hash, hash_stamped, Stamped};
use super::engine_checkpoint::MachineState;
use super::engine_io::WriteStream;
use super::engine_run::{FileReader, Machine};
use super::Engine;
use crate::backend::backend_hash::ContentHasher;

impl Machine<'_, '_> {
    /// The machine's state, owned (the native job's checkpoint state, `MachineState`); `None`
    /// while a file is `\input` or a native job's state is bound (`kernel`, `doc`), which
    /// are not resumed.
    pub fn save(&self) -> Option<MachineState> {
        if self.kernel.is_some() || self.doc.is_some() {
            return None;
        }
        self.save_state()
    }

    /// A machine at the state `s` on engine `e` (a copy of the one saved with it), reading on
    /// over `src` from where the saved one stopped reading, with `read` for its files.
    pub fn resume<'e, 'a>(e: &'e mut Engine, src: &'a str, s: &MachineState, read: FileReader<'e>) -> Machine<'e, 'a> {
        let mut m = Machine::new(e, src);
        Machine::load_state(&mut m, s.clone(), src);
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

/// A run paused and copied, and what it read before the pause: it is resumed only when all
/// of that reads the same. Two pauses (`engine_job::run_kept`): where the preamble ends
/// (`\document`'s last `\ignorespaces`, after the `.aux` and every hook), and earlier, where
/// `\document` is about to read the `.aux` (copied only from a run that started with no
/// files, so it serves every run whose prior files answer none of its requests).
pub struct Snapshot {
    pub engine: Engine,
    pub m: MachineState,
    /// The document's text up to the next unread line at the pause.
    pub read_to: String,
    /// Each file the run asked for before the pause, once, with the hash of what it got.
    pub files: Vec<(String, u128)>,
    /// [`prior_hash`] of the files the run started with; `None` for the copy taken before the
    /// `.aux` is read, which no prior file had answered.
    pub prior: Option<u128>,
    /// The job's date in minutes (Unix seconds / 60) and its name.
    pub minute: i64,
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
        prior_ok && self.minute == minute && self.job_name == job_name && src.as_bytes().starts_with(self.read_to.as_bytes()) && self.files.iter().all(|(name, h)| hash_of(name) == *h)
    }
}

/// What a `--watch` session keeps of its engine runs between compiles.
#[derive(Default)]
pub struct Kept {
    /// The last compile's runs, each copied where its preamble ended.
    pub runs: Vec<Snapshot>,
    /// A run copied where `\document` is about to read the `.aux`.
    pub aux: Option<Snapshot>,
    /// Each file a copy's key names, with where it was found, that file's stamp and its hash
    /// ([`hash_stamped`]).
    pub stamps: std::cell::RefCell<std::collections::HashMap<String, Stamped>>,
}

impl Kept {
    /// Drops the copies (the format changed); the files' stamps stay true.
    pub fn clear(&mut self) {
        self.runs.clear();
        self.aux = None;
    }
}

