//! `--features mprof`: where the macro interpreter's work goes, by the document command
//! that caused it (`native-port-plan.md` §2 step 1, `RT_PROFILE` time per macro).
//!
//! Every token read is charged to the last control sequence read from the document's own
//! text (the main file and the `.tex` and `.dtx` files it reads): `\section`
//! is charged for its whole expansion and everything it calls, tail calls included, and
//! `\usepackage` for the package files it reads. `\begin` and `\end` are charged with their
//! environment's name. Document tokens themselves have their own bucket.
//! `RT_MPROF=<file>` appends one line per entry at the end of each run:
//! `tokens times name`.
//!
//! `RT_MPROF_IN=<macro>` drills into one macro: while a call of it runs, its work is also
//! charged to the step of its own body that caused it (each control sequence read directly
//! from the body), as `@in tokens times name`. The last step keeps the charge until the
//! next document command, so a tail call is charged to it.
//!
//! `RT_MPROF_SELF=<entry>` (or `all`): a flat profile inside that document command, each
//! token charged to the macro whose own body it was read from (argument and put-back
//! tokens to `@args`, other lists to `@lists`, package files to `@files`), as `@self tokens name`.

use super::engine_catcode::Cat;
use super::engine_run::Machine;
use super::engine_token::Token;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct MacroProfile {
    /// Entry names and their (tokens charged, times entered).
    index: HashMap<String, usize>,
    rows: Vec<(String, u64, u64)>,
    /// The entry being charged, and an environment name being read after `\begin`/`\end`.
    entry: Option<usize>,
    naming: Option<String>,
    doc: u64,
    before: u64,
    /// Tokens charged before `\begin{document}` (preamble), and whether it has begun.
    preamble: u64,
    body: bool,
    /// `RT_MPROF_IN`: the macro, its body frame while one call runs (index, end), and its
    /// steps' rows.
    target: Option<Option<String>>,
    focus: Option<(usize, u32)>,
    step: Option<usize>,
    steps: Vec<(String, u64, u64)>,
    step_index: HashMap<String, usize>,
    /// `RT_MPROF_SELF`: the entry selected, the macro owning each body frame, and the
    /// tokens read from each macro's body.
    self_entry: Option<Option<String>>,
    owner: HashMap<(usize, u32), u32>,
    self_tokens: HashMap<u32, u64>,
    self_args: u64,
    self_lists: u64,
    self_files: u64,
}

impl MacroProfile {
    fn enter(&mut self, name: &str) {
        let i = match self.index.get(name) {
            Some(&i) => i,
            None => {
                self.rows.push((name.to_string(), 0, 0));
                self.index.insert(name.to_string(), self.rows.len() - 1);
                self.rows.len() - 1
            }
        };
        self.rows[i].2 += 1;
        self.entry = Some(i);
    }
}

/// Whether a file of this name is the document's own text, as opposed to package, class,
/// font or configuration code, or a file LaTeX wrote (`.aux`, `.toc`, …), whose reading is
/// charged to the command that reads it.
pub(super) fn document_file(name: &str) -> bool {
    !name.ends_with(".code.tex") && [".tex", ".dtx", ".ltx", ".ins"].iter().any(|e| name.ends_with(e)) || !name.contains('.')
}

impl Machine<'_, '_> {
    /// Macro `id`'s body has just been pushed (`RT_MPROF_IN`).
    pub(super) fn mprof_body(&mut self, id: u32, end: u32) {
        let p = &mut self.mprof;
        let target = p.target.get_or_insert_with(|| std::env::var("RT_MPROF_IN").ok());
        p.owner.insert((self.input.depth() - 1, end), id);
        if p.focus.is_none() && target.as_deref() == Some(self.e.cs.name(id)) {
            p.focus = Some((self.input.depth() - 1, end));
            p.step = None;
        }
    }

    /// Token `t`, just read.
    pub(super) fn mprof_token(&mut self, t: Token) {
        if let (Some((i, end)), Some(id)) = (self.mprof.focus, t.cs_id()) {
            if self.input.top_frame() == Some((i, end)) {
                let name = format!("\\{}", self.e.cs.name(id));
                let p = &mut self.mprof;
                let k = match p.step_index.get(&name) {
                    Some(&k) => k,
                    None => {
                        p.steps.push((name.clone(), 0, 0));
                        p.step_index.insert(name, p.steps.len() - 1);
                        p.steps.len() - 1
                    }
                };
                p.steps[k].2 += 1;
                p.step = Some(k);
            }
        }
        let sel = self.mprof.self_entry.get_or_insert_with(|| std::env::var("RT_MPROF_SELF").ok()).clone();
        if let Some(sel) = sel {
            let p = &self.mprof;
            let here = sel == "all" || p.entry.is_some_and(|i| p.rows[i].0 == sel);
            if here && !self.input.reading_document() {
                let top = self.input.top_frame();
                let p = &mut self.mprof;
                match top.and_then(|k| p.owner.get(&k).copied()) {
                    Some(id) => *p.self_tokens.entry(id).or_default() += 1,
                    None if top.is_some() => p.self_lists += 1,
                    None if self.input.reading_file_any() => p.self_files += 1,
                    None => p.self_args += 1,
                }
            }
        }
        let p = &mut self.mprof;
        if p.focus.is_some() {
            if let Some(k) = p.step {
                p.steps[k].1 += 1;
            }
        }
        if !self.input.reading_document() {
            match p.entry {
                Some(i) => p.rows[i].1 += 1,
                None => p.before += 1,
            }
            if !p.body {
                p.preamble += 1;
            }
            return;
        }
        p.doc += 1;
        if !p.body {
            p.preamble += 1;
        }
        if let Some(name) = &mut p.naming {
            match (t.cs_id(), t.cat()) {
                (None, Some(Cat::EndGroup)) => {
                    name.push('}');
                    let n = std::mem::take(name);
                    p.naming = None;
                    p.body |= n == "begin{document}";
                    p.enter(&n);
                }
                (None, Some(Cat::BeginGroup | Cat::Space)) => {}
                (None, _) => name.push(t.chr()),
                _ => p.naming = None,
            }
            return;
        }
        if let Some(id) = t.cs_id() {
            p.focus = None;
            let name = self.e.cs.name(id);
            if name == "begin" || name == "end" {
                p.naming = Some(format!("{name}{{"));
            }
            let name = format!("\\{name}");
            p.enter(&name);
        }
    }

    /// `RT_MPROF`: this run's profile appended to the file.
    pub(super) fn mprof_report(&mut self) {
        let Ok(path) = std::env::var("RT_MPROF") else { return };
        let p = std::mem::take(&mut self.mprof);
        let mut out = format!("@doc {}\n@before {}\n@preamble {}\n", p.doc, p.before, p.preamble);
        for (name, tokens, times) in &p.rows {
            out.push_str(&format!("{tokens} {times} {name}\n"));
        }
        out.push_str(&format!("@self {} @args\n@self {} @lists\n@self {} @files\n", p.self_args, p.self_lists, p.self_files));
        for (&id, tokens) in &p.self_tokens {
            out.push_str(&format!("@self {tokens} {}\n", self.e.cs.name(id)));
        }
        for (name, tokens, times) in &p.steps {
            out.push_str(&format!("@in {tokens} {times} {name}\n"));
        }
        out.push_str("@end\n");
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(out.as_bytes());
        }
    }
}
