//! # A run's errors in plain words (`engine_explain`)
//!
//! The engine records each error in TeX's words (`l.<line> [<file depth>] <message>`, as
//! `Machine::error` writes it), exactly pdfTeX's, so the testing framework can compare them.
//! The command line shows a beginner something else: for each error one short block saying
//! what went wrong, where (the file and line, the line itself with a caret under the
//! problem) and one thing to try, then TeX's own message, to search for. Nothing here
//! changes what the engine computes; `RT_TEX_LOG=1` prints TeX's words alone (`src/main.rs`).

use std::fmt::Write as _;

/// What the presenter knows about the run besides its errors.
pub struct Run<'a> {
    /// The document's path as given on the command line.
    pub file: &'a str,
    /// The folder the document's own files are looked for in, absolute.
    pub dir: &'a str,
    /// The document, one `char` per byte, as the engine read it.
    pub source: &'a str,
    /// What the run wrote to the terminal (`\message`, `\typeout`, LaTeX's warnings).
    pub log: &'a str,
    /// The control sequences the run had defined when it ended whose names are two or more
    /// letters, sorted: the commands a misspelt one is compared with.
    pub defined: &'a [String],
}

/// At most this many blocks: later errors mostly follow from the first ones.
const MAX_BLOCKS: usize = 5;

/// One error explained.
struct Block {
    /// What went wrong, one sentence.
    what: String,
    /// The line of the document it is on (1-based), and in the main file (else in a file it
    /// loads, which the engine does not name).
    line: Option<(usize, bool)>,
    /// What the caret points at on that line, found by its text.
    at: Option<String>,
    hint: String,
    /// TeX's message, first line.
    tex: String,
}

/// The errors of a run in plain words; empty when there are none.
pub fn report(errors: &[String], run: &Run) -> String {
    let mut blocks: Vec<Block> = Vec::new();
    let mut prior: Vec<(usize, String)> = Vec::new();
    let mut hidden = 0;
    for raw in errors {
        let (line, main, msg) = split(raw);
        let head = msg.lines().next().unwrap_or("").trim_start_matches("! ").trim_end_matches('.').to_string();
        let consequence = follows(&head, line, &prior);
        prior.push((line, head.clone()));
        if consequence || blocks.len() >= MAX_BLOCKS {
            hidden += 1;
            continue;
        }
        let mut b = explain(&head, line, main, run);
        // TeX's line for a missing file is where the run stopped, not the `\usepackage`.
        if let Some((n, true)) = b.line {
            if b.at.as_deref().is_some_and(|a| !line_text(run.source, n).contains(a)) {
                b.line = find_line(run.source, b.at.as_deref().unwrap_or("")).map(|k| (k, true)).or(b.line);
            }
        }
        blocks.push(b);
    }
    let mut out = String::new();
    for b in &blocks {
        let place = match b.line {
            Some((n, true)) => format!("{}, line {n}", run.file),
            Some((n, false)) => format!("{}, in a file it loads (line {n} of that file)", run.file),
            None => run.file.to_string(),
        };
        let _ = writeln!(out, "[ERROR] {place}: {}", b.what);
        if let Some((n, true)) = b.line {
            let text = shown(line_text(run.source, n));
            if !text.trim().is_empty() {
                let gutter = n.to_string().len();
                let _ = writeln!(out, "  {n} | {text}");
                if let Some((col, len)) = b.at.as_deref().and_then(|a| caret(&text, a)) {
                    let _ = writeln!(out, "  {} | {}{}", " ".repeat(gutter), " ".repeat(col), "^".repeat(len.max(1)));
                }
            }
        }
        let _ = writeln!(out, "  Hint: {}", b.hint);
        let _ = writeln!(out, "  (TeX's message: {})\n", b.tex);
    }
    if !blocks.is_empty() {
        let n = blocks.len() + hidden;
        let more = if hidden > 0 { format!(" ({hidden} more not shown, on the same lines or following from these: fix these first and compile again)") } else { String::new() };
        let _ = writeln!(out, "{n} {} in {}{more}. The PDF was written but may be wrong where the errors are.", if n == 1 { "error" } else { "errors" }, run.file);
    }
    out
}

