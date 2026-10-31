//! A document run as `pdflatex doc.tex` runs it: from a dumped format, and again while the
//! files it writes change (LaTeX's `.aux` and `.toc` protocol, as latexmk drives it). The
//! command line and the `format_smoke` harness both run documents through here, so the two
//! cannot drift apart.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::engine_io::WriteStream;
use super::engine_resume::{file_hash, hash_stamped, prior_hash, Kept, Snapshot};
use super::engine_run::Machine;
use super::Engine;

/// The most runs: varioref's "on the following page" texts can move a float and need more
/// than 3.
pub const MAX_RUNS: u32 = 5;

/// What a finished job leaves: the PDF, its page count, the runs it took and the errors of
/// the last run, in TeX's words.
pub struct Job {
    pub pdf: Vec<u8>,
    pub pages: usize,
    pub runs: u32,
    pub errors: Vec<String>,
}

/// A job's name (`\jobname`), its date (Unix seconds) and the most runs it may take.
pub struct Opts<'a> {
    pub job_name: &'a str,
    pub now: i64,
    pub max_runs: u32,
}

/// Runs `src` from the format `fmt` loads up to `o.max_runs` times; the format's bytes are
/// loaded for each run and dropped once undumped, so a job holds only the engine. Each run reads back the files the
/// one before it wrote; the run whose files repeat what it read is the last, since running
/// it again gives the same pages. `setup` sees each machine before it runs and `inspect` the
/// last one after, its `closed` holding the files that run wrote. `None` when there is no
/// format or it is not a format of this build.
pub fn run(fmt: &dyn Fn() -> Option<Vec<u8>>, src: &str, o: &Opts, read: &dyn Fn(&str) -> Option<Vec<u8>>, setup: &mut dyn FnMut(&mut Machine), inspect: &mut dyn FnMut(&mut Machine)) -> Option<Job> {
    run_from(&|| Engine::undump(&fmt()?), src, o, read, setup, inspect)
}

/// [`run`] with each run's engine made by `engine`: a session that keeps the format
/// undumped (`--watch`) gives a copy of it, about a tenth of an undump's work.
pub fn run_from(engine: &dyn Fn() -> Option<Engine>, src: &str, o: &Opts, read: &dyn Fn(&str) -> Option<Vec<u8>>, setup: &mut dyn FnMut(&mut Machine), inspect: &mut dyn FnMut(&mut Machine)) -> Option<Job> {
    runs(engine, src, o, read, setup, inspect, None)
}

/// [`run_from`] for a `--watch` session keeping `kept` (`engine_resume`): the last compile's
/// runs, each copied where its preamble ended, and one run copied where `\document` is about
/// to read the `.aux`. A run whose preamble would read what a copy read starts from it and
/// runs only the body; one whose `.aux` (or other prior file) differs but whose preamble is
/// the same starts from the copy before the `.aux`, and is copied again where its preamble
/// ends; any other runs whole and is copied at both points. `kept` is left holding this
/// compile's runs. `locate` says where `read` finds a file, so the copies' keys hash only the
/// files that changed (`engine_resume::hash_stamped`). `RT_SNAP=1` says which ran.
pub fn run_kept(engine: &dyn Fn() -> Option<Engine>, src: &str, o: &Opts, read: &dyn Fn(&str) -> Option<Vec<u8>>, locate: Locate, inspect: &mut dyn FnMut(&mut Machine), kept: &mut Kept) -> Option<Job> {
    runs(engine, src, o, read, &mut |_| {}, inspect, Some((kept, locate)))
}

/// Where a run's file reader finds a file on disk ([`run_kept`]).
type Locate<'a> = &'a dyn Fn(&str) -> Option<PathBuf>;

