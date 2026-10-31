//! pdfTeX's raster images: `\pdfximage`, `\pdfrefximage`, `\pdflastximage` (pdftex.web
//! `scan_image`, `scale_image`, `out_image`; writepng.c), in our own form.
//!
//! `\pdfximage` reads only the file's header, takes an object number and sizes the image;
//! `\pdfrefximage` puts a [`XIMAGE`] whatsit on the list, its width, height and depth in a
//! rule record (`a`) so packing, line breaking and page building see them like a rule's,
//! and the image in `b`. The pixels are decoded and written when the document is finished,
//! for the images a shipped page showed (pdfTeX's `pdf_ximage_list`).

use super::engine_node::{Kind, Node};
use super::engine_pdfdoc::PdfDoc;
use super::engine_run::Machine;
use super::engine_whatsit::XIMAGE;
use crate::backend::backend_deflate::DeflateCompressor;
use crate::backend::backend_pdftex::{ext_xn_over_d, print_int};
use crate::backend::backend_pdfread::{write_form, FormSpec, PdfFile};
use crate::backend::backend_png::{png_info, png_to_pdf, ColorSpace};

/// `\pdfimageresolution`, `\pdfimagehicolor` and `\pdfminorversion` (§236 slots).
const PDF_IMAGE_RESOLUTION: u32 = 60;
const PDF_IMAGE_HICOLOR: u32 = 73;
const PDF_MINOR_VERSION: u32 = 67;
/// `\pdfoptionalwaysusepdfpagebox` and `\pdfpagebox`.
const PDF_ALWAYS_PAGE_BOX: u32 = 64;
const PDF_PAGE_BOX: u32 = 69;
/// pdfTeX's `one_hundred_inch` (100in in sp) and `one_inch` (1in over `max_dimen`).
const ONE_HUNDRED_INCH: i64 = 473_628_672;
const ONE_INCH: f64 = 226.0;

/// One `\pdfximage`: its object, file, size, `attr` text, the transparency group
/// of the pages that show it (0 for none), and whether a shipped page showed it.
#[derive(PartialEq, Debug, Clone)]
pub struct XImage {
    pub obj: i32,
    file: String,
    pub dims: [i32; 3],
    attr: String,
    pub group: i32,
    pub wanted: bool,
    /// A page of a PDF file, written as a form XObject.
    pub pdf: Option<PdfPage>,
}

/// An included PDF page: its number, the page box (0 media to 4 art), its size and the
/// box's lower left corner in sp (writeimg.c `bp2int`), and `/Rotate`.
#[derive(PartialEq, Debug, Clone)]
pub struct PdfPage {
    page: i64,
    page_box: usize,
    pub size: [i32; 2],
    pub orig: [i32; 2],
    pub rotate: i64,
}

/// writeimg.c `bp2int`: big points (a C `float`) to sp, C `round`.
fn bp2int(p: f32) -> i32 {
    (p as f64 * (6578176.0 / 100.0)).round() as i32
}

