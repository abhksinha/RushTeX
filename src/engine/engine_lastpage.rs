//! lastpage.sty (2025-08-14 v2.1h, `lastpagemodern.sty`) natively: `\pageref{LastPage}` in
//! one pass.
//!
//! The package writes `\newlabel{LastPage}{{}{\thepage}..}` with `\immediate\write` in the
//! `enddocument/afterlastpage` hook (`\lastpage@putl@bel`), so pdfLaTeX's last run prints the
//! number of the last page the run before shipped; it leaves nothing in the page lists. The
//! native job types a stand-in digit `0` after a marker whatsit ([`LASTPAGE`]) and keeps the
//! pages (as a contents list does, `finish_contents`); at the end each marker goes and its
//! digit becomes the last page's `\thepage`. That changes no width, so no glue set changes
//! and nothing is packed again, only when `\thepage` is one digit with the stand-in's width,
//! height, depth and italic correction, and the digit has no character next to it (no
//! ligature or kern it could take part in). Anything else declines.

use super::super::engine_node::{Kind, Node};
use super::super::engine_run::{Machine, Mode};
use super::super::engine_whatsit::LASTPAGE;
use super::super::engine_catcode::Cat;
use super::super::engine_token::Token;
use super::book;
use super::label_tokens;

/// lastpage's natives in `DOC` (`DOC_PARTS`).
pub(super) const NATIVES: &[(&str, super::Native)] = &[("rt@lastpage", |m| m.lastpage_mark())];

/// The stand-in digit.
const STAND_IN: u8 = b'0';

impl Machine<'_, '_> {
    /// `\pageref{LastPage}` with lastpage loaded: the marker and the stand-in, then `\null`
    /// (`\@setref`'s `\hbox{}`), as `page_reference` types any page.
    pub(super) fn lastpage_ref(&mut self) {
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.packages.contains(&"microtype") {
            return self.not_native("\\pageref{LastPage} with microtype");
        }
        doc.used.insert("LastPage".into());
        // hyperref's `\pageref`: the number in a link to the label's anchor, lastpage's
        // `\@currentHpage` of the last page (`page.N`), and no `\null`. The link is made to a
        // stand-in name and retargeted with the digit.
        if doc.packages.contains(&"hyperref") {
            doc.link = Some((super::hyperref::LINK, "page.0".into()));
            let mut list = vec![self.doc_id(super::LEAVE), self.doc_id(super::START_LINK_NATIVE), self.named_cs(named!("begingroup")), self.doc_id(doc!("rt@lastpage"))];
            list.extend(label_tokens(&(STAND_IN as char).to_string()));
            list.extend([self.named_cs(named!("endgroup")), self.doc_id(super::END_LINK_NATIVE)]);
            return self.back_list(&list);
        }
        let mut v = vec![self.doc_id(doc!("rt@lastpage"))];
        v.extend(label_tokens(&(STAND_IN as char).to_string()));
        v.extend([self.named_cs(named!("hbox")), Token::char(Cat::BeginGroup, '{'), Token::char(Cat::EndGroup, '}')]);
        self.back_list(&v);
    }

    /// `rt@lastpage`: the marker, in a paragraph (the digit after it would start one).
    pub(super) fn lastpage_mark(&mut self) {
        if self.leave_vmode() {
            return;
        }
        if self.mode != Mode::Horizontal {
            return self.not_native("\\pageref{LastPage} in math");
        }
        self.cur.push(Node { kind: Kind::Whatsit, sub: LASTPAGE, font: 0, a: 0, b: 0 });
        let link = self.pdf_doc.link_count().checked_sub(1);
        if let Some(doc) = self.doc.as_mut() {
            doc.pkg.lastpage += 1;
            if doc.packages.contains(&"hyperref") {
                match link {
                    Some(i) => doc.pkg.lastpage_links.push(i),
                    None => self.not_native("\\pageref{LastPage} without its link"),
                }
            }
        }
    }

    /// After the text, before the kept pages ship: every marker out of its list and its
    /// stand-in made the last page's number.
    pub(super) fn lastpage_fill(&mut self) -> Result<(), String> {
        let Some(doc) = self.doc.as_ref() else { return Ok(()) };
        if doc.pkg.lastpage == 0 {
            return Ok(());
        }
        let text = book::page_text(book::page_value(doc.page - 1, doc.page_style));
        let decline = |why: &str| Err(format!("not native yet: \\pageref{{LastPage}} {why}"));
        let &[digit] = text.as_bytes() else { return decline("of more than one digit") };
        if !digit.is_ascii_digit() {
            return decline("of a page not numbered in arabic");
        }
        let (want, roots) = (doc.pkg.lastpage, doc.deferred.clone());
        let mut found = 0;
        let mut stack: Vec<u32> = roots.into_iter().rev().collect();
        while let Some(g) = stack.pop() {
            let old = self.e.nodes.boxes[g as usize];
            let list = self.e.nodes.get(old.list);
            stack.extend(list.iter().rev().filter(|n| n.kind == Kind::Box).map(|n| n.a as u32));
            if !list.iter().any(|n| n.kind == Kind::Whatsit && n.sub == LASTPAGE) {
                continue;
            }
            let mut new: Vec<Node> = Vec::with_capacity(list.len());
            let mut k = 0;
            while k < list.len() {
                let n = list[k];
                k += 1;
                if !(n.kind == Kind::Whatsit && n.sub == LASTPAGE) {
                    new.push(n);
                    continue;
                }
                let textual = |n: Option<&Node>| n.is_some_and(|n| matches!(n.kind, Kind::Char | Kind::Lig | Kind::Word));
                let Some(&c) = list.get(k).filter(|c| c.kind == Kind::Char && c.a == STAND_IN as i32) else { return decline("not followed by its digit") };
                if textual(new.last()) || textual(list.get(k + 1)) {
                    return decline("next to a character");
                }
                let f = c.font;
                if self.e.fonts.dims(f, STAND_IN) != self.e.fonts.dims(f, digit) || self.e.fonts.dims(f, digit).is_none() {
                    return decline("in a font whose digits differ");
                }
                new.push(Node { a: digit as i32, ..c });
                k += 1;
                found += 1;
            }
            let l = self.e.nodes.store(&new);
            self.e.nodes.free_block(old.list);
            self.e.nodes.boxes[g as usize].list = l;
        }
        if found != want {
            return decline("outside the kept pages");
        }
        // hyperref's anchor of the last page (`\Hy@EveryPageAnchor`'s `page.N`).
        let links = self.doc.as_mut().map(|d| std::mem::take(&mut d.pkg.lastpage_links)).unwrap_or_default();
        let name = format!("page.{}", digit as char);
        for i in links {
            if !self.pdf_doc.retarget_link(i, &name) {
                return decline("with a link already out");
            }
        }
        Ok(())
    }
}
