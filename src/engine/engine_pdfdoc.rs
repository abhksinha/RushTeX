//! pdfTeX's document-level extensions as hyperref uses them: `\pdfinfo`, `\pdfcatalog`,
//! `\pdfdest`, `\pdfstartlink`, `\pdfendlink`, `\pdfobj`, `\pdfrefobj` and `\pdfoutline`
//! (pdftex.web, "pdf_info", "pdf_catalog", "scan_action", "scan_alt_rule", "do_dest",
//! "do_link", "append_link", "end_link", "write_action", "pdfoutline", "Output name tree"),
//! in our own form.
//!
//! The syntax is scanned exactly as pdfTeX scans it, so the tokens around them are read the
//! same way. Destinations and links are whatsit nodes on the current list whose `a` indexes
//! their specification here; they are placed when their page is shipped out, and the
//! annotation and destination objects are rendered then. Every object number the document
//! can see (`\pdflastobj`, `\pdflastlink`, `goto num`) comes from one counter here; the PDF
//! writer numbers its own objects after the last of them.

use std::collections::{HashMap, HashSet};

use super::engine_node::{Kind, Node};
use super::engine_prim::Prim;
use super::engine_run::{Machine, Mode};
use crate::backend::backend_pdftex::{print_bp, print_int};

/// The `sub` of the link and destination whatsits (after `engine_whatsit`'s).
pub(super) const DEST: u8 = 14;
pub(super) const START_LINK: u8 = 15;
pub(super) const END_LINK: u8 = 16;
/// `\pdfrefobj`: `a` the object number.
pub(super) const REF_OBJ: u8 = 17;

/// `\pdfdest`'s view types, in the order pdfTeX scans them.
const XYZ: u8 = 0;
const FIT: u8 = 1;
const FITH: u8 = 2;
const FITV: u8 = 3;
const FITB: u8 = 4;
const FITBH: u8 = 5;
const FITBV: u8 = 6;
const FITR: u8 = 7;

/// What a `goto` or `thread` action points at.
#[derive(Debug, Clone)]
enum Target {
    Page(i32, String),
    Name(String),
    Num(i32),
}

/// `scan_action`'s result.
#[derive(Debug, Clone)]
enum Action {
    User(String),
    Go { thread: bool, file: Option<String>, new_window: u8, target: Target },
}