/// `scale_image`: the size of an image of `x`×`y` pixels at `xr`×`yr` dpi, with
/// `\pdfimageresolution` `default_res` and the dimensions given (`None` running).
fn scale_image(x: i64, y: i64, mut xr: i64, mut yr: i64, default_res: i64, given: [Option<i32>; 3], pdf: bool) -> [i32; 3] {
    if xr > 65535 || yr > 65535 {
        xr = 0;
        yr = 0;
    }
    if (xr != 0 || yr != 0) && (x as f64 / ONE_INCH >= xr as f64 || y as f64 / ONE_INCH >= yr as f64) {
        xr = 0;
        yr = 0;
    }
    if default_res > 0 && (xr == 0 || yr == 0) {
        xr = default_res;
        yr = default_res;
    }
    // A PDF page's size is its box's, already in sp.
    let (w, h) = if pdf {
        (x, y)
    } else if xr > 0 && yr > 0 {
        (ext_xn_over_d(ONE_HUNDRED_INCH, x, 100 * xr), ext_xn_over_d(ONE_HUNDRED_INCH, y, 100 * yr))
    } else {
        (ext_xn_over_d(ONE_HUNDRED_INCH, x, 7200), ext_xn_over_d(ONE_HUNDRED_INCH, y, 7200))
    };
    let xn = |a: i64, n: i64, d: i64| ext_xn_over_d(a, n, d) as i32;
    match given {
        [None, None, None] => [w as i32, h as i32, 0],
        [None, None, Some(dp)] => [xn(h, x, y), h as i32 - dp, dp],
        [None, Some(ht), None] => [xn(ht as i64, x, y), ht, 0],
        [None, Some(ht), Some(dp)] => [xn(ht as i64 + dp as i64, x, y), ht, dp],
        [Some(wd), None, None] => [wd, xn(wd as i64, y, x), 0],
        [Some(wd), None, Some(dp)] => [wd, xn(wd as i64, y, x) - dp, dp],
        [Some(wd), Some(ht), None] => [wd, ht, 0],
        [Some(wd), Some(ht), Some(dp)] => [wd, ht, dp],
    }
}

/// writeimg.c `read_pdf_info`: the file's page count and page `page`'s box `page_box`.
fn pdf_page_info(bytes: Vec<u8>, page: i64, page_box: usize) -> Option<(i32, PdfPage)> {
    let mut f = PdfFile::open(bytes)?;
    f.load_object_streams();
    let (p, _) = f.page(page)?;
    let [x1, y1, x2, y2] = p.boxes[page_box];
    let size = [bp2int(x2 - x1), bp2int(y2 - y1)];
    Some((f.page_count() as i32, PdfPage { page, page_box, size, orig: [bp2int(x1), bp2int(y1)], rotate: p.rotate }))
}

impl Machine<'_, '_> {
    /// `scan_image`: `\pdfximage [<rule spec>] [attr {…}] [named {…}|page n] [colorspace n]
    /// [<box spec>] {file}`; with `\immediate` the image is written even if no page shows it.
    pub(super) fn pdf_ximage(&mut self, immediate: bool) {
        let given = self.scan_alt_rule();
        let attr = if self.scan_keyword("attr") { self.general_text_string() } else { String::new() };
        let mut page = 1;
        if self.scan_keyword("named") {
            self.general_text_string();
        } else if self.scan_keyword("page") {
            page = self.scan_int() as i64;
        }
        if self.scan_keyword("colorspace") {
            self.scan_int();
        }
        // pdftex.web `scan_image`: the box asked for, else `\pdfpagebox`, else the crop box;
        // `\pdfoptionalwaysusepdfpagebox` overrides.
        let mut page_box = ["mediabox", "cropbox", "bleedbox", "trimbox", "artbox"].iter().position(|kw| self.scan_keyword(kw));
        let param = |v: i32| (1..=5).contains(&v).then(|| v as usize - 1);
        if let Some(b) = param(self.image_param(PDF_ALWAYS_PAGE_BOX)) {
            page_box = Some(b);
        }
        let page_box = page_box.or_else(|| param(self.image_param(PDF_PAGE_BOX))).unwrap_or(1);
        let file = self.general_text_string();
        self.image_file(file, attr, given, page, page_box, immediate);
    }

