//! Document classes built on `article`, as the code they run after `\LoadClass{article}`.
//!
//! rushtex lays out `article` natively; a derived class adds its own commands and settings,
//! which the expander reads right after `\documentclass`, like LaTeX reads the `.cls` file.
//! Each entry follows its class file line by line (`scripts/refindex.py` finds the
//! original); lines needing machinery rushtex does not model yet are noted in place.

/// The code of class `name` after its `\LoadClass{article}`, if rushtex knows the class.
pub(crate) fn class_code(name: &str) -> Option<&'static str> {
    Some(match name {
        "ltxguide" => LTXGUIDE,
        _ => return None,
    })
}

/// The standard class that class `name` loads (`\LoadClass`), for the page geometry and
/// everything else rushtex lays out natively; `name` itself for the standard classes.
pub(crate) fn base_class(name: &str) -> &str {
    match name {
        "ltxguide" => "article",
        _ => name,
    }
}

/// Whether class `name` redefines `verbatim` as ltxguide.cls does: `\ifhmode\unskip\par\fi`
/// (so the trivlist always starts in vertical mode), then `\small` when the text is at
/// `\normalsize`, then the kernel's `verbatim`.
pub(crate) fn ltxguide_verbatim(name: &str) -> bool {
    name == "ltxguide"
}

/// ltxguide.cls 2001/05/28 (the LaTeX team's guides: usrguide, clsguide, fntguide, ...).
const LTXGUIDE: &str = r"%
\setlength{\parindent}{0pt}%
\setlength{\parskip}{\medskipamount}%
\newcommand{\clsguide}{\LaTeXe{} for Class and Package Writers}%
\newcommand{\usrguide}{\LaTeXe{} for Authors}%
\newcommand{\fntguide}{\LaTeXe{} Font Selection}%
\newcommand{\cfgguide}{Configuration options for \LaTeXe}%
\newcommand{\cyrguide}{Cyrillic languages support in \LaTeX}%
\newcommand{\modguide}{Modifying \LaTeX}%
\newcommand{\sourcecode}{\LaTeX: the program}%
\newcommand{\LaTeXbook}{\LaTeX: A Document Preparation System}%
\newcommand{\LaTeXcomp}{The \LaTeX{} Companion}%
\newcommand\LaTeXGcomp{The \LaTeX{} Graphics Companion}%
\newcommand\LaTeXWcomp{The \LaTeX{} Web Companion}%
\newcommand{\babel}{\textsf{babel}}%
\newcommand{\ctan}{\textsc{ctan}}%
\newcommand{\eg}{e.g.,~}%
\newcommand{\ie}{i.e.,~}%
\newcommand{\SLiTeX}{\textsc{Sli}\TeX}%
\MakeShortVerb{\|}%
\newcommand{\m}[1]{\mbox{$\langle$\it #1\/$\rangle$}}%
\renewcommand{\arg}[1]{{\tt\char`\{}\m{#1}{\tt\char`\}}}%
\newcommand{\oarg}[1]{{\tt[}\m{#1}{\tt]}}%
\newcommand{\NFSS}{\textsf{NFSS}}%
\newcommand{\AmSLaTeX}{\mbox{\AmS-\LaTeX}}%
\setcounter{tocdepth}{2}%
\setlength{\tolerance}{500}%
\newcommand{\URL}[1]{\texttt{#1}}%
";