/// A `\pdfstartlink`'s box dimensions (`None` running), attributes, action and object.
#[derive(Debug)]
struct Link {
    dims: [Option<i32>; 3],
    attr: String,
    action: Action,
    obj: i32,
    shipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DestId {
    Name(String),
    Num(i32),
}

#[derive(Debug)]
struct Dest {
    id: DestId,
    view: u8,
    zoom: Option<i32>,
    dims: [Option<i32>; 3],
}

/// `\pdfobj`: its text, stream attributes when it is a stream, whether the text names a
/// file, and whether it is to be written.
#[derive(Debug)]
struct RawObj {
    obj: i32,
    data: String,
    stream: Option<String>,
    file: bool,
    wanted: bool,
}

/// `\pdfoutline`, linked as pdfTeX links it (indices into `outlines`, `usize::MAX` none).
#[derive(Debug)]
struct Outline {
    obj: i32,
    title: String,
    action: Vec<u8>,
    attr: String,
    count: i32,
    parent: usize,
    prev: usize,
    next: usize,
    first: usize,
    last: usize,
}

const NONE: usize = usize::MAX;

/// Numbered objects and their bodies, as the PDF writer takes them.
pub type Objects = Vec<(i32, Vec<u8>)>;

/// The document-level state of pdfTeX's extensions.
#[derive(Debug)]
pub struct PdfDoc {
    /// `\pdfinfo` and `\pdfcatalog` text, in the order given.
    pub info: String,
    pub catalog: String,
    open_action: Option<Vec<u8>>,
    /// The last object number given out, and the last of each kind (`\pdflastobj`,
    /// `\pdflastlink`).
    pub objects: i32,
    pub last_obj: i32,
    pub last_link: i32,
    links: Vec<Link>,
    dests: Vec<Dest>,
    raw: Vec<RawObj>,
    outlines: Vec<Outline>,
    first_outline: usize,
    last_outline: usize,
    parent_outline: usize,
    /// `get_obj`: the objects of named and numbered destinations and of pages.
    dest_obj: HashMap<DestId, i32>,
    page_obj: HashMap<i32, i32>,
    /// Destinations placed on a page; annotation and destination objects already rendered.
    placed: HashSet<i32>,
    pub done: Vec<(i32, Vec<u8>)>,
    /// `\pdfximage`s, `\pdflastximage`, and the transparency group object (0 until an
    /// image with alpha needs one).
    pub images: Vec<super::engine_image::XImage>,
    pub last_ximage: i32,
    pub group_obj: i32,
}

impl Default for PdfDoc {
    fn default() -> Self {
        PdfDoc {
            info: String::new(),
            catalog: String::new(),
            open_action: None,
            objects: 0,
            last_obj: 0,
            last_link: 0,
            links: Vec::new(),
            dests: Vec::new(),
            raw: Vec::new(),
            outlines: Vec::new(),
            first_outline: NONE,
            last_outline: NONE,
            parent_outline: NONE,
            dest_obj: HashMap::new(),
            page_obj: HashMap::new(),
            placed: HashSet::new(),
            done: Vec::new(),
            images: Vec::new(),
            last_ximage: 0,
            group_obj: 0,
        }
    }
}

/// pdfTeX's `pdf_print_str`: text already in `(…)` or hex `<…>` as it is, else in parentheses.
fn print_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let hex = b.len() >= 2 && b.len().is_multiple_of(2) && b[0] == b'<' && b[b.len() - 1] == b'>' && b[1..b.len() - 1].iter().all(u8::is_ascii_hexdigit);
    if s.is_empty() {
        out.extend_from_slice(b"()");
    } else if (b[0] == b'(' && b[b.len() - 1] == b')') || hex {
        latin1(out, s);
    } else {
        out.push(b'(');
        latin1(out, s);
        out.push(b')');
    }
}

/// Text whose characters are bytes, as pdfTeX writes its strings.
fn latin1(out: &mut Vec<u8>, s: &str) {
    out.extend(s.chars().map(|c| c as u32 as u8));
}

/// `pdf_print_toks_ln`: nothing for empty text.
fn toks_ln(out: &mut Vec<u8>, s: &str) {
    if !s.is_empty() {
        latin1(out, s);
        out.push(b'\n');
    }
}

fn reference(out: &mut Vec<u8>, id: i32) {
    print_int(out, id as i64);
    out.extend_from_slice(b" 0 R");
}

impl PdfDoc {
    pub(super) fn new_obj(&mut self) -> i32 {
        self.objects += 1;
        self.objects
    }

    /// `get_obj` for a destination.
    fn dest_obj(&mut self, id: &DestId) -> i32 {
        if let Some(&n) = self.dest_obj.get(id) {
            return n;
        }
        let n = self.new_obj();
        self.dest_obj.insert(id.clone(), n);
        n
    }

    /// `get_obj(obj_type_page, n)`: page `n`'s object, made when first asked for.
    pub fn page_obj(&mut self, n: i32) -> i32 {
        if let Some(&o) = self.page_obj.get(&n) {
            return o;
        }
        let o = self.new_obj();
        self.page_obj.insert(n, o);
        o
    }