    /// `scan_image` after its keywords: image `file` read and numbered; its index in
    /// `pdf_doc.images`, or `None` (with pdfTeX's error) when it cannot be read.
    pub(super) fn image_file(&mut self, file: String, attr: String, given: [Option<i32>; 3], page: i64, page_box: usize, immediate: bool) -> Option<usize> {
        let bytes = self.open_bytes(&file);
        let res = self.image_param(PDF_IMAGE_RESOLUTION).clamp(0, 65535) as i64;
        let (dims, alpha, pdf) = match bytes {
            Some(b) if b.starts_with(b"%PDF") => {
                let Some(p) = pdf_page_info(b, page, page_box) else {
                    self.error(format!("pdfTeX error (ext1): cannot read image file `{file}'"));
                    return None;
                };
                let (pages, p) = p;
                self.pdf_doc.last_ximage_pages = pages;
                let [w, h] = if p.rotate == 90 || p.rotate == 270 { [p.size[1], p.size[0]] } else { p.size };
                (scale_image(w as i64, h as i64, 0, 0, res, given, true), false, Some(p))
            }
            b => {
                let Some(info) = b.as_deref().and_then(png_info) else {
                    self.error(format!("pdfTeX error (ext1): cannot read image file `{file}'"));
                    return None;
                };
                self.pdf_doc.last_ximage_pages = 1;
                (scale_image(info.width as i64, info.height as i64, info.x_res as i64, info.y_res as i64, res, given, false), info.alpha, None)
            }
        };
        let doc = &mut self.pdf_doc;
        let obj = doc.new_obj();
        // `read_png_info`: an image with alpha in PDF 1.4 or later needs a transparency
        // group on its pages, one object for the whole document.
        let group = if alpha {
            if doc.group_obj == 0 {
                doc.group_obj = doc.new_obj();
            }
            doc.group_obj
        } else {
            0
        };
        doc.images.push(XImage { obj, file, dims, attr, group, wanted: immediate, pdf });
        doc.last_ximage = obj;
        Some(doc.images.len() - 1)
    }

    /// Image `k` of `pdf_doc.images` as a `\pdfrefximage` whatsit on the current list.
    pub(super) fn ref_image(&mut self, k: usize) {
        let a = self.e.nodes.add_rule(self.pdf_doc.images[k].dims) as i32;
        self.cur.push(Node { kind: Kind::Whatsit, sub: XIMAGE, font: 0, a, b: k as i32 });
    }

    /// `\pdfrefximage n`: a whatsit of the image's size.
    pub(super) fn pdf_refximage(&mut self) {
        let n = self.scan_int();
        let Some(k) = self.pdf_doc.images.iter().position(|i| i.obj == n) else {
            self.error("pdfTeX error (ext1): cannot find referenced object");
            return;
        };
        if self.building() {
            self.ref_image(k);
        }
    }

    fn image_param(&self, slot: u32) -> i32 {
        self.e.eqtb.word(super::engine_eqtb::Reg { region: super::engine_eqtb::Region::Int, slot })
    }

    /// `\pdfimagehicolor` takes effect from PDF 1.5.
    pub(super) fn image_hicolor(&self) -> bool {
        self.image_param(PDF_MINOR_VERSION) >= 5 && self.image_param(PDF_IMAGE_HICOLOR) > 0
    }
}

