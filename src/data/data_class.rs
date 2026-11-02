//! The state a class leaves at the first token of the document body, as data (P2,
//! `2026-10-23-p2-native-document-path.md`): what the native document job starts from in
//! place of a format and the class's code.
//!
//! `rushtex-gen class` runs the class on the oracle up to that token, writes this table and
//! checks it entry by entry against the oracle's state (`engine_doc::load_class` reads it
//! into a fresh engine, and every value is compared). One table per class and size; the
//! first is `article` at 10pt.
//!
//! Sections:
//! - `ints`, `dimens`: every integer and dimension parameter (§236, §247), in slot order;
//! - `glues`: every glue parameter as width, stretch, shrink, stretch order, shrink order;
//! - `codes`: `\lccode`, `\uccode`, `\sfcode`, `\mathcode`, `\delcode` of the 256 characters;
//! - `cats`: the 256 catcodes;
//! - `fonts`: one line per loaded typesetting font, `tfm<TAB>identifier`; `fontnums` holds
//!   per font its size, `\hyphenchar`, `\skewchar` and 1 if it is the current font;
//! - `class`: the values of [`CLASS_DIMENS`] in order, then [`CLASS_FLAGS`];
//! - `regs`: per [`CLASS_REGS`] entry its region (0 integer, 1 dimension, 2 glue), its
//!   register's eqtb slot and its value as width, stretch, shrink and the two orders (an
//!   integer or dimension in the width);
//! - `sizename`, `sizes`: per size command of [`SIZE_COMMANDS`] (the `size1x.clo` of the
//!   table), `\f@size`, `\sf@size` and `\ssf@size` after it (`\DeclareMathSizes`), then
//!   [`SIZE_ROW`] words: `\baselineskip`, the display skips it sets and the `\@listi` it
//!   defines (each with a mask of the glues it sets, see [`SizeRow`]);
//! - `lists`: `\@listii` to `\@listvi`, a mask and four glues each;
//! - `fontdims` (optional): per font whose parameters a class changed (IEEEtran's
//!   `\@IEEEtunefonts` sets `\fontdimen2` to `4` of its Times fonts), (font index, parameter
//!   number, value) for each parameter that differs from the font's TFM;
//! - `optkeys`, `optrows`, `optdiffs`: per combination of the parameter-only options
//!   ([`option_key`]), the words that differ from the table, as (section, index, value):
//!   section 0 `ints`, 1 `dimens`, 2 `glues`, 3 `class`, 4 `regs`.
//!
//! Not in the table, because the native job has no use for them: LaTeX's registers and
//! macros (the native commands keep their state as typed fields), and expl3's `intarray`
//! fonts (integers stored in the `\fontdimen`s of `cmr10` at 1sp to 26sp).

use super::data_blob::{Blob, U32s};

/// The class's page geometry and the dimensions the native output routine reads, by
/// register name.
pub const CLASS_DIMENS: [&str; 9] = ["textwidth", "textheight", "oddsidemargin", "evensidemargin", "topmargin", "headheight", "headsep", "footskip", "@maxdepth"];

/// LaTeX's registers the native commands use, by name: `\name`'s register, or for `count
/// x`, `dimen x` and `skip x` that register of the insertion class `\x`. The native job
/// keeps them at their LaTeX register numbers, so grouping restores them as in LaTeX.
pub const CLASS_REGS: [&str; 62] = [
    "footnotesep", "leftmargini", "leftmarginii", "leftmarginiii", "leftmarginiv", "leftmarginv", "leftmarginvi",
    "labelsep", "labelwidth", "leftmargin", "rightmargin", "listparindent", "itemindent", "@totalleftmargin", "linewidth",
    "@beginparpenalty", "@endparpenalty", "@itempenalty", "@clubpenalty", "@lowpenalty", "interfootnotelinepenalty",
    "@listdepth", "@itemdepth", "@enumdepth", "c@enumi", "c@enumii", "c@enumiii", "c@enumiv",
    "partopsep", "topsep", "parsep", "itemsep", "@topsep", "@topsepadd", "@outerparskip",
    "skip footins", "count footins", "dimen footins", "bigskipamount", "tabcolsep", "@rightskip",
    "arraycolsep", "arrayrulewidth", "doublerulesep",
    "floatsep", "textfloatsep", "intextsep", "@fptop", "@fpsep", "@fpbot", "abovecaptionskip", "belowcaptionskip",
    "c@topnumber", "c@bottomnumber", "c@totalnumber", "c@figure", "c@table",
    "dblfloatsep", "dbltextfloatsep", "c@dbltopnumber",
    "marginparsep", "marginparwidth",
];