    /// `write_action`.
    fn write_action(&mut self, a: &Action, out: &mut Vec<u8>) {
        let (thread, file, new_window, target) = match a {
            Action::User(s) => {
                toks_ln(out, s);
                return;
            }
            Action::Go { thread, file, new_window, target } => (*thread, file, *new_window, target),
        };
        out.extend_from_slice(b"<< ");
        if let Some(f) = file {
            out.extend_from_slice(b"/F ");
            print_str(out, f);
            out.push(b' ');
            if new_window > 0 {
                out.extend_from_slice(if new_window == 1 { b"/NewWindow true " } else { b"/NewWindow false " });
            }
        }
        match target {
            Target::Page(n, view) => {
                if file.is_none() {
                    out.extend_from_slice(b"/S /GoTo /D [");
                    let p = self.page_obj(*n);
                    reference(out, p);
                } else {
                    out.extend_from_slice(b"/S /GoToR /D [");
                    print_int(out, *n as i64 - 1);
                }
                out.push(b' ');
                latin1(out, view);
                out.push(b']');
            }
            Target::Name(s) => {
                out.extend_from_slice(if thread { b"/S /Thread " } else if file.is_none() { b"/S /GoTo " } else { b"/S /GoToR " });
                if file.is_none() && !thread {
                    self.dest_obj(&DestId::Name(s.clone()));
                }
                out.extend_from_slice(b"/D ");
                print_str(out, s);
            }
            Target::Num(n) => {
                out.extend_from_slice(if thread { b"/S /Thread " } else if file.is_none() { b"/S /GoTo " } else { b"/S /GoToR " });
                out.extend_from_slice(b"/D ");
                if file.is_none() && !thread {
                    let d = self.dest_obj(&DestId::Num(*n));
                    reference(out, d);
                } else {
                    print_int(out, *n as i64);
                }
            }
        }
        out.extend_from_slice(b" >>\n");
    }

    /// The link annotation object of `\pdfstartlink` `link` with its rectangle (left, top,
    /// right, bottom in TeX's page coordinates) on a page `page_h` high: "Write out PDF link
    /// annotations".
    pub fn write_link(&mut self, link: usize, obj: i32, rect: [i32; 4], page_h: i32) {
        let mut out = Vec::new();
        let action = self.links[link].action.clone();
        out.extend_from_slice(b"<<\n/Type /Annot\n");
        let user = matches!(action, Action::User(_));
        if !user {
            out.extend_from_slice(b"/Subtype /Link\n");
        }
        toks_ln(&mut out, &self.links[link].attr);
        let [l, t, r, b] = rect;
        out.extend_from_slice(b"/Rect [");
        for (i, v) in [l as i64, (page_h - b) as i64, r as i64, (page_h - t) as i64].into_iter().enumerate() {
            if i > 0 {
                out.push(b' ');
            }
            print_bp(&mut out, v);
        }
        out.extend_from_slice(b"]\n");
        if !user {
            out.extend_from_slice(b"/A ");
        }
        self.write_action(&action, &mut out);
        out.extend_from_slice(b">>\n");
        self.done.push((obj, out));
    }

    /// `do_link`'s object: a link shipped a second time (a copied box) gets a new one.
    pub fn link_obj(&mut self, link: usize) -> i32 {
        if std::mem::replace(&mut self.links[link].shipped, true) {
            return self.new_obj();
        }
        self.links[link].obj
    }

    /// The object of a further piece of a link broken across lines (`append_link`).
    pub fn new_link_piece(&mut self) -> i32 {
        self.new_obj()
    }

    pub fn link_dims(&self, link: usize) -> [Option<i32>; 3] {
        self.links[link].dims
    }

    pub fn dest_dims(&self, dest: usize) -> [Option<i32>; 3] {
        self.dests[dest].dims
    }

    pub fn dest_is_fitr(&self, dest: usize) -> bool {
        self.dests[dest].view == FITR
    }

