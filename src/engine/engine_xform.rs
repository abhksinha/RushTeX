//! pdfTeX's form XObjects: `\pdfxform`, `\pdfrefxform`, `\pdflastxform` (pdftex.web
//! `Implement \pdfxform`, `out_form`, `pdf_ship_out` of a form), in our own form.
//!
//! `\pdfxform` takes the box out of its register and keeps the box's record (the node
//! store is never freed), its size and its `attr` and `resources` text. The form is shipped
//! (its content stream walked like a page's, with the colour stacks' form side) when
//! `\immediate` asks, else at the end of the first page that shows it, and the forms it
//! shows after it (pdfTeX's `Write out pending forms`). `\pdfrefxform` puts an
//! [`XFORM`] whatsit with the form's size on the list, the form in `b`.

use super::engine_node::{Kind, Node};
use super::engine_run::Machine;
use super::engine_whatsit::XFORM;

/// One `\pdfxform`: its object, box, size, `attr` and `resources` text, and whether it was
/// shipped.
#[derive(Debug, Clone)]
pub struct XForm {
    pub obj: i32,
    pub box_rec: u32,
    pub dims: [i32; 3],
    pub attr: String,
    pub resources: String,
    pub shipped: bool,
}

impl Machine<'_, '_> {
    /// The form commands among the long-tail primitives; `false` for any other.
    pub(super) fn xform_command(&mut self, name: &str, immediate: bool) -> bool {
        match name {
            "pdfxform" => self.pdf_xform(immediate),
            "pdfrefxform" if !immediate => self.pdf_refxform(),
            _ => return false,
        }
        true
    }

    /// `\pdfxform [attr {…}] [resources {…}] n`.
    fn pdf_xform(&mut self, immediate: bool) {
        let attr = if self.scan_keyword("attr") { self.general_text_string() } else { String::new() };
        let resources = if self.scan_keyword("resources") { self.general_text_string() } else { String::new() };
        let n = self.scan_box_num();
        let Some(b) = self.e.eqtb.box_reg(n) else {
            self.error("pdfTeX error (ext1): \\pdfxform cannot be used with a void box");
            return;
        };
        let bx = &self.e.nodes.boxes[b as usize];
        let dims = [bx.width, bx.height, bx.depth];
        // `change_box(null)`.
        self.e.eqtb.replace_box(n, None);
        let obj = self.pdf_doc.new_obj();
        self.pdf_doc.forms.push(XForm { obj, box_rec: b, dims, attr, resources, shipped: false });
        self.pdf_doc.last_xform = obj;
        if immediate {
            self.ship_form(self.pdf_doc.forms.len() - 1);
        }
    }

    /// `\pdfrefxform n`: a whatsit of the form's size.
    fn pdf_refxform(&mut self) {
        let n = self.scan_int();
        let Some(k) = self.pdf_doc.forms.iter().position(|f| f.obj == n) else {
            self.error("pdfTeX error (ext1): cannot find referenced object");
            return;
        };
        if self.building() {
            self.e.nodes.rules.push(self.pdf_doc.forms[k].dims);
            let a = self.e.nodes.rules.len() as i32 - 1;
            self.cur.push(Node { kind: Kind::Whatsit, sub: XFORM, font: 0, a, b: k as i32 });
        }
    }
}
