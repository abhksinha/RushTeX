//! screenplay.cls (v1.6, 2012/06/30) on the GDL path: article at 12pt with the class's own
//! page, courier as the family default, `\raggedright`, `myheadings`, and its commands
//! `\coverpage`, `\slug` (`\intslug`, `\extslug`), `dialogue` and `\paren`. The
//! definitions of `\slug`, `\paren` and `dialogue` start with a blank line (`\par`), and
//! `\slug`'s and `\enddialogue`'s end with one.
//!
//! The class's parameters (its `\textwidth`, `\textheight`, margins, `\parskip`, the flush
//! `\rightskip`) are the class table (`rushtex-gen class --class screenplay`). What is code
//! is here: the macros the table does not hold (`\familydefault`, `\baselinestretch`, the
//! float fractions), geometry's `dvips` first-page `\special{papersize=..}`, and the
//! commands, as the tokens the class's definitions expand to: its line ends are spaces where
//! TeX reads them so (after a character or a control symbol), its `\ifthenelse{\not\equal
//! {#1}{}}` decided here, its lengths written as the class sets them.

use super::super::engine_run::Machine;
use super::super::engine_catcode::Cat;
use super::super::engine_token::Token;

/// The class's `\@realauthor` (unset: `\@author`), `\@address`, `\@agent`.
#[derive(Default, Clone, PartialEq)]
pub(super) struct Screenplay {
    realauthor: Option<Vec<Token>>,
    address: Option<Vec<Token>>,
    agent: Option<Vec<Token>>,
}

pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("realauthor", |m| m.sp_field(0)),
    ("agent", |m| m.sp_field(2)),
    ("coverpage", |m| m.sp_coverpage()),
    ("slug", |m| m.sp_slug(None)),
    ("intslug", |m| m.sp_slug(Some("INT"))),
    ("extslug", |m| m.sp_slug(Some("EXT"))),
    ("paren", |m| m.sp_paren()),
    ("rt@nopagebreak", |m| m.sp_nopagebreak()),
];

/// The commands screenplay.cls defines that the pre-scan takes under it (and `dialogue`).
pub(super) const SCREENPLAY_OWN: &[&str] = &["realauthor", "agent", "address", "coverpage", "slug", "intslug", "extslug", "paren"];

/// screenplay.cls's commands in place of the kernel's (`engine_doc::class_hooks`).
pub(super) const HOOKS: super::ClassHooks = super::ClassHooks {
    setup: |m, _| {
        if let Some(doc) = m.doc.as_mut() {
            doc.pkg.screenplay = Some(Box::default());
            // `\pagestyle{myheadings}`.
            (doc.empty_style, doc.headings, doc.my_headings, doc.std_heads) = (false, false, true, true);
        }
        // courier.sty's `\renewcommand{\ttdefault}{pcr}`, then the class's
        // `\renewcommand*{\familydefault}{\ttdefault}` and its float fractions. Its
        // `\renewcommand*{\baselinestretch}{0.8}` is in the table's size rows already (the
        // generator ran each size command under it), so `\fontsize` stops in the pre-scan.
        let t = m.tex_tokens("\\long\\def\\ttdefault{pcr}\\def\\familydefault{\\ttdefault}\\def\\textfraction{1}\\def\\topfraction{0}\\def\\bottomfraction{0}");
        m.back_list(&t);
    },
    address: |m| m.sp_field(1),
    ..super::KERNEL_HOOKS
};

impl Machine<'_, '_> {
    fn sp(&mut self) -> Option<&mut Screenplay> {
        self.doc.as_mut().and_then(|d| d.pkg.screenplay.as_deref_mut())
    }

    /// The class's `\raggedright` at its load, done at `\begin{document}`.
    pub(super) fn screenplay_begin(&mut self) {
        if self.sp().is_some() {
            let t = self.tex_tokens("\\raggedright ");
            self.back_list(&t);
        }
    }

    /// `\realauthor{#1}` (0), `\address{#1}` (1), `\agent{#1}` (2): `\renewcommand{\@..}{#1}`.
    fn sp_field(&mut self, k: usize) {
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let Some(s) = self.sp() else { return self.not_native("screenplay's commands under another class") };
        *[&mut s.realauthor, &mut s.address, &mut s.agent][k] = Some(arg);
    }

    /// Length `a` plus length `b` in sp, as `\setlength` then `\addtolength` leave it.
    fn sp_sum(&mut self, a: &str, b: &str) -> String {
        format!("{}sp", self.glue_of(a).width + self.glue_of(b).width)
    }