    /// `do_dest` and "Write out PDF mark destinations": destination `dest` on page object
    /// `page` at (h, v), or over `rect` for `fitr`; a second destination of one name is
    /// ignored.
    pub fn place_dest(&mut self, dest: usize, page: i32, h: i32, v: i32, rect: [i32; 4], page_h: i32) {
        let id = self.dests[dest].id.clone();
        let obj = self.dest_obj(&id);
        if !self.placed.insert(obj) {
            return;
        }
        let d = &self.dests[dest];
        let named = matches!(id, DestId::Name(_));
        let mut out = Vec::new();
        if named {
            out.extend_from_slice(b"<<\n/D ");
        }
        out.push(b'[');
        reference(&mut out, page);
        out.push(b' ');
        let x = |out: &mut Vec<u8>, h: i32| {
            print_bp(out, h as i64);
        };
        let y = |out: &mut Vec<u8>, v: i32| {
            print_bp(out, (page_h - v) as i64);
        };
        match d.view {
            XYZ => {
                out.extend_from_slice(b"/XYZ ");
                x(&mut out, h);
                out.push(b' ');
                y(&mut out, v);
                out.push(b' ');
                match d.zoom {
                    None => out.extend_from_slice(b"null"),
                    Some(z) => {
                        print_int(&mut out, (z / 1000) as i64);
                        out.push(b'.');
                        print_int(&mut out, (z % 1000) as i64);
                    }
                }
            }
            FIT => out.extend_from_slice(b"/Fit"),
            FITH => {
                out.extend_from_slice(b"/FitH ");
                y(&mut out, v);
            }
            FITV => {
                out.extend_from_slice(b"/FitV ");
                x(&mut out, h);
            }
            FITB => out.extend_from_slice(b"/FitB"),
            FITBH => {
                out.extend_from_slice(b"/FitBH ");
                y(&mut out, v);
            }
            FITBV => {
                out.extend_from_slice(b"/FitBV ");
                x(&mut out, h);
            }
            _ => {
                let [l, t, r, b] = rect;
                out.extend_from_slice(b"/FitR ");
                x(&mut out, l);
                out.push(b' ');
                y(&mut out, b);
                out.push(b' ');
                x(&mut out, r);
                out.push(b' ');
                y(&mut out, t);
            }
        }
        out.extend_from_slice(b"]\n");
        if named {
            out.extend_from_slice(b">>\n");
        }
        self.done.push((obj, out));
    }

    /// `\pdfrefobj n` shipped: the object is written.
    pub fn want_obj(&mut self, n: i32) {
        if let Some(r) = self.raw.iter_mut().find(|r| r.obj == n) {
            r.wanted = true;
        }
    }