/// Switches of the class, each 1 or 0: `\raggedbottom` (`\@textbottom` is a skip) and
/// `\if@twoside`.
pub const CLASS_FLAGS: [&str; 2] = ["raggedbottom", "twoside"];

/// article's size commands, in order (`\tiny` is 0, `\normalsize` 4).
pub const SIZE_COMMANDS: [&str; 10] = ["tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];

/// Words per size command: `\baselineskip` (5), the display-skip mask and
/// `\abovedisplayskip`, `\abovedisplayshortskip`, `\belowdisplayshortskip`,
/// `\belowdisplayskip` (1 + 20), the `\@listi` mask and `\topsep`, `\parsep`, `\itemsep`,
/// `\partopsep` (1 + 20).
pub const SIZE_ROW: usize = 47;

/// Glue words: width, stretch, shrink, stretch order, shrink order.
pub type GlueWords = [i32; 5];

/// A size command's settings: in each mask, bit k says the command sets glue k.
#[derive(Clone, Copy, Debug)]
pub struct SizeRow<'a> {
    pub fsize: &'a str,
    pub sf: &'a str,
    pub ssf: &'a str,
    pub baselineskip: GlueWords,
    pub display: (u32, [GlueWords; 4]),
    pub listi: (u32, [GlueWords; 4]),
}

/// The parameter-only class options of [`option_key`], by name: the paper sizes, then
/// `landscape`, `twoside` and `draft`; their defaults (`letterpaper`, `oneside`, `final`,
/// `onecolumn`, `notitlepage`) change nothing.
pub const PAPERS: [&str; 6] = ["letterpaper", "legalpaper", "executivepaper", "a4paper", "a5paper", "b5paper"];

/// A combination of the parameter-only options: the paper's index in [`PAPERS`] (bits
/// 0–2), `landscape` (bit 3), `twoside` (bit 4), `draft` (bit 5); `twocolumn` is bit 6
/// ([`TWOCOLUMN`]), added to a key.
pub fn option_key(paper: usize, landscape: bool, twoside: bool, draft: bool) -> u32 {
    paper as u32 | (landscape as u32) << 3 | (twoside as u32) << 4 | (draft as u32) << 5
}

/// The `twocolumn` option's bit in an [`option_key`]: the class's `\twocolumn \sloppy
/// \flushbottom` and the sizes' two-column values (`\textwidth`, `\parindent`, …).
pub const TWOCOLUMN: u32 = 64;

/// Every combination [`option_key`] makes, each also with [`TWOCOLUMN`].
pub fn option_keys() -> impl Iterator<Item = u32> {
    (0..PAPERS.len()).flat_map(|p| (0..16u32).map(move |b| option_key(p, b & 1 != 0, b & 2 != 0, b & 4 != 0) | (b & 8) << 3))
}

/// A loaded font of the table.
#[derive(Clone, Copy, Debug)]
pub struct ClassFont<'a> {
    pub tfm: &'a str,
    pub ident: &'a str,
    pub size: i32,
    pub hyphen_char: i32,
    pub skew_char: i32,
    pub current: bool,
}

