//! listings 1.11b (2025/11/14) on the GDL path (`2026-11-01-w9-listings.md`): `\lstset{keys}`
//! in the preamble, the `lstlisting` environment and `\lstinline`, with the keys [`KEYS`]
//! holds and the languages [`language`] knows (keywords, comments and strings); anything
//! else (tabs, escapes, other languages, C's directives) goes to the engine.
//!
//! The listing is listings' own steps (listings.sty, lstmisc.sty), as the boxes they make:
//! - `\lst@Init`: `\par\penalty-50\vspace\lst@aboveskip`, `\normalbaselines`,
//!   `\lst@MakeCaption t` (`\allowbreak`, `\lst@HRefStepCounter` or `\refstepcounter
//!   {lstlisting}`), then the `Init` hooks: `\lst@basicstyle`, lineshape's `\parskip\z@` and
//!   `breaklines` (`\hbadness\@M \pretolerance\@M`, `\rightskip\@flushglue`), and frames'
//!   `\lst@frameInit`: the side box `\lst@framebox` (`\lst@frameMakeBoxV`) and the top rule
//!   (`\lst@frameH T`) as a paragraph of its own; then `\lineskiplimit\maxdimen \lineskip\z@`.
//! - The text: each line `\lst@NewLine` (`\par\noindent\hbox{}`), `\everypar` holding
//!   `\lst@parshape`, `\lst@framelr` (a copy of the side box) and labels' `\refstepcounter
//!   {lstnumber}`; each token `\lst@OutputToken`: the lost space as a kerned box
//!   (`\lst@Kern`), the token's box (`\hbox to\lst@length\lst@width` with `\hss` between the
//!   characters under `columns=fixed`, `\hbox` under `fullflexible`) in a box of its own
//!   (`\lst@CalcLostSpaceAndOutput`), then with `breaklines` the `\lst@discretionary`.
//!   [`layout`] is the character loop (`\lst@ProcessLetter`, `\lst@ProcessSpace`, ...).
//! - `\lst@DeInit`: `\par\removelastskip`, the bottom rule (`\lst@frameExit`),
//!   `\normalbaselines\normalcolor`, `\par\penalty-50\vspace\lst@belowskip`.

use super::super::engine_eqtb::{Glue, Reg, Region};
use super::super::engine_node::{BoxRec, Kind, Node, RUNNING};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_catcode::Cat;
use super::super::engine_token::Token;
use super::super::engine_box::{Ctx, HBOX_GROUP};
use super::{Change, Op};

/// The keys a native listing takes (listings' keys of the same names); the pre-scan's list
/// ([`keys_ok`]).
const KEYS: &[&str] = &["basicstyle", "keywordstyle", "commentstyle", "stringstyle", "columns", "frame", "framerule", "framesep", "rulesep", "backgroundcolor", "rulecolor", "breaklines", "language", "showstringspaces", "numbers", "numberstyle", "numbersep", "stepnumber", "firstnumber", "tabsize", "caption", "style", "showspaces", "showtabs", "extendedchars", "keepspaces", "upquote", "numberbychapter", "captionpos", "breakatwhitespace", "aboveskip", "belowskip"];

/// `\medskipamount` (latex.ltx: 6pt plus 2pt minus 2pt), `aboveskip`'s and `belowskip`'s default.
const MEDSKIP: Glue = Glue { width: 6 * 65536, stretch: 2 * 65536, shrink: 2 * 65536, stretch_order: 0, shrink_order: 0 };

/// Commands a style value may hold (with `\color{name}`).
const STYLES: &[&str] = &["ttfamily", "bfseries", "mdseries", "itshape", "upshape", "slshape", "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large"];

/// lstlang1.sty's `[2]Python` (listings.cfg's `defaultdialect=[2]Python`): `morekeywords`
/// and `morekeywords=[2]`, both in `keywordstyle` (`\lst@ProvideStyle` gives class 2 the
/// first class's style when it has none). Sorted, for `binary_search`.
const PYTHON_KEYWORDS: &[&str] = &[
    "__import__", "abs", "all", "and", "any", "apply", "as", "assert", "basestring", "bin", "bool", "break", "buffer", "bytearray", "callable", "chr", "class", "classmethod", "cmp", "coerce", "compile", "complex", "continue", "def", "del", "delattr", "dict", "dir", "divmod", "elif", "else", "enumerate", "eval", "except", "exec", "execfile", "file", "filter", "finally", "float", "for", "format", "from", "frozenset", "getattr", "global", "globals", "hasattr", "hash", "help", "hex", "id", "if", "import", "in", "input", "int", "intern", "is", "isinstance", "issubclass", "iter", "lambda", "len", "list", "locals", "long", "map", "max", "memoryview", "min", "next", "not", "object", "oct", "open", "or", "ord", "pass", "pow", "print", "property", "raise", "range", "raw_input", "reduce", "reload", "repr", "return", "reversed", "round", "set", "setattr", "slice", "sorted", "staticmethod", "str", "sum", "super", "try", "tuple", "type", "unichr", "unicode", "vars", "while", "with", "xrange", "yield", "zip",
];

/// lstlang1.sty's `[ANSI]C` (listings.cfg's `defaultdialect=[ANSI]C`). Sorted.
const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum", "extern", "float", "for", "goto", "if", "int", "long", "register", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void", "volatile", "while",
];

/// lstlang1.sty's `[ISO]C++` (the default dialect): `[ANSI]C`'s and its own. Sorted.
const CPP_KEYWORDS: &[&str] = &[
    "and", "and_eq", "asm", "auto", "bad_cast", "bad_typeid", "bitand", "bitor", "bool", "break", "case", "catch", "char", "class", "compl", "const", "const_cast", "continue", "default", "delete", "do", "double", "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto", "if", "inline", "int", "long", "mutable", "namespace", "new", "not", "not_eq", "operator", "or", "or_eq", "private", "protected", "public", "register", "reinterpret_cast", "return", "short", "signed", "sizeof", "static", "static_cast", "struct", "switch", "template", "this", "throw", "true", "try", "type_info", "typedef", "typeid", "typename", "union", "unsigned", "using", "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq",
];

/// lstlang1.sty's `Java`. Sorted.
const JAVA_KEYWORDS: &[&str] = &[
    "abstract", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do", "double", "else", "extends", "false", "final", "finally", "float", "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "label", "long", "native", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "true", "try", "void", "volatile", "while",
];

/// lstlang1.sty's `sh`. Sorted.
const SH_KEYWORDS: &[&str] = &[
    "awk", "break", "case", "cat", "cd", "continue", "do", "done", "echo", "elif", "else", "env", "esac", "eval", "exec", "exit", "export", "expr", "false", "fi", "for", "function", "getopts", "hash", "history", "if", "in", "kill", "login", "newgrp", "nice", "nohup", "ps", "pwd", "read", "readonly", "return", "sed", "set", "shift", "test", "then", "times", "trap", "true", "type", "ulimit", "umask", "unset", "until", "wait", "while",
];

/// lstlang1.sty's `bash`: `sh`'s and its own. Sorted.
const BASH_KEYWORDS: &[&str] = &[
    "alias", "awk", "bg", "bind", "break", "builtin", "caller", "case", "cat", "cd", "command", "compgen", "complete", "compopt", "continue", "coproc", "declare", "dirs", "disown", "do", "done", "echo", "elif", "else", "enable", "env", "esac", "eval", "exec", "exit", "export", "expr", "false", "fc", "fg", "fi", "for", "function", "getopts", "hash", "help", "history", "if", "in", "jobs", "kill", "let", "local", "login", "logout", "mapfile", "newgrp", "nice", "nohup", "popd", "printf", "ps", "pushd", "pwd", "read", "readarray", "readonly", "return", "sed", "select", "set", "shift", "shopt", "source", "suspend", "test", "then", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", "unset", "until", "wait", "while",
];

/// lstlang1.sty's `Matlab`. Sorted.
const MATLAB_KEYWORDS: &[&str] = &[
    "Colon", "ColorSpec", "Inf", "NaN", "abs", "acos", "acosh", "acot", "acoth", "acsc", "acsch", "all", "amp", "angle", "ans", "any", "asec", "asech", "asin", "asinh", "atan", "atan2", "atanh", "auread", "auwrite", "axes", "axis", "balance", "bar", "bessel", "besselk", "bessely", "beta", "betainc", "betaln", "blanks", "bone", "break", "brighten", "capture", "cart2pol", "cart2sph", "caxis", "cd", "cdf2rdf", "cedit", "ceil", "chol", "cla", "clabel", "clc", "clear", "clf", "clock", "close", "colmmd", "colorbar", "colormap", "colperm", "comet", "comet3", "compan", "compass", "computer", "cond", "condest", "conj", "contour", "contour3", "contourc", "contrast", "conv", "conv2", "cool", "copper", "corrcoef", "cos", "cosh", "cot", "coth", "cov", "cplxpair", "cputime", "cross", "csc", "csch", "csvread", "csvwrite", "cumprod", "cumsum", "cylinder", "date", "dbclear", "dbcont", "dbdown", "dbquit", "dbstack", "dbstatus", "dbstep", "dbstop", "dbtype", "dbup", "ddeadv", "ddeexec", "ddeinit", "ddepoke", "ddereq", "ddeterm", "ddeunadv", "deblank", "dec2hex", "deconv", "del2", "delete", "demo", "det", "diag", "diary", "diff", "diffuse", "dir", "disp", "dlmread", "dlmwrite", "dmperm", "dot", "drawnow", "echo", "eig", "ellipj", "ellipke", "else", "elseif", "end", "engClose", "engEvalString", "engGetFull", "engGetMatrix", "engOpen", "engOutputBuffer", "engPutFull", "engPutMatrix", "engSetEvalCallback", "engSetEvalTimeout", "engWinInit", "eps", "erf", "erfc", "erfcx", "erfinv", "error", "errorbar", "etime", "etree", "eval", "exist", "exp", "expint", "expm", "expo", "eye", "fclose", "feather", "feof", "ferror", "feval", "fft", "fft2", "fftshift", "fgetl", "fgets", "figure", "fill", "fill3", "filter", "filter2", "find", "findstr", "finite", "fix", "flag", "fliplr", "flipud", "floor", "flops", "fmin", "fmins", "fopen", "for", "format", "fplot", "fprintf", "fread", "frewind", "fscanf", "fseek", "ftell", "full", "function", "funm", "fwrite", "fzero", "gallery", "gamma", "gammainc", "gammaln", "gca", "gcd", "gcf", "gco", "get", "getenv", "getframe", "ginput", "global", "gplot", "gradient", "gray", "graymon", "grid", "griddata", "gt", "gtext", "hadamard", "hankel", "help", "hess", "hex2dec", "hex2num", "hidden", "hilb", "hist", "hold", "home", "hostid", "hot", "hsv", "hsv2rgb", "if", "ifft", "ifft2", "imag", "image", "imagesc", "info", "input", "int2str", "interp1", "interp2", "interpft", "inv", "invhilb", "isempty", "isglobal", "ishold", "isieee", "isinf", "isletter", "isnan", "isreal", "isspace", "issparse", "isstr", "jet", "keyboard", "kron", "lasterr", "lcm", "legend", "legendre", "length", "lin2mu", "line", "linspace", "load", "log", "log10", "log2", "loglog", "logm", "logspace", "lookfor", "lower", "ls", "lscov", "lt", "lu", "magic", "matClose", "matDeleteMatrix", "matGetDir", "matGetFp", "matGetFull", "matGetMatrix", "matGetNextMatrix", "matGetString", "matOpen", "matPutFull", "matPutMatrix", "matPutString", "matlabrc", "matlabroot", "max", "mean", "median", "menu", "mesh", "meshc", "meshgrid", "meshz", "mexAtExit", "mexCallMATLAB", "mexErrMsgTxt", "mexEvalString", "mexFunction", "mexGetFull", "mexGetMatrix", "mexGetMatrixPtr", "mexPrintf", "mexPutFull", "mexPutMatrix", "mexSetTrapFlag", "mexdebug", "min", "more", "movie", "moviein", "mu2lin", "mxCalloc", "mxCopyCharacterToPtr", "mxCopyComplex16ToPtr", "mxCopyInteger4ToPtr", "mxCopyPtrToCharacter", "mxCopyPtrToComplex16", "mxCopyPtrToInteger4", "mxCopyPtrToReal8", "mxCopyReal8ToPtr", "mxCreateFull", "mxCreateSparse", "mxCreateString", "mxFree", "mxFreeMatrix", "mxGetIr", "mxGetJc", "mxGetM", "mxGetN", "mxGetName", "mxGetNzmax", "mxGetPi", "mxGetPr", "mxGetScalar", "mxGetString", "mxIsComplex", "mxIsFull", "mxIsNumeric", "mxIsSparse", "mxIsString", "mxIsTypeDouble", "mxSetIr", "mxSetJc", "mxSetM", "mxSetN", "mxSetName", "mxSetNzmax", "mxSetPi", "mxSetPr", "nargchk", "nargin", "nargout", "newplot", "nextpow2", "nnls", "nnz", "nonzeros", "norm", "normest", "null", "num2str", "nzmax", "ode23", "ode45", "orient", "orth", "pack", "pascal", "patch", "path", "pause", "pcolor", "pi", "pink", "pinv", "plot", "plot3", "pol2cart", "polar", "poly", "polyder", "polyeig", "polyfit", "polyval", "polyvalm", "pow2", "print", "printopt", "prism", "prod", "pwd", "qr", "qrdelete", "qrinsert", "quad", "quad8", "quit", "quiver", "qz", "rand", "randn", "randperm", "rank", "rat", "rats", "rbbox", "rcond", "real", "realmax", "realmin", "refresh", "rem", "reset", "reshape", "residue", "return", "rgb2hsv", "rgbplot", "rootobject", "roots", "rose", "rosser", "rot90", "rotate", "round", "rref", "rrefmovie", "rsf2csf", "save", "saxis", "schur", "sec", "sech", "semilogx", "semilogy", "set", "setstr", "shading", "sign", "sin", "sinh", "size", "slice", "sort", "sound", "spalloc", "sparse", "spaugment", "spconvert", "spdiags", "specular", "speye", "spfun", "sph2cart", "sphere", "spinmap", "spline", "spones", "spparms", "sprandn", "sprandsym", "sprank", "sprintf", "spy", "sqrt", "sqrtm", "sscanf", "stairs", "startup", "std", "stem", "str2mat", "str2num", "strcmp", "strings", "strrep", "strtok", "subplot", "subscribe", "subspace", "sum", "surf", "surface", "surfc", "surfl", "surfnorm", "svd", "symbfact", "symmmd", "symrcm", "tan", "tanh", "tempdir", "tempname", "terminal", "text", "tic", "title", "toc", "toeplitz", "trace", "trapz", "tril", "triu", "type", "uicontrol", "uigetfile", "uimenu", "uiputfile", "unix", "unwrap", "upper", "vander", "ver", "version", "view", "viewmtx", "waitforbuttonpress", "waterfall", "wavread", "wavwrite", "what", "whatsnew", "which", "while", "white", "whitebg", "who", "whos", "wilkinson", "wk1read", "wk1write", "xlabel", "xor", "ylabel", "zeros", "zlabel", "zoom",
];