/// The warnings worth a beginner's notice, one line each: undefined references and
/// citations (LaTeX's `Reference ... undefined`, `Citation ... undefined`), and the boxes
/// TeX reports as overfull or underfull ([`Machine::box_warnings`](super::engine_run::Machine)).
pub fn warnings(log: &str, box_warnings: [u32; 2]) -> String {
    let mut out = String::new();
    let names = |what: &str| {
        let mut v: Vec<&str> = log.split(what).skip(1).filter_map(|r| r.split_once('\'').map(|x| x.0)).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let (refs, cites) = (names("LaTeX Warning: Reference `"), names("LaTeX Warning: Citation `"));
    let list = |v: &[&str], cmd: &str| v.iter().take(3).map(|n| format!("\\{cmd}{{{n}}}")).collect::<Vec<_>>().join(", ") + if v.len() > 3 { ", ..." } else { "" };
    if !refs.is_empty() {
        let _ = writeln!(out, "[WARN] {} {} no \\label (printed as ??): {}. Check the label names.", count(refs.len(), "reference"), if refs.len() == 1 { "has" } else { "have" }, list(&refs, "ref"));
    }
    if !cites.is_empty() {
        let _ = writeln!(out, "[WARN] {} not found in the bibliography (printed as ?): {}. Check the keys against your .bib file, and run bibtex or biber.", count(cites.len(), "citation"), list(&cites, "cite"));
    }
    let [under, over] = box_warnings;
    if under + over > 0 {
        let mut parts = Vec::new();
        if over > 0 {
            parts.push(format!("{} too wide (overfull: text may stick out into the margin)", count(over as usize, "line or box|lines or boxes")));
        }
        if under > 0 {
            parts.push(format!("{} too loose (underfull: spaces stretched)", count(under as usize, "line or box|lines or boxes")));
        }
        let _ = writeln!(out, "[WARN] {}. Only the looks are affected; rewording or \\usepackage{{microtype}} usually helps.", parts.join(", and "));
    }
    out
}

/// `1 reference`, `3 references`; `what` is `singular|plural` when the plural is not an `s` added.
fn count(n: usize, what: &str) -> String {
    let (one, many) = what.split_once('|').map_or((what.to_string(), format!("{what}s")), |(a, b)| (a.to_string(), b.to_string()));
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// `l.12 [1] message` into the line, whether it is in the main file (depth 1) and the
/// message; a string of another shape has no line.
fn split(raw: &str) -> (usize, bool, &str) {
    let parse = || {
        let rest = raw.strip_prefix("l.")?;
        let (n, rest) = rest.split_once(" [")?;
        let (d, msg) = rest.split_once("] ")?;
        Some((n.parse().ok()?, d == "1", msg))
    };
    parse().unwrap_or((0, true, raw))
}

/// Whether an error follows from one before it: the closing `$` TeX inserts after the one
/// that opened math, a second error on the same line, an `\end` of an environment LaTeX did
/// not know, the stop after an earlier error.
fn follows(head: &str, line: usize, prior: &[(usize, String)]) -> bool {
    let Some((last_line, last)) = prior.last() else { return false };
    if head.starts_with("Missing $ inserted") && last.starts_with("Missing $ inserted") {
        return prior.iter().rev().take_while(|(_, h)| h.starts_with("Missing $ inserted")).count() % 2 == 1;
    }
    // A stop after an earlier error (a `}` missing to the end of the file ends the input
    // inside an argument, with no `\end` read).
    if head.starts_with("Emergency stop") || head.starts_with("(That makes 100 errors") || head.starts_with("*** (job aborted") {
        return true;
    }
    if head.contains("ended by \\end{") {
        let env = head.rsplit_once("\\end{").map_or("", |x| x.1).trim_end_matches('}');
        if prior.iter().any(|(_, h)| h == &format!("LaTeX Error: Environment {env} undefined")) {
            return true;
        }
    }
    if head.starts_with("I can't find file `") && head.contains(".aux'") {
        return true;
    }
    line != 0 && *last_line == line
}

/// The block for one error.
fn explain(head: &str, line: usize, main: bool, run: &Run) -> Block {
    let mut b = Block { what: String::new(), line: (line > 0).then_some((line, main)), at: None, hint: String::new(), tex: head.to_string() };
    let latex = head.strip_prefix("LaTeX Error: ");
    // Undefined control sequence (§370), which the engine's box builder also reports as
    // `Not in the engine's boxes yet: \name` when it meets one.
    let engine_form = head.strip_prefix("Not in the engine's boxes yet: ");
    if head == "Undefined control sequence" || engine_form.is_some() {
        let named = engine_form.and_then(|w| w.strip_prefix('\\')).map(str::to_string);
        // pdfTeX's wording names no command: the first undefined one on the line, else on the
        // lines before it (an environment's body runs at its `\end`, as amsmath's `align`).
        let found = (named.is_none() && main).then(|| (line.saturating_sub(30).max(1)..=line).rev().find_map(|k| undefined_on(line_text(run.source, k), run).map(|n| (k, n)))).flatten();
        if let Some((k, _)) = found {
            b.line = Some((k, true));
        }
        let name = named.clone().or(found.map(|x| x.1));
        match name {
            Some(n) if named.is_none() || !is_defined(&n, run) => {
                b.tex = "Undefined control sequence".into();
                b.what = format!("LaTeX does not know the command \\{n}.");
                b.at = Some(format!("\\{n}"));
                b.hint = command_hint(&n, run);
            }
            // pdfTeX's wording with no undefined name on the line: from a macro, or in a
            // file the document loads.
            None if engine_form.is_none() => {
                b.what = if main { "LaTeX does not know a command used on this line (perhaps inside a macro of your own).".into() } else { "LaTeX does not know a command used here.".into() };
                b.hint = "Check the spelling of the commands here and in your \\newcommand definitions, and that the package defining them is loaded with \\usepackage.".into();
            }
            _ => {
                let what = engine_form.unwrap_or(head);
                b.what = format!("RushTeX cannot typeset {what} here yet.");
                b.at = what.starts_with('\\').then(|| what.to_string());
                b.hint = "This is a gap in RushTeX, not a mistake in your document: pdflatex compiles it meanwhile. Please report it.".into();
            }
        }
        return b;
    }
    if head == "Missing $ inserted" {
        b.what = "A math symbol (such as ^ or _ or a Greek letter) is used outside math.".into();
        b.at = math_char(line_text(run.source, line)).map(String::from);
        b.hint = "Put math between dollar signs, as in $x^2$ or $a_1$. For a plain _ or ^ in text, write \\_ or \\textasciicircum{}. Also check that every $ has its closing $ and that no blank line falls inside math.".into();
        return b;
    }
    if head == "Too many }'s" || head.starts_with("Extra }, or forgotten") {
        b.what = if head.contains("forgotten $") { "There is a } with no { before it to match it (or a $ is missing).".into() } else { "There is a } with no { before it to match it.".into() };
        b.at = unmatched_close(line_text(run.source, line)).map(|_| "}".to_string());
        b.hint = "Delete the extra }, or add the { it was meant to close.".into();
        return b;
    }
    if let Some(cmd) = head.strip_prefix("Paragraph ended before ").and_then(|r| r.strip_suffix(" was complete")).or_else(|| head.strip_prefix("File ended while scanning use of ")) {
        let file_end = head.starts_with("File ended");
        let whose = if cmd.contains('@') { "a command's argument".to_string() } else { format!("the argument of {cmd}") };
        if let Some(k) = main.then(|| unclosed_brace(run.source, if file_end { line } else { line.saturating_sub(1) })).flatten() {
            b.line = Some((k, true));
            b.at = Some("{".into());
            b.what = if file_end { format!("A }} is missing: the {{ on line {k} that opens {whose} is never closed.") } else { format!("A }} is missing: the {{ on line {k} that opens {whose} is not closed before the paragraph ends (a blank line or \\par).") };
        } else {
            b.what = if file_end { format!("The file ended inside {whose}: a }} is missing.") } else { format!("A blank line (a new paragraph) came inside {whose}: a }} is probably missing.") };
        }
        b.hint = if file_end { "Add the missing } where the argument should end.".into() } else { "Add the closing } where the argument should end. Most command arguments may not hold a blank line; remove it if the } is in place.".into() };
        return b;
    }
    if head == "Missing } inserted" || head == "Missing { inserted" {
        let (have, want) = if head.contains('}') { ("{", "}") } else { ("}", "{") };
        b.what = format!("TeX expected a {want} here: a {have} has no partner, or a brace is in the wrong place.");
        b.hint = "Check that the braces on this line pair up; in math, check \\left/\\right pairs and that subscripts with more than one character are braced, as in x_{10}.".into();
        return b;
    }
    if let Some(l) = latex {
        if let Some((open, rest)) = l.strip_prefix("\\begin{").and_then(|r| r.split_once('}')) {
            let close = rest.rsplit_once("ended by ").map_or("", |x| x.1).trim_end_matches('.');
            let close_env = close.trim_start_matches("\\end{").trim_end_matches('}');
            b.at = Some(close.to_string());
            if open == "document" {
                b.what = format!("{close} has no \\begin{{{close_env}}} before it.");
                b.hint = format!("Delete {close}, or add \\begin{{{close_env}}} where the environment should start.");
            } else {
                let on = rest.strip_prefix(" on input line ").and_then(|r| r.split(' ').next()).map_or(String::new(), |n| format!(" on line {n}"));
                b.what = format!("\\begin{{{open}}}{on} is closed by {close}.");
                b.hint = format!("Every \\begin{{{open}}} needs its own \\end{{{open}}}, and environments close in the reverse order they open. Change {close} to \\end{{{open}}}, or add the missing \\end{{{open}}} before it.");
            }
            return b;
        }
        if let Some(env) = l.strip_prefix("Environment ").and_then(|r| r.strip_suffix(" undefined")) {
            b.what = format!("LaTeX does not know the environment {{{env}}}.");
            b.at = Some(format!("\\begin{{{env}}}"));
            b.hint = environment_hint(env, run);
            return b;
        }
        if let Some(file) = l.strip_prefix("File `").and_then(|r| r.split_once('\'')).map(|x| x.0) {
            return missing_file(b, file, run);
        }
        if l == "Missing \\begin{document}" {
            b.what = "Text comes before \\begin{document}.".into();
            b.at = first_word(line_text(run.source, line));
            b.hint = "Between \\documentclass and \\begin{document} (the preamble) go only settings such as \\usepackage and \\newcommand. Move the text below \\begin{document}, or add \\begin{document} if it is missing.".into();
            return b;
        }
    }
    if let Some(file) = head.strip_prefix("Package pdftex.def Error: File `").and_then(|r| r.split_once('\'')).map(|x| x.0) {
        return missing_file(b, file, run);
    }
    if let Some(file) = head.strip_prefix("I can't find file `").and_then(|r| r.split_once('\'')).map(|x| x.0) {
        return missing_file(b, file, run);
    }
    // LaTeX asks on the terminal for another file name (`\@missingfileerror`: a `\typeout`,
    // then `\read`), which a non-interactive run cannot answer: the file is in the log.
    if head.starts_with("*** (cannot \\read from terminal") {
        if let Some(file) = run.log.rsplit("LaTeX Error: File `").next().filter(|_| run.log.contains("LaTeX Error: File `")).and_then(|r| r.split_once('\'')).map(|x| x.0) {
            b.tex = format!("LaTeX Error: File `{file}' not found");
            return missing_file(b, file, run);
        }
        b.what = "TeX stopped to ask a question on the terminal, which a compiler run cannot answer.".into();
        b.hint = "This follows the error before it; fix that one.".into();
        return b;
    }
    if head.starts_with("*** (job aborted, no legal \\end found)") || (head == "Emergency stop" && !run.source.contains("\\end{document}")) {
        b.what = "The file ends without \\end{document}.".into();
        b.line = None;
        b.hint = "Add \\end{document} as the last line of the document.".into();
        return b;
    }
    if head == "Emergency stop" || head.starts_with("(That makes 100 errors") {
        b.what = "Compilation stopped because of the errors above.".into();
        b.hint = "Fix the first error and compile again; the later ones often follow from it.".into();
        return b;
    }
    for &(key, what, hint) in COMMON {
        if head.contains(key) {
            b.what = what.into();
            b.hint = hint.into();
            return b;
        }
    }
    if let Some((who, msg)) = head.split_once(" Error: ") {
        b.what = format!("{who} reports: {msg}.");
    } else {
        b.what = format!("{head}.");
    }
    b.hint = "This is the error as TeX gives it; the first error is usually the one to fix. Searching the web for the message often finds the cause.".into();
    b
}

/// Other errors beginners meet often: a piece of TeX's message, what it means and what to try.
const COMMON: &[(&str, &str, &str)] = &[
    ("Lonely \\item", "\\item is used outside a list.", "Put \\item inside itemize, enumerate or description, as in \\begin{itemize} \\item one \\end{itemize}."),
    ("Something's wrong--perhaps a missing \\item", "A list has text before its first \\item, or \\item is used outside a list.", "Start each list with \\item, and use \\item only inside itemize, enumerate or description."),
    ("There's no line here to end", "\\\\ (a line break) is used where no line is being typeset, such as after a blank line.", "Remove the \\\\; for space between paragraphs use a blank line or \\vspace{...}."),
    ("Can be used only in preamble", "This command may only come before \\begin{document}.", "Move it (often a \\usepackage) into the preamble, above \\begin{document}."),
    ("Misplaced alignment tab character &", "& is used outside a table or alignment.", "& separates columns only in tabular, array or align; in text write \\&."),
    ("Extra alignment tab has been changed to \\cr", "A row of the table has more & than the table has columns.", "Remove the extra &, or add a column to the table's column list, as in {lll}."),
    ("Double superscript", "Two ^ follow each other on one symbol.", "Group them with braces: x^{a^b} or {x^a}^b."),
    ("Double subscript", "Two _ follow each other on one symbol.", "Group them with braces: x_{a_b} or {x_a}_b."),
    ("Illegal unit of measure", "A length is missing its unit.", "Give lengths a unit, as in 2cm, 10pt or 0.5\\textwidth."),
    ("Missing number, treated as zero", "TeX expected a number or a length here.", "Check the command's argument: a length such as 2cm, or a number, belongs there."),
    ("already defined", "A command or environment of that name exists already.", "Use \\renewcommand (or \\renewenvironment) to change it, or choose another name."),
    ("Option clash for package", "A package is loaded twice with different options.", "Load it once, with all its options, or pass them first with \\PassOptionsToPackage."),
    ("Illegal parameter number in definition", "A # in a definition does not name an argument.", "In \\newcommand use #1, #2 ... up to the number of arguments; in text write \\#."),
    ("Display math should end with $$", "Display math is not closed the way it was opened.", "Close \\[ with \\] and $$ with $$; better, use \\[ ... \\] for displayed math."),
    ("Bad math environment delimiter", "Math is opened or closed twice.", "Do not put $ ... $ or \\( \\) inside \\[ ... \\] or an equation environment."),
    ("Undefined color", "This color is not defined.", "Define it with \\definecolor, or use a name xcolor knows (red, blue, ...); some names need \\usepackage[dvipsnames]{xcolor}."),
    ("Unknown graphics extension", "The picture's file type cannot be included.", "pdflatex includes .pdf, .png and .jpg pictures; convert the picture to one of these."),
    ("Unicode character", "A character in the file is not one LaTeX can typeset with these fonts.", "Write it with a LaTeX command, or remove it (copied text often holds invisible characters)."),
];

/// The hint for an undefined command: the package that defines it, else the nearest known
/// command.
fn command_hint(name: &str, run: &Run) -> String {
    if let Some(pkg) = package_of(name) {
        return format!("\\{name} comes from the {pkg} package: add \\usepackage{{{pkg}}} to the preamble, before \\begin{{document}}.");
    }
    match nearest(name, run) {
        Some((near, Some(pkg))) => format!("Did you mean \\{near}? It comes from the {pkg} package: add \\usepackage{{{pkg}}} to the preamble."),
        Some((near, None)) => format!("Did you mean \\{near}? Check the spelling."),
        None => format!("Check the spelling. To make a command of your own, define it in the preamble: \\newcommand{{\\{name}}}{{...}}."),
    }
}

/// The hint for an undefined environment.
fn environment_hint(env: &str, run: &Run) -> String {
    const THEOREMS: &[&str] = &["theorem", "lemma", "corollary", "proposition", "definition", "remark", "example", "conjecture", "claim", "exercise", "problem", "assumption"];
    if THEOREMS.contains(&env) {
        let mut title = env.to_string();
        title[..1].make_ascii_uppercase();
        return format!("Theorem-like environments are declared first, in the preamble: \\newtheorem{{{env}}}{{{title}}} (with \\usepackage{{amsthm}} for proof and styles).");
    }
    if let Some(pkg) = package_of(env) {
        return format!("The {env} environment comes from the {pkg} package: add \\usepackage{{{pkg}}} to the preamble.");
    }
    let envs: Vec<String> = run.defined.iter().filter(|n| is_defined(&format!("end{n}"), run)).cloned().collect();
    let envs = Run { defined: &envs, ..*run };
    match nearest(env, &envs) {
        Some((near, _)) => format!("Did you mean {{{near}}}? Check the spelling."),
        None => format!("Check the spelling, or define it in the preamble with \\newenvironment{{{env}}}{{...}}{{...}}."),
    }
}

/// The block for a file that was not found.
fn missing_file(mut b: Block, file: &str, run: &Run) -> Block {
    let ext = file.rsplit_once('.').map_or("", |x| x.1);
    let stem = file.strip_suffix(&format!(".{ext}")).unwrap_or(file);
    b.at = Some(if matches!(ext, "sty" | "cls" | "tex") { stem.to_string() } else { file.to_string() });
    match ext {
        "sty" => {
            b.what = format!("The package {stem} is not installed (no file {file} was found).");
            b.hint = format!("Check its name in \\usepackage{{{stem}}}. If the name is right, install it: in TeX Live run `tlmgr install {stem}` (Linux distributions package it in a texlive-* package; MiKTeX installs packages when first used).");
        }
        "cls" => {
            b.what = format!("The document class {stem} is not installed (no file {file} was found).");
            b.hint = format!("Check its name in \\documentclass{{{stem}}}. A journal's class usually comes with its template: put the .cls file beside your .tex file, or install it with `tlmgr install {stem}`.");
        }
        _ => {
            let kind = if matches!(ext, "png" | "jpg" | "jpeg" | "pdf" | "eps") { "picture file" } else { "file" };
            b.what = format!("The {kind} '{file}' was not found.");
            b.hint = format!("Paths are relative to the folder of {} ({}). Check the file's name and folder: names are case-sensitive, and a folder is written with / as in figures/plot.png.", run.file, run.dir);
        }
    }
    b
}

/// Which package defines `name` (a command or environment, no backslash): first the
/// common ones by hand, as the registry also lists every package that merely changes a
/// command; then the registry (`data_pkgs`) when one package alone defines it.
fn package_of(name: &str) -> Option<&'static str> {
    if let Some(&(_, pkg)) = PACKAGES.iter().find(|(n, _)| *n == name) {
        return Some(pkg);
    }
    let t = crate::data::data_pkgs::pkgs()?;
    let mut by = t.defined_by(name).iter().map(|i| t.file_name(i)).filter_map(|f| f.strip_suffix(".sty"));
    let one = by.next()?;
    by.next().is_none().then_some(one)
}

/// Commands and environments of the packages beginners load most, and the package to load.
const PACKAGES: &[(&str, &str)] = &[
    ("includegraphics", "graphicx"),
    ("graphicspath", "graphicx"),
    ("rotatebox", "graphicx"),
    ("scalebox", "graphicx"),
    ("resizebox", "graphicx"),
    ("mathbb", "amssymb"),
    ("mathfrak", "amssymb"),
    ("checkmark", "amssymb"),
    ("therefore", "amssymb"),
    ("text", "amsmath"),
    ("eqref", "amsmath"),
    ("boldsymbol", "amsmath"),
    ("operatorname", "amsmath"),
    ("DeclareMathOperator", "amsmath"),
    ("tfrac", "amsmath"),
    ("dfrac", "amsmath"),
    ("binom", "amsmath"),
    ("iint", "amsmath"),
    ("align", "amsmath"),
    ("gather", "amsmath"),
    ("multline", "amsmath"),
    ("split", "amsmath"),
    ("cases", "amsmath"),
    ("pmatrix", "amsmath"),
    ("bmatrix", "amsmath"),
    ("vmatrix", "amsmath"),
    ("proof", "amsthm"),
    ("newtheoremstyle", "amsthm"),
    ("theoremstyle", "amsthm"),
    ("mathscr", "mathrsfs"),
    ("bm", "bm"),
    ("textcolor", "xcolor"),
    ("color", "xcolor"),
    ("definecolor", "xcolor"),
    ("colorbox", "xcolor"),
    ("rowcolor", "colortbl"),
    ("href", "hyperref"),
    ("url", "url"),
    ("hypersetup", "hyperref"),
    ("autoref", "hyperref"),
    ("toprule", "booktabs"),
    ("midrule", "booktabs"),
    ("bottomrule", "booktabs"),
    ("multirow", "multirow"),
    ("tabularx", "tabularx"),
    ("longtable", "longtable"),
    ("SI", "siunitx"),
    ("si", "siunitx"),
    ("num", "siunitx"),
    ("qty", "siunitx"),
    ("unit", "siunitx"),
    ("citep", "natbib"),
    ("citet", "natbib"),
    ("citeauthor", "natbib"),
    ("subfloat", "subfig"),
    ("subfigure", "subcaption"),
    ("subcaption", "subcaption"),
    ("captionof", "caption"),
    ("captionsetup", "caption"),
    ("lstlisting", "listings"),
    ("lstinputlisting", "listings"),
    ("lstset", "listings"),
    ("tikzpicture", "tikz"),
    ("tikz", "tikz"),
    ("hl", "soul"),
    ("sout", "ulem"),
    ("uline", "ulem"),
    ("cancel", "cancel"),
    ("doublespacing", "setspace"),
    ("onehalfspacing", "setspace"),
    ("lipsum", "lipsum"),
    ("blindtext", "blindtext"),
    ("todo", "todonotes"),
    ("FloatBarrier", "placeins"),
    ("wrapfigure", "wrapfig"),
    ("landscape", "pdflscape"),
    ("algorithm", "algorithm"),
    ("algorithmic", "algpseudocode"),
    ("ce", "mhchem"),
    ("nicefrac", "nicefrac"),
    ("geometry", "geometry"),
    ("newgeometry", "geometry"),
    ("fancyhead", "fancyhdr"),
    ("fancyfoot", "fancyhdr"),
    ("xspace", "xspace"),
    ("setlist", "enumitem"),
    ("titleformat", "titlesec"),
    ("multicols", "multicol"),
    ("verbatiminput", "verbatim"),
    ("comment", "comment"),
    ("minted", "minted"),
    ("adjustbox", "adjustbox"),
    ("printbibliography", "biblatex"),
    ("addbibresource", "biblatex"),
];

/// The known command nearest to `name` by edit distance (case differences cost half), with
/// the package to load when it is not defined yet: the run's commands first, then the
/// common packages'.
fn nearest(name: &str, run: &Run) -> Option<(String, Option<&'static str>)> {
    let limit = match name.len() {
        0..=3 => 1,
        4..=7 => 2,
        _ => 3,
    };
    let mut best: Option<(usize, String, Option<&'static str>)> = None;
    let mut consider = |cand: &str, pkg: Option<&'static str>| {
        if cand == name || cand.len().abs_diff(name.len()) > limit {
            return;
        }
        let d = distance(name, cand);
        if d <= limit * 2 && best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
            best = Some((d, cand.to_string(), pkg));
        }
    };
    for cand in run.defined {
        consider(cand, None);
    }
    for &(cand, pkg) in PACKAGES {
        if !is_defined(cand, run) {
            consider(cand, Some(pkg));
        }
    }
    best.map(|(_, n, p)| (n, p))
}