#[derive(PartialEq)]
pub struct ClassTable<'a> {
    pub ints: U32s<'a>,
    pub dimens: U32s<'a>,
    pub glues: U32s<'a>,
    pub codes: U32s<'a>,
    pub cats: &'a [u8],
    fonts: &'a str,
    font_nums: U32s<'a>,
    class: U32s<'a>,
    regs: U32s<'a>,
    size_names: &'a str,
    /// `size_names`' rows split once: each size command's `\f@size`, `\sf@size`, `\ssf@size`.
    size_triples: [(&'a str, &'a str, &'a str); SIZE_COMMANDS.len()],
    sizes: U32s<'a>,
    lists: U32s<'a>,
    opt_keys: U32s<'a>,
    opt_rows: U32s<'a>,
    opt_diffs: U32s<'a>,
    font_dims: U32s<'a>,
}

impl<'a> ClassTable<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<ClassTable<'a>> {
        let t = ClassTable {
            ints: b.u32s("ints")?,
            dimens: b.u32s("dimens")?,
            glues: b.u32s("glues")?,
            codes: b.u32s("codes")?,
            cats: b.section("cats")?,
            fonts: std::str::from_utf8(b.section("fonts")?).ok()?,
            font_nums: b.u32s("fontnums")?,
            class: b.u32s("class")?,
            regs: b.u32s("regs")?,
            size_names: std::str::from_utf8(b.section("sizename")?).ok()?,
            size_triples: [("", "", ""); SIZE_COMMANDS.len()],
            sizes: b.u32s("sizes")?,
            lists: b.u32s("lists")?,
            opt_keys: b.u32s("optkeys")?,
            opt_rows: b.u32s("optrows")?,
            opt_diffs: b.u32s("optdiffs")?,
            font_dims: b.u32s("fontdims").unwrap_or_default(),
        };
        let sound = t.size_names.lines().count() == SIZE_COMMANDS.len() && t.size_names.lines().all(|l| l.split('\t').count() == 3) && t.sizes.len() == SIZE_ROW * SIZE_COMMANDS.len() && t.lists.len() == 5 * 21 && t.opt_rows.len() == t.opt_keys.len() + 1 && t.opt_diffs.len().is_multiple_of(3) && t.glues.len().is_multiple_of(5) && t.codes.len() == 5 * 256 && t.cats.len() == 256 && t.class.len() == CLASS_DIMENS.len() + CLASS_FLAGS.len() && t.font_nums.len() == 4 * t.fonts.lines().count() && t.fonts.lines().all(|l| l.contains('\t')) && t.regs.len() == 7 * CLASS_REGS.len() && t.font_dims.len().is_multiple_of(3);
        let mut t = t;
        for (i, l) in t.size_names.lines().take(SIZE_COMMANDS.len()).enumerate() {
            let mut names = l.split('\t');
            let mut next = || names.next().unwrap_or("");
            t.size_triples[i] = (next(), next(), next());
        }
        sound.then_some(t)
    }

    pub fn fonts(&self) -> impl Iterator<Item = ClassFont<'a>> + '_ {
        self.fonts.lines().enumerate().map(|(i, l)| {
            let (tfm, ident) = l.split_once('\t').unwrap_or((l, ""));
            let n = |k: usize| self.font_nums.get(4 * i + k) as i32;
            ClassFont { tfm, ident, size: n(0), hyphen_char: n(1), skew_char: n(2), current: n(3) != 0 }
        })
    }

    /// The parameters the class set on its fonts: (index in [`Self::fonts`], parameter
    /// number from 1, value).
    pub fn font_dims(&self) -> impl Iterator<Item = (usize, usize, i32)> + '_ {
        (0..self.font_dims.len() / 3).map(|i| (self.font_dims.get(3 * i) as usize, self.font_dims.get(3 * i + 1) as usize, self.font_dims.get(3 * i + 2) as i32))
    }

    /// The value of the [`CLASS_DIMENS`] entry `name`, in scaled points.
    pub fn dimen(&self, name: &str) -> i32 {
        CLASS_DIMENS.iter().position(|n| *n == name).map_or(0, |i| self.class.get(i) as i32)
    }

    /// [`CLASS_REGS`] entry `i`: region (0 integer, 1 dimension, 2 glue), slot, and value.
    pub fn reg(&self, i: usize) -> (u32, u32, [i32; 5]) {
        let n = |k: usize| self.regs.get(7 * i + k);
        (n(0), n(1), [2, 3, 4, 5, 6].map(|k| n(k) as i32))
    }

    pub fn flag(&self, name: &str) -> bool {
        CLASS_FLAGS.iter().position(|n| *n == name).is_some_and(|i| self.class.get(CLASS_DIMENS.len() + i) != 0)
    }

    /// The `class` section's values, [`CLASS_DIMENS`] then [`CLASS_FLAGS`].
    pub fn class_values(&self) -> [i32; CLASS_DIMENS.len() + CLASS_FLAGS.len()] {
        std::array::from_fn(|i| self.class.get(i) as i32)
    }

    /// Size command `i` of [`SIZE_COMMANDS`].
    pub fn size(&self, i: usize) -> SizeRow<'a> {
        let (fsize, sf, ssf) = self.size_triples.get(i).copied().unwrap_or(("", "", ""));
        let w = |k: usize| self.sizes.get(SIZE_ROW * i + k) as i32;
        let glue = |k: usize| std::array::from_fn(|j| w(k + j));
        let four = |k: usize| [glue(k), glue(k + 5), glue(k + 10), glue(k + 15)];
        SizeRow { fsize, sf, ssf, baselineskip: glue(0), display: (w(5) as u32, four(6)), listi: (w(26) as u32, four(27)) }
    }

    /// Size command `i`'s `\f@size` alone (`self.size(i).fsize`).
    pub fn fsize(&self, i: usize) -> &'a str {
        self.size_triples.get(i).map_or("", |t| t.0)
    }

    /// The first size command whose `\f@size` is `size`.
    pub fn size_index(&self, size: &str) -> Option<usize> {
        self.size_triples.iter().position(|t| t.0 == size)
    }

    /// `\@listii` (depth 2) to `\@listvi` (6): the mask of the glues set, and `\topsep`,
    /// `\parsep`, `\itemsep`, `\partopsep`.
    pub fn list(&self, depth: usize) -> (u32, [GlueWords; 4]) {
        let w = |k: usize| self.lists.get(21 * (depth - 2) + k) as i32;
        let glue = |k: usize| std::array::from_fn(|j| w(k + j));
        (w(0) as u32, [glue(1), glue(6), glue(11), glue(16)])
    }

    /// The words option combination `key` changes, as (section, index, value); `None` when
    /// the table does not have the combination.
    pub fn option_diffs(&self, key: u32) -> Option<impl Iterator<Item = (u32, u32, u32)> + '_> {
        let k = (0..self.opt_keys.len()).find(|&k| self.opt_keys.get(k) == key)?;
        let (a, b) = (self.opt_rows.get(k) as usize, self.opt_rows.get(k + 1) as usize);
        Some((a..b).map(|r| (self.opt_diffs.get(3 * r), self.opt_diffs.get(3 * r + 1), self.opt_diffs.get(3 * r + 2))))
    }
}

