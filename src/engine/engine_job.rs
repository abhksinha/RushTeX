//! A document run as `pdflatex doc.tex` runs it: from a dumped format, and again while the
//! files it writes change (LaTeX's `.aux` and `.toc` protocol, as latexmk drives it). The
//! command line and the `format_smoke` harness both run documents through here, so the two
//! cannot drift apart.

use super::engine_io::WriteStream;
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
    let mut prior: Vec<WriteStream> = Vec::new();
    for runs in 1..=o.max_runs.max(1) {
        let mut e = Engine::undump(&fmt()?)?;
        let mut m = Machine::new(&mut e, src);
        m.read_file = Some(Box::new(read));
        m.job_name = o.job_name.to_string();
        m.typeset = true;
        let n = prior.len();
        m.closed = std::mem::take(&mut prior);
        m.fix_date_and_time(o.now);
        setup(&mut m);
        m.start_job();
        m.run();
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
            return Some(Job { pdf: m.pdf(), pages: m.page.pages, runs, errors: std::mem::take(&mut m.errors) });
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