/// Levenshtein distance in half steps: a change of letter case costs 1, any other edit 2.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).map(|j| 2 * j).collect();
    for i in 1..=a.len() {
        let mut diag = row[0];
        row[0] = 2 * i;
        for j in 1..=b.len() {
            let sub = if a[i - 1] == b[j - 1] { 0 } else if a[i - 1].eq_ignore_ascii_case(&b[j - 1]) { 1 } else { 2 };
            let next = (diag + sub).min(row[j] + 2).min(row[j - 1] + 2);
            diag = row[j];
            row[j] = next;
        }
    }
    row[b.len()]
}

/// Whether the run had `name` defined; a name the list does not hold (one letter, or not
/// all letters) counts as defined, so nothing is claimed about it.
fn is_defined(name: &str, run: &Run) -> bool {
    name.len() < 2 || !name.bytes().all(|c| c.is_ascii_alphabetic()) || run.defined.binary_search_by(|n| n.as_str().cmp(name)).is_ok()
}

/// The first command on `line` the run did not define.
fn undefined_on(line: &str, run: &Run) -> Option<String> {
    let mut rest = line;
    while let Some(i) = rest.find('\\') {
        let name: String = rest[i + 1..].chars().take_while(char::is_ascii_alphabetic).collect();
        if !is_defined(&name, run) {
            return Some(name);
        }
        // A one-character name may be any character, of any UTF-8 length.
        let step = if name.is_empty() { rest[i + 1..].chars().next().map_or(0, char::len_utf8) } else { name.len() };
        rest = &rest[i + 1 + step..];
    }
    None
}