/// One of a language's delimiters (lstmisc.sty's comments and strings aspects), in the order
/// its keys define them: `\lst@DefDelims` defines each on its first character over the
/// meaning the character had (`\lst@CDef`), so the last defined is tried first and passes
/// the character on to the earlier ones when it does not fire.
#[derive(Clone, Copy, PartialEq)]
enum Delim {
    /// `morecomment=[l]d`: a comment to the line's end (`\lst@CommentDM@l`, `\lst@Lmodetrue`).
    Line(&'static str),
    /// `morecomment=[s]{b}{e}` (`\lst@CommentDM@s`).
    Block(&'static str, &'static str),
    /// `morestring=[t]d` for `t` one of `b` (`\lst@StringDM@b`: a backslash before `d` keeps
    /// the string open), `d` (`\lst@StringDM@d`), `m` (`\lst@StringDM@m`: Matlab's, which does
    /// not open after a letter or one of `)].0123456789}`).
    Str(u8, &'static str),
    /// `morestring=[s]{b}{e}` (`\lst@StringDM@s`).
    StrS(&'static str, &'static str),
    /// C's `moredelim=*[directive]\#` (lstmisc's directives aspect, `\lst@Delim@directive`):
    /// at the line's first column (`\lst@CalcColumn` zero) out of any mode, `\lst@CDmode` to
    /// the line's end, the delimiter in `directivestyle` (`keywordstyle` when not set). The
    /// mode is not `\lst@ifmode` (the key's star): other delimiters open in it, keywords are
    /// looked up, and `moredirectives` are keywords in it.
    Directive(&'static str),
}

/// A language as lstlang1.sty defines it: keywords (one class, or classes sharing
/// `keywordstyle`, all `sensitive`) and delimiters.
#[derive(PartialEq)]
struct Lang {
    keywords: &'static [&'static str],
    delims: &'static [Delim],
    /// `moredirectives`, sorted.
    directives: &'static [&'static str],
}

const C_DELIMS: &[Delim] = &[Delim::Block("/*", "*/"), Delim::Line("//"), Delim::Str(b'b', "\""), Delim::Str(b'b', "'"), Delim::Directive("#")];
const SH_DELIMS: &[Delim] = &[Delim::Line("#"), Delim::Str(b'd', "\""), Delim::Str(b'd', "'")];
const PYTHON: Lang = Lang { directives: &[], keywords: PYTHON_KEYWORDS, delims: &[Delim::Line("#"), Delim::StrS("'''", "'''"), Delim::StrS("\"\"\"", "\"\"\""), Delim::Str(b'b', "'"), Delim::Str(b'b', "\"")] };
/// lstlang1.sty's `[ANSI]C` `moredirectives`. Sorted.
const C_DIRECTIVES: &[&str] = &["define", "elif", "else", "endif", "error", "if", "ifdef", "ifndef", "include", "line", "pragma", "undef", "warning"];
const C: Lang = Lang { keywords: C_KEYWORDS, delims: C_DELIMS, directives: C_DIRECTIVES };
const CPP: Lang = Lang { keywords: CPP_KEYWORDS, delims: C_DELIMS, directives: C_DIRECTIVES };
const JAVA: Lang = Lang { directives: &[], keywords: JAVA_KEYWORDS, delims: &[Delim::Line("//"), Delim::Block("/*", "*/"), Delim::Str(b'b', "\""), Delim::Str(b'b', "'")] };
const SH: Lang = Lang { keywords: SH_KEYWORDS, delims: SH_DELIMS, directives: &[] };
const BASH: Lang = Lang { keywords: BASH_KEYWORDS, delims: SH_DELIMS, directives: &[] };
const MATLAB: Lang = Lang { directives: &[], keywords: MATLAB_KEYWORDS, delims: &[Delim::Line("%"), Delim::Str(b'm', "'")] };

/// `language=name` (listings compares the name without case).
fn language(name: &str) -> Option<&'static Lang> {
    Some(match name.to_ascii_lowercase().as_str() {
        "python" => &PYTHON,
        "c" => &C,
        "c++" => &CPP,
        "java" => &JAVA,
        "sh" => &SH,
        "bash" => &BASH,
        "matlab" => &MATLAB,
        _ => return None,
    })
}

/// The source's `key=value` items at the top level, values without one pair of braces.
fn items(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    let b = s.as_bytes();
    for (k, &c) in b.iter().enumerate().chain(std::iter::once((b.len(), &b','))) {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                let item = s[start..k].trim();
                start = k + 1;
                if item.is_empty() {
                    continue;
                }
                let (key, val) = item.split_once('=').map_or((item, ""), |(a, v)| (a.trim(), v.trim()));
                let val = val.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(val).trim();
                out.push((key, val));
            }
            _ => {}
        }
    }
    out
}

/// A style value's commands: `\name` of [`STYLES`] or `\color{c}`, `c` of letters, digits,
/// `!` and `.`; `None` for anything else.
fn style_cmds(s: &str) -> Option<Vec<(&str, &str)>> {
    let mut out = Vec::new();
    let mut r = s.trim();
    while !r.is_empty() {
        let rest = r.strip_prefix('\\')?;
        let n = rest.bytes().take_while(|c| c.is_ascii_alphabetic()).count();
        let name = &rest[..n];
        r = rest[n..].trim_start();
        if name == "color" {
            let a = r.strip_prefix('{')?;
            let (c, after) = a.split_once('}')?;
            if c.is_empty() || !c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'!' || b == b'.') {
                return None;
            }
            out.push((name, c));
            r = after.trim_start();
        } else if STYLES.contains(&name) {
            out.push((name, ""));
        } else {
            return None;
        }
    }
    Some(out)
}

/// A plain length (`3pt`, `.4pt`, `0pt`) in sp.
fn dim(s: &str) -> Option<i32> {
    let v = s.trim();
    if !v.ends_with("pt") || v.contains("em") || v.contains("ex") {
        return None;
    }
    crate::backend::backend_scaled::scan_dimen(v, 0, 0).map(|d| d as i32)
}

/// `aboveskip`, `belowskip`: a plain length, or the kernel's `\smallskipamount`,
/// `\medskipamount`, `\bigskipamount` (latex.ltx: 3pt, 6pt, 12pt, each plus and minus a third).
fn skip(s: &str) -> Option<Glue> {
    let third = |w: i32| Glue { width: w, stretch: w / 3, shrink: w / 3, ..Glue::default() };
    Some(match s.trim() {
        "\\smallskipamount" => third(3 * 65536),
        "\\medskipamount" => third(6 * 65536),
        "\\bigskipamount" => third(12 * 65536),
        v => Glue { width: dim(v)?, ..Glue::default() },
    })
}

/// The pre-scan's check of a key list (`\lstset{..}`, `\begin{lstlisting}[..]`): every key of
/// [`KEYS`] with a value the job reads.
pub(super) fn keys_ok(s: &str) -> bool {
    items(s).into_iter().all(|(key, val)| {
        KEYS.contains(&key)
            && match key {
                "basicstyle" => style_cmds(val).is_some_and(|c| c.iter().any(|(n, _)| *n == "ttfamily") && c.iter().all(|(n, _)| *n != "color")),
                "keywordstyle" | "commentstyle" | "stringstyle" => style_cmds(val).is_some(),
                "backgroundcolor" | "rulecolor" => style_cmds(val).is_some_and(|c| c.len() == 1 && c[0].0 == "color"),
                "columns" => matches!(val, "fixed" | "[c]fixed" | "fullflexible" | "[c]fullflexible"),
                "frame" => matches!(val, "single" | "none"),
                "framerule" | "framesep" | "rulesep" => dim(val).is_some(),
                "breaklines" | "showstringspaces" => matches!(val, "" | "true" | "false"),
                "language" => language(val).is_some(),
                "numbers" => matches!(val, "none" | "left" | "right"),
                "numberstyle" => style_cmds(val).is_some(),
                "numbersep" => dim(val).is_some(),
                "stepnumber" => val == "1",
                "firstnumber" => val == "auto",
                "tabsize" => val.parse::<u8>().is_ok_and(|n| n > 0),
                // Words and punctuation, no `[`.
                "caption" => !val.is_empty() && val.bytes().all(|b| b.is_ascii_alphanumeric() || b" .,;:!?()-".contains(&b)),
                "style" => style_name(val).is_some(),
                // Their defaults (listings.sty:801-1612), or values a native listing cannot
                // show: its text is printable ASCII without tabs (`env_len`), so `showtabs` and
                // `extendedchars` change nothing.
                "showspaces" | "keepspaces" | "upquote" => val == "false",
                "showtabs" | "extendedchars" | "breakatwhitespace" => matches!(val, "" | "true" | "false"),
                "numberbychapter" => matches!(val, "" | "true"),
                "captionpos" => matches!(val, "t" | "b"),
                "aboveskip" | "belowskip" => skip(val).is_some(),
                _ => false,
            }
    })
}

/// A style's name as `\lst@NormedDef` keeps it (lower case, no spaces): letters, digits, `-`
/// and `_`.
pub(super) fn style_name(v: &str) -> Option<String> {
    let n: String = v.chars().filter(|c| *c != ' ').map(|c| c.to_ascii_lowercase()).collect();
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')).then_some(n)
}

/// The pre-scan's check of `\lstdefinestyle{name}{keys}` at the start of `rest`: the bytes
/// taken, or `None`.
pub(super) fn define_style_len(rest: &str) -> Option<usize> {
    let a = rest.strip_prefix('{')?;
    let n = a.find('}')?;
    style_name(&a[..n])?;
    let b = a[n + 1..].strip_prefix('{')?;
    let k = super::balanced(b)?;
    keys_ok(&b[..k]).then_some(1 + n + 2 + k + 1)
}

/// The pre-scan's check of `\lstinputlisting[keys]{file}` at the start of `rest`: keys of
/// [`KEYS`] and a file name of letters, digits and `/ . _ -`. The bytes taken, or `None`.
pub(super) fn input_len(rest: &str) -> Option<usize> {
    let t = rest.trim_start_matches(' ');
    let mut at = rest.len() - t.len();
    if let Some(r) = t.strip_prefix('[') {
        let k = bracket_len(r)?;
        if !keys_ok(&r[..k]) {
            return None;
        }
        at += k + 2;
    }
    let a = rest[at..].strip_prefix('{')?;
    let n = a.find('}')?;
    let ok = |b: u8| b.is_ascii_alphanumeric() || b"/._-".contains(&b);
    (n > 0 && a.as_bytes()[..n].iter().all(|&b| ok(b))).then_some(at + n + 2)
}

/// The pre-scan's check of `\begin{lstlisting}` and what follows in `rest` (the source after
/// `\begin{lstlisting}`): its options and a text of printable ASCII (no tab). The bytes up
/// to and with `\end{lstlisting}`, or `None`.
pub(super) fn env_len(rest: &str) -> Option<usize> {
    let end = rest.find("\\end{lstlisting}")?;
    let text = &rest[..end];
    let (first, body) = text.split_once('\n')?;
    let t = first.trim_start();
    if let Some(r) = t.strip_prefix('[') {
        let k = bracket_len(r)?;
        if !keys_ok(&r[..k]) || !r[k + 1..].trim().is_empty() {
            return None;
        }
    } else if !t.trim().is_empty() {
        return None;
    }
    let bad = |c: char| !(' '..='~').contains(&c) && c != '\n' && c != '\t' || matches!(c, '^' | '~');
    (!body.contains(bad)).then_some(end + "\\end{lstlisting}".len())
}

/// `\lstinline[opts]<d>text<d>` (or `{text}`) at the start of `line`, the source after
/// `\lstinline`: its options, its text, and the bytes taken. The text is one line of
/// letters, digits, single inner spaces and punctuation that no scan of the source reads as
/// markup (no `\ { } % $ & # ^ ~ _`).
pub(super) fn inline_args(line: &str) -> Option<(&str, &str, usize)> {
    let t = line.trim_start_matches(' ');
    let mut at = line.len() - t.len();
    let mut opts = "";
    if let Some(r) = t.strip_prefix('[') {
        let k = bracket_len(r)?;
        opts = &r[..k];
        if !keys_ok(opts) {
            return None;
        }
        at += k + 2;
    }
    let rest = &line[at..];
    let d = rest.chars().next()?;
    let close = match d {
        '{' => '}',
        '!' | '|' | '+' | '/' | '=' | '@' | ':' | ';' | '?' | '*' | '-' | '.' | ',' | '"' | '\'' => d,
        _ => return None,
    };
    let k = rest[1..].find(close)?;
    let text = &rest[1..1 + k];
    let ok = |c: char| c.is_ascii_alphanumeric() || " !\"'()*+,-./:;<=>?@[]|`".contains(c);
    if text.is_empty() || !text.chars().all(ok) || text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") {
        return None;
    }
    Some((opts, text, at + 2 + k))
}

