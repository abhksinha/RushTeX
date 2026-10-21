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
use crate::backend::backend_png::{png_info, png_to_pdf, ColorSpace};

/// `\pdfimageresolution`, `\pdfimagehicolor` and `\pdfminorversion` (§236 slots).
const PDF_IMAGE_RESOLUTION: u32 = 60;
const PDF_IMAGE_HICOLOR: u32 = 73;
const PDF_MINOR_VERSION: u32 = 67;
/// pdfTeX's `one_hundred_inch` (100in in sp) and `one_inch` (1in over `max_dimen`).
const ONE_HUNDRED_INCH: i64 = 473_628_672;
const ONE_INCH: f64 = 226.0;

/// One `\pdfximage`: its object, file, size, `attr` text, the transparency group
/// of the pages that show it (0 for none), and whether a shipped page showed it.
#[derive(Debug, Clone)]
pub struct XImage {
    pub obj: i32,
    file: String,
    pub dims: [i32; 3],
    attr: String,
    pub group: i32,
    pub wanted: bool,
}

/// `scale_image`: the size of an image of `x`×`y` pixels at `xr`×`yr` dpi, with
/// `\pdfimageresolution` `default_res` and the dimensions given (`None` running).
fn scale_image(x: i64, y: i64, mut xr: i64, mut yr: i64, default_res: i64, given: [Option<i32>; 3]) -> [i32; 3] {
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
    let (w, h) = if xr > 0 && yr > 0 {
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

impl Machine<'_, '_> {
    /// `scan_image`: `\pdfximage [<rule spec>] [attr {…}] [named {…}|page n] [colorspace n]
    /// [<box spec>] {file}`; with `\immediate` the image is written even if no page shows it.
    pub(super) fn pdf_ximage(&mut self, immediate: bool) {
        let given = self.scan_alt_rule();
        let attr = if self.scan_keyword("attr") { self.general_text_string() } else { String::new() };
        if self.scan_keyword("named") {
            self.general_text_string();
        } else if self.scan_keyword("page") {
            self.scan_int();
        }
        if self.scan_keyword("colorspace") {
            self.scan_int();
        }
        for kw in ["mediabox", "cropbox", "bleedbox", "trimbox", "artbox"] {
            if self.scan_keyword(kw) {
                break;
            }
        }
        let file = self.general_text_string();
        let Some(info) = self.open_bytes(&file).as_deref().and_then(png_info) else {
            self.error(format!("pdfTeX error (ext1): cannot read image file `{file}'"));
            return;
        };
        let res = self.image_param(PDF_IMAGE_RESOLUTION).clamp(0, 65535) as i64;
        let dims = scale_image(info.width as i64, info.height as i64, info.x_res as i64, info.y_res as i64, res, given);
        let doc = &mut self.pdf_doc;
        let obj = doc.new_obj();
        // `read_png_info`: an image with alpha in PDF 1.4 or later needs a transparency
        // group on its pages, one object for the whole document.
        let group = if info.alpha {
            if doc.group_obj == 0 {
                doc.group_obj = doc.new_obj();
            }
            doc.group_obj
        } else {
            0
        };
        doc.images.push(XImage { obj, file, dims, attr, group, wanted: immediate });
        doc.last_ximage = obj;
    }

    /// `\pdfrefximage n`: a whatsit of the image's size.
    pub(super) fn pdf_refximage(&mut self) {
        let n = self.scan_int();
        let Some(k) = self.pdf_doc.images.iter().position(|i| i.obj == n) else {
            self.error("pdfTeX error (ext1): cannot find referenced object");
            return;
        };
        if self.building() {
            self.e.nodes.rules.push(self.pdf_doc.images[k].dims);
            let a = self.e.nodes.rules.len() as i32 - 1;
            self.cur.push(Node { kind: Kind::Whatsit, sub: XIMAGE, font: 0, a, b: k as i32 });
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
    /// from `*next`; and the transparency group.
    pub fn write_images(&self, read: &dyn Fn(&str) -> Option<Vec<u8>>, hicolor: bool, objs: &mut Vec<(i32, Vec<u8>)>, next: &mut i32) {
        let mut deflate = DeflateCompressor::new();
        for im in self.images.iter().filter(|i| i.wanted) {
            let Some(img) = read(&im.file).and_then(|b| png_to_pdf(&b, hicolor)) else { continue };
            let smask = img.smask.as_ref().map(|_| {
                *next += 1;
                *next - 1
            });
            let stream = |deflate: &mut DeflateCompressor, extra: &[u8], space: &[u8], data: &[u8]| {
                let mut packed = Vec::new();
                deflate.compress_zlib(data, &mut packed);
                let mut out = b"<<\n".to_vec();
                out.extend_from_slice(extra);
                out.extend_from_slice(b"/Type /XObject\n/Subtype /Image\n/Width ");
                print_int(&mut out, img.width as i64);
                out.extend_from_slice(b"\n/Height ");
                print_int(&mut out, img.height as i64);
                out.extend_from_slice(b"\n/BitsPerComponent ");
                print_int(&mut out, img.bits as i64);
                out.extend_from_slice(b"\n/ColorSpace ");
                out.extend_from_slice(space);
                out.extend_from_slice(b"\n/Length ");
                print_int(&mut out, packed.len() as i64);
                out.extend_from_slice(b"\n/Filter /FlateDecode\n>>\nstream\n");
                out.extend_from_slice(&packed);
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
            objs.push((im.obj, stream(&mut deflate, &extra, &space, &img.data)));
            if let (Some(m), Some(mask)) = (smask, &img.smask) {
                objs.push((m, stream(&mut deflate, b"", b"/DeviceGray", mask)));
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
        let [w, h, d] = scale_image(833, 286, 0, 0, 72, [None; 3]);
        assert_eq!((w, h, d), (54_796_206, 18_813_583, 0));
        // A given width scales the height by the aspect ratio.
        assert_eq!(scale_image(100, 50, 0, 0, 72, [Some(1000), None, None]), [1000, 500, 0]);
    }
}
