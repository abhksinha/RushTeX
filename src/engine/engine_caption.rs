//! caption.sty (v3.6, TeX Live 2026) without options on the GDL path
//! (`2026-11-02-caption-design.md`): `\caption` in `figure` and `table` of the standard
//! classes as caption3's `\caption@@make` builds it, not the kernel's `\@makecaption`.
//!
//! caption.sty's `\caption@@caption` (caption.sty:212) is the kernel's `\@caption` up to
//! the list entry (`\caption@beginex`: the same `\addcontentsline`), then
//! `\caption@makecaption` (caption.sty:243): `\caption@make@above`, `\caption@@make`,
//! `\caption@make@below`. Where the caption goes is `position=auto` (caption3.sty:1035):
//! in vertical mode with `\prevdepth` above -1pt it is below the float's material
//! (`\abovecaptionskip` after `\caption@rule`, an `\hrule` of height 0, before it;
//! `\belowcaptionskip` after it), else above (`\belowcaptionskip` before; after it
//! `\abovecaptionskip` and the rule).
//!
//! `\caption@@make@` (caption3.sty:1495) is `\caption@box\hsize{...}` (box `none`: the
//! material as it is) holding `\hspace\caption@leftmargin` (0), `\caption@parbox` (a
//! `\parbox[b]` of `\caption@width`, `\hsize` here: `\vbox{\hsize.. \@parboxrestore
//! \leavevmode ...\par}`) and `\hspace\caption@rightmargin` (0, dropped by the line
//! break). Its text is `\caption@fmt{label}{: }{text}` (format `plain`, `#1#2#3\par`), the
//! text between struts (`\caption@ifstrut`, caption3.sty:829): `\vrule height\ht\strutbox
//! width\z@ \nobreak\hskip\z@skip` before it, `\unskip\nobreak\vrule height\z@
//! depth\dp\strutbox width\z@` after it. `\caption@slc` (caption3.sty:1520) sets that in
//! an `\hbox` first: no wider than `\caption@width`, the line is set `\centering`
//! (`singlelinecheck`), else as a justified paragraph.
//!
//! The first `\label` in a float (`\caption@xlabel`, caption.sty:376) writes once to the
//! `.aux` (`\providecommand*\caption@xref[2]{...}`): an inert write here
//! (`caption_xlabel`), the native job keeping `.aux` writes as whatsits.
//!
//! Not native (each declines): options and `\captionsetup` (the pre-scan), `\caption*`,
//! an empty caption text, hyperref (`hypcap`'s anchor at the float's start), classes other
//! than article, report and book (their `.sto` files), babel's french, longtable,
//! threeparttable, float, listings, subcaption, a `\label` before the float's `\caption` (it refers
//! to `\caption@undefinedlabel`) or in a paragraph.

use super::super::engine_box::{Ctx, HBOX_GROUP, VBOX_GROUP};
use super::super::engine_catcode::Cat;
use super::super::engine_eqtb::Glue;
use super::super::engine_node::{Kind, Node, RUNNING};
use super::super::engine_run::{Machine, Mode, SEMI_SIMPLE_GROUP};
use super::super::engine_scan::scan_dimen_ref;
use super::super::engine_token::Token;
use super::{CAPTION_MID, R_ABOVECAPTIONSKIP, R_BELOWCAPTIONSKIP, STRUT_DP, STRUT_HT, TEMP_BOX};

pub(super) const NATIVES: &[(&str, super::Native)] = &[
    ("rt@cappbox", |m| m.caption_parbox()),
    ("rt@cappboxend", |m| m.caption_parbox_end()),
    ("rt@capstrut", |m| m.caption_strut(false)),
    ("rt@capstrutend", |m| m.caption_strut(true)),
    ("rt@captionof", |m| m.captionof()),
    ("rt@captionsetup", |m| m.caption_setup()),
    ("rt@subfigend", |m| m.subfig_end()),
    ("rt@subfloat", |m| m.subfloat()),
    ("rt@subfloatbody", |m| m.subfloat_body()),
    ("rt@subfloatcap", |m| m.subfloat_cap()),
    ("rt@subfloatend", |m| m.subfloat_end()),
    ("rt@subref", |m| m.subref()),
];

/// `\DeclareCaptionFont`'s values the native job takes (caption3.sty:860), each one command.
const FONTS: &[(&str, usize)] = &[
    ("tiny", doc!("tiny")),
    ("scriptsize", doc!("scriptsize")),
    ("footnotesize", doc!("footnotesize")),
    ("small", doc!("small")),
    ("normalsize", doc!("normalsize")),
    ("large", doc!("large")),
    ("Large", doc!("Large")),
    ("LARGE", doc!("LARGE")),
    ("huge", doc!("huge")),
    ("Huge", doc!("Huge")),
    ("normalfont", doc!("normalfont")),
    ("up", doc!("upshape")),
    ("it", doc!("itshape")),
    ("sl", doc!("slshape")),
    ("sc", doc!("scshape")),
    ("md", doc!("mdseries")),
    ("bf", doc!("bfseries")),
    ("rm", doc!("rmfamily")),
    ("sf", doc!("sffamily")),
    ("tt", doc!("ttfamily")),
];

/// The caption being made: where it goes (`\caption@iftop`) and whether it is one line;
/// whether the first `\label` in a float wrote its `.aux` line; the options.
#[derive(Default, Clone, PartialEq)]
pub(super) struct Caption {
    top: bool,
    single: bool,
    xlabel: bool,
    opts: Opts,
    /// subcaption: the open `subfigure` (1) or `subtable` (2), 0 outside; its `\caption`
    /// came; the counters `subfigure` and `subtable` (reset by their float's).
    sub: u8,
    sub_captioned: bool,
    sub_n: [i32; 2],
    /// hyperref: the open float's `hypcap` anchor (`\hc@currentHref`).
    hc: String,
    /// The caption being made is a listing's (`\lst@MakeCaption`): its label `Listing N`,
    /// its text, no `\normalcolor`, its own skips.
    lst: Option<(String, String)>,
    /// `\captionsetup[figure]` and `\captionsetup[table]`'s keys, in order; the options
    /// before a float applied them (`\caption@settype`'s `\caption@setoptions`), restored
    /// at its end.
    typed: [String; 2],
    saved: Option<Opts>,
    /// The caption being made is a float.sty restyled float's: caption.sty's
    /// `\caption@setfloatcapt` box (`\@makecaption` is `\caption@@make`, no skips).
    floatcapt: bool,
    /// subfig: a `\subfloat` is open (its body, its caption: `subfig` set with `sub`), how
    /// its `\vtop` starts (`\@tempcnta`: 0 first in a minipage, 1 `\vskip\sf@top`, 2 after
    /// a skip), its caption (`None`: no `[..]`) and list entry flag, the body's width
    /// (`\@tempdima`); the list entries waiting for `\end@float` (`\sf@captionlist`).
    subfig: bool,
    sf_case: u8,
    sf_caption: Option<(Vec<Token>, bool)>,
    sf_w: i32,
    sf_list: u8,
}