/// The length of a bracketed text after its `[`, up to its `]` at brace depth 0.
fn bracket_len(r: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (k, c) in r.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ']' if depth == 0 => return Some(k),
            _ => {}
        }
    }
    None
}

/// listings' state in a native document: `\lstset`'s items, the listing being typeset, and
/// `\lst@neglisting` (the unnamed listings' counter, global).
#[derive(Default, Clone, PartialEq)]
pub(super) struct LstState {
    set: Vec<(String, String)>,
    run: Option<Box<Run>>,
    neg: i32,
    /// `\c@chapter` at the last captioned listing and `\c@lstlisting`.
    cap: (i32, i32),
    /// `\lst@breakcurrindent` (global): set by `breaklines`' `\lst@breakNewLine`, read by
    /// every `\lst@@discretionary`.
    bci: i32,
    /// `\lstdefinestyle`'s styles (`\lststy@name$`, global): the name, its items.
    styles: Vec<(String, Vec<(String, String)>)>,
    /// Inside `\lst@MakeCaption t`'s group, where `\vskip` is `\nobreak\@@vskip..\nobreak`
    /// (listings.sty:1660): an output routine fired there runs with it.
    cap_open: bool,
    /// The output routine fired there (`fire_up`): it runs with that `\vskip`.
    out_nobreak: bool,
}

/// The keys of one listing, as listings' macros hold them.
#[derive(Clone, PartialEq)]
struct Cfg {
    basic: String,
    keyword: String,
    /// `commentstyle` (`\itshape`: the `EmptyStyle` hook at the package's load), `stringstyle`.
    comment: String,
    string: String,
    /// `showstringspaces`: a string's spaces kept and visible.
    showspaces: bool,
    /// `tabsize` (8).
    tabsize: u32,
    /// `columns=fixed` (else `fullflexible`).
    fixed: bool,
    /// `frame=single` (else none).
    frame: bool,
    framerule: i32,
    framesep: i32,
    rulesep: i32,
    bkg: Option<String>,
    rulecolor: Option<String>,
    breaklines: bool,
    /// `language`.
    lang: Option<&'static Lang>,
    caption: Option<String>,
    /// `numbers`: 0 `none`, 1 `left`, 2 `right`; `numberstyle`, `numbersep`.
    numbers: u8,
    numstyle: String,
    numsep: i32,
    /// `aboveskip`, `belowskip` (`\medskipamount`).
    above: Glue,
    below: Glue,
}

impl Cfg {
    /// listings' defaults (the keys' initial values), then the items in order.
    fn new<'a>(list: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Cfg> {
        let mut c = Cfg { basic: String::new(), keyword: "\\bfseries".to_string(), comment: "\\itshape".to_string(), string: String::new(), showspaces: true, tabsize: 8, fixed: true, frame: false, framerule: 26214, framesep: 3 * 65536, rulesep: 2 * 65536, bkg: None, rulecolor: None, breaklines: false, lang: None, caption: None, numbers: 0, numstyle: String::new(), numsep: 10 * 65536, above: MEDSKIP, below: MEDSKIP };
        let (mut bottom, mut at_space) = (false, false);
        let color = |v: &str| style_cmds(v).and_then(|c| c.first().map(|x| x.1.to_string()));
        for (k, v) in list {
            match k {
                "basicstyle" => c.basic = v.to_string(),
                "keywordstyle" => c.keyword = v.to_string(),
                "commentstyle" => c.comment = v.to_string(),
                "stringstyle" => c.string = v.to_string(),
                "showstringspaces" => c.showspaces = v != "false",
                "tabsize" => c.tabsize = v.parse().ok().filter(|n| *n > 0)?,
                "stepnumber" | "firstnumber" => {}
                "numbers" => c.numbers = ["none", "left", "right"].iter().position(|n| *n == v)? as u8,
                "numberstyle" => c.numstyle = v.to_string(),
                "numbersep" => c.numsep = dim(v)?,
                "columns" => c.fixed = v.ends_with("]fixed") || v == "fixed",
                "frame" => c.frame = v == "single",
                "framerule" => c.framerule = dim(v)?,
                "framesep" => c.framesep = dim(v)?,
                "rulesep" => c.rulesep = dim(v)?,
                "backgroundcolor" => c.bkg = Some(color(v)?),
                "rulecolor" => c.rulecolor = Some(color(v)?),
                "breaklines" => c.breaklines = v != "false",
                "language" => c.lang = Some(language(v)?),
                "caption" => c.caption = Some(v.to_string()),
                "showspaces" | "showtabs" | "extendedchars" | "keepspaces" | "upquote" | "numberbychapter" => {}
                "captionpos" => bottom = v == "b",
                "breakatwhitespace" => at_space = v != "false",
                "aboveskip" => c.above = skip(v)?,
                "belowskip" => c.below = skip(v)?,
                _ => return None,
            }
        }
        // `captionpos=b` matters only with a caption (`\lst@MakeCaption b`), and
        // `breakatwhitespace` only with `breaklines`: not ported.
        if bottom && c.caption.is_some() || at_space && c.breaklines {
            return None;
        }
        Some(c)
    }
}

/// One character of a token's box: a character, `\lst@nolig`'s `\leavevmode\kern\z@`, or
/// `\lst@outputspace` (`\ `).
#[derive(Clone, Copy, PartialEq)]
enum Elem {
    Ch(u8),
    NoLig,
    Space,
}

/// A token's `\lst@thestyle`: none (`\lst@identifierstyle` or `\relax`, both empty here),
/// `keywordstyle`, or the delimiter mode's `\lst@currstyle` (`commentstyle`, `stringstyle`).
const ST_NONE: u8 = 0;
const ST_KEYWORD: u8 = 1;
const ST_COMMENT: u8 = 2;
const ST_STRING: u8 = 3;

/// What `\lst@OutputToken` puts on a line.
#[derive(Clone, Copy, PartialEq)]
enum Item {
    /// `\lst@Kern` of the lost space, in `\lst@width`s, in `\lst@currstyle` (`ST_NONE` out of
    /// a delimiter's mode, else its style).
    Kern(u32, u8),
    /// The `\lst@@discretionary` of gobbled white space.
    Disc,
    /// `\lst@GotoTabStop` after something on the line: a box of `\lst@outputspace`'s width
    /// holding `{\lst@currstyle{\hss}}` (its style), output as a token of the tab's columns;
    /// the white space lost before it (in `\lst@width`s) is in its lost space.
    Tab(u32, u8, u32),
    /// A token's box: its elements `e0..e1`, `\lst@length`, its style (`ST_*`).
    Tok { e0: u32, e1: u32, len: u32, style: u8 },
}

/// One `\lst@NewLine`'s paragraph: the lost space at its `\lst@breakNewLine` (in
/// `\lst@width`s) and its items.
#[derive(Clone, Copy, PartialEq)]
struct Para {
    lead: u32,
    i0: u32,
    i1: u32,
}

/// A listing's lines as listings' character loop leaves them.
#[derive(Clone, PartialEq, Default)]
struct Layout {
    paras: Vec<Para>,
    items: Vec<Item>,
    elems: Vec<Elem>,
    /// The `\lst@NewLine`s of `\lst@DeInit`'s `\setbox\@tempboxa\vbox{\lst@DoNewLines}`.
    trailing: u32,
}

/// `\lst@lastother`: the character the last `\lst@AppendOther` appended, `\relax` after
/// `\lst@Output`, empty after `\lst@PrintToken` of other characters, at a line's start and
/// after a delimiter's mode (the `EndGroup` hook).
#[derive(Clone, Copy, PartialEq)]
enum Last {
    Empty,
    Relax,
    Ch(u8),
}

/// A delimiter as `\lst@CDef` defines it on its first character: its text, its kind (`B`
/// `\lst@DefDelimB`, `E` `\lst@DefDelimE`, `b` `d` `m` `\lst@DefDelimBE` of a string type,
/// `#` a directive), its mode (the dynamic mode, one per delimiter), the mode's style and
/// whether it ends at the line's end (`\lst@Lmodetrue`).
#[derive(Clone, Copy)]
struct Entry {
    text: &'static [u8],
    kind: u8,
    mode: u8,
    style: u8,
    line: bool,
}

/// A language's delimiters as `\lst@DefDelims` defines them, in order.
fn entries(lang: &Lang) -> Vec<Entry> {
    let mut out = Vec::with_capacity(lang.delims.len() * 2);
    for (k, d) in lang.delims.iter().enumerate() {
        let mode = k as u8 + 1;
        let e = |text: &'static str, kind: u8, style: u8, line: bool| Entry { text: text.as_bytes(), kind, mode, style, line };
        match *d {
            Delim::Line(t) => out.push(e(t, b'B', ST_COMMENT, true)),
            Delim::Block(b, t) => out.extend([e(b, b'B', ST_COMMENT, false), e(t, b'E', ST_COMMENT, false)]),
            Delim::Str(kind, t) => out.push(e(t, kind, ST_STRING, false)),
            Delim::StrS(b, t) => out.extend([e(b, b'B', ST_STRING, false), e(t, b'E', ST_STRING, false)]),
            Delim::Directive(t) => out.push(e(t, b'#', ST_NONE, false)),
        }
    }
    out
}

