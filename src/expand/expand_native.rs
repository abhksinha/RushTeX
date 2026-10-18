//! Control words the parsers implement natively.
//!
//! `\providecommand` must not shadow these (LaTeX defines a command only when it is
//! undefined). Extracted from the dispatch arms of `math_parser.rs` and
//! `frontend_parser.rs`; symbol-table commands are checked via [`crate::math::lookup_symbol`].
//! Kept sorted for binary search.

pub(crate) static NATIVE_COMMANDS: [&str; 336] = [
    "Big", "Bigg", "Biggl", "Biggm", "Biggr", "Bigl", "Bigm", "Bigr", "Bmatrix", "Delta", "Gamma",
    "Lambda", "Leftarrow", "Leftrightarrow", "Omega", "Phi", "Pi", "Pr", "Psi", "Rightarrow",
    "Sigma", "Theta", "Upsilon", "Uskewed", "Vmatrix", "Xi", "acute", "align", "alignat",
    "aligned", "alpha", "approx", "array", "bar", "begin", "beta", "big", "bigcap", "bigcup",
    "bigg", "biggl", "biggm", "biggr", "bigl", "bigm", "bigodot", "bigoplus", "bigotimes", "bigr",
    "bigskip", "bigsqcup", "biguplus", "bigvee", "bigwedge", "binom", "bm", "bmatrix", "bmod",
    "boldsymbol", "bordermatrix", "breve", "cancel", "cap", "caption", "cases", "cdot", "cdots",
    "cfrac", "check", "chi", "circ", "clearpage", "colon", "cong", "coprod", "cr", "crcr", "cup",
    "dag", "dbinom", "ddag", "ddddot", "dddot", "ddot", "ddots", "degree", "delta", "description",
    "det", "dfrac", "displaymath", "displaystyle", "div", "dot", "dots", "dotsb", "dotsm", "dotso",
    "emergencystretch", "emph", "empty", "emptyset", "end", "enspace", "enumerate", "epsilon",
    "eqalign", "eqalignno", "equation", "equiv", "eta", "exists", "figure", "forall", "frac",
    "frak", "frenchspacing", "fussy", "gamma", "gather", "gathered", "gcd", "ge", "genfrac", "geq",
    "gets", "grave", "halign", "hangafter", "hangindent", "hat", "hbar", "hbox", "hphantom",
    "hrule", "hsize", "hspace", "iiint", "iint", "in", "includegraphics", "indent", "inf", "infty",
    "int", "iota", "item", "itemindent", "itemize", "itemsep", "jot", "kappa", "labelsep",
    "lambda", "lastlinefit", "ldots", "le", "left", "leftarrow", "leftmargin", "leftrightarrow",
    "leftskip", "leq", "leqalignno", "lim", "liminf", "limsup", "linewidth", "listoffigures",
    "listoftables", "listparindent", "llap", "longtable", "looseness", "lstlisting", "maketitle",
    "mathbb", "mathbf", "mathbin", "mathcal", "mathchoice", "mathclose", "mathfrak", "mathinner",
    "mathit", "mathop", "mathopen", "mathord", "mathpunct", "mathrel", "mathring", "mathrm",
    "mathscr", "mathsf", "mathtt", "matrix", "max", "mbox", "medskip", "medspace", "middle", "min",
    "mu", "multispan", "multline", "nabla", "ne", "negmedspace", "negthickspace", "negthinspace",
    "neq", "newpage", "noindent", "nonfrenchspacing", "nonscript", "not", "notin", "nu", "oiiint",
    "oiint", "oint", "omega", "omit", "openup", "operatorname", "oplus", "otimes", "overbrace",
    "overbracket", "overleftarrow", "overline", "overrightarrow", "pagestyle", "par",
    "parfillskip", "parindent", "parshape", "parskip", "partial", "perp", "phantom", "phi", "pi",
    "pm", "pmatrix", "pmod", "pretolerance", "prevgraf", "prod", "propto", "psi", "qquad", "quad",
    "quotation", "quote", "rho", "rightarrow", "rightskip", "rlap", "scriptscriptstyle",
    "scriptstyle", "section", "setlength", "sfrac", "sigma", "sim", "sloppy", "smallmatrix",
    "smallskip", "smash", "split", "sqrt", "strut", "subsection", "subset", "subseteq",
    "subsubsection", "sum", "sup", "supset", "supseteq", "table", "tableofcontents", "tabular",
    "tau", "tbinom", "text", "textasciicircum", "textasciitilde", "textbackslash", "textbf",
    "textbullet", "textemdash", "textendash", "textit", "textstyle", "texttt", "textwidth",
    "tfrac", "theta", "thickspace", "thinspace", "thispagestyle", "tilde", "times", "to",
    "tolerance", "underaccent", "underbar", "underbrace", "underbracket", "underline", "upsilon",
    "utilde", "varepsilon", "varphi", "varpi", "varrho", "varsigma", "vartheta", "vcenter",
    "vdots", "vec", "verb", "verbatim", "vmatrix", "vphantom", "vspace", "widehat", "widetilde",
    "xi", "xleftarrow", "xleftrightarrow", "xrightarrow", "zero", "zeta",
];

#[inline]
pub(crate) fn is_native(name: &str) -> bool {
    NATIVE_COMMANDS.binary_search(&name).is_ok() || crate::math::lookup_symbol(name).is_some()
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_table_is_sorted() {
        assert!(super::NATIVE_COMMANDS.windows(2).all(|w| w[0] < w[1]));
    }
}