/// The size options, each with its table.
pub const CLASS_SIZES: [&str; 3] = ["10pt", "11pt", "12pt"];
static ARTICLE_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-article.rtdb"), include_bytes!("../../assets/tables/class-article-11pt.rtdb"), include_bytes!("../../assets/tables/class-article-12pt.rtdb")];

/// `article` at 10pt, checked on first use; `None` if it is not a table of this build.
pub fn article() -> Option<&'static ClassTable<'static>> {
    article_size(0)
}

/// `article` at size option `i` of [`CLASS_SIZES`].
pub fn article_size(i: usize) -> Option<&'static ClassTable<'static>> {
    class_size(0, i)
}

static REPORT_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-report.rtdb"), include_bytes!("../../assets/tables/class-report-11pt.rtdb"), include_bytes!("../../assets/tables/class-report-12pt.rtdb")];
static BOOK_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-book.rtdb"), include_bytes!("../../assets/tables/class-book-11pt.rtdb"), include_bytes!("../../assets/tables/class-book-12pt.rtdb")];

static LETTER_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-letter.rtdb"), include_bytes!("../../assets/tables/class-letter-11pt.rtdb"), include_bytes!("../../assets/tables/class-letter-12pt.rtdb")];

/// KOMA-Script's `scrartcl` (`rushtex-gen class --class scrartcl`): the type area typearea.sty
/// computes at the class load (`DIV`, `BCOR`, `\textwidth`, `\textheight`, the margins) for
/// each paper and size, read from the oracle, as every other word.
static SCRARTCL_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-scrartcl.rtdb"), include_bytes!("../../assets/tables/class-scrartcl-11pt.rtdb"), include_bytes!("../../assets/tables/class-scrartcl-12pt.rtdb")];