impl PdfDoc {
    /// The images shown, as `write_png` writes them: the image, then its soft mask, numbered
    /// from `*next`; and the transparency group. With a `--watch` session's `cache`, a PNG
    /// whose file bytes (and `\pdfimagehicolor`) an earlier recompile wrote is not decoded and
    /// compressed again; the file is still read, so it stays one the session watches.
    pub fn write_images(&self, read: &dyn Fn(&str) -> Option<Vec<u8>>, hicolor: bool, objs: &mut Vec<(i32, Vec<u8>)>, next: &mut i32, cache: Option<&crate::backend::backend_cache::PdfCache>) {
        let mut deflate = DeflateCompressor::new();
        for im in self.images.iter().filter(|i| i.wanted) {
            if let Some(p) = &im.pdf {
                let Some(mut f) = read(&im.file).and_then(PdfFile::open) else { continue };
                f.load_object_streams();
                let spec = FormSpec { page: p.page, page_box: p.page_box, attr: im.attr.as_bytes(), file_name: &im.file };
                write_form(&f, &spec, im.obj, objs, next);
                continue;
            }
            let Some(bytes) = read(&im.file) else { continue };
            let key = cache.map(|_| {
                let mut h = crate::backend::backend_hash::ContentHasher::new();
                h.u64(3);
                h.u64(hicolor as u64);
                h.bytes(&bytes);
                h.finish()
            });
            let hit = match (cache, key) {
                (Some(c), Some(k)) => c.image(k),
                _ => None,
            };
            let made = match hit {
                Some(made) => made,
                None => {
                    let Some(img) = png_to_pdf(&bytes, hicolor) else { continue };
                    let pack = |deflate: &mut DeflateCompressor, data: &[u8]| {
                        let mut packed = Vec::new();
                        deflate.compress_zlib(data, &mut packed);
                        packed
                    };
                    let space: Vec<u8> = match &img.space {
                        ColorSpace::Gray => b"/DeviceGray".to_vec(),
                        ColorSpace::Rgb => b"/DeviceRGB".to_vec(),
                        ColorSpace::Indexed(pal) => {
                            let mut s = b"[/Indexed /DeviceRGB ".to_vec();
                            print_int(&mut s, pal.len() as i64 / 3 - 1);
                            s.extend_from_slice(b" <");
                            for b in pal {
                                s.extend_from_slice(format!("{b:02X}").as_bytes());
                            }
                            s.extend_from_slice(b">]");
                            s
                        }
                    };
                    let data = pack(&mut deflate, &img.data);
                    let mask = img.smask.as_ref().map(|m| pack(&mut deflate, m));
                    let made = std::rc::Rc::new((img.width, img.height, img.bits, space, data, mask));
                    if let (Some(c), Some(k)) = (cache, key) {
                        c.put_image(k, made.clone());
                    }
                    made
                }
            };
            let (width, height, bits, ref space, ref data, ref mask) = *made;
            let smask = mask.as_ref().map(|_| {
                *next += 1;
                *next - 1
            });
            let stream = |extra: &[u8], space: &[u8], packed: &[u8]| {
                let mut out = b"<<\n".to_vec();
                out.extend_from_slice(extra);
                out.extend_from_slice(b"/Type /XObject\n/Subtype /Image\n/Width ");
                print_int(&mut out, width as i64);
                out.extend_from_slice(b"\n/Height ");
                print_int(&mut out, height as i64);
                out.extend_from_slice(b"\n/BitsPerComponent ");
                print_int(&mut out, bits as i64);
                out.extend_from_slice(b"\n/ColorSpace ");
                out.extend_from_slice(space);
                out.extend_from_slice(b"\n/Length ");
                print_int(&mut out, packed.len() as i64);
                out.extend_from_slice(b"\n/Filter /FlateDecode\n>>\nstream\n");
                out.extend_from_slice(packed);
                out.extend_from_slice(b"\nendstream\n");
                out
            };
            let mut extra = Vec::new();
            if !im.attr.is_empty() {
                extra.extend_from_slice(im.attr.as_bytes());
                extra.push(b'\n');
            }
            if let Some(m) = smask {
                extra.extend_from_slice(b"/SMask ");
                print_int(&mut extra, m as i64);
                extra.extend_from_slice(b" 0 R\n");
            }
            objs.push((im.obj, stream(&extra, space, data)));
            if let (Some(m), Some(mask)) = (smask, mask) {
                objs.push((m, stream(b"", b"/DeviceGray", mask)));
            }
        }
        if self.group_obj != 0 && self.images.iter().any(|i| i.wanted && i.group != 0) {
            objs.push((self.group_obj, b"<</Type/Group /S/Transparency /CS/DeviceRGB /I true>>\n".to_vec()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_image_follows_pdftex() {
        // 833×286 pixels, no pHYs, `\pdfimageresolution` 72: 1bp per pixel.
        let [w, h, d] = scale_image(833, 286, 0, 0, 72, [None; 3], false);
        assert_eq!((w, h, d), (54_796_206, 18_813_583, 0));
        // A given width scales the height by the aspect ratio.
        assert_eq!(scale_image(100, 50, 0, 0, 72, [Some(1000), None, None], false), [1000, 500, 0]);
    }
}