    /// `\coverpage`: `\thispagestyle{empty}\vspace*{\titheadskip}`, the centred
    /// `{\MakeUppercase{\@title}}`, `\bytext` and `\@author`, `\vspace{\titskip}`, the agent's
    /// and the writer's `\parbox[t]{\addrwidth}`s, `\newpage\setcounter{page}{1}`.
    fn sp_coverpage(&mut self) {
        let Some(s) = self.sp() else { return self.not_native("screenplay's commands under another class") };
        let (realauthor, address, agent) = (s.realauthor.clone(), s.address.clone(), s.agent.clone());
        let Some(doc) = self.doc.as_ref() else { return };
        let [title, author, _] = doc.stored.clone();
        if title.is_empty() || !doc.authored {
            return self.not_native("screenplay's \\coverpage without \\title and \\author");
        }
        let mut t = self.tex_tokens(" \\thispagestyle{empty} \\vspace*{2.25in} \\begin{center}{\\MakeUppercase{");
        t.extend_from_slice(&title);
        t.extend(self.tex_tokens("}}\\\\ \\vspace{\\baselineskip} by\\\\ \\vspace{\\baselineskip} "));
        t.extend_from_slice(&author);
        t.extend(self.tex_tokens("\\end{center} \\vspace{3.75in plus 0in minus 1.5in} \\parbox[t]{2.0in}{"));
        match agent {
            Some(a) => t.extend(a),
            None => t.push(Token::char(Cat::Active, '~')),
        }
        t.extend(self.tex_tokens("}\\hspace{2.0in plus 0in minus 0.5in} \\parbox[t]{2.0in}{"));
        t.extend(realauthor.unwrap_or(author));
        t.extend(self.tex_tokens("\\\\"));
        match address {
            Some(a) => t.extend(a),
            None => t.extend(self.tex_tokens("Contact via Agency")),
        }
        t.extend(self.tex_tokens("} \\newpage\\setcounter{page}{1} "));
        self.back_list(&t);
    }

    /// `\slug[#1]{#2}{#3}`: `\MakeUppercase{#2\placesep#3}`, then when `#1` is not empty
    /// `\slugspace\MakeUppercase{#1}\hspace{\fill}`, a space, `\nopagebreak`. `\intslug[#1]
    /// {#2}` and `\extslug` are `\slug[#1]{\inttext}{#2}` and kin (`place`).
    fn sp_slug(&mut self, place: Option<&str>) {
        if self.sp().is_none() {
            return self.not_native("screenplay's commands under another class");
        }
        let opt = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let mut a = Vec::new();
        match place {
            Some(p) => a = self.tex_tokens(p),
            None => self.read_arg(&mut a),
        }
        let mut b = Vec::new();
        self.read_arg(&mut b);
        // `\placesep` (`.~`) between the two, outside `\MakeUppercase` (the same characters).
        let mut t = self.tex_tokens("\\par\\MakeUppercase{");
        t.extend(a);
        t.extend(self.tex_tokens("}."));
        t.push(Token::char(Cat::Active, '~'));
        t.extend(self.tex_tokens("\\MakeUppercase{"));
        t.extend(b);
        t.extend(self.tex_tokens("}"));
        if !opt.is_empty() {
            t.extend(self.tex_tokens("\\hspace{2.0em plus 0em minus 0em}\\MakeUppercase{"));
            t.extend(opt);
            t.extend(self.tex_tokens("}\\hspace{\\fill} "));
        }
        t.push(Token::char(Cat::Space, ' '));
        t.push(self.doc_id(doc!("rt@nopagebreak")));
        t.extend(self.tex_tokens("\\par"));
        self.back_list(&t);
    }

    /// `\paren{#1}`: `\parentab\parbox[t]{\parenwidth}{\raggedright(#1)\dialfix}\\`, a space.
    fn sp_paren(&mut self) {
        if self.sp().is_none() {
            return self.not_native("screenplay's commands under another class");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let t = self.sp_paren_tokens(arg);
        self.back_list(&t);
    }

    fn sp_paren_tokens(&mut self, arg: Vec<Token>) -> Vec<Token> {
        let w = self.sp_sum("1.5in", "0.2in");
        let mut t = self.tex_tokens(&format!("\\par\\hspace{{0.7in}}\\parbox[t]{{{w}}}{{\\raggedright("));
        t.extend(arg);
        t.extend(self.tex_tokens(")\\vspace{2pt}}\\\\ "));
        t
    }

    /// `\begin{dialogue}[#1]{#2}` after `\begin`'s group: `\dialtab\dialnametab\hbox
    /// {\MakeUppercase{#2}}\\*`, a space, `\dialtab\begin{minipage}[b]{\dialwidth}`, a space,
    /// `\dialfix\raggedright`, ` \paren{#1} ` when `#1` is not empty, a space.
    pub(super) fn sp_dialogue_begin(&mut self) {
        let opt = if self.next_is('[') { self.bracket_arg() } else { Vec::new() };
        let mut name = Vec::new();
        self.read_arg(&mut name);
        let w = self.sp_sum("3.0in", "0.2in");
        let mut t = self.tex_tokens("\\par\\hspace*{1.5in}\\hspace*{1.2in}\\hbox{\\MakeUppercase{");
        t.extend(name);
        t.extend(self.tex_tokens(&format!("}}}}\\\\* \\hspace*{{1.5in}}\\begin{{minipage}}[b]{{{w}}} \\vspace{{2pt}}\\raggedright ")));
        if !opt.is_empty() {
            t.extend(self.tex_tokens(" "));
            let p = self.sp_paren_tokens(opt);
            t.extend(p);
        }
        t.extend(self.tex_tokens(" "));
        self.back_list(&t);
    }

    /// `\nopagebreak` (`\@nopgbk[4]`): `\penalty\@M` in vertical mode, else `\@bsphack
    /// \vadjust{\penalty\@M}\@esphack`; `\@testopt` first skips spaces.
    fn sp_nopagebreak(&mut self) {
        if self.next_is('[') {
            return self.not_native("\\nopagebreak with a number");
        }
        self.no_page_break();
    }
}