/// Line `n` (1-based) of the source, without its line end.
fn line_text(source: &str, n: usize) -> &str {
    source.split('\n').nth(n.wrapping_sub(1)).unwrap_or("").trim_end_matches('\r')
}

/// The first line holding `needle`.
fn find_line(source: &str, needle: &str) -> Option<usize> {
    (!needle.is_empty()).then(|| source.split('\n').position(|l| l.contains(needle)).map(|i| i + 1)).flatten()
}

/// A source line for the terminal: its bytes (one `char` each) read as UTF-8, tabs as spaces.
fn shown(line: &str) -> String {
    let bytes: Vec<u8> = line.chars().map(|c| c as u32 as u8).collect();
    String::from_utf8_lossy(&bytes).replace('\t', " ")
}

/// The column and width of `at` on `text`, in characters.
fn caret(text: &str, at: &str) -> Option<(usize, usize)> {
    // A command name is matched whole: `\text` does not mark the start of `\textbf`.
    let mut from = 0;
    while let Some(i) = text[from..].find(at).map(|i| i + from) {
        let after = text[i + at.len()..].chars().next();
        if !(at.starts_with('\\') && at[1..].chars().all(|c| c.is_ascii_alphabetic()) && after.is_some_and(|c| c.is_ascii_alphabetic())) {
            let col = if at == "}" { unmatched_close(text).unwrap_or(i) } else if at == "{" { unclosed_open(text).unwrap_or(i) } else { i };
            return Some((text[..col].chars().count(), at.chars().count()));
        }
        from = i + at.len();
    }
    None
}