    /// The objects written at the end: the rendered annotations and destinations, wanted
    /// raw objects, destinations referred to but never placed (`pdf_fix_dest`), the outline
    /// tree and the name tree, numbered from `self.objects + 1`; with the catalog's extra
    /// entries and the next free object number.
    pub fn finish(&self, first_page: i32, read: &dyn Fn(&str) -> Option<Vec<u8>>, hicolor: bool) -> (Objects, Vec<u8>, i32) {
        let mut objs: Vec<(i32, Vec<u8>)> = self.done.clone();
        let mut next = self.objects + 1;
        self.write_images(read, hicolor, &mut objs, &mut next);
        for r in self.raw.iter().filter(|r| r.wanted) {
            let data = if r.file { read(&r.data).unwrap_or_default() } else { r.data.chars().map(|c| c as u32 as u8).collect() };
            let mut out = Vec::new();
            match &r.stream {
                Some(attr) => {
                    out.extend_from_slice(b"<<\n");
                    toks_ln(&mut out, attr);
                    out.extend_from_slice(b"/Length ");
                    print_int(&mut out, data.len() as i64);
                    out.extend_from_slice(b"\n>>\nstream\n");
                    out.extend_from_slice(&data);
                    out.extend_from_slice(b"\nendstream\n");
                }
                None => {
                    out.extend_from_slice(&data);
                    out.push(b'\n');
                }
            }
            objs.push((r.obj, out));
        }
        for (_, &obj) in self.dest_obj.iter().filter(|(_, o)| !self.placed.contains(o)) {
            let mut out = b"[".to_vec();
            reference(&mut out, first_page);
            out.extend_from_slice(b" /Fit]\n");
            objs.push((obj, out));
        }
        let mut catalog = Vec::new();
        if self.first_outline != NONE {
            let root = next;
            next += 1;
            self.write_outlines(root, &mut objs);
            catalog.extend_from_slice(b"/Outlines ");
            reference(&mut catalog, root);
            catalog.push(b'\n');
        }
        // The name tree: one leaf with every named destination, sorted as `strcmp` sorts.
        let mut names: Vec<(&str, i32)> = self.dest_obj.iter().filter_map(|(k, &o)| if let DestId::Name(s) = k { Some((s.as_str(), o)) } else { None }).collect();
        if !names.is_empty() {
            names.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            let (leaf, dict) = (next, next + 1);
            next += 2;
            let mut out = b"<<\n/Names [".to_vec();
            for (i, (s, o)) in names.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                print_str(&mut out, s);
                out.push(b' ');
                reference(&mut out, *o);
            }
            out.extend_from_slice(b"]\n/Limits [");
            print_str(&mut out, names[0].0);
            out.push(b' ');
            print_str(&mut out, names[names.len() - 1].0);
            out.extend_from_slice(b"]\n>>\n");
            objs.push((leaf, out));
            let mut out = b"<<\n/Dests ".to_vec();
            reference(&mut out, leaf);
            out.extend_from_slice(b"\n>>\n");
            objs.push((dict, out));
            catalog.extend_from_slice(b"/Names ");
            reference(&mut catalog, dict);
            catalog.push(b'\n');
        }
        latin1(&mut catalog, &self.catalog);
        if let Some(a) = &self.open_action {
            let id = next;
            next += 1;
            objs.push((id, a.clone()));
            catalog.extend_from_slice(b"\n/OpenAction ");
            reference(&mut catalog, id);
        }
        objs.sort_unstable_by_key(|o| o.0);
        (objs, catalog, next)
    }

    /// "Output outlines" and "Output PDF outline entries": the counts as `open_subentries`
    /// sets them, the top level's parent the outline root `root`.
    fn write_outlines(&self, root: i32, objs: &mut Vec<(i32, Vec<u8>)>) {
        let o = &self.outlines;
        let mut count: Vec<i32> = o.iter().map(|x| x.count).collect();
        let mut last: Vec<usize> = o.iter().map(|x| x.last).collect();
        fn subentries(o: &[Outline], p: usize, count: &mut [i32], last: &mut [usize]) -> i32 {
            let mut k = 0;
            let mut l = o[p].first;
            while l != NONE {
                k += 1;
                let c = subentries(o, l, count, last);
                if o[l].count > 0 {
                    k += c;
                }
                if o[l].next == NONE {
                    last[p] = l;
                }
                l = o[l].next;
            }
            count[p] = if o[p].count > 0 { k } else { -k };
            k
        }
        let (mut k, mut l, mut top_last) = (0, self.first_outline, self.first_outline);
        while l != NONE {
            k += 1;
            let a = subentries(o, l, &mut count, &mut last);
            if o[l].count > 0 {
                k += a;
            }
            top_last = l;
            l = o[l].next;
        }
        let mut out = b"<<\n/Type /Outlines\n/First ".to_vec();
        reference(&mut out, o[self.first_outline].obj);
        out.extend_from_slice(b"\n/Last ");
        reference(&mut out, o[top_last].obj);
        out.extend_from_slice(b"\n/Count ");
        print_int(&mut out, k as i64);
        out.extend_from_slice(b"\n>>\n");
        objs.push((root, out));
        for (i, x) in o.iter().enumerate() {
            let mut out = b"<<\n/Title ".to_vec();
            print_str(&mut out, &x.title);
            out.extend_from_slice(b"\n/A ");
            out.extend_from_slice(&x.action);
            out.extend_from_slice(b"/Parent ");
            reference(&mut out, if x.parent == NONE { root } else { o[x.parent].obj });
            for (key, v) in [(&b"\n/Prev "[..], x.prev), (b"\n/Next ", x.next), (b"\n/First ", x.first), (b"\n/Last ", last[i])] {
                if v != NONE {
                    out.extend_from_slice(key);
                    reference(&mut out, o[v].obj);
                }
            }
            if count[i] != 0 {
                out.extend_from_slice(b"\n/Count ");
                print_int(&mut out, count[i] as i64);
            }
            out.push(b'\n');
            toks_ln(&mut out, &x.attr);
            out.extend_from_slice(b">>\n");
            objs.push((x.obj, out));
        }
    }

    /// `\pdfoutline`'s linking into the tree: an entry with a count opens a level, and a
    /// level closes when it has as many entries as its parent's count.
    fn add_outline(&mut self, mut x: Outline) {
        let k = self.outlines.len();
        x.parent = self.parent_outline;
        if self.first_outline == NONE {
            self.first_outline = k;
        }
        if self.last_outline == NONE {
            if self.parent_outline != NONE {
                self.outlines[self.parent_outline].first = k;
            }
        } else {
            self.outlines[self.last_outline].next = k;
            x.prev = self.last_outline;
        }
        let count = x.count;
        self.outlines.push(x);
        self.last_outline = k;
        let o = &mut self.outlines;
        let list_count = |o: &[Outline], mut p: usize| {
            let mut n = 1;
            while o[p].prev != NONE {
                n += 1;
                p = o[p].prev;
            }
            n
        };
        if count != 0 {
            self.parent_outline = k;
            self.last_outline = NONE;
        } else if self.parent_outline != NONE && list_count(o, k) == o[self.parent_outline].count.abs() {
            let mut j = self.last_outline;
            loop {
                o[self.parent_outline].last = j;
                j = self.parent_outline;
                self.parent_outline = o[self.parent_outline].parent;
                if self.parent_outline == NONE || list_count(o, j) < o[self.parent_outline].count.abs() {
                    break;
                }
            }
            let mut l = if self.parent_outline == NONE { self.first_outline } else { o[self.parent_outline].first };
            while o[l].next != NONE {
                l = o[l].next;
            }
            self.last_outline = l;
        }
    }
}