fn runs(engine: &dyn Fn() -> Option<Engine>, src: &str, o: &Opts, read: &dyn Fn(&str) -> Option<Vec<u8>>, setup: &mut dyn FnMut(&mut Machine), inspect: &mut dyn FnMut(&mut Machine), kept: Option<(&mut Kept, Locate)>) -> Option<Job> {
    let watch = kept.is_some();
    let said = watch && std::env::var_os("RT_SNAP").is_some();
    let (mut store, locate) = match kept {
        Some((k, l)) => (Some(k), Some(l)),
        None => (None, None),
    };
    let Kept { runs: mut old, mut aux, stamps } = store.as_deref_mut().map(std::mem::take).unwrap_or_default();
    let mut new: Vec<Snapshot> = Vec::new();
    let minute = o.now.div_euclid(60);
    let mut prior: Vec<WriteStream> = Vec::new();
    // Each file's hash once per compile: every run's preamble asks for nearly the same files.
    let hashed: RefCell<HashMap<String, u128>> = RefCell::new(HashMap::new());
    let hash_of = |name: &str| {
        if let Some(&h) = hashed.borrow().get(name) {
            return h;
        }
        let h = match locate {
            Some(locate) => hash_stamped(&stamps, name, locate, read),
            None => file_hash(read(name).as_deref()),
        };
        hashed.borrow_mut().insert(name.to_string(), h);
        h
    };
    for runs in 1..=o.max_runs.max(1) {
        let n = prior.len();
        // Each file the run asks for until its pause, with the hash of what it got.
        let asked: RefCell<Option<Vec<(String, u128)>>> = RefCell::new(None);
        let recorded = |name: &str| {
            let got = read(name);
            if let Some(log) = asked.borrow_mut().as_mut() {
                let h = file_hash(got.as_deref());
                hashed.borrow_mut().insert(name.to_string(), h);
                log.push((name.to_string(), h));
            }
            got
        };
        // A file asked for again got the same bytes (nothing changes it mid-run).
        let once = |files: Vec<(String, u128)>| {
            let mut seen = HashSet::new();
            files.into_iter().filter(|(name, _)| seen.insert(name.clone())).collect::<Vec<_>>()
        };
        let ph = prior_hash(&prior);
        // Only when said: `Instant` is not there on every target the job runs on (wasm).
        let keyed = said.then(std::time::Instant::now);
        let hit = if watch { old.iter().position(|k| k.fits(src, o.job_name, minute, &prior, ph, &hash_of)) } else { None };
        let before_aux = if watch && hit.is_none() { aux.as_ref().filter(|a| a.fits(src, o.job_name, minute, &prior, ph, &hash_of)) } else { None };
        if let Some(t) = keyed {
            eprintln!("[SNAP] run {runs}: copies' keys checked in {:.2} ms", t.elapsed().as_secs_f64() * 1e3);
        }
        let mut e;
        let mut m;
        if let Some(i) = hit {
            let k = old.swap_remove(i);
            e = k.engine.clone();
            m = Machine::resume(&mut e, src, &k.m, Box::new(read));
            m.run_index = runs;
            if said {
                eprintln!("[SNAP] run {runs}: resumed after the preamble ({} bytes, {} files)", k.read_to.len(), k.files.len());
            }
            new.push(k);
            m.run();
        } else {
            // Copied before the `.aux` only from a run that starts with no files (`Snapshot::fits`).
            let mut aux_pending = false;
            if let Some(a) = before_aux {
                e = a.engine.clone();
                m = Machine::resume(&mut e, src, &a.m, Box::new(recorded));
                m.run_index = runs;
                // The copy started with no files, this run with `prior`, which none of its
                // requests reached; they come first in `closed`, as for a run made whole.
                let own = std::mem::take(&mut m.closed);
                m.closed = std::mem::take(&mut prior);
                m.closed.extend(own);
                *asked.borrow_mut() = Some(a.files.clone());
                m.stop_at_document = true;
                if said {
                    eprintln!("[SNAP] run {runs}: resumed before the .aux ({} bytes, {} files)", a.read_to.len(), a.files.len());
                }
            } else {
                e = engine()?;
                m = Machine::new(&mut e, src);
                m.run_index = runs;
                m.job_name = o.job_name.to_string();
                m.typeset = true;
                m.closed = std::mem::take(&mut prior);
                m.fix_date_and_time(o.now);
                if watch {
                    *asked.borrow_mut() = Some(Vec::new());
                    m.read_file = Some(Box::new(recorded));
                    m.stop_at_document = true;
                    m.stop_at_aux = n == 0;
                    aux_pending = n == 0;
                } else {
                    m.read_file = Some(Box::new(read));
                }
                setup(&mut m);
                m.start_job();
            }
            let resumed = before_aux.is_some();
            let mut copied = false;
            m.run();
            while m.paused {
                m.paused = false;
                // The pause before the `.aux` comes first; the other is the preamble's end.
                let at_aux = aux_pending && !m.stop_at_aux;
                aux_pending = false;
                let files = if at_aux { asked.borrow().clone() } else { asked.borrow_mut().take() };
                if !at_aux {
                    m.stop_at_aux = false;
                }
                if let Some(saved) = m.save() {
                    let files = once(files.unwrap_or_default());
                    if said {
                        eprintln!("[SNAP] run {runs}: {}, copied {} ({} files)", if resumed { "resumed before the .aux" } else { "ran whole" }, if at_aux { "before the .aux" } else { "after the preamble" }, files.len());
                    }
                    let read_to = src[..m.input.base_read_to()].to_string();
                    let snap = Snapshot { engine: (*m.e).clone(), m: saved, read_to, files, prior: (!at_aux).then_some(ph), minute, job_name: o.job_name.to_string() };
                    if at_aux {
                        aux = Some(snap);
                    } else {
                        new.push(snap);
                        copied = true;
                    }
                }
                m.run();
            }
            if said && !copied {
                eprintln!("[SNAP] run {runs}: {}, no pause after the preamble", if resumed { "resumed before the .aux" } else { "ran whole" });
            }
        }
        // §360: the input ran out without `\end` (no `\end{document}`): in nonstop mode TeX
        // stops with a fatal error (`fatal_error`, whose help line the engine records).
        if !m.halted {
            m.error("*** (job aborted, no legal \\end found)");
        }
        // What the next run reads back is each file's final content, as on disk: a file
        // written several times in one run (listings' `.tmp`, doc's `.glo`) is compared by
        // its last version only, and only that version is kept.
        let mut wrote = m.closed.split_off(n);
        last_versions(&mut wrote);
        prior = std::mem::take(&mut m.closed);
        let same = wrote.iter().all(|w| prior.iter().find(|p| p.name == w.name).is_some_and(|p| p.text == w.text));
        if same || runs >= o.max_runs {
            m.closed = wrote;
            inspect(&mut m);
            let job = Job { pdf: m.pdf(), pages: m.page.pages, runs, errors: std::mem::take(&mut m.errors) };
            drop(m);
            if let Some(k) = store {
                *k = Kept { runs: new, aux, stamps };
            }
            return Some(job);
        }
        prior = wrote;
    }
    None
}

/// Keeps the last version of each file name, in the order of those last versions.
fn last_versions(files: &mut Vec<WriteStream>) {
    let mut i = files.len();
    while i > 0 {
        i -= 1;
        if files[i + 1..].iter().any(|w| w.name == files[i].name) {
            files.remove(i);
        }
    }
}

/// IniTeX on `ini` (`pdflatex.ini`), then `\dump`: the format `run` starts from. `None` when
/// the run ended inside a group, where `\dump` is not allowed.
pub fn build_format(ini: &str, job_name: &str, now: i64, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    let mut e = Engine::default();
    e.eqtb.use_initex_catcodes();
    {
        let mut m = Machine::new(&mut e, ini);
        m.read_file = Some(Box::new(read));
        m.job_name = job_name.to_string();
        m.fix_date_and_time(now);
        m.run();
    }
    e.dump()
}