/// caption's keys the native job takes (`\usepackage[..]{caption}`, `\captionsetup` in the
/// preamble): `font`, `labelfont`, `textfont` (indices in [`FONTS`]), `labelsep`,
/// `justification`, `singlelinecheck`, `position`; `skip`, `aboveskip` and `belowskip` set
/// the kernel's lengths as they are read.
#[derive(Default, Clone, PartialEq)]
pub(super) struct Opts {
    font: Vec<u8>,
    label: Vec<u8>,
    text: Vec<u8>,
    /// `\DeclareCaptionLabelSeparator` (caption3.sty:776): 0 colon, 1 period, 2 space,
    /// 3 none, 4 quad, 5 endash (the last two starred: not in the label font).
    sep: u8,
    /// `\DeclareCaptionJustification` (caption3.sty:964): 0 justified, 1 centering, 2
    /// raggedright, 3 raggedleft.
    just: u8,
    no_slc: bool,
    /// `\DeclareCaptionPosition`: 0 auto, 1 top, 2 bottom.
    position: u8,
    /// `\DeclareCaptionLabelFormat` (caption3.sty:734): 0 simple, 1 empty, 2 parens.
    lfmt: u8,
    /// `format=hang` (caption3.sty:720): `\@hangfrom{#1#2}` when the caption takes lines.
    hang: bool,
    /// `margin` (`\caption@setmargin`: both margins, the width 0) or `width`
    /// (`\caption@setwidth`: the margins 0), and `indention` (`\caption@indent`), set in sp
    /// as `\setlength` does when the key is read.
    margin: i32,
    width: i32,
    indent: i32,
    /// caption.sty's `ruled` style (caption.sty:907, `labelfont=bf,labelsep=space,strut=0`
    /// and no single-line style): no struts, a single line not centred.
    ruled: bool,
}

/// A length key's value: `skip`, `aboveskip` or `belowskip` (`\setlength`).
pub(super) enum Skip {
    Above(String),
    Below(String),
    Margin(String),
    Width(String),
    Indent(String),
}

impl Opts {
    /// Reads `list` into `self`; the lengths come back in order, for the caller to set.
    pub(super) fn keys(&mut self, list: &str) -> Result<Vec<Skip>, &'static str> {
        let mut skips = Vec::new();
        for item in split_top(list) {
            let (k, v) = item.split_once('=').map_or((item, ""), |(k, v)| (k.trim(), unbrace(v.trim())));
            match k {
                "font" => self.font = fonts(v)?,
                "labelfont" => self.label = fonts(v)?,
                "textfont" => self.text = fonts(v)?,
                "labelsep" | "labelseparator" => self.sep = ["colon", "period", "space", "none", "quad", "endash"].iter().position(|s| *s == v).ok_or("caption: this labelsep")? as u8,
                "justification" => self.just = ["justified", "centering", "raggedright", "raggedleft"].iter().position(|s| *s == v).ok_or("caption: this justification")? as u8,
                "singlelinecheck" | "slc" => {
                    self.no_slc = match v {
                        "" | "true" | "on" | "yes" | "1" => false,
                        "false" | "off" | "no" | "0" => true,
                        _ => return Err("caption: this singlelinecheck"),
                    }
                }
                "position" => {
                    self.position = match v {
                        "auto" | "a" => 0,
                        "top" | "above" | "t" => 1,
                        "bottom" | "below" | "b" => 2,
                        _ => return Err("caption: this position"),
                    }
                }
                "format" => {
                    self.hang = match v {
                        "plain" => false,
                        "hang" => true,
                        _ => return Err("caption: this format"),
                    }
                }
                "labelformat" => self.lfmt = ["simple", "empty", "parens"].iter().position(|s| *s == v).or((v == "default").then_some(0)).ok_or("caption: this labelformat")? as u8,
                "margin" | "width" | "indention" | "indent" => {
                    if v.is_empty() || split_top(v).len() != 1 {
                        return Err("caption: a margin, width or indention of this form");
                    }
                    skips.push(match k {
                        "margin" => Skip::Margin(v.to_string()),
                        "width" => Skip::Width(v.to_string()),
                        _ => Skip::Indent(v.to_string()),
                    });
                }
                "skip" | "aboveskip" | "belowskip" => {
                    if v.is_empty() {
                        return Err("caption: a length with commands");
                    }
                    skips.push(if k == "belowskip" { Skip::Below(v.to_string()) } else { Skip::Above(v.to_string()) });
                }
                "" => {}
                _ => return Err("caption: an option the native job does not take"),
            }
        }
        Ok(skips)
    }
}