static AMSART_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-amsart.rtdb"), include_bytes!("../../assets/tables/class-amsart-11pt.rtdb"), include_bytes!("../../assets/tables/class-amsart-12pt.rtdb")];
static MEMOIR_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-memoir.rtdb"), include_bytes!("../../assets/tables/class-memoir-11pt.rtdb"), include_bytes!("../../assets/tables/class-memoir-12pt.rtdb")];
/// KOMA-Script's `scrreprt` (`rushtex-gen class --class scrreprt`): scrartcl's type area and
/// sizes, with report's counters (`\c@chapter` and the resets under it).
static SCRREPRT_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-scrreprt.rtdb"), include_bytes!("../../assets/tables/class-scrreprt-11pt.rtdb"), include_bytes!("../../assets/tables/class-scrreprt-12pt.rtdb")];

static IEEETRAN_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-IEEEtran.rtdb"), include_bytes!("../../assets/tables/class-IEEEtran-11pt.rtdb"), include_bytes!("../../assets/tables/class-IEEEtran-12pt.rtdb")];
/// KOMA-Script's `scrlttr2` (`rushtex-gen class --class scrlttr2`): typearea's page at each
/// size with DIN.lco loaded at the end of the class; the option bit 32 is `parskip=half`
/// there (`option_key`'s `draft`).
static SCRLTTR2_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-scrlttr2.rtdb"), include_bytes!("../../assets/tables/class-scrlttr2-11pt.rtdb"), include_bytes!("../../assets/tables/class-scrlttr2-12pt.rtdb")];
/// KOMA-Script's `scrbook` (`rushtex-gen class --class scrbook`): scrreprt's table two-sided
/// by default (bit 16 off is `oneside`), with the `bk1x.clo` sizes.
static SCRBOOK_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-scrbook.rtdb"), include_bytes!("../../assets/tables/class-scrbook-11pt.rtdb"), include_bytes!("../../assets/tables/class-scrbook-12pt.rtdb")];

/// a0poster.cls (v1.22b, `rushtex-gen class --class a0poster`): article with the poster's
/// paper, margins and size commands. Its paper options stand in [`A0_PAPERS`] for
/// [`PAPERS`]' and `portrait` for `landscape` (the class's default is landscape); a size
/// option changes nothing the class sets, so the three tables are alike.
/// screenplay.cls (v1.6, `rushtex-gen class --class screenplay`): article at 12pt with its
/// own page, courier and `myheadings`. It takes no option, so the three tables are alike.
static SCREENPLAY_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-screenplay.rtdb"), include_bytes!("../../assets/tables/class-screenplay-11pt.rtdb"), include_bytes!("../../assets/tables/class-screenplay-12pt.rtdb")];