/// The first `^` or `_` on a line, not escaped: what a `Missing $` most often points at.
fn math_char(line: &str) -> Option<&'static str> {
    let b = line.as_bytes();
    (0..b.len()).find(|&i| matches!(b[i], b'^' | b'_') && (i == 0 || b[i - 1] != b'\\')).map(|i| if b[i] == b'^' { "^" } else { "_" })
}

/// The byte offset of the first `}` on a line that closes nothing opened on it.
fn unmatched_close(line: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut esc = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if !esc => {
                esc = true;
                continue;
            }
            '%' if !esc => break,
            '{' if !esc => depth += 1,
            '}' if !esc => {
                depth -= 1;
                if depth < 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        esc = false;
    }
    None
}

/// The byte offset of the last `{` on a line that is not closed on it.
fn unclosed_open(line: &str) -> Option<usize> {
    let mut open = Vec::new();
    let mut esc = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if !esc => {
                esc = true;
                continue;
            }
            '%' if !esc => break,
            '{' if !esc => open.push(i),
            '}' if !esc => {
                open.pop();
            }
            _ => {}
        }
        esc = false;
    }
    open.last().copied()
}

/// The line, at or before `line`, whose `{` is still open there: scanning back up to 40
/// lines, the first whose braces, with those of the lines after it up to `line`, leave one
/// open.
fn unclosed_brace(source: &str, line: usize) -> Option<usize> {
    let lines: Vec<&str> = source.split('\n').collect();
    let end = line.min(lines.len());
    let balance = |l: &str| {
        let mut esc = false;
        let mut d = 0i32;
        for c in l.chars() {
            match c {
                '\\' if !esc => {
                    esc = true;
                    continue;
                }
                '%' if !esc => break,
                '{' if !esc => d += 1,
                '}' if !esc => d -= 1,
                _ => {}
            }
            esc = false;
        }
        d
    };
    let mut total = 0;
    for k in (end.saturating_sub(40)..end).rev() {
        total += balance(lines[k]);
        if total > 0 {
            return Some(k + 1);
        }
    }
    None
}