/// listings' character loop over `lines` (`\lst@SelectCharTable`'s classes):
/// `\lst@ProcessLetter`, `\lst@ProcessDigit`, `\lst@ProcessOther` (with `breaklines`, `)` is
/// `\lst@breakProcessOther`: printed at once), `\lst@ProcessSpace`, the line end's
/// `\lst@MProcessListing`, and the language's delimiters (comments and strings, each a mode
/// of its own: `\lst@DelimOpen`, `\lst@DelimClose`); tokens leave through `\lst@Output`
/// (letters, keywords looked up out of a mode) and `\lst@OutputOther`. `None` for a
/// character the native job does not take.
fn layout(lines: &[&str], cfg: &Cfg, inline: bool, flexible: bool) -> Option<Layout> {
    struct St {
        out: Layout,
        tok: Vec<Elem>,
        len: u32,
        word: String,
        letter: bool,
        white: bool,
        gobbled: bool,
        newline: bool,
        lost: u32,
        newlines: u32,
        fixed: bool,
        inline: bool,
        breaklines: bool,
        keywords: Option<&'static [&'static str]>,
        directives: &'static [&'static str],
        /// `\lst@CDmode` (a directive's line), and the directive's delimiter being printed
        /// (in `directivestyle`).
        cd: bool,
        cd_delim: bool,
        /// The line's columns so far (`\lst@column` less `\lst@pos`), `tabsize`, and a tab's
        /// lost space not yet given to a `\lst@Kern` (a `Kern` item is due at the next token).
        col: u32,
        tabsize: u32,
        tabbed: bool,
        last: Last,
        /// `\lst@mode` (0: no delimiter's), its style, `\lst@ifLmode`, and
        /// `\lst@ifkeepspaces` with `\lst@visiblespace` (a string under `showstringspaces`).
        mode: u8,
        style: u8,
        lmode: bool,
        keep: bool,
    }
    impl St {
        fn append(&mut self, e: &[Elem], c: char) {
            self.tok.extend_from_slice(e);
            self.len += 1;
            self.word.push(c);
        }
        fn output_token(&mut self, letter: bool) {
            if self.len == 0 {
                return;
            }
            // `\lst@TrackNewLines`: `\lst@breakNewLine`, then `\lst@DoNewLines`. `\lstinline`'s
            // `\lst@NewLine` is empty: `\lst@newlines` stays 1, `\lst@ifnewline` false, one line.
            if self.inline {
                if self.out.paras.is_empty() {
                    self.out.paras.push(Para { lead: 0, i0: 0, i1: 0 });
                }
            } else if self.newlines > 0 {
                let i = self.out.items.len() as u32;
                for _ in 0..self.newlines {
                    self.out.paras.push(Para { lead: self.lost, i0: i, i1: i });
                }
                self.newlines = 0;
                self.newline = true;
            }
            // `\lst@OutputLostSpace`: always under `fixed` and `flexible`, at a line's start under
            // `fullflexible`.
            if (self.lost > 0 || self.tabbed) && (self.fixed || self.newline) {
                self.out.items.push(Item::Kern(self.lost, if self.mode > 0 { self.style } else { ST_NONE }));
                self.lost = 0;
            }
            self.tabbed = false;
            if self.gobbled {
                self.gobbled = false;
                self.out.items.push(Item::Disc);
            }
            let style = if self.mode > 0 {
                self.style
            } else if self.cd_delim || letter && (self.keywords.is_some_and(|k| k.binary_search(&self.word.as_str()).is_ok()) || self.cd && self.directives.binary_search(&self.word.as_str()).is_ok()) {
                ST_KEYWORD
            } else {
                ST_NONE
            };
            let e0 = self.out.elems.len() as u32;
            self.out.elems.append(&mut self.tok);
            self.out.items.push(Item::Tok { e0, e1: self.out.elems.len() as u32, len: self.len, style });
            if let Some(p) = self.out.paras.last_mut() {
                p.i1 = self.out.items.len() as u32;
            }
            self.newline = false;
            self.len = 0;
            self.word.clear();
        }
        /// `\lst@PrintToken`.
        fn print(&mut self) {
            let l = self.letter;
            self.letter = false;
            self.output_token(l);
            self.last = if l { Last::Relax } else { Last::Empty };
        }
        /// `\lst@AppendOther`: a letter token before it leaves through `\lst@Output`.
        fn other(&mut self, e: &[Elem], c: char) {
            if self.letter {
                self.output_token(true);
                self.letter = false;
            }
            self.append(e, c);
            self.last = Last::Ch(c as u8);
        }
        /// `\lst@ProcessTabulator`: `\lst@XPrintToken \lst@whitespacetrue`, the columns to the
        /// next tab stop, then `\lst@PreGotoTabStop`: kept spaces as one token, at a line's
        /// start lost space, else `\lst@GotoTabStop`'s box.
        fn tab(&mut self) -> Option<()> {
            self.print();
            self.white = true;
            let n = self.tabsize - self.col % self.tabsize;
            self.col += n;
            if self.keep {
                for _ in 0..n {
                    self.other(&[Elem::Ch(32)], ' ');
                }
                self.output_token(false);
            } else if self.newlines > 0 {
                self.lost += n;
            } else {
                self.out.items.push(Item::Tab(n, if self.mode > 0 { self.style } else { ST_NONE }, std::mem::take(&mut self.lost)));
                if let Some(p) = self.out.paras.last_mut() {
                    p.i1 = self.out.items.len() as u32;
                }
                self.tabbed = true;
            }
            Some(())
        }
        /// `\lst@LeaveMode`: the mode's group ends, then the `EndGroup` hook
        /// (`\lst@ResetToken \let\lst@lastother\@empty`).
        fn leave(&mut self) {
            (self.mode, self.style, self.lmode, self.keep, self.letter, self.last) = (0, ST_NONE, false, false, false, Last::Empty);
        }
        /// A character's own meaning (`\lst@SelectStdCharTable`).
        fn char(&mut self, c: char) -> Option<()> {
            if c == '\t' {
                return self.tab();
            }
            self.col += 1;
            match c {
                'a'..='z' | 'A'..='Z' | '@' | '$' | '_' => {
                    self.white = false;
                    if !self.letter {
                        self.output_token(false);
                        self.letter = true;
                    }
                    self.append(&[Elem::Ch(c as u8)], c);
                }
                '0'..='9' => {
                    self.white = false;
                    if self.letter {
                        self.append(&[Elem::Ch(c as u8)], c);
                    } else {
                        self.other(&[Elem::Ch(c as u8)], c);
                    }
                }
                // `\lst@ifkeepspaces`: the space as a token of its own, `\verbvisiblespace`
                // (pdfTeX's `\asciispace`: `\char32`).
                ' ' if self.keep => {
                    self.print();
                    self.white = true;
                    self.other(&[Elem::Ch(32)], ' ');
                    self.print();
                }
                ' ' => {
                    if self.newlines > 0 && self.len == 0 {
                        self.lost += 1;
                        self.white = true;
                    } else if self.white {
                        // `\lst@AppendSpecialSpace` after white space: gobbled.
                        self.print();
                        self.lost += 1;
                        self.gobbled = true;
                    } else {
                        self.print();
                        self.white = true;
                        self.other(&[Elem::Space], ' ');
                        self.print();
                    }
                }
                // `\verbatim@nolig@list`: `\lst@NoLig` first, into the token being collected (no
                // `\lst@length`), then the character.
                '`' | '<' | '>' | ',' | '\'' | '-' => {
                    self.white = false;
                    self.tok.push(Elem::NoLig);
                    self.other(&[Elem::Ch(c as u8)], c);
                }
                '!'..='~' => {
                    self.white = false;
                    self.other(&[Elem::Ch(c as u8)], c);
                    if c == ')' && self.breaklines {
                        self.letter = false;
                        self.output_token(false);
                    }
                }
                _ => return None,
            }
            Some(())
        }
    }
    let ents = cfg.lang.map(entries).unwrap_or_default();
    // `\lst@ifbstring`: `\\` is two other characters, then `\let\lst@lastother\relax`.
    let bstring = ents.iter().any(|e| e.kind == b'b');
    let mut s = St { out: Layout::default(), tok: Vec::new(), len: 0, word: String::new(), letter: false, white: true, gobbled: false, newline: false, lost: 0, newlines: 1, fixed: cfg.fixed || flexible, inline, breaklines: cfg.breaklines, keywords: cfg.lang.map(|l| l.keywords), directives: cfg.lang.map_or(&[], |l| l.directives), cd: false, cd_delim: false, col: 0, tabsize: cfg.tabsize, tabbed: false, last: Last::Empty, mode: 0, style: ST_NONE, lmode: false, keep: false };
    for (k, line) in lines.iter().enumerate() {
        if k > 0 {
            // `\lst@MProcessListing`: `\lst@XPrintToken \lst@EOLUpdate` (the `EOL` hook leaves
            // the modes that end with the line) `\lsthk@InitVarsBOL`.
            s.print();
            if s.lmode || s.cd {
                s.leave();
                s.cd = false;
            }
            s.newlines += 1;
            s.lost = 0;
            s.white = true;
            s.last = Last::Empty;
            s.col = 0;
            s.tabbed = false;
        }
        let b = line.as_bytes();
        let mut i = 0;
        'chars: while i < b.len() {
            let c = b[i];
            if c == b'\\' && bstring && b.get(i + 1) == Some(&b'\\') {
                s.char('\\')?;
                s.char('\\')?;
                s.last = Last::Relax;
                i += 2;
                continue;
            }
            if !ents.is_empty() {
                // `\lst@StringDM@b`'s `\lst@ifletter \lst@Output \lst@letterfalse \fi`, before
                // its test.
                if s.letter && ents.iter().any(|e| e.kind == b'b' && e.text[0] == c) {
                    s.output_token(true);
                    s.letter = false;
                    s.last = Last::Relax;
                }
                for e in ents.iter().rev() {
                    if e.text[0] != c || !b[i..].starts_with(e.text) {
                        continue;
                    }
                    let open = match e.kind {
                        // `\lst@CalcColumn`: nothing before it on the line.
                        b'#' if s.mode == 0 && !s.cd && i == 0 => {
                            s.print();
                            s.cd = true;
                            s.cd_delim = true;
                            let last = s.last;
                            for &d in e.text {
                                s.char(d as char)?;
                            }
                            s.print();
                            (s.cd_delim, s.last) = (false, last);
                            i += e.text.len();
                            continue 'chars;
                        }
                        b'#' => continue,
                        // `\lst@ifmode`: no delimiter opens in another's mode.
                        b'B' if s.mode == 0 => true,
                        b'E' if s.mode == e.mode => false,
                        b'B' | b'E' => continue,
                        kind if s.mode == e.mode => {
                            if kind == b'b' && s.last == Last::Ch(b'\\') {
                                continue;
                            }
                            false
                        }
                        kind if s.mode == 0 => {
                            if kind == b'm' && (s.letter || matches!(s.last, Last::Ch(b')' | b']' | b'.' | b'0'..=b'9' | b'}'))) {
                                continue;
                            }
                            true
                        }
                        _ => continue,
                    };
                    s.print();
                    if open {
                        (s.mode, s.style, s.lmode, s.keep) = (e.mode, e.style, e.line, e.style == ST_STRING && cfg.showspaces);
                    }
                    // `\lst@DelimPrint`: the delimiter as a token of its own, in a group (the
                    // characters' own meanings: the earlier delimiters do not fire in it).
                    let last = s.last;
                    for &d in e.text {
                        s.char(d as char)?;
                    }
                    s.print();
                    s.last = last;
                    if !open {
                        s.leave();
                    }
                    i += e.text.len();
                    continue 'chars;
                }
            }
            s.char(c as char)?;
            i += 1;
        }
    }
    // `\lst@DeInit`: `\lst@XPrintToken \lst@EOLUpdate`, `\global\advance\lst@newlines\m@ne`.
    s.print();
    s.out.trailing = s.newlines.saturating_sub(1);
    Some(s.out)
}

/// A listing being typeset: its keys, lines and the boxes its steps share.
#[derive(Clone, PartialEq)]
struct Run {
    cfg: Cfg,
    lay: Layout,
    /// The next paragraph of `lay`.
    next: usize,
    /// `\lst@width`, `\linewidth`, `\@totalleftmargin`.
    width: i32,
    linewidth: i32,
    left: i32,
    /// `\lst@breakcurrindent`, and `\lst@breakshape`'s width.
    bci: i32,
    bshape: i32,
    /// `\lst@framebox`, and the top or bottom rule's box (`\lst@frameH`'s `\z@`).
    framebox: Option<u32>,
    rule_box: Option<u32>,
    /// The colours' literals: `\lst@bkgcolor` (also fill and rule separation), `\lst@rulecolor`.
    bkg: Option<String>,
    rulecolor: Option<String>,
    /// `\c@lstnumber`, and the name part of `\theHlstnumber` before `.\thelstnumber`.
    number: i32,
    href: String,
    /// `\lst@lostspace` (global in listings, reset at each line's start).
    lost: i32,
    /// `\lstinline`: no lines, `\lst@breakNewLine` before every token; its columns
    /// `flexible` (`\lst@column@flexible`, `[c]`: the lost space inserted around a token).
    inline: bool,
    flexible: bool,
    /// The styles' fonts and colours at their first token (`ST_*`).
    styled: [Option<(u16, Vec<String>)>; 4],
    /// `\lstinputlisting`: its end is `\egroup \lst@doendpe \@newlistfalse \ignorespaces`
    /// (no `\@nobreakfalse`).
    input: bool,
}

pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@lstset", |m| m.lst_set()),
    ("rt@lstdefinestyle", |m| m.lst_define_style()),
    ("rt@lstinit", |m| m.lst_init()),
    ("rt@lstbox", |m| m.lst_rule_box()),
    ("rt@lstcapmid", |m| m.lst_caption_mid()),
    ("rt@lsttop", |m| m.lst_top()),
    ("rt@lstline", |m| m.lst_line()),
    ("rt@lstbot", |m| m.lst_bottom()),
    ("rt@lstclose", |m| m.lst_close()),
    ("rt@lstinline", |m| m.lst_inline()),
    ("rt@lstinputlisting", |m| m.lst_input()),
    ("rt@lstinputend", |m| m.lst_end()),
];

/// `\smallskipamount`: `\abovecaptionskip` and `\belowcaptionskip` of a listing.
pub(super) const SMALLSKIP: Glue = Glue { width: 3 * 65536, stretch: 65536, shrink: 65536, stretch_order: 0, shrink_order: 0 };

/// `0pt plus 1fil minus 1fil`: `\hss`.
const HSS: Glue = Glue { width: 0, stretch: 65536, stretch_order: 1, shrink: 65536, shrink_order: 1 };

impl Machine<'_, '_> {
    /// A page about to be broken inside a listing's caption group: pdfTeX's output routine
    /// then runs with listings' `\vskip`, `\penalty10000` on each side of every `\vskip`
    /// of `\@outputpage` and `\@makecol` (probe lc/c45); the native one does not.
    pub(crate) fn lst_caption_open(&mut self) -> bool {
        self.lst().is_some_and(|s| s.cap_open)
    }

    /// `fire_up` inside a listing's caption group: the output routine to come runs with
    /// listings' `\vskip`.
    pub(crate) fn lst_set_out_nobreak(&mut self) {
        if let Some(s) = self.lst() {
            s.out_nobreak = true;
        }
    }

    /// The output routine runs with listings' `\vskip` (`\nobreak\@@vskip..\nobreak`).
    pub(crate) fn lst_out_nobreak(&mut self) -> bool {
        self.lst().is_some_and(|s| s.out_nobreak)
    }

    /// The page is shipped: `\vskip` is the primitive again in the next output routine.
    pub(crate) fn lst_out_done(&mut self) {
        if let Some(s) = self.lst() {
            s.out_nobreak = false;
        }
    }

    /// A `\vskip` of the output routine onto `cur`: with listings' `\vskip` in force,
    /// `\penalty10000` on each side.
    pub(crate) fn out_vskip(&mut self, g: Glue) {
        let wrap = self.lst_out_nobreak();
        if wrap {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
        let n = self.e.nodes.glue_node(g, 0);
        self.cur.push(n);
        if wrap {
            self.cur.push(Node::new(Kind::Penalty, 10000));
        }
    }

    fn lst(&mut self) -> Option<&mut LstState> {
        self.doc.as_mut().map(|d| &mut d.pkg.lst)
    }

    fn lst_run(&mut self) -> Option<&mut Run> {
        self.lst().and_then(|s| s.run.as_deref_mut())
    }

    /// The listing ends: its `breaklines` leave `\lst@breakcurrindent` (global) as its last
    /// `\lst@breakNewLine` set it. Its side box, to free.
    fn lst_take_run(&mut self) -> Option<u32> {
        let s = self.lst()?;
        let r = s.run.take()?;
        if r.cfg.breaklines {
            s.bci = r.bci;
        }
        r.framebox
    }

    /// `\lstset{keys}` in the preamble: its items, applied at each listing.
    pub(super) fn lst_set(&mut self) {
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("\\lstset in the body");
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut text = String::new();
        self.show(&t, &mut text);
        let Some(list) = self.lst_keys(&text) else { return self.not_native("an undefined listing style") };
        if let Some(s) = self.lst() {
            s.set.extend(list);
        }
    }

    /// `\lstdefinestyle{name}{keys}`: `\lststy@name$` defined (globally) as `\lstset{keys}`.
    pub(super) fn lst_define_style(&mut self) {
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut name = String::new();
        self.show(&t, &mut name);
        t.clear();
        self.read_arg(&mut t);
        let mut text = String::new();
        self.show(&t, &mut text);
        let Some(name) = style_name(&name) else { return self.not_native("a listing style's name") };
        let list = items(&text).into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        if let Some(s) = self.lst() {
            match s.styles.iter_mut().find(|(n, _)| *n == name) {
                Some(st) => st.1 = list,
                None => s.styles.push((name, list)),
            }
        }
    }

    /// A key list's items, `style=name` replaced by the style's (`\lst@LAS`: its `\lstset`
    /// then and there); `None` for a style not defined.
    fn lst_keys(&mut self, text: &str) -> Option<Vec<(String, String)>> {
        fn expand(styles: &[(String, Vec<(String, String)>)], list: Vec<(String, String)>, depth: u8, out: &mut Vec<(String, String)>) -> Option<()> {
            for (k, v) in list {
                if k == "style" {
                    let name = style_name(&v)?;
                    let st = styles.iter().find(|(n, _)| *n == name)?;
                    if depth > 8 {
                        return None;
                    }
                    expand(styles, st.1.clone(), depth + 1, out)?;
                } else {
                    out.push((k, v));
                }
            }
            Some(())
        }
        let list: Vec<(String, String)> = items(text).into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let styles = &self.doc.as_ref()?.pkg.lst.styles;
        let mut out = Vec::with_capacity(list.len());
        expand(styles, list, 0, &mut out)?;
        Some(out)
    }

    /// `\begin{lstlisting}[keys]` after `\begin`'s group: the text to `\end{lstlisting}` read
    /// as it is in the file, laid out; then `\lst@Init`'s `\par` (LaTeX's, as it stands).
    pub(super) fn lst_begin(&mut self) {
        if self.mode == Mode::Math || self.inner {
            return self.not_native("a listing in a box or a formula");
        }
        if self.doc.as_ref().is_some_and(|d| d.inlabel || d.floats.minipage || !d.pbox.is_empty() || !d.lists.is_empty()) {
            return self.not_native("a listing in a list, minipage or box");
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::super::engine_names::END_LINE_CHAR });
        let Some(text) = self.input.raw_until("\\end{lstlisting}", elc) else {
            return self.not_native("a listing without its end in the same file");
        };
        let lines: Vec<&str> = text.split('\n').collect();
        // `\lstenv@Process`: the rest of the `\begin` line after the options is dropped.
        let first = lines[0].trim_start();
        let opts = match first.strip_prefix('[') {
            Some(r) => match bracket_len(r) {
                Some(k) => r[..k].to_string(),
                None => return self.not_native("listing options over lines"),
            },
            None => String::new(),
        };
        if lines.len() < 2 || !lines[lines.len() - 1].trim().is_empty() {
            return self.not_native("listing text on the \\begin or \\end line");
        }
        self.lst_start(&opts, &lines[1..lines.len() - 1], false);
    }