impl Machine<'_, '_> {
    /// One pdfTeX extension that builds the document rather than the page.
    pub(super) fn pdf_doc_command(&mut self, p: Prim) {
        match p {
            Prim::PdfInfo => {
                let s = self.general_text_string();
                self.pdf_doc.info.push_str(&s);
                self.pdf_doc.info.push(' ');
            }
            Prim::PdfCatalog => {
                let s = self.general_text_string();
                self.pdf_doc.catalog.push_str(&s);
                self.pdf_doc.catalog.push(' ');
                if self.scan_keyword("openaction") {
                    if let Some(a) = self.scan_action() {
                        let mut out = Vec::new();
                        self.pdf_doc.write_action(&a, &mut out);
                        self.pdf_doc.open_action = Some(out);
                    }
                }
            }
            Prim::PdfDest => {
                let id = if self.scan_keyword("num") {
                    DestId::Num(self.scan_int())
                } else if self.scan_keyword("name") {
                    DestId::Name(self.general_text_string())
                } else {
                    self.error("Identifier type missing");
                    return;
                };
                let (mut view, mut zoom, mut dims) = (FIT, None, [None; 3]);
                if self.scan_keyword("xyz") {
                    view = XYZ;
                    if self.scan_keyword("zoom") {
                        zoom = Some(self.scan_int());
                    }
                } else if self.scan_keyword("fitr") {
                    view = FITR;
                    dims = self.scan_alt_rule();
                } else if let Some(k) = [("fitbh", FITBH), ("fitbv", FITBV), ("fitb", FITB), ("fith", FITH), ("fitv", FITV), ("fit", FIT)].iter().find(|k| self.scan_keyword(k.0)) {
                    view = k.1;
                } else {
                    self.error("Destination type missing");
                }
                self.pdf_doc.dests.push(Dest { id, view, zoom, dims });
                self.cur.push(Node { kind: Kind::Whatsit, sub: DEST, font: 0, a: self.pdf_doc.dests.len() as i32 - 1, b: 0 });
            }
            Prim::PdfObj => self.pdf_obj(false),
            Prim::PdfRefObj => {
                let n = self.scan_int();
                if self.building() {
                    self.cur.push(Node { kind: Kind::Whatsit, sub: REF_OBJ, font: 0, a: n, b: 0 });
                }
            }
            // `\pdfoutline [attr {…}] <action> [count n] {…}`.
            Prim::PdfOutline => {
                let attr = if self.scan_keyword("attr") { self.general_text_string() } else { String::new() };
                let Some(a) = self.scan_action() else { return };
                let count = if self.scan_keyword("count") { self.scan_int() } else { 0 };
                let title = self.general_text_string();
                let mut action = Vec::new();
                self.pdf_doc.write_action(&a, &mut action);
                let obj = self.pdf_doc.new_obj();
                self.pdf_doc.add_outline(Outline { obj, title, action, attr, count, parent: NONE, prev: NONE, next: NONE, first: NONE, last: NONE });
            }
            Prim::PdfStartLink => {
                if self.mode == Mode::Vertical {
                    self.error("\\pdfstartlink cannot be used in vertical mode");
                    return;
                }
                let dims = self.scan_alt_rule();
                let attr = if self.scan_keyword("attr") { self.general_text_string() } else { String::new() };
                let Some(action) = self.scan_action() else { return };
                let obj = self.pdf_doc.new_obj();
                self.pdf_doc.last_link = obj;
                self.pdf_doc.links.push(Link { dims, attr, action, obj, shipped: false });
                self.cur.push(Node { kind: Kind::Whatsit, sub: START_LINK, font: 0, a: self.pdf_doc.links.len() as i32 - 1, b: 0 });
            }
            Prim::PdfEndLink => {
                if self.mode == Mode::Vertical {
                    self.error("\\pdfendlink cannot be used in vertical mode");
                    return;
                }
                self.cur.push(Node { kind: Kind::Whatsit, sub: END_LINK, font: 0, a: 0, b: 0 });
            }
            _ => {}
        }
    }