/// The first word of a line, for the caret.
fn first_word(line: &str) -> Option<String> {
    line.split_whitespace().next().map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<'a>(source: &'a str, log: &'a str, defined: &'a [String]) -> Run<'a> {
        Run { file: "doc.tex", dir: "/home/me", source, log, defined }
    }

    #[test]
    fn undefined_command_gets_its_package_or_nearest_name() {
        let defined: Vec<String> = ["textbf", "textit", "section"].iter().map(|s| s.to_string()).collect();
        let src = "\\documentclass{article}\n\\begin{document}\nHello \\textbff{world}.\n\n\\includegraphics{x.png}\n\\end{document}\n";
        let out = report(&["l.3 [1] Not in the engine's boxes yet: \\textbff".into(), "l.5 [1] Undefined control sequence".into()], &run(src, "", &defined));
        assert!(out.contains("[ERROR] doc.tex, line 3: LaTeX does not know the command \\textbff."), "{out}");
        assert!(out.contains("  3 | Hello \\textbff{world}.\n    |       ^^^^^^^^\n"), "{out}");
        assert!(out.contains("Did you mean \\textbf?"), "{out}");
        assert!(out.contains("add \\usepackage{graphicx}"), "{out}");
        assert!(out.contains("(TeX's message: Undefined control sequence)"), "{out}");
        assert!(!out.lines().any(|l| l.starts_with("! ")), "{out}");
    }

    #[test]
    fn undefined_without_a_name_is_still_a_mistake() {
        let out = report(&["l.3 [2] Undefined control sequence".into()], &run("a\nb\nc\n", "", &[]));
        assert!(!out.contains("RushTeX cannot typeset"), "{out}");
        assert!(out.contains("in a file it loads"), "{out}");
        let out = report(&["l.1 [1] Undefined control sequence".into()], &run("x \\\u{e9} \\\\ \\", "", &[]));
        assert!(out.contains("LaTeX does not know a command used on this line"), "{out}");
    }

    #[test]
    fn undefined_in_an_environment_body_is_found_above() {
        let src = "a\n\\begin{align}\nc &= \\mathbb{R}\n\\end{align}\n";
        let defined: Vec<String> = vec!["begin".into(), "end".into()];
        let out = report(&["l.4 [1] Undefined control sequence".into(), "l.4 [1] Undefined control sequence".into(), "l.5 [1] *** (job aborted, no legal \\end found)".into()], &run(src, "", &defined));
        assert!(out.contains("doc.tex, line 3: LaTeX does not know the command \\mathbb."), "{out}");
        assert!(out.contains("add \\usepackage{amssymb}"), "{out}");
        assert_eq!(out.matches("[ERROR]").count(), 1, "{out}");
    }

    #[test]
    fn engine_gap_is_not_called_a_mistake() {
        let defined: Vec<String> = vec!["vadjust".into()];
        let out = report(&["l.3 [1] Not in the engine's boxes yet: \\vadjust".into()], &run("a\nb\nx\\vadjust{}\n", "", &defined));
        assert!(out.contains("RushTeX cannot typeset \\vadjust here yet."), "{out}");
    }

    #[test]
    fn missing_dollar_pair_is_one_block() {
        let out = report(&["l.3 [1] Missing $ inserted".into(), "l.4 [1] Missing $ inserted".into()], &run("a\nb\nThe value x^2 is large.\n\\end{document}\n", "", &[]));
        assert_eq!(out.matches("[ERROR]").count(), 1, "{out}");
        assert!(out.contains("  3 | The value x^2 is large.\n    |            ^\n"), "{out}");
        assert!(out.contains("2 errors in doc.tex (1 more not shown"), "{out}");
    }

    #[test]
    fn missing_package_is_found_in_the_log_and_its_line() {
        let log = "\n! LaTeX Error: File `nosuchpkg.sty' not found.\n\nType X to quit or <RETURN> to proceed,\n";
        let out = report(&["l.3 [1] *** (cannot \\read from terminal in nonstop modes)".into()], &run("\\documentclass{article}\n\\usepackage{nosuchpkg}\n\\begin{document}\n", log, &[]));
        assert!(out.contains("doc.tex, line 2: The package nosuchpkg is not installed"), "{out}");
        assert!(out.contains("tlmgr install nosuchpkg"), "{out}");
    }

    #[test]
    fn environment_errors() {
        let e = |m: &str| format!("l.5 [1] ! LaTeX Error: {m}\n\nSee the LaTeX manual or LaTeX Companion for explanation.\nType  H <return>  for immediate help");
        let src = "a\nb\n\\begin{itemize}\n\\item one\n\\end{enumerate}\n";
        let out = report(&[e("\\begin{itemize} on input line 3 ended by \\end{enumerate}.")], &run(src, "", &[]));
        assert!(out.contains("\\begin{itemize} on line 3 is closed by \\end{enumerate}."), "{out}");
        let out = report(&[e("Environment theorem undefined."), e("\\begin{document} ended by \\end{theorem}.")], &run("a\nb\nc\nd\n\\begin{theorem}\n", "", &[]));
        assert!(out.contains("\\newtheorem{theorem}{Theorem}"), "{out}");
        assert_eq!(out.matches("[ERROR]").count(), 1, "{out}");
    }

    #[test]
    fn unclosed_brace_points_at_its_line() {
        let src = "\\documentclass{article}\n\\begin{document}\n\\textbf{bold text\nand more.\n\n\\end{document}\n";
        let out = report(&["l.6 [1] File ended while scanning use of \\textbf".into()], &run(src, "", &[]));
        assert!(out.contains("line 3: A } is missing: the { on line 3 that opens the argument of \\textbf is never closed."), "{out}");
        assert!(out.contains("  3 | \\textbf{bold text\n    |        ^\n"), "{out}");
        let src = "\\documentclass{article}\n\\begin{document}\n\\section{abc\n\ndef}\n\\end{document}\n";
        let out = report(&["l.5 [1] Paragraph ended before \\@sect was complete".into(), "l.5 [1] Too many }'s".into()], &run(src, "", &[]));
        assert!(out.contains("line 3: A } is missing: the { on line 3 that opens a command's argument"), "{out}");
        assert!(out.contains("(1 more not shown, on the same lines"), "{out}");
    }

    #[test]
    fn warnings_are_one_line_each() {
        let log = "LaTeX Warning: Citation `x' on page 1 undefined on input line 5.\nLaTeX Warning: Reference `y' on page 1 undefined on input line 5.\nLaTeX Warning: Reference `y' on page 1 undefined on input line 6.\n";
        let out = warnings(log, [1, 2]);
        assert!(out.contains("[WARN] 1 reference has no \\label (printed as ??): \\ref{y}."), "{out}");
        assert!(out.contains("[WARN] 1 citation not found"), "{out}");
        assert!(out.contains("2 lines or boxes too wide"), "{out}");
        assert_eq!(out.lines().count(), 3);
    }
}
