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
pub const CLASS_REGS: [&str; 57] = [
    "footnotesep", "leftmargini", "leftmarginii", "leftmarginiii", "leftmarginiv", "leftmarginv", "leftmarginvi",
    "labelsep", "labelwidth", "leftmargin", "rightmargin", "listparindent", "itemindent", "@totalleftmargin", "linewidth",
    "@beginparpenalty", "@endparpenalty", "@itempenalty", "@clubpenalty", "@lowpenalty", "interfootnotelinepenalty",
    "@listdepth", "@itemdepth", "@enumdepth", "c@enumi", "c@enumii", "c@enumiii", "c@enumiv",
    "partopsep", "topsep", "parsep", "itemsep", "@topsep", "@topsepadd", "@outerparskip",
    "skip footins", "count footins", "dimen footins", "bigskipamount", "tabcolsep", "@rightskip",
    "arraycolsep", "arrayrulewidth", "doublerulesep",
    "floatsep", "textfloatsep", "intextsep", "@fptop", "@fpsep", "@fpbot", "abovecaptionskip", "belowcaptionskip",
    "c@topnumber", "c@bottomnumber", "c@totalnumber", "c@figure", "c@table",
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
/// 0–2), `landscape` (bit 3), `twoside` (bit 4), `draft` (bit 5).
pub fn option_key(paper: usize, landscape: bool, twoside: bool, draft: bool) -> u32 {
    paper as u32 | (landscape as u32) << 3 | (twoside as u32) << 4 | (draft as u32) << 5
}

/// Every combination [`option_key`] makes.
pub fn option_keys() -> impl Iterator<Item = u32> {
    (0..PAPERS.len()).flat_map(|p| (0..8u32).map(move |b| option_key(p, b & 1 != 0, b & 2 != 0, b & 4 != 0)))
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
    sizes: U32s<'a>,
    lists: U32s<'a>,
    opt_keys: U32s<'a>,
    opt_rows: U32s<'a>,
    opt_diffs: U32s<'a>,
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
            sizes: b.u32s("sizes")?,
            lists: b.u32s("lists")?,
            opt_keys: b.u32s("optkeys")?,
            opt_rows: b.u32s("optrows")?,
            opt_diffs: b.u32s("optdiffs")?,
        };
        let sound = t.size_names.lines().count() == SIZE_COMMANDS.len() && t.size_names.lines().all(|l| l.split('\t').count() == 3) && t.sizes.len() == SIZE_ROW * SIZE_COMMANDS.len() && t.lists.len() == 5 * 21 && t.opt_rows.len() == t.opt_keys.len() + 1 && t.opt_diffs.len().is_multiple_of(3) && t.glues.len().is_multiple_of(5) && t.codes.len() == 5 * 256 && t.cats.len() == 256 && t.class.len() == CLASS_DIMENS.len() + CLASS_FLAGS.len() && t.font_nums.len() == 4 * t.fonts.lines().count() && t.fonts.lines().all(|l| l.contains('\t')) && t.regs.len() == 7 * CLASS_REGS.len();
        sound.then_some(t)
    }

    pub fn fonts(&self) -> impl Iterator<Item = ClassFont<'a>> + '_ {
        self.fonts.lines().enumerate().map(|(i, l)| {
            let (tfm, ident) = l.split_once('\t').unwrap_or((l, ""));
            let n = |k: usize| self.font_nums.get(4 * i + k) as i32;
            ClassFont { tfm, ident, size: n(0), hyphen_char: n(1), skew_char: n(2), current: n(3) != 0 }
        })
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
        let mut names = self.size_names.lines().nth(i).unwrap_or("").split('\t');
        let mut next = || names.next().unwrap_or("");
        let (fsize, sf, ssf) = (next(), next(), next());
        let w = |k: usize| self.sizes.get(SIZE_ROW * i + k) as i32;
        let glue = |k: usize| std::array::from_fn(|j| w(k + j));
        let four = |k: usize| [glue(k), glue(k + 5), glue(k + 10), glue(k + 15)];
        SizeRow { fsize, sf, ssf, baselineskip: glue(0), display: (w(5) as u32, four(6)), listi: (w(26) as u32, four(27)) }
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
    static TABLES: [std::sync::OnceLock<Option<ClassTable<'static>>>; 3] = [const { std::sync::OnceLock::new() }; 3];
    TABLES.get(i)?.get_or_init(|| Blob::new(ARTICLE_BLOBS[i]).and_then(|b| ClassTable::from_blob(&b))).as_ref()
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
}