    /// `\pdfobj reserveobjnum`, or `[useobjnum n] [stream [attr {…}]] [file] {…}`; with
    /// `\immediate` the object is written whether or not a page refers to it.
    pub(super) fn pdf_obj(&mut self, immediate: bool) {
        if self.scan_keyword("reserveobjnum") {
            self.pdf_doc.last_obj = self.pdf_doc.new_obj();
            return;
        }
        let mut n = if self.scan_keyword("useobjnum") { self.scan_int() } else { -1 };
        if n <= 0 || n > self.pdf_doc.objects || self.pdf_doc.raw.iter().any(|r| r.obj == n) {
            n = self.pdf_doc.new_obj();
        }
        let stream = if self.scan_keyword("stream") {
            Some(if self.scan_keyword("attr") { self.general_text_string() } else { String::new() })
        } else {
            None
        };
        let file = self.scan_keyword("file");
        let data = self.general_text_string();
        self.pdf_doc.raw.push(RawObj { obj: n, data, stream, file, wanted: immediate });
        self.pdf_doc.last_obj = n;
    }

    /// pdfTeX's `scan_alt_rule`: `width`, `height` and `depth` in any order; `None` running.
    pub(super) fn scan_alt_rule(&mut self) -> [Option<i32>; 3] {
        let mut dims = [None; 3];
        loop {
            let k = if self.scan_keyword("width") {
                0
            } else if self.scan_keyword("height") {
                1
            } else if self.scan_keyword("depth") {
                2
            } else {
                return dims;
            };
            dims[k] = Some(self.scan_dimen(false).0);
        }
    }

    /// pdfTeX's `scan_action`: `user {…}`, `goto …` or `thread …`.
    fn scan_action(&mut self) -> Option<Action> {
        if self.scan_keyword("user") {
            return Some(Action::User(self.general_text_string()));
        }
        let goto = self.scan_keyword("goto");
        if !goto && !self.scan_keyword("thread") {
            self.error("Action type missing");
            return None;
        }
        let file = if self.scan_keyword("file") { Some(self.general_text_string()) } else { None };
        let target = if goto && self.scan_keyword("page") {
            let n = self.scan_int();
            Target::Page(n, self.general_text_string())
        } else if self.scan_keyword("name") {
            Target::Name(self.general_text_string())
        } else if self.scan_keyword("num") {
            Target::Num(self.scan_int())
        } else {
            self.error("Identifier type missing");
            return None;
        };
        let new_window = if file.is_some() {
            if self.scan_keyword("newwindow") {
                1
            } else if self.scan_keyword("nonewwindow") {
                2
            } else {
                0
            }
        } else {
            0
        };
        Some(Action::Go { thread: !goto, file, new_window, target })
    }
}