static A0POSTER_BLOBS: [&[u8]; 3] = [include_bytes!("../../assets/tables/class-a0poster.rtdb"), include_bytes!("../../assets/tables/class-a0poster-11pt.rtdb"), include_bytes!("../../assets/tables/class-a0poster-12pt.rtdb")];

/// extsizes' extarticle.cls (v1.0, `rushtex-gen class --class extarticle`): article.cls of
/// 1996 with the sizes of [`EXT_SIZES`] (`size8.clo` .. `size20.clo`).
static EXTARTICLE_BLOBS: [&[u8]; 8] = [include_bytes!("../../assets/tables/class-extarticle.rtdb"), include_bytes!("../../assets/tables/class-extarticle-11pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-12pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-8pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-9pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-14pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-17pt.rtdb"), include_bytes!("../../assets/tables/class-extarticle-20pt.rtdb")];

/// The size options of extsizes' classes: article's three first, so that index 0 is the
/// default (`10pt`) for every class.
pub const EXT_SIZES: [&str; 8] = ["10pt", "11pt", "12pt", "8pt", "9pt", "14pt", "17pt", "20pt"];

/// The size options of class `cls` of [`CLASSES`], by [`class_size`]'s index.
pub fn class_sizes(cls: usize) -> &'static [&'static str] {
    if cls == EXTARTICLE { &EXT_SIZES } else { &CLASS_SIZES }
}

/// Every size name the class tables' size commands give (`\f@size`, `\sf@size`,
/// `\ssf@size`), in the order of [`CLASSES`], their size options and [`SIZE_COMMANDS`];
/// a test checks it against the tables, so a font size is matched without loading them.
pub const CLASS_SIZE_NAMES: [&str; 41] = ["5", "7", "8", "6", "9", "10", "12", "14.4", "17.28", "20.74", "24.88", "10.95", "14", "9.79996", "17", "11.89995", "8.5", "20", "13.99994", "24", "16.79993", "11", "7.69997", "5.5", "22", "15.39993", "29.86", "35.83", "43", "51.6", "61.92", "20.9019", "14.93", "25.08089", "17.915", "42.99", "30.09286", "21.495", "51.59", "36.11284", "25.795"];

/// a0poster's paper options by [`option_key`]'s paper index (the sixth repeats the default).
pub const A0_PAPERS: [&str; 6] = ["a0b", "a0", "a1", "a2", "a3", "a0b"];

/// The classes with a table, by [`class_size`]'s index: `article`, `report`
/// (`rushtex-gen class --class report`), `letter` (`--class letter`), `book`
/// (`--class book`), `scrartcl` (`--class scrartcl`), `amsart` (`--class amsart`), `memoir` (`--class memoir`),
/// `IEEEtran` (`--class IEEEtran`), `scrreprt` (`--class scrreprt`), `scrlttr2` (`--class scrlttr2`),
/// `a0poster` (`--class a0poster`), `extarticle` (`--class extarticle`), `scrbook` (`--class scrbook`),
/// `screenplay` (`--class screenplay`).
pub const CLASSES: [&str; 14] = ["article", "report", "letter", "book", "scrartcl", "amsart", "memoir", "IEEEtran", "scrreprt", "scrlttr2", "a0poster", "extarticle", "scrbook", "screenplay"];
/// [`CLASSES`]'s index of scrartcl.
pub const SCRARTCL: usize = 4;
/// [`CLASSES`]'s index of amsart.
pub const AMSART: usize = 5;
/// [`CLASSES`]'s index of memoir.
pub const MEMOIR: usize = 6;
/// IEEEtran.cls (V1.8b), `2026-10-31-w8-journal-classes.md`.
pub const IEEETRAN: usize = 7;
/// [`CLASSES`]'s index of scrreprt.
pub const SCRREPRT: usize = 8;
/// [`CLASSES`]'s index of scrlttr2 (`2026-11-01-w9-scrlttr2.md`).
pub const SCRLTTR2: usize = 9;
/// [`CLASSES`]'s index of a0poster.
pub const A0POSTER: usize = 10;
/// [`CLASSES`]'s index of screenplay.
pub const SCREENPLAY: usize = 13;
/// [`CLASSES`]'s index of extarticle.
pub const EXTARTICLE: usize = 11;
/// [`CLASSES`]'s index of scrbook (`2026-11-01-w9-scrbook.md`).
pub const SCRBOOK: usize = 12;