    /// `\lstinputlisting[keys]{file}` (`\lst@inputlisting`): `\bgroup`, the file read (a
    /// read the session logs, as every file the job opens) and laid out as a listing's text
    /// (`\lst@InputListing`: `\lst@Init`, `\input`, `\lst@DeInit`), `\egroup \lst@doendpe
    /// \@newlistfalse \ignorespaces`.
    pub(super) fn lst_input(&mut self) {
        if self.mode == Mode::Math || self.inner {
            return self.not_native("a listing in a box or a formula");
        }
        if self.doc.as_ref().is_some_and(|d| d.inlabel || d.floats.minipage || !d.pbox.is_empty() || !d.lists.is_empty()) {
            return self.not_native("a listing in a list, minipage or box");
        }
        let mut opts = String::new();
        if self.next_is('[') {
            let t = self.bracket_arg();
            self.show(&t, &mut opts);
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        let mut name = String::new();
        self.show(&t, &mut name);
        let Some(bytes) = self.open_bytes(name.trim()) else { return self.not_native("a listing's file not found") };
        // pdfTeX reads a byte as one character; a carriage return is not native.
        if bytes.iter().any(|&b| !(b' '..=b'~').contains(&b) && b != b'\n' && b != b'\t') {
            return self.not_native("a listing file's character");
        }
        let text = String::from_utf8(bytes).unwrap_or_default();
        let mut lines: Vec<&str> = text.split('\n').collect();
        if text.ends_with('\n') {
            lines.pop();
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(doc) = self.doc.as_mut() {
            doc.endpe = false;
        }
        self.lst_start(&opts, &lines, true);
    }

    /// A display listing of `lines` (the text's, or the file's) under `opts`: its keys, its
    /// layout, then `\lst@Init`'s `\par`; the end after it (`\end{lstlisting}`, or
    /// `\lst@InputListing`'s `\lst@DeInit`).
    fn lst_start(&mut self, opts: &str, lines: &[&str], input: bool) {
        let set = self.doc.as_ref().map(|d| d.pkg.lst.set.clone()).unwrap_or_default();
        let Some(opts) = self.lst_keys(opts) else { return self.not_native("an undefined listing style") };
        let list = set.iter().chain(&opts).map(|(k, v)| (k.as_str(), v.as_str()));
        let Some(cfg) = Cfg::new(list) else { return self.not_native("listing keys") };
        // TeX's input lines lose their final spaces (§31).
        let body: Vec<&str> = lines.iter().map(|l| l.trim_end_matches(' ')).collect();
        // `$` is `\textdollar`: the font's own character only under T1.
        if !self.t1() && body.iter().any(|l| l.contains('$')) {
            return self.not_native("a listing's $ outside T1");
        }
        let Some(lay) = layout(&body, &cfg, false, false).filter(|l| !l.paras.is_empty()) else { return self.not_native("a listing character, or an empty listing") };
        let run = Run { cfg, lay, next: 0, width: 0, linewidth: 0, left: 0, bci: 0, bshape: 0, framebox: None, rule_box: None, bkg: None, rulecolor: None, number: 0, href: String::new(), lost: 0, inline: false, flexible: false, styled: Default::default(), input };
        if let Some(s) = self.lst() {
            s.run = Some(Box::new(run));
        }
        let mut t = vec![self.named_cs(named!("par")), self.doc_id(doc!("rt@lstinit"))];
        if input {
            t.push(self.doc_id(doc!("rt@lstinputend")));
        } else {
            // `\lstenv@End`: `\end{lstlisting}`, read with the text.
            t.extend([self.named_cs(named!("end")), Token::char(Cat::BeginGroup, '{')]);
            t.extend("lstlisting".chars().map(|c| Token::char(Cat::Letter, c)));
            t.push(Token::char(Cat::EndGroup, '}'));
        }
        self.back_list(&t);
    }

    /// `\lstinline[opts]|text|`: `\leavevmode\bgroup`, `\lstset{flexiblecolumns,opts}`, the
    /// `TextStyle` hooks (no frame, background, caption or numbers), `\lst@Init` out of
    /// display style (no skips, no caption), the text's tokens as a listing line's (see
    /// [`Run::inline`]), `\lst@DeInit`'s `\normalbaselines\normalcolor`, `\egroup`.
    pub(super) fn lst_inline(&mut self) {
        if self.mode == Mode::Math || self.inner {
            return self.not_native("\\lstinline in a box or a formula");
        }
        if self.lst().is_none_or(|s| s.run.is_some()) {
            return self.not_native("\\lstinline here");
        }
        if self.leave_vmode() {
            return;
        }
        let elc = self.e.eqtb.word(Reg { region: Region::Int, slot: super::super::engine_names::END_LINE_CHAR });
        let Some(line) = self.input.raw_peek() else { return self.not_native("\\lstinline in an argument") };
        let set = self.doc.as_ref().map(|d| d.pkg.lst.set.clone()).unwrap_or_default();
        let Some((opts, text, used)) = inline_args(&line) else { return self.not_native("\\lstinline with this text") };
        let (opts, text) = (opts.to_string(), text.to_string());
        let used = line[..used].to_string();
        if self.input.raw_until(&used, elc).is_none() {
            return self.not_native("\\lstinline in an argument");
        }
        let Some(opts) = self.lst_keys(&opts) else { return self.not_native("an undefined listing style") };
        let list: Vec<(&str, &str)> = set.iter().chain(&opts).map(|(k, v)| (k.as_str(), v.as_str())).collect();
        // `flexiblecolumns`: `\lst@columnsflexible`, which only `columns=fullflexible` sets.
        let flexible = !list.iter().any(|(k, v)| *k == "columns" && v.ends_with("fullflexible"));
        let Some(mut cfg) = Cfg::new(list.into_iter()) else { return self.not_native("listing keys") };
        (cfg.fixed, cfg.frame, cfg.bkg, cfg.caption, cfg.numbers) = (false, false, None, None, 0);
        if !self.t1() && text.contains('$') {
            return self.not_native("a listing's $ outside T1");
        }
        let Some(lay) = layout(&[text.as_str()], &cfg, true, flexible) else { return self.not_native("a listing character") };
        let run = Run { cfg, lay, next: 0, width: 0, linewidth: 0, left: 0, bci: 0, bshape: 0, framebox: None, rule_box: None, bkg: None, rulecolor: None, number: 0, href: String::new(), lost: 0, inline: true, flexible, styled: Default::default(), input: false };
        if let Some(s) = self.lst() {
            s.run = Some(Box::new(run));
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.lst_normal_baselines();
        let Some((basic, rc, breaklines)) = self.lst_run().map(|r| (r.cfg.basic.clone(), r.cfg.rulecolor.clone(), r.cfg.breaklines)) else { return };
        if self.lst_style(&basic).is_none() {
            return self.not_native("listing basicstyle");
        }
        let rulecolor = match rc {
            Some(c) => match self.color_literal(None, &c, false) {
                Some(l) => Some(l),
                None => return self.not_native("a listing colour the native job does not know"),
            },
            None => None,
        };
        let (linewidth, left, width) = (self.rw(super::R_LINEWIDTH), self.rw(super::R_TOTALLEFTMARGIN), self.dimen_of("0.45em"));
        if let Some(r) = self.lst_run() {
            (r.rulecolor, r.linewidth, r.left, r.width) = (rulecolor, linewidth, left, width);
        }
        // `\lst@frameInit`'s side box, read by `\lst@@discretionary`'s `\lsthk@EveryLine`.
        if breaklines {
            let (ht, dp) = (self.strut_height(), self.strut_depth());
            let fb = self.lst_frame_box_v(ht, dp);
            if let Some(r) = self.lst_run() {
                r.framebox = Some(fb);
            }
        }
        if let Some(p) = self.lst_run().and_then(|r| r.lay.paras.first().copied()) {
            self.lst_items(p);
        }
        self.lst_normal_baselines();
        let colored = self.colored();
        if colored {
            self.lst_normal_color();
        }
        self.end_group(SEMI_SIMPLE_GROUP);
        if colored {
            self.color_stack(None);
        }
        if let Some(b) = self.lst_take_run() {
            self.e.nodes.flush_node(Node::new(Kind::Box, b as i32));
        }
        self.aux = 1000;
    }

    /// `\penalty-50\relax \vspace\lst@aboveskip` (`\medskipamount`), then the rest of
    /// `\lst@Init`.
    fn lst_init(&mut self) {
        let above = self.lst_run().map_or(MEDSKIP, |r| r.cfg.above);
        self.run_ops(vec![Op::Penalty(-50), Op::Vskip(above), Op::Vskip(Glue::default()), Op::Call(|m| m.lst_init_caption())]);
    }

    /// `\normalbaselines`, then `\lst@MakeCaption t`'s `\allowbreak`.
    fn lst_init_caption(&mut self) {
        self.lst_normal_baselines();
        self.run_ops(vec![Op::Penalty(0), Op::Call(|m| m.lst_init_hooks())]);
    }

    /// `\normalbaselines`: `\lineskip\normallineskip \baselineskip\normalbaselineskip
    /// \lineskiplimit\normallineskiplimit` (1pt and 0pt, the kernel's).
    fn lst_normal_baselines(&mut self) {
        let nb = self.e.eqtb.glue(super::NORMAL_BASELINE_SKIP);
        self.set_glue_named(param!("lineskip"), Glue { width: 65536, ..Glue::default() });
        self.set_glue_named(param!("baselineskip"), nb);
        self.set_dimen_named(param!("lineskiplimit"), 0);
    }

    /// `\lst@MakeCaption t` without a caption: `\lst@HRefStepCounter` (with hyperref the
    /// anchor `lstlisting.\theHlstlisting` in vertical mode); then the `PreInit` and `Init`
    /// hooks and `\lst@frameInit`.
    fn lst_init_hooks(&mut self) {
        if self.lst_run().is_some_and(|r| r.cfg.caption.is_some()) {
            return self.lst_caption();
        }
        let neg = self.lst().map_or(0, |s| {
            s.neg -= 1;
            s.neg
        });
        if let Some(r) = self.lst_run() {
            r.href = neg.to_string();
        }
        if self.hyper() {
            // `\hyper@refstepcounter{lstlisting}`: `\theHlstlisting`, by `\@addtoreset
            // {lstlisting}{chapter}` (hyperref's) `\theHchapter.\the\value{lstlisting}`.
            let within = self.doc.as_ref().filter(|d| d.report || d.book).map(|d| super::hyperref::the_h_top(d.counters[0], d.appendix));
            let name = match within {
                Some(w) => format!("lstlisting.{w}.{neg}"),
                None => format!("lstlisting.{neg}"),
            };
            self.set_current_href(&name);
            self.vertical_anchor(name);
        }
        // `\lstinputlisting`'s `\lst@intname`, the file's name: `\addcontentsline{lol}
        // {lstlisting}\lst@name`.
        if self.lst_run().is_some_and(|r| r.input) {
            self.aux_write(0);
        }
        self.lst_init_rest();
    }

    /// `\lst@MakeCaption t` with a caption: `\refstepcounter{lstlisting}`, the
    /// `\addcontentsline{lol}` write, then in a group `\@parboxrestore\normalsize\normalfont`
    /// and the class's `\@makecaption` (article's, report's, book's) with `\vskip X` as
    /// `\nobreak\vskip X\nobreak` and `\abovecaptionskip` `\smallskipamount`.
    fn lst_caption(&mut self) {
        if self.hyper() || self.memoir() || self.ams_on() || self.babel_french() || self.doc.as_ref().is_none_or(|d| d.koma || d.appendix && (d.report || d.book)) {
            return self.not_native("a listing caption under this class or package");
        }
        // `\newcounter{lstlisting}[chapter]` with a chapter, `\thelstlisting`
        // `\ifnum\c@chapter>\z@ \thechapter.\fi \@arabic\c@lstlisting`.
        let chapter = self.doc.as_ref().filter(|d| d.report || d.book).map_or(0, |d| d.counters[0]);
        let n = self.lst().map_or(0, |s| {
            if s.cap.0 != chapter {
                s.cap = (chapter, 0);
            }
            s.cap.1 += 1;
            s.cap.1
        });
        let the = if chapter > 0 { format!("{chapter}.{n}") } else { n.to_string() };
        self.set_current_label(&the);
        self.cref_step("lstlisting");
        if let Some(r) = self.lst_run() {
            r.href = the;
        }
        self.aux_write(0);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if let Some(s) = self.lst() {
            s.cap_open = true;
        }
        self.par_box_restore();
        self.size_command(super::NORMALSIZE);
        self.normal_font();
        // caption's `\lst@MakeCaption` (caption.sty:1102): `\caption@setposition{t}` swaps the
        // two skips (`\abovecaptionskip\@tempdima`, a dimension: the stretch goes), `rule=0`,
        // then `\lst@makecaption` is `\caption@makecaption`: `\caption@make@above` (the old
        // `\abovecaptionskip` as `\nobreak\vskip..\nobreak`), caption3's box
        // (`caption_lst_measure`), `\caption@make@below` (`caption_parbox_end`).
        let then: super::Native = if self.caption_pkg() { |m| m.caption_lst_measure() } else { |m| m.lst_caption_box() };
        self.run_ops(vec![Op::Penalty(10000), Op::Vskip(SMALLSKIP), Op::Penalty(10000), Op::Call(then)]);
    }

    /// `\sbox\@tempboxa{#1: #2}`, `#1` `\fnum@lstlisting` (`{\lstlistingname~\thelstlisting}`).
    fn lst_caption_box(&mut self) {
        let Some((text, the)) = self.lst_run().map(|r| (r.cfg.caption.clone().unwrap_or_default(), r.href.clone())) else { return };
        self.open_box(HBOX_GROUP, Ctx::Set { n: super::TEMP_BOX, global: false }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            let c = self.current_color();
            self.color_stack(Some(&c));
        }
        let mut t = vec![Token::char(Cat::BeginGroup, '{')];
        t.extend("Listing".chars().map(|c| Token::char(Cat::Letter, c)));
        t.push(self.named_cs(named!("nobreakspace")));
        t.extend(the.chars().map(|c| Token::char(Cat::Other, c)));
        t.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::Other, ':'), Token::char(Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
        t.extend(self.tex_tokens(&text));
        t.push(self.doc_id(doc!("rt@lstcapmid")));
        self.back_list(&t);
    }

    /// After the `\sbox`: `\global\@minipagefalse \hb@xt@\hsize{\hfil\box\@tempboxa\hfil}`
    /// (a caption wider than `\hsize`, set as a paragraph, is not native),
    /// `\vskip\belowcaptionskip`, the group's end; then the rest of `\lst@Init`.
    fn lst_caption_mid(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(super::TEMP_BOX).unwrap_or(0);
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        if self.e.nodes.boxes[b as usize].width > hsize {
            return self.not_native("a listing caption wider than the line");
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.floats.minipage = false;
        }
        self.e.eqtb.replace_box(super::TEMP_BOX, None);
        let fil = Glue { stretch: 65536, stretch_order: 1, ..Glue::default() };
        let start = self.cur.len();
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        self.cur.push(Node::new(Kind::Box, b as i32));
        let g = self.e.nodes.glue_node(fil, 0);
        self.cur.push(g);
        let h = self.hpack(start, hsize, true);
        let h = self.e.nodes.add_box(h);
        self.append_to_vlist(h);
        self.run_ops(vec![
            Op::Penalty(10000),
            Op::Vskip(SMALLSKIP),
            Op::Penalty(10000),
            Op::Call(|m| {
                m.end_group(SEMI_SIMPLE_GROUP);
                m.lst_init_rest();
            }),
        ]);
    }

    /// The listing's caption text and `\thelstlisting` (`caption_lst_measure`'s).
    pub(super) fn lst_caption_parts(&mut self) -> Option<(String, String)> {
        self.lst_run().map(|r| (r.cfg.caption.clone().unwrap_or_default(), r.href.clone()))
    }

    /// The `PreInit` and `Init` hooks, `\lst@frameInit`.
    pub(super) fn lst_init_rest(&mut self) {
        if let Some(s) = self.lst() {
            s.cap_open = false;
        }
        // `PreInit`: `\rightskip\z@ \leftskip\z@ \parfillskip=\z@ plus 1fil`, `\linewidth`.
        self.set_glue_named(param!("rightskip"), Glue::default());
        self.set_glue_named(param!("leftskip"), Glue::default());
        self.set_glue_named(param!("parfillskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
        let Some(cfg) = self.lst_run().map(|r| r.cfg.clone()) else { return };
        // `Init`: `\lst@basicstyle`.
        if self.lst_style(&cfg.basic).is_none() {
            return self.not_native("listing basicstyle");
        }
        // lineshape's: `\parskip\z@`; `breaklines`.
        self.set_glue_named(param!("parskip"), Glue::default());
        if cfg.breaklines {
            self.set_int_named(param!("hbadness"), 10000);
            self.set_int_named(param!("pretolerance"), 10000);
            self.set_glue_named(param!("rightskip"), Glue { stretch: 65536, stretch_order: 1, ..Glue::default() });
            self.set_glue_named(param!("leftskip"), Glue::default());
            self.set_dimen_named(param!("parindent"), 0);
        }
        let colors = [cfg.bkg.as_deref(), cfg.rulecolor.as_deref()].map(|c| c.map(|c| self.color_literal(None, c, false)));
        if colors.iter().any(|c| matches!(c, Some(None))) {
            return self.not_native("a listing colour the native job does not know");
        }
        let [bkg, rulecolor] = colors.map(|c| c.flatten());
        let linewidth = self.rw(super::R_LINEWIDTH);
        let left = self.rw(super::R_TOTALLEFTMARGIN);
        // `InitVars`: `\lsthk@FontAdjust`, `\lst@width` of the style's font (`basewidth`'s
        // `0.6em` fixed, `0.45em` flexible).
        let width = self.dimen_of(if cfg.fixed { "0.6em" } else { "0.45em" });
        if let Some(r) = self.lst_run() {
            (r.bkg, r.rulecolor, r.linewidth, r.left, r.width) = (bkg, rulecolor, linewidth, left, width);
            // `\lst@breakshape`'s value at the start: `\@totalleftmargin \linewidth`, as the
            // last listing left it (only its first line is used before the first line).
            (r.bci, r.bshape) = (left, linewidth);
        }
        // `\lst@frameInit`: the side box (also with `frame=none`), then the top rule as its
        // own paragraph.
        let (ht, dp) = (self.strut_height(), self.strut_depth());
        let fb = self.lst_frame_box_v(ht, dp);
        if !cfg.frame {
            if let Some(r) = self.lst_run() {
                r.framebox = Some(fb);
            }
            return self.lst_top();
        }
        let top = self.lst_frame_h(true);
        if let Some(r) = self.lst_run() {
            r.framebox = Some(fb);
            r.rule_box = Some(top);
        }
        self.run_ops(vec![Op::Par, Op::Call(move |m| m.lst_top_rule())]);
    }

    /// `\par\lst@parshape`, `\@tempdima` (`-\baselineskip` plus the rule's height, plus
    /// `\prevdepth` from 256pt on), `\vskip\@tempdima\vskip\lineskip` when it is negative,
    /// then `\noindent\box\z@\par`.
    fn lst_top_rule(&mut self) {
        self.lst_parshape();
        let Some(b) = self.lst_run().and_then(|r| r.rule_box) else { return };
        let mut d = self.e.nodes.boxes[b as usize].height - self.glue_param(super::BASELINE_SKIP).width;
        if self.aux >= 256 * 65536 {
            d += self.aux;
        }
        if d < 0 {
            self.vskip(Glue { width: d, ..Glue::default() });
            let ls = self.glue_param(super::super::engine_pack::LINE_SKIP);
            self.vskip(ls);
        }
        let t = [self.named_cs(named!("noindent")), self.doc_id(doc!("rt@lstbox")), self.doc_id(super::PRIM_PAR), self.doc_id(doc!("rt@lsttop"))];
        self.back_list(&t);
    }

    /// `\lst@parshape`: `\parshape\@ne \@totalleftmargin \linewidth`, with `breaklines`
    /// `\parshape\tw@ \@totalleftmargin\linewidth \lst@breakshape`.
    fn lst_parshape(&mut self) {
        let Some(r) = self.lst_run() else { return };
        let mut shape = vec![[r.left, r.linewidth]];
        if r.cfg.breaklines {
            shape.push([r.bci, r.bshape]);
        }
        self.e.eqtb.set_par_shape(shape, false);
    }

    /// `\box\z@` of `\lst@frameInit` or `\lst@frameExit`.
    fn lst_rule_box(&mut self) {
        if let Some(b) = self.lst_run().and_then(|r| r.rule_box.take()) {
            self.cur.push(Node::new(Kind::Box, b as i32));
        }
    }

    /// After the top rule: `\lineskiplimit\maxdimen \lineskip\z@`; the text's `\everypar`;
    /// then the first line.
    fn lst_top(&mut self) {
        if self.lst_run().is_some_and(|r| r.cfg.frame) {
            self.set_dimen_named(param!("lineskiplimit"), 0x3fff_ffff);
            self.set_glue_named(param!("lineskip"), Glue::default());
        }
        self.set_everypar(super::LST_LINE);
        self.lst_next_line();
    }

    /// The next `\lst@NewLine` (`\par\noindent\hbox{}` and its items), its
    /// `\lst@breakNewLine` first; after the last, `\lst@DeInit`.
    fn lst_next_line(&mut self) {
        let Some(r) = self.lst_run() else { return };
        let Some(&p) = r.lay.paras.get(r.next) else { return };
        // `\lst@breakNewLine` (with `breaklines`): `\lst@breakindent` (20pt) plus the lost space.
        if r.cfg.breaklines {
            let lead = p.lead as i32 * r.width + 20 * 65536;
            (r.bci, r.bshape) = (lead + r.left, r.linewidth - lead);
        }
        let t = [self.doc_id(super::PRIM_PAR), self.named_cs(named!("noindent")), self.doc_id(doc!("rt@lstline"))];
        self.back_list(&t);
    }

    /// The text's `\everypar`: `\lst@parshape`, `\lst@framelr` (`\copy\lst@framebox`), and
    /// labels' `\refstepcounter{lstnumber}` (with hyperref its raised anchor,
    /// `lstnumber.\theHlstnumber`).
    pub(super) fn lst_everypar(&mut self) {
        self.lst_parshape();
        if let Some(b) = self.lst_run().and_then(|r| r.framebox) {
            let c = self.e.nodes.copy_box(b);
            self.cur.push(Node::new(Kind::Box, c as i32));
        }
        let Some(r) = self.lst_run() else { return };
        r.number += 1;
        let name = format!("lstnumber.{}.{}", r.href, r.number);
        let n = r.number;
        if self.hyper() {
            self.set_current_href(&name);
            self.lst_raised_anchor(name);
        }
        self.lst_place_number(n);
    }

    /// `\lst@SkipOrPrintLabel` with `stepnumber=1`: every line's `\lst@PlaceNumber`,
    /// `\llap{\normalfont\lst@numberstyle{\thelstnumber}\kern\lst@numbersep}` (`left`) or
    /// `\rlap{\normalfont\kern\linewidth\kern\lst@numbersep\lst@numberstyle{\thelstnumber}}`
    /// (`right`); a `\color` of the style pushes in the box and pops after it.
    fn lst_place_number(&mut self, n: i32) {
        let Some((side, style, sep, lw)) = self.lst_run().map(|r| (r.cfg.numbers, r.cfg.numstyle.clone(), r.cfg.numsep, r.linewidth)) else { return };
        if side == 0 {
            return;
        }
        self.begin_group(SEMI_SIMPLE_GROUP);
        let mut pushes = None;
        self.build_hbox_to(0, |m| {
            if side == 1 {
                m.lst_hss();
            }
            m.normal_font();
            if side == 2 {
                m.lst_kern(lw);
                m.lst_kern(sep);
            }
            pushes = m.lst_style(&style);
            m.build_text(n.to_string().as_bytes());
            if side == 1 {
                m.lst_kern(sep);
            } else {
                m.lst_hss();
            }
        });
        self.end_group(SEMI_SIMPLE_GROUP);
        match pushes {
            Some(k) => (0..k).for_each(|_| self.color_stack(None)),
            None => self.not_native("listing numberstyle"),
        }
    }

    /// `\Hy@raisedlink{\hyper@anchorstart{name}\hyper@anchorend}` in a paragraph:
    /// `\penalty\@M`, `\smash{\raise\baselineskip\hbox{..}}`, `\kern0pt`.
    fn lst_raised_anchor(&mut self, name: String) {
        let bs = self.glue_param(super::BASELINE_SKIP).width;
        let sf = self.aux;
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let start = self.cur.len();
        self.append_dest(name);
        self.cur.push(Node::new(Kind::Penalty, 10000));
        let inner = self.hpack(start, 0, false);
        let inner = self.e.nodes.add_box(BoxRec { shift: -bs, ..inner });
        self.cur.push(Node::new(Kind::Box, inner as i32));
        let outer = self.hpack(start, 0, false);
        let outer = self.e.nodes.add_box(BoxRec { height: 0, depth: 0, ..outer });
        self.cur.extend([Node::new(Kind::Box, outer as i32), Node { sub: 1, ..Node::new(Kind::Kern, 0) }]);
        self.aux = sf;
    }

    /// One line after `\noindent` and its `\everypar`: `\hbox{}`, then the line's items.
    fn lst_line(&mut self) {
        let Some(r) = self.lst_run() else { return };
        let p = r.lay.paras[r.next];
        r.next += 1;
        r.lost = 0;
        let start = self.cur.len();
        let b = self.hpack(start, 0, false);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        self.lst_items(p);
        self.lst_next_line();
    }

    /// A line's items: `\lst@OutputLostSpace`'s kern (the white space before a token, in
    /// `\lst@width`s, added to `\lst@lostspace`, a box when that is positive), gobbled
    /// white space's discretionary, tokens.
    fn lst_items(&mut self, p: Para) {
        let mut kerned = false;
        for i in p.i0..p.i1 {
            let Some(item) = self.lst_run().map(|r| r.lay.items[i as usize]) else { return };
            match item {
                Item::Kern(n, style) => {
                    let Some(r) = self.lst_run() else { return };
                    r.lost += r.width * n as i32;
                    // `\lstinline`'s `\lst@breakNewLine`, before `\lst@OutputLostSpace`.
                    if r.inline && r.cfg.breaklines {
                        r.bci = 20 * 65536 + r.lost + r.left;
                    }
                    kerned = true;
                    let w = std::mem::replace(&mut r.lost, 0);
                    if w > 0 {
                        self.lst_kern_box(w, style);
                    } else if let Some(r) = self.lst_run() {
                        r.lost = w;
                    }
                }
                Item::Disc => self.lst_disc(),
                Item::Tab(n, style, before) => self.lst_tab(n, style, before),
                Item::Tok { e0, e1, len, style } => {
                    self.lst_token(e0, e1, len, style, kerned);
                    kerned = false;
                }
            }
        }
    }

    /// `\lst@Kern`: `\hbox{{\lst@currstyle{\kern w}}}`.
    fn lst_kern_box(&mut self, w: i32, style: u8) {
        let s = self.cur.len();
        let Some(pushes) = self.lst_begin_style(style) else { return };
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
        self.lst_end_style(pushes);
        let b = self.hpack(s, 0, false);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
    }

    /// `\lst@OutputToken`'s box in `\lst@thestyle`'s group, then `\lsthk@PostOutput`'s
    /// `\lst@discretionary`.
    fn lst_token(&mut self, e0: u32, e1: u32, len: u32, style: u8, kerned: bool) {
        let Some((fixed, width, breaklines, inline, flexible)) = self.lst_run().map(|r| (r.cfg.fixed && !r.inline, r.width, r.cfg.breaklines, r.inline, r.flexible)) else { return };
        // `\lst@TrackNewLines` of `\lstinline` (its `\lst@NewLine` empty, so `\lst@newlines`
        // stays 1): `\lst@breakNewLine` before each token, from the lost space so far (set by
        // the token's `Item::Kern` when it has one).
        if inline && breaklines && !kerned {
            if let Some(r) = self.lst_run() {
                r.bci = 20 * 65536 + r.lost + r.left;
            }
        }
        let Some(pushes) = self.lst_begin_style(style) else { return };
        let f = self.cur_font();
        let start = self.cur.len();
        if fixed {
            self.cur.push(self.e.nodes.glue_node(HSS, 0));
        }
        for k in e0..e1 {
            if fixed && k > e0 {
                let g = self.e.nodes.glue_node(HSS, 0);
                self.cur.push(g);
            }
            let Some(e) = self.lst_run().map(|r| r.lay.elems[k as usize]) else { return };
            match e {
                Elem::Ch(c) => {
                    if self.e.fonts.list[f as usize].metrics.exists(c) {
                        self.cur.push(Node { kind: Kind::Char, sub: 0, font: f, a: c as i32, b: 0 });
                    }
                }
                Elem::NoLig => self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, 0) }),
                Elem::Space => self.build_control_space(),
            }
        }
        if fixed {
            let g = self.e.nodes.glue_node(HSS, 0);
            self.cur.push(g);
        }
        let b = if fixed { self.hpack(start, len as i32 * width, true) } else { self.hpack(start, 0, false) };
        let wd = b.width;
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        // `\lst@CalcLostSpaceAndOutput`: `\lst@lostspace` plus `\lst@length\lst@width` less
        // the box's width, then `\setbox\@tempboxa\hbox{\lst@leftinsert \box\@tempboxa
        // \lst@rightinsert}`; the inserts are not empty only under `flexible` (`[c]`:
        // `\lst@InsertHalfLostSpace`, `\lst@InsertLostSpace`) and when the space is positive.
        let half = self.lst_run().map_or(0, |r| {
            r.lost += len as i32 * width - wd;
            if flexible && r.lost > 0 {
                r.lost /= 2;
                std::mem::replace(&mut r.lost, 0)
            } else {
                0
            }
        });
        if half > 0 {
            // `\lst@Kern` in `\lst@currstyle`: the mode's style, `\relax` out of a mode.
            let cs = if style >= ST_COMMENT { style } else { ST_NONE };
            let tb = self.cur.pop().expect("the token's box");
            self.lst_kern_box(half, cs);
            self.cur.push(tb);
            self.lst_kern_box(half, cs);
        }
        let b = self.hpack(start, 0, false);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        if breaklines {
            self.lst_disc();
        }
        self.lst_end_style(pushes);
    }

    /// `\lst@GotoTabStop`: `\setbox\@tempboxa\hbox to\wd(\hbox{\lst@outputspace})
    /// {{\lst@currstyle{\hss}}}`, then `\lst@CalcLostSpaceAndOutput` with `\lst@length` the
    /// tab's columns.
    fn lst_tab(&mut self, n: u32, style: u8, before: u32) {
        // `[c]`'s inserts (`\lst@InsertHalfLostSpace`, `\lst@InsertLostSpace`), empty under
        // `fullflexible`.
        let Some((width, breaklines, inserts)) = self.lst_run().map(|r| (r.width, r.cfg.breaklines, r.cfg.fixed || r.flexible)) else { return };
        let s0 = self.cur.len();
        self.build_control_space();
        let sp = self.hpack(s0, 0, false);
        let sw = sp.width;
        let sp = self.e.nodes.add_box(sp);
        self.e.nodes.flush_node(Node::new(Kind::Box, sp as i32));
        let start = self.cur.len();
        let Some(pushes) = self.lst_begin_style(style) else { return };
        self.lst_hss();
        self.lst_end_style(pushes);
        let b = self.hpack(start, sw, true);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        let half = self.lst_run().map_or(0, |r| {
            r.lost += (n + before) as i32 * width - sw;
            if inserts && r.lost > 0 {
                r.lost /= 2;
                std::mem::replace(&mut r.lost, 0)
            } else {
                0
            }
        });
        if half > 0 {
            let tb = self.cur.pop().expect("the tab's box");
            self.lst_kern_box(half, style);
            self.cur.push(tb);
            self.lst_kern_box(half, style);
        }
        let b = self.hpack(start, 0, false);
        let b = self.e.nodes.add_box(b);
        self.cur.push(Node::new(Kind::Box, b as i32));
        if breaklines {
            self.lst_disc();
        }
    }

    /// `{\lst@thestyle ...`: a group and the style's commands (none for `ST_NONE` or an empty
    /// style); the colours pushed, or `None` when the job declines the style.
    fn lst_begin_style(&mut self, style: u8) -> Option<Option<u32>> {
        // A style's fonts and colours are the same at each token of a listing (from the same
        // `basicstyle` font): the first token's, kept.
        if let Some((f, colors)) = self.lst_run().and_then(|r| r.styled[style as usize].clone()) {
            self.begin_group(SEMI_SIMPLE_GROUP);
            self.select_font(f, false);
            for c in &colors {
                self.lst_set_color(c);
            }
            return Some(Some(colors.len() as u32));
        }
        let text = match (style, self.lst_run()) {
            (ST_NONE, _) | (_, None) => return Some(None),
            (ST_KEYWORD, Some(r)) if !r.cfg.keyword.is_empty() => r.cfg.keyword.clone(),
            (ST_COMMENT, Some(r)) if !r.cfg.comment.is_empty() => r.cfg.comment.clone(),
            (ST_STRING, Some(r)) if !r.cfg.string.is_empty() => r.cfg.string.clone(),
            _ => return Some(None),
        };
        self.begin_group(SEMI_SIMPLE_GROUP);
        match self.lst_style_colors(&text) {
            Some(colors) => {
                let (f, n) = (self.cur_font(), colors.len() as u32);
                if let Some(r) = self.lst_run() {
                    r.styled[style as usize] = Some((f, colors));
                }
                Some(Some(n))
            }
            None => {
                self.not_native("a listing style");
                None
            }
        }
    }

    /// The style's group ends, then its colours' pops (`\reset@color` after the group).
    fn lst_end_style(&mut self, pushes: Option<u32>) {
        if let Some(n) = pushes {
            self.end_group(SEMI_SIMPLE_GROUP);
            for _ in 0..n {
                self.color_stack(None);
            }
        }
    }

    /// `\lst@@discretionary`: `\discretionary{}{\llap{\lsthk@EveryLine \kern
    /// \lst@breakcurrindent \kern-\@totalleftmargin}}{}`.
    fn lst_disc(&mut self) {
        let Some(st) = self.lst() else { return };
        let global = st.bci;
        let Some((fb, bci, left)) = st.run.as_deref().map(|r| (r.framebox, if r.cfg.breaklines { r.bci } else { global }, r.left)) else { return };
        let start = self.cur.len();
        let g = self.e.nodes.glue_node(HSS, 0);
        self.cur.push(g);
        if let Some(b) = fb {
            let c = self.e.nodes.copy_box(b);
            self.cur.push(Node::new(Kind::Box, c as i32));
        }
        self.cur.extend([Node { sub: 1, ..Node::new(Kind::Kern, bci) }, Node { sub: 1, ..Node::new(Kind::Kern, -left) }]);
        let b = self.hpack(start, 0, true);
        let b = self.e.nodes.add_box(b);
        let post = self.e.nodes.store(&[Node::new(Kind::Box, b as i32)]);
        let d = self.e.nodes.add_disc([(0, 0), post]);
        self.cur.push(Node::new(Kind::Disc, d as i32));
    }

    /// A style's commands in order: the fonts by their natives, `\color{c}` as a push of the
    /// colour on the list (the caller pops it after the style's group). The pushes, or
    /// `None` for a colour the native job does not know.
    fn lst_style(&mut self, s: &str) -> Option<u32> {
        self.lst_style_colors(s).map(|c| c.len() as u32)
    }

    /// [`Machine::lst_style`], with the colours' literals it pushed.
    fn lst_style_colors(&mut self, s: &str) -> Option<Vec<String>> {
        let cmds = style_cmds(s)?;
        let mut pushes = Vec::new();
        for (name, arg) in cmds {
            match name {
                "ttfamily" => self.font_declaration(Change::Default(2), false),
                "bfseries" => self.font_declaration(Change::Series("bx"), false),
                "mdseries" => self.font_declaration(Change::Series("m"), false),
                "itshape" => self.font_declaration(Change::Shape("it"), false),
                "upshape" => self.font_declaration(Change::Shape("n"), false),
                "slshape" => self.font_declaration(Change::Shape("sl"), false),
                "color" => {
                    let lit = self.color_literal(None, arg, false)?;
                    self.lst_set_color(&lit);
                    pushes.push(lit);
                }
                size => {
                    let i = ["tiny", "scriptsize", "footnotesize", "small", "normalsize", "large"].iter().position(|n| *n == size)?;
                    self.size_command(i);
                }
            }
        }
        Some(pushes)
    }

    /// `\color`'s `\current@color` (local) and `\set@color` push.
    fn lst_set_color(&mut self, lit: &str) {
        let Some(doc) = self.doc.as_mut() else { return };
        let i = match doc.colors.iter().position(|c| c == lit) {
            Some(i) => i,
            None => {
                doc.colors.push(lit.to_string());
                doc.colors.len() - 1
            }
        };
        self.e.eqtb.set_word(super::CURRENT_COLOR, i as i32 + 1, false);
        self.color_stack(Some(lit));
    }

    /// The current list from `start` packed as an `\hbox` to `w` (or natural), appended.
    fn lst_pack(&mut self, start: usize, w: Option<i32>, shift: i32) -> u32 {
        let b = match w {
            Some(w) => self.hpack(start, w, true),
            None => self.hpack(start, 0, false),
        };
        let b = self.e.nodes.add_box(BoxRec { shift, ..b });
        self.cur.push(Node::new(Kind::Box, b as i32));
        b
    }

    fn lst_hss(&mut self) {
        let g = self.e.nodes.glue_node(HSS, 0);
        self.cur.push(g);
    }

    fn lst_kern(&mut self, w: i32) {
        self.cur.push(Node { sub: 1, ..Node::new(Kind::Kern, w) });
    }

    fn lst_rule(&mut self, w: i32, h: i32, d: i32) {
        let r = self.e.nodes.rule_node([w, h, d]);
        self.cur.push(r);
    }

    /// `\lst@frameBlock{color}{w}{h}{d}`: `\color@begingroup color \setbox\z@\hbox{\vrule
    /// \@height h \@depth d` and `\@width w`, or with no colour `\@width\z@ \kern w`, `}`
    /// `\box\z@ \color@endgroup`.
    fn lst_frame_block(&mut self, color: Option<&str>, w: i32, h: i32, d: i32) {
        if let Some(c) = color {
            self.color_stack(Some(c));
        }
        let s = self.cur.len();
        match color {
            Some(_) => self.lst_rule(w, h, d),
            None => {
                self.lst_rule(0, h, d);
                self.lst_kern(w);
            }
        }
        self.lst_pack(s, None, 0);
        if color.is_some() {
            self.color_stack(None);
        }
    }

    /// `\llap{..}` or `\rlap{..}` of the list from `start`: `\hb@xt@\z@{\hss ..}` or
    /// `\hb@xt@\z@{.. \hss}`.
    fn lst_lap(&mut self, start: usize, left: bool) {
        if left {
            let g = self.e.nodes.glue_node(HSS, 0);
            self.cur.insert(start, g);
        } else {
            self.lst_hss();
        }
        self.lst_pack(start, Some(0), 0);
    }

    /// `\lst@frameMakeBoxV\lst@framebox{\ht\strutbox}{\dp\strutbox}` for `frame=single`
    /// (`\lst@framelshape` and `\lst@framershape` `Y`, no x margins): the box, kept.
    fn lst_frame_box_v(&mut self, h: i32, d: i32) -> u32 {
        let Some((frw, fts, rs, w, bkg, rc, sides)) = self.lst_run().map(|r| (r.cfg.framerule, r.cfg.framesep, r.cfg.rulesep, r.linewidth, r.bkg.clone(), r.rulecolor.clone(), r.cfg.frame)) else { return 0 };
        let start = self.cur.len();
        if let Some(c) = &rc {
            self.color_stack(Some(c));
        }
        // `\llap{\lst@frameBlock\lst@fillcolor\lst@frametextsep{#2}{#3}\kern\lst@framexleftmargin}`,
        // when `\lst@framelshape` is not empty.
        if sides {
            let s = self.cur.len();
            self.lst_frame_block(bkg.as_deref(), fts, h, d);
            self.lst_kern(0);
            self.lst_lap(s, true);
        }
        // `\llap{\setbox\z@\hbox{\vrule\@width\z@\@height#2\@depth#3\lst@frameL}
        // \rlap{\lst@frameBlock\lst@rulesepcolor{\wd\z@}{\ht\z@}{\dp\z@}}\box\z@
        // \kern\lst@frametextsep\kern\lst@framexleftmargin}`, `\lst@frameL` `\kern-\lst@rulesep
        // \kern\lst@rulesep\lst@framevrule` (`\@empty` without a left side).
        let s = self.cur.len();
        let r0 = self.cur.len();
        self.lst_rule(0, h, d);
        if sides {
            self.lst_kern(-rs);
            self.lst_kern(rs);
            self.lst_rule(frw, RUNNING, RUNNING);
        }
        let b0 = self.hpack(r0, 0, false);
        let wd0 = b0.width;
        let b0 = self.e.nodes.add_box(b0);
        let s1 = self.cur.len();
        self.lst_frame_block(bkg.as_deref(), wd0, h, d);
        self.lst_lap(s1, false);
        self.cur.push(Node::new(Kind::Box, b0 as i32));
        self.lst_kern(fts);
        self.lst_kern(0);
        self.lst_lap(s, true);
        // `\rlap{\kern-\lst@framexleftmargin \lst@frameBlock\lst@bkgcolor{\linewidth}{#2}{#3}
        // \lst@frameBlock\lst@fillcolor\lst@frametextsep{#2}{#3} \setbox\z@\hbox{\vrule
        // \@width\z@\@height#2\@depth#3\lst@frameR} \rlap{\lst@frameBlock\lst@rulesepcolor
        // ..}\box\z@}`, `\lst@frameR` `\lst@framevrule\kern\lst@rulesep \kern-\lst@rulesep`.
        let s = self.cur.len();
        self.lst_kern(0);
        self.lst_frame_block(bkg.as_deref(), w, h, d);
        if sides {
            self.lst_frame_block(bkg.as_deref(), fts, h, d);
        } else {
            self.lst_kern(fts);
        }
        let r0 = self.cur.len();
        self.lst_rule(0, h, d);
        if sides {
            self.lst_rule(frw, RUNNING, RUNNING);
            self.lst_kern(rs);
            self.lst_kern(-rs);
        }
        let b0 = self.hpack(r0, 0, false);
        let wd0 = b0.width;
        let b0 = self.e.nodes.add_box(b0);
        let s1 = self.cur.len();
        self.lst_frame_block(bkg.as_deref(), wd0, h, d);
        self.lst_lap(s1, false);
        self.cur.push(Node::new(Kind::Box, b0 as i32));
        self.lst_lap(s, false);
        if rc.is_some() {
            self.color_stack(None);
        }
        let b = self.hpack(start, 0, false);
        self.e.nodes.add_box(b)
    }

    /// `\lst@frameH T\lst@frametshape` (or `B`, `\lst@framebshape`) for `frame=single`
    /// (shape `YYY`, square corners, no x margins): the box `\z@` it leaves.
    fn lst_frame_h(&mut self, top: bool) -> u32 {
        let Some((frw, fts, w, bkg, rc)) = self.lst_run().map(|r| (r.cfg.framerule, r.cfg.framesep, r.linewidth, r.bkg.clone(), r.rulecolor.clone())) else { return 0 };
        // `\lst@frameh 0 T Y Y Y`: `\lst@frameCalcDimA0` is `2(\lst@frametextsep+
        // \lst@framerulewidth)`; the corners `\lst@frameTL` and `\lst@frameTR` (`BL`, `BR`).
        let half = fts + frw;
        let start = self.cur.len();
        let inner = self.cur.len();
        self.lst_kern(0);
        if let Some(c) = &rc {
            self.color_stack(Some(c));
        }
        let corner = |m: &mut Self, right: bool| {
            let s = m.cur.len();
            match (top, right) {
                (true, false) => {
                    let r = m.cur.len();
                    m.lst_rule(frw, 0, half);
                    m.lst_pack(r, None, -frw);
                    m.lst_kern(-frw);
                    m.lst_rule(half, frw, 0);
                }
                (true, true) => {
                    m.lst_rule(half, frw, 0);
                    m.lst_kern(-frw);
                    let r = m.cur.len();
                    m.lst_rule(frw, 0, half);
                    m.lst_pack(r, None, -frw);
                }
                (false, false) => {
                    m.lst_rule(frw, half, 0);
                    m.lst_kern(-frw);
                    m.lst_rule(half, frw, 0);
                }
                (false, true) => {
                    m.lst_rule(half, frw, 0);
                    m.lst_kern(-frw);
                    m.lst_rule(frw, half, 0);
                }
            }
            m.lst_pack(s, None, 0);
            m.lst_lap(s, !right);
        };
        corner(self, false);
        self.lst_rule(w, frw, 0);
        corner(self, true);
        if rc.is_some() {
            self.color_stack(None);
        }
        let ib = self.hpack(inner, 0, false);
        // `\rlap{\raise\dp\z@\box\z@}` (`\lower\ht\z@`).
        let shift = if top { -ib.depth } else { ib.height };
        let r = self.cur.len();
        let ib = self.e.nodes.add_box(BoxRec { shift, ..ib });
        self.cur.push(Node::new(Kind::Box, ib as i32));
        self.lst_lap(r, false);
        // `\box\z@`: frameH's `\setbox\z@\hbox{}`.
        self.lst_pack(self.cur.len(), None, 0);
        let b0 = self.hpack(start, 0, false);
        let (h0, d0) = (b0.height, b0.depth);
        let b0 = self.e.nodes.add_box(b0);
        // `\@tempdimb` `\lst@frametextsep+\lst@framerulewidth`, `\@tempdimc` `-\@tempdimb+
        // \ht\z@+\dp\z@`; `\lst@frameHBkg\lst@fillcolor\@tempdimb\@firstoftwo`: `\kern
        // -\lst@framediml` (the corner, `\@tempdimb` wide) and a block of `\linewidth+2\@tempdimb`.
        let dimb = half;
        let dimc = -dimb + h0 + d0;
        let s = self.cur.len();
        let t = self.cur.len();
        self.lst_kern(0);
        self.lst_kern(-half);
        self.lst_frame_block(bkg.as_deref(), w + 2 * dimb, dimb, 0);
        let tb = self.hpack(t, 0, false);
        let shift = if top { -tb.depth } else { tb.height };
        let tb = self.e.nodes.add_box(BoxRec { shift, ..tb });
        let r = self.cur.len();
        self.cur.push(Node::new(Kind::Box, tb as i32));
        self.lst_lap(r, false);
        // `\lst@frameHBkg\lst@rulesepcolor\@tempdimc\@secondoftwo`, raised by
        // `\lst@frametextsep` (lowered by `\@tempdimb` plus its height).
        let t = self.cur.len();
        self.lst_kern(0);
        self.lst_frame_block(bkg.as_deref(), w, dimc, 0);
        let tb = self.hpack(t, 0, false);
        let shift = if top { -fts } else { dimb + tb.height };
        let tb = self.e.nodes.add_box(BoxRec { shift, ..tb });
        let r = self.cur.len();
        self.cur.push(Node::new(Kind::Box, tb as i32));
        self.lst_lap(r, false);
        let r = self.cur.len();
        self.cur.push(Node::new(Kind::Box, b0 as i32));
        self.lst_lap(r, false);
        let b = self.hpack(s, 0, false);
        self.e.nodes.add_box(b)
    }

    /// `\lst@DeInit` at the last line's end: the trailing empty lines' `\lst@NewLine`s in a
    /// box that is dropped (their `\refstepcounter{lstnumber}` stays), then `\par
    /// \removelastskip`.
    pub(super) fn lst_end(&mut self) {
        let Some(r) = self.lst_run() else { return };
        let n = r.lay.trailing;
        for _ in 0..n {
            let Some(r) = self.lst_run() else { return };
            r.number += 1;
            let name = format!("lstnumber.{}.{}", r.href, r.number);
            if self.hyper() {
                self.set_current_href(&name);
            }
        }
        let t = [self.doc_id(super::PRIM_PAR), self.doc_id(doc!("rt@lstbot"))];
        self.back_list(&t);
    }

    /// `\removelastskip`, `\everypar{}`, then `\lst@frameExit`: the bottom rule as a
    /// paragraph of its own (`\par\lst@parshape\nointerlineskip\noindent\box\z@`), with
    /// `\normalbaselines\normalcolor` in it; then `\par`.
    fn lst_bottom(&mut self) {
        if self.mode == Mode::Horizontal {
            // The bottom rule's paragraph: `\box\z@`, `\normalbaselines\normalcolor`.
            self.lst_rule_box();
            self.lst_normal_baselines();
            if self.colored() {
                self.lst_normal_color();
            }
            let t = [self.doc_id(super::PRIM_PAR), self.doc_id(doc!("rt@lstclose"))];
            return self.back_list(&t);
        }
        let g = self.last_skip();
        if g.width != 0 {
            self.vskip(super::neg(g));
        }
        self.set_everypar(0);
        if !self.lst_run().is_some_and(|r| r.cfg.frame) {
            self.lst_normal_baselines();
            if self.colored() {
                self.lst_normal_color();
            }
            return self.lst_close();
        }
        let b = self.lst_frame_h(false);
        if let Some(r) = self.lst_run() {
            r.rule_box = Some(b);
        }
        self.normal_paragraph();
        self.lst_parshape();
        self.aux = -65536000;
        let t = [self.named_cs(named!("noindent")), self.doc_id(doc!("rt@lstbot"))];
        self.back_list(&t);
    }

    /// `\normalcolor`: `\let\current@color\default@color\set@color`.
    fn lst_normal_color(&mut self) {
        self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
        self.color_stack(Some(super::DEFAULT_COLOR));
    }

    /// `\penalty-50\vspace\lst@belowskip`, `\endgroup` (the `\normalcolor`'s pop), the
    /// environment's `\@nobreakfalse`, `\lst@doendpe`, `\end`'s `\endgroup` and
    /// `\@ignoretrue`.
    fn lst_close(&mut self) {
        let below = self.lst_run().map_or(MEDSKIP, |r| r.cfg.below);
        self.run_ops(vec![Op::Penalty(-50), Op::Vskip(below), Op::Vskip(Glue::default()), Op::Call(|m| m.lst_finish())]);
    }

    fn lst_finish(&mut self) {
        if self.colored() {
            self.color_stack(None);
        }
        let input = self.lst_run().is_some_and(|r| r.input);
        if let Some(b) = self.lst_take_run() {
            self.e.nodes.flush_node(Node::new(Kind::Box, b as i32));
        }
        if let Some(doc) = self.doc.as_mut() {
            if input {
                doc.newlist = false;
            } else {
                doc.nobreak = false;
            }
        }
        let t = [self.doc_id(doc!("rt@ignorespaces"))];
        self.back_list(&t);
        self.endpe_true();
        self.end_group(SEMI_SIMPLE_GROUP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session's recompile after the listing's file changed (the source the same) reads
    /// the new file: the read is logged, and the PDF is a fresh compile's.
    #[test]
    fn input_listing_file_is_a_session_read() {
        let src = "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{listings}\n\\lstset{basicstyle=\\ttfamily}\n\\begin{document}\nCode:\n\\lstinputlisting{code.py}\n\\end{document}\n";
        let file = std::cell::RefCell::new(b"x = 1\n".to_vec());
        let read = |name: &str| (name == "code.py").then(|| file.borrow().clone());
        let mut session = super::super::Session::default();
        let first = super::super::run_session(src, 0, Some(&read), None, None, &mut session).expect("a compile");
        *file.borrow_mut() = b"y = 22\n".to_vec();
        let again = super::super::run_session(src, 0, Some(&read), None, None, &mut session).expect("a recompile");
        let fresh = super::super::run(src, 0, Some(&read)).expect("a fresh compile");
        assert!(first.pdf != again.pdf);
        assert!(again.pdf == fresh.pdf);
    }

    /// The tokens of a line under `language` with their styles.
    fn tokens(language: &str, line: &str) -> Vec<(String, u8)> {
        let cfg = Cfg::new([("language", language)].into_iter()).expect("keys");
        let lay = layout(&[line], &cfg, false, false).expect("a layout");
        let text = |e0: u32, e1: u32| lay.elems[e0 as usize..e1 as usize].iter().filter_map(|e| match e { Elem::Ch(c) => Some(*c as char), Elem::Space => Some(' '), Elem::NoLig => None }).collect::<String>();
        lay.items.iter().filter_map(|i| if let Item::Tok { e0, e1, style, .. } = *i { Some((text(e0, e1), style)) } else { None }).collect()
    }

    #[test]
    fn delimiters_open_and_close_as_listings_does() {
        // Matlab: `'` after a letter is a transpose; after a closed string it opens another.
        let t = tokens("Matlab", "a = b' + 'it''s'; % c");
        assert!(t.contains(&("'".to_string(), ST_NONE)));
        assert!(t.contains(&("it".to_string(), ST_STRING)) && t.contains(&("s".to_string(), ST_STRING)));
        assert!(t.contains(&("%".to_string(), ST_COMMENT)) && t.contains(&("c".to_string(), ST_COMMENT)));
        // C: a comment's opener in a string is text; `\"` keeps a `[b]` string open.
        let t = tokens("C", "s = \"a /* \\\" b\"; /* c */ int");
        assert!(t.contains(&("/*".to_string(), ST_STRING)) && t.contains(&("\\\"".to_string(), ST_STRING)));
        assert!(t.contains(&("b".to_string(), ST_STRING)) && t.contains(&("*/".to_string(), ST_COMMENT)));
        assert!(t.contains(&("int".to_string(), ST_KEYWORD)));
        // A string's spaces are visible tokens of their own.
        assert!(tokens("Python", "'a b'").contains(&(" ".to_string(), ST_STRING)));
    }
}
