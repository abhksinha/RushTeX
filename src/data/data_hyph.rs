//! The hyphenation of `\language` 0 (US English, `hyphen.tex`) as data (P2.2,
//! `2026-10-23-p2-native-document-path.md`): what the native document job hyphenates with,
//! in place of the patterns and exceptions a format carries.
//!
//! `rushtex-gen class` reads them from the oracle's format (`engine_hyph::Hyph::language`),
//! writes this table and checks that it reads back to the same patterns and exceptions.
//!
//! Sections:
//! - `patterns`: the language's pattern records exactly as the engine keeps them (`k`, the
//!   `k` letters, the `k+1` digits two to a byte);
//! - `except`: the exceptions, each the letters' count, the letters, the positions' count
//!   and the positions;
//! - `tlo`, `thi`, `tpool`: the patterns' packed trie, its slots' low and high halves and
//!   its op pool, built here (`engine_hyph::Hyph::trie_parts`) so the job does not sort,
//!   build and pack it per document.

use super::data_blob::{Blob, U32s};

/// The trie's sections, in `Hyph::trie_parts`'s order.
const TRIE: [&str; 3] = ["tlo", "thi", "tpool"];

pub struct HyphTable<'a> {
    pub patterns: &'a [u8],
    except: &'a [u8],
    pub trie: Option<[U32s<'a>; 3]>,
}

impl<'a> HyphTable<'a> {
    pub fn from_blob(b: &Blob<'a>) -> Option<HyphTable<'a>> {
        let trie = TRIE.iter().map(|n| b.u32s(n)).collect::<Option<Vec<_>>>().and_then(|v| v.try_into().ok());
        let t = HyphTable { patterns: b.section("patterns")?, except: b.section("except")?, trie };
        // Every entry in bounds, once, so `exceptions` never stops early.
        let mut rest = t.except;
        while !rest.is_empty() {
            let k = *rest.first()? as usize;
            let n = *rest.get(1 + k)? as usize;
            rest = rest.get(2 + k + n..)?;
        }
        Some(t)
    }

    /// The exceptions: each word's letters (as `\lccode`s) and its hyphen positions.
    pub fn exceptions(&self) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + '_ {
        let mut rest = self.except;
        std::iter::from_fn(move || {
            let k = *rest.first()? as usize;
            let n = *rest.get(1 + k)? as usize;
            let entry = (rest.get(1..1 + k)?, rest.get(2 + k..2 + k + n)?);
            rest = &rest[2 + k + n..];
            Some(entry)
        })
    }
}

static ENGLISH_BLOB: &[u8] = include_bytes!("../../assets/tables/hyph-en.rtdb");

/// Language 0, checked on first use; `None` if it is not a table of this build.
pub fn english() -> Option<&'static HyphTable<'static>> {
    static TABLE: std::sync::OnceLock<Option<HyphTable<'static>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Blob::new(ENGLISH_BLOB).and_then(|b| HyphTable::from_blob(&b))).as_ref()
}

#[cfg(any(test, feature = "gen"))]
pub fn write<'e>(w: &mut super::data_blob::BlobWriter, patterns: &[u8], exceptions: impl Iterator<Item = (&'e [u8], &'e [u8])>, trie: &[Vec<u32>; 3]) {
    w.bytes("patterns", patterns.to_vec());
    for (name, v) in TRIE.iter().zip(trie) {
        w.u32s(name, v);
    }
    let mut ex = Vec::new();
    for (k, v) in exceptions {
        ex.push(k.len() as u8);
        ex.extend_from_slice(k);
        ex.push(v.len() as u8);
        ex.extend_from_slice(v);
    }
    w.bytes("except", ex);
}