/// Class `cls` of [`CLASSES`] at size option `i` of its [`class_sizes`]. A register letter.cls
/// does not allocate (the float counters and skips) has region 3 in [`ClassTable::reg`].
pub fn class_size(cls: usize, i: usize) -> Option<&'static ClassTable<'static>> {
    static TABLES: [[std::sync::OnceLock<Option<ClassTable<'static>>>; EXT_SIZES.len()]; CLASSES.len()] = [const { [const { std::sync::OnceLock::new() }; EXT_SIZES.len()] }; CLASSES.len()];
    static BLOBS: [&[&[u8]]; CLASSES.len()] = [&ARTICLE_BLOBS, &REPORT_BLOBS, &LETTER_BLOBS, &BOOK_BLOBS, &SCRARTCL_BLOBS, &AMSART_BLOBS, &MEMOIR_BLOBS, &IEEETRAN_BLOBS, &SCRREPRT_BLOBS, &SCRLTTR2_BLOBS, &A0POSTER_BLOBS, &EXTARTICLE_BLOBS, &SCRBOOK_BLOBS, &SCREENPLAY_BLOBS];
    let blob = BLOBS.get(cls)?.get(i)?;
    TABLES.get(cls)?.get(i)?.get_or_init(|| Blob::new(blob).and_then(|b| ClassTable::from_blob(&b))).as_ref()
}

// ---- Generation -------------------------------------------------------------------------

/// The table's content as the generator reads it from the oracle.
#[cfg(any(test, feature = "gen"))]
#[derive(Default)]
pub struct GenClass {
    pub ints: Vec<i32>,
    pub dimens: Vec<i32>,
    pub glues: Vec<[i32; 5]>,
    pub codes: Vec<i32>,
    pub cats: Vec<u8>,
    /// (tfm, identifier, size, hyphen char, skew char, current)
    pub fonts: Vec<(String, String, i32, i32, i32, bool)>,
    pub class: Vec<i32>,
    /// Per [`CLASS_REGS`] entry: region, slot, width, stretch, shrink, stretch order,
    /// shrink order.
    pub regs: Vec<[i32; 7]>,
    /// Per size command: (`\f@size`, `\sf@size`, `\ssf@size`) and its [`SIZE_ROW`] words.
    pub sizes: Vec<([String; 3], Vec<i32>)>,
    /// `\@listii` to `\@listvi`: 21 words each.
    pub lists: Vec<i32>,
    /// Per option combination: its key and its differences.
    pub options: Vec<(u32, Vec<[u32; 3]>)>,
    /// (font index, parameter number, value): the parameters differing from the TFM's.
    pub font_dims: Vec<[i32; 3]>,
}

#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, g: &GenClass) {
    let u = |v: &[i32]| v.iter().map(|&x| x as u32).collect::<Vec<u32>>();
    w.u32s("ints", &u(&g.ints));
    w.u32s("dimens", &u(&g.dimens));
    w.u32s("glues", &u(&g.glues.concat()));
    w.u32s("codes", &u(&g.codes));
    w.bytes("cats", g.cats.clone());
    w.bytes("fonts", g.fonts.iter().map(|f| format!("{}\t{}\n", f.0, f.1)).collect::<String>().into_bytes());
    w.u32s("fontnums", &u(&g.fonts.iter().flat_map(|f| [f.2, f.3, f.4, f.5 as i32]).collect::<Vec<_>>()));
    w.u32s("class", &u(&g.class));
    w.u32s("regs", &u(&g.regs.concat()));
    w.bytes("sizename", g.sizes.iter().map(|(n, _)| format!("{}\n", n.join("\t"))).collect::<String>().into_bytes());
    w.u32s("sizes", &u(&g.sizes.iter().flat_map(|(_, v)| v.iter().copied()).collect::<Vec<_>>()));
    w.u32s("lists", &u(&g.lists));
    w.u32s("optkeys", &g.options.iter().map(|o| o.0).collect::<Vec<_>>());
    let mut rows = vec![0u32];
    for o in &g.options {
        rows.push(rows[rows.len() - 1] + o.1.len() as u32);
    }
    w.u32s("optrows", &rows);
    w.u32s("optdiffs", &g.options.iter().flat_map(|o| o.1.iter().flatten().copied()).collect::<Vec<_>>());
    if !g.font_dims.is_empty() {
        w.u32s("fontdims", &u(&g.font_dims.concat()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few of article's register values, as size10.clo and article.cls set them.
    #[test]
    fn article_registers() {
        let t = article().expect("the article table reads back");
        let reg = |name: &str| t.reg(CLASS_REGS.iter().position(|n| *n == name).unwrap());
        let pt = 65536;
        // 2.5em of cmr10, whose quad is 10.00003pt.
        assert_eq!(reg("leftmargini").2[0], 1638402);
        assert_eq!(reg("labelsep").2[0], 5 * pt);
        assert_eq!(reg("@lowpenalty").2[0], 51);
        assert_eq!(reg("@beginparpenalty").2[0], -51);
        assert_eq!(reg("topsep").2[..3], [8 * pt, 2 * pt, 4 * pt]);
        assert_eq!(reg("skip footins").2[..3], [9 * pt, 4 * pt, 2 * pt]);
        assert_eq!(reg("count footins").2[0], 1000);
        assert_eq!(reg("footnotesep").2[0], 435814);
    }

    /// letter.cls's own values, and the registers it does not allocate.
    #[test]
    fn letter_registers() {
        let t = class_size(2, 0).expect("the letter table reads back");
        let reg = |name: &str| t.reg(CLASS_REGS.iter().position(|n| *n == name).unwrap());
        let pt = 65536;
        assert_eq!(reg("c@figure").0, 3);
        assert_eq!(reg("abovecaptionskip").0, 3);
        assert_eq!(reg("labelsep").2[0], 5 * pt);
        // `\@listI`: `\topsep` and `\itemsep` .4em, `\parsep` 0.
        assert_eq!(reg("parsep").2[0], 0);
        assert_eq!(t.dimen("headsep"), 45 * pt);
        assert!(t.flag("raggedbottom"));
    }

    /// amsart.cls's own values: `\calclayout`'s page, `\@listI`, its head.
    #[test]
    fn amsart_registers() {
        let t = class_size(AMSART, 0).expect("the amsart table reads back");
        let pt = 65536;
        assert_eq!(t.dimen("textwidth"), 360 * pt);
        assert_eq!(t.dimen("headheight"), 8 * pt);
        assert_eq!(t.dimen("headsep"), 14 * pt);
        assert!(t.flag("twoside"));
    }
}
#[cfg(test)]
mod size_names_test {
    /// [`super::CLASS_SIZE_NAMES`] is every class table's `\f@size`, `\sf@size` and `\ssf@size`.
    #[test]
    fn class_size_names_are_the_tables() {
        let mut v: Vec<&str> = Vec::new();
        for c in 0..super::CLASSES.len() {
            for i in 0..super::class_sizes(c).len() {
                let t = super::class_size(c, i).expect("a class table");
                for k in 0..super::SIZE_COMMANDS.len() {
                    let r = t.size(k);
                    for s in [r.fsize, r.sf, r.ssf] {
                        if !v.contains(&s) {
                            v.push(s);
                        }
                    }
                }
            }
        }
        assert_eq!(v, super::CLASS_SIZE_NAMES);
    }
}