/// `list` split at its commas outside braces, each item trimmed.
fn split_top(list: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in list.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(list[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(list[start..].trim());
    out.retain(|s| !s.is_empty());
    out
}

/// `v` without one pair of enclosing braces.
fn unbrace(v: &str) -> &str {
    v.strip_prefix('{').and_then(|x| x.strip_suffix('}')).map_or(v, str::trim)
}

/// A font key's value: font names, comma separated (`{small,it}`).
fn fonts(v: &str) -> Result<Vec<u8>, &'static str> {
    split_top(v).iter().filter(|n| **n != "none").map(|n| FONTS.iter().position(|(f, _)| f == n).map(|i| i as u8).ok_or("caption: this font")).collect()
}

impl Machine<'_, '_> {
    /// caption.sty is loaded.
    pub(super) fn caption_pkg(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.packages.contains(&"caption"))
    }

    /// What caption.sty does here that the native job does not; `None` when it may go on.
    pub(super) fn caption_pkg_refused(&self) -> Option<&'static str> {
        let d = self.doc.as_ref()?;
        if !d.std_heads || d.pkg.ext {
            return Some("caption under this class");
        }
        // caption.sty's `\caption@IfPackageLoaded` blocks for longtable (ltcaption, `\LT@array`,
        // `\LT@makecaption`), threeparttable (`\threeparttable`, `\TPT@caption`) and
        // listings (`\lst@MakeCaption`) change only those packages' captions: each declines
        // where it acts (a longtable's `\caption` is the pre-scan's stop already).
        if d.packages.iter().any(|p| matches!(*p, "wrapfig" | "sidecap")) {
            return Some("caption with wrapfig or sidecap");
        }
        if self.babel_french() {
            return Some("caption with babel's french");
        }
        None
    }

    /// `\caption@makecaption` up to the single-line check: the place (`\caption@fixposition`,
    /// `position=auto`), `\caption@make@above`, then `\caption@slc`'s `\sbox\@tempboxa`
    /// of the label, the separator and the text between struts (`caption_mid` goes on).
    pub(super) fn caption_open(&mut self) {
        if self.mode != Mode::Vertical {
            return self.not_native("caption: a caption in a paragraph");
        }
        if self.flr().caption.iter().all(|t| t.cat() == Some(Cat::Space)) {
            return self.not_native("caption: an empty caption text");
        }
        // `\caption@normalsize` (`font=normal`): `\normalcolor`, popped at `\@caption`'s group end.
        if self.colored() {
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.push_default_color();
        }
        let position = self.doc.as_ref().map_or(0, |d| d.pkg.cap.opts.position);
        let top = if position == 0 { self.aux <= -65536 } else { position == 1 };
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.top = top;
        }
        if top {
            let g = self.rg(R_BELOWCAPTIONSKIP);
            self.build_glue(g, false);
        } else {
            self.build_rule([RUNNING, 0, 0]);
            let g = self.rg(R_ABOVECAPTIONSKIP);
            self.build_glue(g, false);
        }
        self.caption_measure();
    }

    /// caption.sty's `\@float@setevery` for a float.sty restyled float (caption.sty:857),
    /// before the float's box: `\caption@setstyle*{<style>}` and `\caption@setoptions
    /// {<style>}`. Only `ruled` is a caption style (caption.sty:907, the `ruled` option
    /// off): `\caption@resetstyle` (caption3.sty:655, the defaults but the position) and
    /// `labelfont=bf,labelsep=space,strut=0`; `boxed` has the options `skip=2pt`
    /// (caption.sty:903). In the environment's group (the skip is read again by
    /// `\float@makebox`); the options before come back at the float's end
    /// (`caption_typed_end`).
    pub(super) fn caption_float_every(&mut self, ruled: bool, boxed: bool) {
        let saved = self.doc.as_ref().map(|d| d.pkg.cap.opts.clone());
        if ruled {
            let bf = FONTS.iter().position(|f| f.0 == "bf").unwrap_or(0) as u8;
            if let Some(d) = self.doc.as_mut() {
                let o = &mut d.pkg.cap.opts;
                *o = Opts { position: o.position, label: vec![bf], sep: 2, ruled: true, ..Opts::default() };
            }
        }
        if boxed {
            if let Err(what) = self.caption_keys("skip=2pt") {
                return self.not_native(what);
            }
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.saved = saved;
        }
        // Each key the style sets is `\caption@setkeys`, `\@bsphack..\@esphack`: in vertical
        // mode after a heading `\@esphack` is `\nobreak`, seven times for `ruled`'s style,
        // once for `boxed`'s `skip` (counted in pdfLaTeX's trace).
        if self.mode == Mode::Vertical && self.doc.as_ref().is_some_and(|d| d.nobreak || d.noskipsec) {
            for _ in 0..if ruled { 7 } else { usize::from(boxed) } {
                self.cur.push(Node::new(Kind::Penalty, 10000));
            }
        }
    }

    /// caption.sty's `\caption@@caption` in a restyled float after its `\par` and
    /// `\caption@beginex`: `\caption@setfloatcapt{..}` as caption.sty:860 redefines it,
    /// `\global\setbox\@floatcapt\vbox{\color@begingroup \caption@parboxrestore
    /// \caption@normalsize \caption@@make{fnum}{text}\par\color@endgroup}`; `\caption@@make`
    /// is the measure and the parbox without `\caption@make@above` and `@below`.
    pub(super) fn caption_floatcapt(&mut self) {
        if self.flr().caption.iter().all(|t| t.cat() == Some(Cat::Space)) {
            return self.not_native("caption: an empty caption text");
        }
        self.open_box(VBOX_GROUP, Ctx::Set { n: super::float::FLOAT_CAPT, global: true }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        self.par_box_restore();
        if self.flr().minipage {
            self.set_minipage();
        }
        self.size_command(super::NORMALSIZE);
        if self.colored() {
            self.e.eqtb.set_word(super::CURRENT_COLOR, 0, false);
            self.push_default_color();
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.floatcapt = true;
        }
        self.caption_measure();
    }

    /// A listing's caption under caption (`\lst@MakeCaption`, after `\caption@make@above`):
    /// at the top, its label and text from the listing.
    pub(super) fn caption_lst_measure(&mut self) {
        if self.rw(super::R_TOTALLEFTMARGIN) != 0 {
            return self.not_native("caption: a listing caption in a list");
        }
        // `\fnum@lstlisting` is the label as it stands: a `labelformat` is not followed here.
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.lfmt != 0) {
            return self.not_native("caption: a listing caption with a labelformat");
        }
        let parts = self.lst_caption_parts();
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.top = true;
            d.pkg.cap.lst = parts;
        }
        self.caption_measure();
    }

    /// The outer paragraph's start: `\hspace\caption@leftmargin`; for a listing first
    /// `\caption@beginhook`'s `\caption@lst@leftskip` (`\hskip-\@totalleftmargin`, 0 here).
    fn caption_lead(&mut self) -> Vec<Token> {
        let lst = self.doc.as_ref().is_some_and(|d| d.pkg.cap.lst.is_some());
        let (left, _, indent) = self.caption_widths();
        let lead = format!("{}\\hskip{}sp", if lst { "\\hskip0pt" } else { "" }, left + indent);
        self.tex_tokens(&lead)
    }

    /// `\caption@calcmargin` (caption3.sty:1525) against `\hsize`: the left margin, the
    /// caption's width (`\caption@width`) and the indention. With `width` the margins
    /// are half the rest each (`\divide`); else both are `margin`.
    fn caption_widths(&self) -> (i32, i32, i32) {
        let hsize = self.dimen_param(super::super::engine_pack::HSIZE);
        // One line: the single-line style (`\DeclareCaptionStyle{default}[indent=0pt,
        // justification=centering]`, caption3.sty:664) has no indention.
        let o = self.doc.as_ref().map(|d| (d.pkg.cap.opts.margin, d.pkg.cap.opts.width, if d.pkg.cap.single { 0 } else { d.pkg.cap.opts.indent })).unwrap_or_default();
        match o {
            (_, w, i) if w != 0 => ((hsize - w) / 2, w, i),
            (m, _, i) => (m, hsize - 2 * m, i),
        }
    }

    /// `\caption@slc`'s measure, or straight to the parbox with `singlelinecheck=false`.
    fn caption_measure(&mut self) {
        // `singlelinecheck=false`: `\caption@multiline`, no measure.
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.no_slc) {
            if let Some(d) = self.doc.as_mut() {
                d.pkg.cap.single = false;
            }
            let mut t = self.caption_lead();
            t.push(self.doc_id(doc!("rt@cappbox")));
            return self.back_list(&t);
        }
        self.open_box(HBOX_GROUP, Ctx::Set { n: TEMP_BOX, global: false }, false, 0, false);
        self.begin_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            let c = self.current_color();
            self.color_stack(Some(&c));
        }
        let mut list = self.caption_text();
        list.push(self.doc_id(CAPTION_MID));
        self.back_list(&list);
    }

    /// `\caption@@@make` (caption3.sty:1661) after `\caption@applyfont`'s justification:
    /// `\captionfont`, then `#1#2{\captiontextfont\caption@ifstrut..#3..}`, the label in
    /// `\captionlabelfont` (`caption_tokens`'s), the separator (in the label font unless
    /// starred) and the text between the struts.
    fn caption_text(&mut self) -> Vec<Token> {
        self.caption_text_as(false)
    }

    /// The caption's tokens; `hang`: format `hang`'s `\@hangfrom{#1#2}` (`\setbox\@tempboxa
    /// \hbox{{#1#2}}\hangindent\wd\@tempboxa\noindent\box\@tempboxa`) before the text.
    fn caption_text_as(&mut self, hang: bool) -> Vec<Token> {
        let mut all = Vec::new();
        match self.doc.as_ref().and_then(|d| d.pkg.cap.lst.clone()) {
            // `\fnum@lstlisting`, `{\lstlistingname~\thelstlisting}`, and `\ignorespaces #3`.
            Some((text, the)) => {
                all.push(Token::char(Cat::BeginGroup, '{'));
                all.extend("Listing".chars().map(|c| Token::char(Cat::Letter, c)));
                all.push(self.named_cs(named!("nobreakspace")));
                all.extend(the.chars().map(|c| Token::char(Cat::Other, c)));
                all.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::Other, ':'), Token::char(Cat::Space, ' '), self.named_cs(named!("ignorespaces"))]);
                all.extend(self.tex_tokens(&text));
            }
            None => self.caption_tokens(&mut all),
        }
        let mut o = self.doc.as_ref().map(|d| d.pkg.cap.opts.clone()).unwrap_or_default();
        let ign = self.named_cs(named!("ignorespaces"));
        let at = all.iter().position(|t| *t == ign).unwrap_or(all.len());
        let text = all.split_off(at);
        all.truncate(at.saturating_sub(2));
        // `labelformat`: `empty` (a label of width 0: `\caption@@@make` sets the separator
        // `none`), `parens` (`\bothIfFirst{#1}{\nobreakspace}(#2)`).
        // An empty caption text (`\caption@ifempty{#2}`): the separator `none`.
        if text.iter().skip(1).all(|t| t.cat() == Some(Cat::Space)) {
            o.sep = 3;
        }
        let nbsp = self.named_cs(named!("nobreakspace"));
        match o.lfmt {
            1 => {
                all.clear();
                o.sep = 3;
            }
            2 => {
                if let Some(k) = all.iter().position(|t| *t == nbsp) {
                    all.insert(k + 1, Token::char(Cat::Other, '('));
                    all.push(Token::char(Cat::Other, ')'));
                }
            }
            _ => {}
        }
        // subcaption's `sub` options (subcaption.sty:218): `font+=smaller` (`\caption@smaller`,
        // a size down from where `font` leaves it), `labelformat=parens` (`(\thesubfigure)`,
        // the name empty), `labelsep=space`; `skip=6pt` is `subfig_open`'s.
        if let Some((subfig, n)) = self.doc.as_ref().filter(|d| d.pkg.cap.sub != 0).map(|d| (d.pkg.cap.subfig, d.pkg.cap.sub_n[d.pkg.cap.sub as usize - 1])) {
            // subfig's `\captionsetup[subfloat]{font=footnotesize,labelformat=parens,
            // labelsep=space}` (subfig.sty), the font set, not stepped down.
            if subfig {
                o.font = vec![2];
            } else {
                let size = o.font.iter().rev().find(|&&i| i < 10).map_or(4, |&i| i);
                o.font.push(size.saturating_sub(1));
            }
            o.sep = if text.iter().skip(1).all(|t| t.cat() == Some(Cat::Space)) { 3 } else { 2 };
            all = vec![Token::char(Cat::Other, '(')];
            all.extend(super::alph(n, b'a').map(|c| Token::char(Cat::Letter, c)));
            all.push(Token::char(Cat::Other, ')'));
        }
        let font = |m: &mut Self, f: &[u8]| f.iter().map(|&i| m.doc_id(FONTS[i as usize].1)).collect::<Vec<_>>();
        let mut list = font(self, &o.font);
        let open = Token::char(Cat::BeginGroup, '{');
        let close = Token::char(Cat::EndGroup, '}');
        if hang {
            list.extend(self.tex_tokens(&format!("\\setbox{TEMP_BOX}\\hbox")));
            list.extend([open, open]);
        }
        if o.label.is_empty() {
            list.extend(all);
        } else {
            list.push(open);
            list.extend(font(self, &o.label));
            list.extend(all);
            list.push(close);
        }
        let sep = match o.sep {
            0 => self.tex_tokens(": "),
            1 => self.tex_tokens(". "),
            2 => vec![Token::char(Cat::Space, ' ')],
            3 => Vec::new(),
            4 => self.tex_tokens("\\quad"),
            _ => self.tex_tokens("\\space\\textendash\\space"),
        };
        if o.label.is_empty() || o.sep >= 4 || sep.is_empty() {
            list.extend(sep);
        } else {
            list.push(open);
            list.extend(font(self, &o.label));
            list.extend(sep);
            list.push(close);
        }
        if hang {
            list.extend([close, close]);
            list.extend(self.tex_tokens(&format!("\\hangindent\\wd{TEMP_BOX}\\box{TEMP_BOX}")));
        }
        list.push(open);
        list.extend(font(self, &o.text));
        // `strut=0` (the `ruled` style): `\caption@ifstrut` takes neither strut.
        if !o.ruled {
            list.push(self.doc_id(doc!("rt@capstrut")));
        }
        list.extend(self.tex_tokens("\\penalty10000 \\hskip0pt"));
        list.extend(text);
        if !o.ruled {
            list.extend(self.tex_tokens("\\unskip\\penalty10000 "));
            list.push(self.doc_id(doc!("rt@capstrutend")));
        }
        list.push(close);
        list
    }

    /// `\vrule\@height\ht\strutbox\@width\z@` before the text, `\@finalstrut`'s rule
    /// after it, with the strut of the caption's size.
    fn caption_strut(&mut self, end: bool) {
        let (h, d) = (self.e.eqtb.word(STRUT_HT), self.e.eqtb.word(STRUT_DP));
        let t = if end { format!("\\vrule height0pt depth{d}sp width0pt") } else { format!("\\vrule height{h}sp width0pt") };
        let t = self.tex_tokens(&t);
        self.back_list(&t);
    }

    /// `\captionsetup{keys}` in the preamble.
    fn caption_setup(&mut self) {
        // `\captionsetup[figure]` and `[table]`: kept for `\caption@settype` (`caption_typed`).
        let mut typed = None;
        if self.next_is('[') {
            let arg = self.bracket_arg();
            let name: String = arg.iter().map(|t| if t.cs_id().is_none() { t.chr() } else { '\\' }).collect();
            match name.trim() {
                "figure" => typed = Some(0),
                "table" => typed = Some(1),
                _ => return self.not_native("caption: \\captionsetup[type] of another type"),
            }
        }
        let mut t = Vec::new();
        self.read_arg(&mut t);
        if self.doc.as_ref().is_none_or(|d| d.body) {
            return self.not_native("caption: \\captionsetup after \\begin{document}");
        }
        let Some(text) = super::counter::the_text(self, &t) else {
            return self.not_native("caption: \\captionsetup with commands");
        };
        if let Some(k) = typed {
            if let Err(what) = Opts::default().keys(&text) {
                return self.not_native(what);
            }
            if let Some(d) = self.doc.as_mut() {
                let s = &mut d.pkg.cap.typed[k];
                s.push(',');
                s.push_str(&text);
            }
            return;
        }
        if let Err(what) = self.caption_keys(&text) {
            self.not_native(what);
        }
    }

    /// `\caption@settype{figure}`'s `\caption@setoptions{figure}`: the type's keys over the
    /// options, in the float's group (the skips locally); `caption_typed_end` restores them.
    pub(super) fn caption_typed(&mut self, t: u8) {
        // A `\newfloat` type has no keys (`\captionsetup[type]` of it declines).
        let Some(keys) = self.doc.as_ref().filter(|d| d.packages.contains(&"caption")).and_then(|d| d.pkg.cap.typed.get(t as usize - 1).cloned()) else { return };
        if keys.is_empty() {
            return;
        }
        let saved = self.doc.as_ref().map(|d| d.pkg.cap.opts.clone());
        if let Err(what) = self.caption_keys(&keys) {
            return self.not_native(what);
        }
        // After a restyled float's style options the ones before those come back.
        if let Some(d) = self.doc.as_mut().filter(|d| d.pkg.cap.saved.is_none()) {
            d.pkg.cap.saved = saved;
        }
    }

    /// The float's end: the options before its type's keys.
    pub(super) fn caption_typed_end(&mut self) {
        if let Some(d) = self.doc.as_mut() {
            if let Some(o) = d.pkg.cap.saved.take() {
                d.pkg.cap.opts = o;
            }
        }
    }

    /// caption's keys (`Opts::keys`), the lengths set in the current font's units.
    pub(super) fn caption_keys(&mut self, list: &str) -> Result<(), &'static str> {
        let mut o = self.doc.as_ref().map(|d| d.pkg.cap.opts.clone()).unwrap_or_default();
        let skips = o.keys(list)?;
        let (em, ex) = self.em_ex();
        // `\textwidth`, `\linewidth`, `\columnwidth` in the preamble: the class's text width
        // (geometry sets it again at `\begin{document}`, so with geometry they decline).
        let tw = self.doc.as_ref().filter(|d| !d.packages.contains(&"geometry") && !d.floats.twocol).map(|d| d.class.dimen("textwidth"));
        let lookup = |n: &str| tw.filter(|_| matches!(n, "textwidth" | "linewidth" | "columnwidth")).map(|w| Glue { width: w, ..Glue::default() });
        for s in skips {
            let (r, v) = match &s {
                Skip::Above(v) => (Some(R_ABOVECAPTIONSKIP), v),
                Skip::Below(v) => (Some(R_BELOWCAPTIONSKIP), v),
                Skip::Margin(v) | Skip::Width(v) | Skip::Indent(v) => (None, v),
            };
            if r.is_some() && v.contains('\\') {
                return Err("caption: a length with commands");
            }
            let w = scan_dimen_ref(v, em, ex, &lookup).ok_or("caption: this length")?;
            match (r, &s) {
                (Some(r), _) => self.set_rg(r, Glue { width: w, ..Glue::default() }),
                (None, Skip::Margin(_)) => (o.margin, o.width) = (w, 0),
                (None, Skip::Width(_)) => (o.margin, o.width) = (0, w),
                _ => o.indent = w,
            }
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.opts = o;
        }
        Ok(())
    }

    /// After the `\sbox`: one line or more (`\ifdim\wd\@tempboxa>\caption@width`), then
    /// `\hspace\caption@leftmargin` (it starts the paragraph) and the parbox.
    pub(super) fn caption_measured(&mut self) {
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.package();
        let b = self.e.eqtb.box_reg(TEMP_BOX).unwrap_or(0);
        let single = self.e.nodes.boxes[b as usize].width <= self.caption_widths().1;
        self.e.eqtb.replace_box(TEMP_BOX, None);
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.single = single;
        }
        let mut t = self.caption_lead();
        t.push(self.doc_id(doc!("rt@cappbox")));
        self.back_list(&t);
    }

    /// `\caption@parbox\caption@width{..}`: `\@iiiparbox`'s `\vbox{\hsize\@tempdima
    /// \@parboxrestore ..\@@par}` (`[b]`: the box as made), in it `\leavevmode`, for one line
    /// `\centering` (`\caption@slfmt`'s justification), the text and `\par`.
    fn caption_parbox(&mut self) {
        let (_, width, indent) = self.caption_widths();
        self.open_box(VBOX_GROUP, Ctx::Shift(0), false, 0, false);
        self.set_dimen_named(param!("hsize"), width - indent);
        self.par_box_restore();
        // `\leavevmode` (`\unhbox\voidb@x`): the paragraph with its indent box, `\everypar`
        // read before what follows (`new_graf`, as `strut` does).
        // `\caption@applyfont`'s justification: `\centering` for one line (`\caption@slsty`).
        let mut t = Vec::new();
        // The `ruled` style has no single-line style: its justification stands.
        let ruled = self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.ruled);
        match self.doc.as_ref().map_or((false, 0), |d| (d.pkg.cap.single && !ruled, d.pkg.cap.opts.just)) {
            (true, _) | (false, 1) => t.push(self.doc_id(doc!("centering"))),
            (false, 2) => t.push(self.doc_id(doc!("raggedright"))),
            (false, 3) => t.push(self.doc_id(doc!("raggedleft"))),
            _ => {}
        }
        // `\caption@make@indention`: `\hskip-\caption@indent` (it starts the paragraph) or
        // `\leavevmode`.
        if indent != 0 {
            t.splice(0..0, self.tex_tokens(&format!("\\hskip{}sp", -indent)));
        }
        let hang = self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.hang && !d.pkg.cap.single);
        t.extend(self.caption_text_as(hang));
        let par = self.doc.as_ref().map_or(0, |d| d.par_id);
        t.push(Token::cs(par));
        t.push(self.doc_id(doc!("rt@cappboxend")));
        self.back_list(&t);
        if indent == 0 {
            self.new_graf(true);
        }
    }

    /// The parbox's end, `\hspace\caption@rightmargin`, the paragraph's end, then
    /// `\caption@make@below` and `\@caption`'s `\par` and group end (`caption_end`'s).
    fn caption_parbox_end(&mut self) {
        self.package();
        let n = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(n);
        // A listing's: `\caption@make@below` is `\vskip\abovecaptionskip` as listings' `\nobreak
        // \@@vskip..\nobreak` (the first `\nobreak` in the paragraph), no rule; then
        // `\lst@MakeCaption`'s group ends and `\lst@Init` goes on.
        // A subfloat's (`\caption@make` in its `\parbox[t]`): no skips, no group.
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.subfig) {
            return self.par();
        }
        // A restyled float's (`\caption@setfloatcapt`): `\par`, `\color@endgroup` and the
        // `\vbox` ended into `\@floatcapt`.
        if self.doc.as_mut().is_some_and(|d| std::mem::take(&mut d.pkg.cap.floatcapt)) {
            self.par();
            self.end_group(SEMI_SIMPLE_GROUP);
            if self.colored() {
                self.color_stack(None);
            }
            return self.package();
        }
        if self.doc.as_mut().and_then(|d| d.pkg.cap.lst.take()).is_some() {
            self.cur.push(super::Node::new(super::Kind::Penalty, 10000));
            self.par();
            let g = Glue { width: super::listings::SMALLSKIP.width, ..Glue::default() };
            return self.run_ops(vec![
                super::Op::Vskip(g),
                super::Op::Penalty(10000),
                super::Op::Call(|m| {
                    m.end_group(SEMI_SIMPLE_GROUP);
                    m.lst_init_rest();
                }),
            ]);
        }
        self.par();
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.top) {
            let g = self.rg(R_ABOVECAPTIONSKIP);
            self.build_glue(g, false);
            self.build_rule([RUNNING, 0, 0]);
        } else {
            let g = self.rg(R_BELOWCAPTIONSKIP);
            self.build_glue(g, false);
        }
        self.par();
        self.end_group(SEMI_SIMPLE_GROUP);
        if self.colored() {
            self.color_stack(None);
        }
        self.captionof_end();
    }

    /// `\captionof{type}` (capt-of.sty: `\def\@captype{#1}\caption`; caption.sty:390:
    /// `\setcaptiontype*{#1}\caption`, whose star leaves out `\caption@setanchor`, so
    /// `\label` stays the kernel's): `\caption` outside a float. caption's
    /// `\@@setcaptiontype` also runs `\caption@settype` (`\caption@clrflags`) and
    /// `\caption@parboxrestore@light` (`\parindent\z@ \parskip\z@skip \sloppy`, local to
    /// the enclosing group). `\@captype` lasts here for the caption alone: a second `\caption`
    /// in the group is stopped by the pre-scan.
    fn captionof(&mut self) {
        if self.flr().captype != 0 || self.mode == Mode::Math || self.inner && self.mode != Mode::Vertical {
            return self.not_native("\\captionof in a float, a formula or a box");
        }
        if self.next_is('*') {
            return self.not_native("\\captionof*");
        }
        if self.hyper() {
            return self.not_native("\\captionof with hyperref");
        }
        if self.doc.as_ref().is_some_and(|d| d.koma || d.memoir) || self.tpt_phase() != 0 {
            return self.not_native("\\captionof under this class or in a threeparttable");
        }
        let mut arg = Vec::new();
        self.read_arg(&mut arg);
        let name: String = arg.iter().map(|t| if t.cs_id().is_none() { t.chr() } else { '\\' }).collect();
        let t = match name.trim() {
            "figure" => 1,
            "table" => 2,
            _ => return self.not_native("\\captionof of a type other than figure and table"),
        };
        let f = self.fl();
        (f.captype, f.of) = (t, true);
        (f.captioned, f.subbed, f.sub_above) = (false, false, false);
        if self.caption_pkg() {
            self.caption_typed(t);
            self.set_dimen_named(param!("parindent"), 0);
            self.set_glue_named(param!("parskip"), Glue::default());
            let t = self.tex_tokens("\\tolerance9999 \\emergencystretch3em\\hfuzz.5pt\\vfuzz.5pt ");
            let mut t = t;
            t.push(self.doc_id(doc!("caption")));
            return self.back_list(&t);
        }
        self.caption();
    }

    /// After `\captionof`'s caption: no float type again.
    pub(super) fn captionof_end(&mut self) {
        let f = self.fl();
        if std::mem::take(&mut f.of) {
            f.captype = 0;
            self.caption_typed_end();
        }
    }

    /// `\caption@xlabel`'s first use (`\caption@@@xlabel`, global once): the `.aux` line
    /// before the label's own.
    pub(super) fn caption_xlabel(&mut self) -> bool {
        // caption's `\threeparttable` runs `\caption@setanchor`: `\@currentlabel` is
        // `\caption@undefinedlabel` in it.
        if self.tpt_phase() != 0 {
            self.not_native("caption: a \\label in a threeparttable");
            return false;
        }
        if self.flr().captype == 0 {
            return true;
        }
        let captioned = match self.doc.as_ref().map(|d| (d.pkg.cap.sub, d.pkg.cap.sub_captioned)) {
            Some((0, _)) | None => self.flr().captioned,
            Some((_, c)) => c,
        };
        if !captioned {
            self.not_native("caption: a \\label before the float's \\caption");
            return false;
        }
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.xlabel) {
            return true;
        }
        if self.mode != Mode::Vertical {
            self.not_native("caption: a float's first \\label in a paragraph");
            return false;
        }
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.xlabel = true;
        }
        self.aux_write(0);
        true
    }

    /// The open float's `hypcap` anchor.
    pub(super) fn caption_hc(&self) -> String {
        self.doc.as_ref().map_or(String::new(), |d| d.pkg.cap.hc.clone())
    }

    /// `\caption@start@` (caption.sty:1028, `hypcap=true`): `\hyper@makecurrent{figure.caption}`
    /// (`\Hy@hypertexnamesfalse`: `figure.caption.N`, `\Hy@linkcounter` stepped, global),
    /// then `\caption@startanchor` in vertical mode: `\nointerlineskip`,
    /// `\vspace*{-\caption@hypcapspace}` (`.5\baselineskip`), the anchor
    /// (`\hyper@anchorstart`), `\vspace*{\caption@hypcapspace}`, `\prevdepth` back.
    pub(super) fn caption_hypcap(&mut self) {
        let t = self.flr().captype;
        let name = match self.doc.as_mut() {
            Some(d) => {
                d.link_counter += 1;
                format!("{}.caption.{}", if t == 1 { "figure" } else { "table" }, d.link_counter)
            }
            None => return,
        };
        self.set_current_href(&name);
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.hc = name.clone();
        }
        self.caption_start_anchor(name);
    }

    /// `\caption@startanchor{name}` in vertical mode (caption.sty:1044).
    pub(super) fn caption_start_anchor(&mut self, name: String) {
        let saved = self.aux;
        self.aux = -65536000;
        let space = self.e.eqtb.glue(param!("baselineskip")).width / 2;
        self.caption_vspace_star(-space);
        self.vertical_anchor(name);
        self.caption_vspace_star(space);
        self.aux = saved;
    }

    /// `\vspace*{h}` in vertical mode (`\@vspacer`): `\hrule\@height\z@ \nobreak \vskip h
    /// \vskip\z@skip`, `\prevdepth` kept.
    fn caption_vspace_star(&mut self, h: i32) {
        let pd = self.aux;
        self.build_rule([RUNNING, 0, 0]);
        self.cur.push(super::Node::new(super::Kind::Penalty, 10000));
        let n = self.e.nodes.glue_node(Glue { width: h, ..Glue::default() }, 0);
        self.cur.push(n);
        let n = self.e.nodes.glue_node(Glue::default(), 0);
        self.cur.push(n);
        self.aux = pd;
    }

    /// In a `subfigure` or `subtable`.
    pub(super) fn caption_in_sub(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pkg.cap.sub != 0)
    }

    /// `\newcounter{subfigure}[figure]`: the float's counter stepped.
    pub(super) fn caption_sub_reset(&mut self, t: u8) {
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.sub_n[t as usize - 1] = 0;
        }
    }

    /// `\begin{subfigure}[pos]{width}` (subcaption.sty:70): `\minipage[pos]{width}`, `[b]`
    /// before the float's `\caption` and `[t]` after (`\caption@ifcaption`), then
    /// `\setcaptionsubtype`: `\caption@subtypehook` steps the float's counter at the
    /// first one before its `\caption` (`\caption@stepcounter`), and the `sub` options
    /// (`skip=6pt` here, the rest in `caption_text`).
    pub(super) fn subfig_open(&mut self, t: u8) {
        if let Some(what) = self.caption_pkg_refused() {
            return self.not_native(what);
        }
        let f = self.flr();
        if f.captype != t || self.caption_in_sub() {
            return self.not_native("subcaption: a subfigure or subtable outside its float");
        }
        if self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.margin != 0 || d.pkg.cap.opts.width != 0 || d.pkg.cap.opts.indent != 0) {
            return self.not_native("subcaption: a subfigure with caption margins, width or indention");
        }
        let f = self.flr();
        if f.captioned && f.sub_above {
            return self.not_native("subcaption: sub-captions on both sides of the \\caption");
        }
        let (captioned, subbed) = (f.captioned, f.subbed);
        if !captioned && !subbed {
            let r = super::R_FIGURE + t as usize - 1;
            let n = self.rw(r) + 1;
            self.set_rw(r, n, true);
            self.caption_sub_reset(t);
        }
        let f = self.fl();
        f.subbed = true;
        f.sub_above |= !captioned;
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.sub = t;
            d.pkg.cap.sub_captioned = false;
        }
        self.set_rg(R_ABOVECAPTIONSKIP, Glue { width: 6 * 65536, ..Glue::default() });
        let mut list = vec![self.doc_id(doc!("rt@minipage"))];
        if self.next_is('[') {
            list.push(Token::char(Cat::Other, '['));
        } else {
            list.extend([Token::char(Cat::Other, '['), Token::char(Cat::Letter, if captioned { 't' } else { 'b' }), Token::char(Cat::Other, ']')]);
        }
        self.back_list(&list);
    }

    /// subfig's `\subfloat[list][caption]{body}` (subfig.sty, `\sf@subfloat`): in a float,
    /// `\refstepcounter{subfigure}` with `\c@figure` one up for it (the main caption below),
    /// `\@tempcnta` from `\if@minipage` and `\lastskip`, `\leavevmode`, then
    /// `\setbox\@tempboxa\hbox{body}` (`subfloat_body` goes on). Not native: the main caption
    /// at the top or before, hyperref, lists of figures or tables, caption margins.
    fn subfloat(&mut self) {
        if let Some(what) = self.caption_pkg_refused() {
            return self.not_native(what);
        }
        let t = self.flr().captype;
        if t == 0 || t > 2 || self.styled(t).is_some() || self.caption_in_sub() || self.mode == Mode::Math || self.flr().of {
            return self.not_native("subfig: a \\subfloat outside a float or in one");
        }
        if self.hyper() || self.doc.as_ref().is_some_and(|d| d.contents.as_ref().is_some_and(|c| c.iter().any(|e| e.level >= super::LOF))) {
            return self.not_native("subfig: a \\subfloat with hyperref or a list of figures");
        }
        if self.flr().captioned || self.doc.as_ref().is_some_and(|d| d.pkg.cap.opts.position == 1 || d.pkg.cap.opts.margin != 0 || d.pkg.cap.opts.width != 0 || d.pkg.cap.opts.indent != 0) {
            return self.not_native("subfig: a \\subfloat after or below its caption, or with caption margins");
        }
        // `\sf@@subfloat`: `[#1]` alone is the list entry and the caption; `[#1][#2]`.
        let o1 = self.next_is('[').then(|| self.bracket_arg());
        let o2 = if o1.is_some() && self.next_is('[') { Some(self.bracket_arg()) } else { None };
        let caption = match (o1, o2) {
            (Some(c), None) => Some((c, true)),
            (Some(l), Some(c)) => Some((c, !l.is_empty())),
            _ => None,
        };
        if caption.as_ref().is_some_and(|(c, _)| c.iter().any(|x| x.cs_id().is_some_and(|id| self.e.cs.name(id) == "label"))) {
            return self.not_native("subfig: a \\label in a sub-caption");
        }
        let r = super::R_FIGURE + t as usize - 1;
        let fig = self.rw(r) + 1;
        let n = self.doc.as_mut().map_or(1, |d| {
            let c = &mut d.pkg.cap;
            c.sub_n[t as usize - 1] += 1;
            c.sub_n[t as usize - 1]
        });
        let prefix = self.doc.as_ref().map_or(String::new(), |d| d.the_prefix[t as usize - 1].clone());
        let label: String = super::alph(n, b'a').collect();
        self.store_current_label(&format!("{prefix}{fig}{label}"));
        let case = if self.flr().minipage {
            0
        } else if self.last_skip().width == 0 {
            1
        } else {
            2
        };
        if let Some(d) = self.doc.as_mut() {
            let c = &mut d.pkg.cap;
            (c.sub, c.subfig, c.sf_case, c.sf_caption) = (t, true, case, caption);
        }
        let mut body = Vec::new();
        self.read_arg(&mut body);
        let mut list = self.tex_tokens(&format!("\\setbox{TEMP_BOX}\\hbox"));
        list.push(Token::char(Cat::BeginGroup, '{'));
        list.extend(body);
        list.push(Token::char(Cat::EndGroup, '}'));
        list.push(self.doc_id(doc!("rt@subfloatbody")));
        self.back_list(&list);
        if self.mode == Mode::Vertical {
            self.new_graf(true);
        }
    }

    /// After the body: `\vtop{\vbox{[\vskip\sf@top]\box\@tempboxa}`, with a caption
    /// `\vskip\sf@capskip` (4pt), `\hrule` of nothing and `\sf@subcaption`, then
    /// `\vskip\sf@bottom}` (`\sf@farskip` 10pt on top, `\sf@nearskip` 0 below, the main
    /// caption being below).
    fn subfloat_body(&mut self) {
        let w = self.e.eqtb.box_reg(TEMP_BOX).map_or(0, |b| self.e.nodes.boxes[b as usize].width);
        let (case, captioned) = self.doc.as_mut().map_or((0, false), |d| {
            d.pkg.cap.sf_w = w;
            (d.pkg.cap.sf_case, d.pkg.cap.sf_caption.is_some())
        });
        let mut t = String::from("\\vtop{\\vbox{");
        if case == 1 {
            t.push_str("\\vskip10pt");
        }
        t.push_str(&format!("\\box{TEMP_BOX}}}"));
        if captioned {
            // `\sf@subcaption`: `\setbox0\vbox{\hb@xt@\@tempdima{\hss\parbox[t]{\@tempdima}
            // {\caption@make..}\hss}}\box0` in a group.
            t.push_str(&format!("\\vskip4pt\\hrule width0pt height0pt depth0pt {{\\setbox0\\vbox{{\\hbox to{w}sp{{\\hss\\vtop{{"));
        }
        let mut list = self.tex_tokens(&t);
        if captioned {
            list.push(self.doc_id(doc!("rt@subfloatcap")));
            list.extend(self.tex_tokens("}\\hss}}\\box0}"));
        }
        list.extend(self.tex_tokens("\\vskip0pt}"));
        list.push(self.doc_id(doc!("rt@subfloatend")));
        self.back_list(&list);
    }

    /// In the `\parbox[t]`: `\hsize\@tempdima \@parboxrestore`, then caption3's box
    /// (`\caption@make{\subfigurename}{\thesubfigure}{caption}`: no skips around it).
    fn subfloat_cap(&mut self) {
        let Some((text, w)) = self.doc.as_ref().and_then(|d| d.pkg.cap.sf_caption.as_ref().map(|c| (c.0.clone(), d.pkg.cap.sf_w))) else { return };
        self.set_dimen_named(param!("hsize"), w);
        self.par_box_restore();
        self.fl().caption = text;
        self.caption_measure();
    }

    /// The subfloat's end: the list entry kept for `\end@float`, `\ignorespaces`.
    fn subfloat_end(&mut self) {
        if let Some(d) = self.doc.as_mut() {
            let c = &mut d.pkg.cap;
            if c.sf_caption.take().is_some_and(|x| x.1) {
                c.sf_list += 1;
            }
            (c.sub, c.subfig) = (0, false);
        }
        self.ignore_spaces();
    }

    /// `\subfloat@label{key}` in a subfloat's body (`\sf@@sub@label`): caption's
    /// `\caption@xlabel` (its `.aux` line once), the label, then `\newlabel{sub@key}` of
    /// `\caption@lstfmt` (`subparens`: `(a)`), `\subref`'s.
    pub(super) fn subfloat_label(&mut self) {
        // `\caption@@@xlabel`'s own `\@bsphack`/`\@esphack`: after a space, `\nobreak
        // \hskip\z@skip` (the kernel `\label`'s `\@bsphack` then sees no skip).
        if self.doc.as_ref().is_some_and(|d| !d.pkg.cap.xlabel) {
            let skip = if self.mode == Mode::Horizontal { self.last_skip().width } else { 0 };
            if let Some(d) = self.doc.as_mut() {
                d.pkg.cap.xlabel = true;
            }
            self.aux_write(0);
            if skip > 0 {
                self.cur.push(super::Node::new(super::Kind::Penalty, 10000));
                let g = self.e.nodes.glue_node(Glue::default(), 0);
                self.cur.push(g);
            }
        }
        let mut key = Vec::new();
        self.read_arg(&mut key);
        self.label_key(&key);
        let n = self.doc.as_ref().map_or(1, |d| d.pkg.cap.sub_n[d.pkg.cap.sub as usize - 1]);
        let saved = self.e.eqtb.toks(super::CURRENT_LABEL);
        let label: String = super::alph(n, b'a').collect();
        self.store_current_label(&format!("({label})"));
        let mut sub: Vec<Token> = "sub@".chars().map(|c| Token::char(Cat::Other, c)).collect();
        sub.extend(key);
        self.label_key(&sub);
        self.e.eqtb.set_toks(super::CURRENT_LABEL, saved, false);
    }

    /// subfig's `\end@float`: the sub counters to 0, `\@listsubcaptions`'s list entries
    /// (`\addcontentsline{lof}{subfigure}`), before `\@endfloatbox`'s `\par` (in a
    /// paragraph after the caption, there). subfig's `\caption@` never runs: caption.sty
    /// sets `\@caption` again at `\begin{document}`, so `\end@float` lists them all.
    pub(super) fn subfloat_float_end(&mut self) -> bool {
        let Some(d) = self.doc.as_mut().filter(|d| d.packages.contains(&"subfig")) else { return true };
        d.pkg.cap.sub_n = [0, 0];
        let n = std::mem::take(&mut d.pkg.cap.sf_list);
        for _ in 0..n {
            self.aux_write(0);
        }
        true
    }

    /// In a subfloat's body.
    pub(super) fn subfloat_open(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.pkg.cap.subfig)
    }

    /// After `\end{subfigure}`'s group.
    fn subfig_end(&mut self) {
        if let Some(d) = self.doc.as_mut() {
            d.pkg.cap.sub = 0;
        }
    }

    /// `\caption` in a `subfigure`: `\@caption{subfigure}`, `\refstepcounter{subfigure}`
    /// (`\@currentlabel` is `\p@subfigure\thesubfigure`, `\thefigure` then `\alph`), no
    /// list entry (`list=0`), the box as caption3 makes it (`caption_text`'s options).
    pub(super) fn subcaption(&mut self) {
        let t = self.flr().captype;
        let r = super::R_FIGURE + t as usize - 1;
        let fig = self.rw(r);
        let n = self.doc.as_mut().map_or(1, |d| {
            let c = &mut d.pkg.cap;
            c.sub_n[t as usize - 1] += 1;
            c.sub_captioned = true;
            c.sub_n[t as usize - 1]
        });
        // `\p@subfigure` is `\thefigure` (`float_the`).
        let Some(parent) = self.float_the(t as usize - 1, fig) else { return self.not_native("a \\thefigure or \\thetable the native job cannot read") };
        let label: String = super::alph(n, b'a').collect();
        // hyperref: `\H@refstepcounter`, the anchor `hypcap`'s (`\caption@makecurrentHref`).
        if self.hyper() {
            self.store_current_label(&format!("{parent}{label}"));
            let hc = self.caption_hc();
            self.set_current_href(&hc);
        } else {
            self.set_current_label(&format!("{parent}{label}"));
        }
        if self.next_is('[') {
            self.bracket_arg();
        }
        let mut text = Vec::new();
        self.read_arg(&mut text);
        self.fl().caption = text;
        self.run_ops(vec![super::Op::Par, super::Op::Call(|m| m.caption_body())]);
    }

    /// `\subcaption@label` (subcaption.sty:178): `\label{key}`, then with `\@currentlabel`
    /// `\thesubfigure` `\label{sub@key}` (`\subref`'s), in vertical mode.
    pub(super) fn subcaption_label(&mut self) {
        if self.mode != Mode::Vertical {
            return self.not_native("subcaption: a \\label in a paragraph");
        }
        let mut key = Vec::new();
        self.read_arg(&mut key);
        self.label_key(&key);
        let (t, n) = self.doc.as_ref().map_or((1, 0), |d| (d.pkg.cap.sub, d.pkg.cap.sub_n[d.pkg.cap.sub as usize - 1]));
        let _ = t;
        let saved = self.e.eqtb.toks(super::CURRENT_LABEL);
        let label: String = super::alph(n, b'a').collect();
        self.store_current_label(&label);
        let mut sub: Vec<Token> = "sub@".chars().map(|c| Token::char(Cat::Other, c)).collect();
        sub.extend(key);
        self.label_key(&sub);
        self.e.eqtb.set_toks(super::CURRENT_LABEL, saved, false);
    }

    /// `\subref{key}` (subcaption.sty:191): `\begingroup` `\ref{sub@key}` (`subrefformat=simple`,
    /// `\p@subref` empty) `\endgroup`.
    fn subref(&mut self) {
        // hyperref's `\@subref`: `\hyperref[key]{..}`, a link.
        if self.hyper() {
            return self.not_native("subcaption: \\subref with hyperref");
        }
        if self.next_is('*') || self.next_is('[') {
            return self.not_native("subcaption: \\subref* or \\subref[..]");
        }
        let mut key = Vec::new();
        self.read_arg(&mut key);
        let mut list = vec![Token::char(Cat::BeginGroup, '{'), self.doc_id(doc!("ref")), Token::char(Cat::BeginGroup, '{')];
        list.extend("sub@".chars().map(|c| Token::char(Cat::Other, c)));
        list.extend(key);
        list.extend([Token::char(Cat::EndGroup, '}'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&list);
    }
}
