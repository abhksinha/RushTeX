//! Content-keyed caches of the PDF writer's pure work, kept by a `--watch` session between
//! recompiles (`Warm` in `main.rs`): a page's or form's compressed content stream, a Type 1
//! font's finished parts (names, encoding, subset program, compressed ToUnicode map), a PNG
//! image decoded and compressed, and the files the binary carries, decompressed.
//! Each entry is keyed by a 128-bit hash (`ContentHasher`) of every byte its output is made
//! from, so a hit gives exactly the bytes the work would; object numbers, which can shift
//! between recompiles, are never inside a cached value (they are written around it).
//! A one-shot run has no cache and hashes nothing.
//!
//! Memory stays the size of the document's working set: a compile stamps what it uses
//! ([`PdfCache::begin`]), and [`PdfCache::sweep`] after it drops what it did not use.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// A memo from a key to its output, with the compile that used each entry last.
struct Memo<K, V> {
    map: HashMap<K, (V, u32)>,
    hits: u32,
    misses: u32,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Memo { map: HashMap::new(), hits: 0, misses: 0 }
    }
}

impl<K: std::hash::Hash + Eq, V: Clone> Memo<K, V> {
    fn get<Q: std::hash::Hash + Eq + ?Sized>(&mut self, key: &Q, gen: u32) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
    {
        match self.map.get_mut(key) {
            Some((v, g)) => {
                *g = gen;
                self.hits += 1;
                Some(v.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    fn sweep(&mut self, gen: u32) -> (u32, u32, usize) {
        self.map.retain(|_, (_, g)| *g == gen);
        let c = (self.hits, self.misses, self.map.len());
        (self.hits, self.misses) = (0, 0);
        c
    }
}

/// A Type 1 font as `FontWork::prepare` leaves it: the ToUnicode map compressed, and the
/// embedded program's stream, `/Length1` and `/Length2`, and `/FontBBox`.
pub(crate) type PreparedFont = (Option<Vec<u8>>, Option<(Vec<u8>, [usize; 2], [i32; 4])>);

/// A PNG as written: width, height, bits per component, the colour space's PDF text, the
/// compressed image and the compressed soft mask.
pub(crate) type PreparedImage = std::rc::Rc<(u32, u32, u8, Vec<u8>, Vec<u8>, Option<Vec<u8>>)>;

#[derive(Default)]
struct Caches {
    streams: Memo<u128, std::rc::Rc<Vec<u8>>>,
    prepared: Memo<u128, std::rc::Rc<PreparedFont>>,
    fonts: Memo<u128, std::rc::Rc<crate::backend::backend_ship::FontDone>>,
    images: Memo<u128, PreparedImage>,
    files: Memo<Box<str>, std::rc::Rc<Vec<u8>>>,
    pages: Memo<u128, std::rc::Rc<ShippedDone>>,
}

/// A page as its walk shipped it (`engine_ship::ship_page`): its content stream, the
/// characters it used per PDF font, and the font sharing and colour stacks after it.
pub struct ShippedDone {
    pub content: Vec<u8>,
    pub used: Vec<[u64; 4]>,
    pub base: Vec<u16>,
    pub colors: crate::engine::engine_color::ColorStacks,
}

/// The session's caches (module doc).
#[derive(Default)]
pub struct PdfCache {
    gen: Cell<u32>,
    /// Whether the job running reads files through [`PdfCache::carried`] (the native job's
    /// first pass): only then is a file name a font read last time known to give the same
    /// bytes now.
    carried_job: Cell<bool>,
    c: RefCell<Caches>,
    last: Cell<[(u32, u32, usize); 6]>,
}

impl PdfCache {
    /// A compile begins: what it uses is stamped with a new generation.
    pub fn begin(&self) {
        self.gen.set(self.gen.get().wrapping_add(1));
    }

    /// The compile ended well: entries it did not use are dropped (a failed compile keeps
    /// them). `RT_TIME=1` prints each cache's hits, misses and size.
    pub fn sweep(&self) {
        let gen = self.gen.get();
        let mut c = self.c.borrow_mut();
        let last = [c.streams.sweep(gen), c.prepared.sweep(gen), c.fonts.sweep(gen), c.images.sweep(gen), c.files.sweep(gen), c.pages.sweep(gen)];
        self.last.set(last);
        if std::env::var_os("RT_TIME").is_some() {
            for (what, (h, m, n)) in Self::NAMES.iter().zip(last) {
                eprintln!("             pdf cache {what}: {h} hits, {m} misses, {n} kept");
            }
        }
    }

    const NAMES: [&'static str; 6] = ["streams", "font subsets", "fonts", "images", "carried files", "shipped pages"];

    /// The last compile's hits, misses and entries kept, per cache.
    pub fn last_counts(&self) -> Vec<(&'static str, (u32, u32, usize))> {
        Self::NAMES.iter().copied().zip(self.last.get()).collect()
    }

    /// File `name` as the binary carries it (`carry`, which gives the same bytes for a name
    /// for the whole process), decompressed once per session. `pdftex.map` is not kept: it
    /// grows with the font packs loaded.
    pub fn carried(&self, name: &str, carry: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        let gen = self.gen.get();
        if let Some(b) = self.c.borrow_mut().files.get(name, gen) {
            return Some((*b).clone());
        }
        let b = carry()?;
        if name != "pdftex.map" {
            self.c.borrow_mut().files.map.insert(name.into(), (std::rc::Rc::new(b.clone()), gen));
        }
        Some(b)
    }

    /// Marks the job that runs now as reading through [`PdfCache::carried`] or not.
    pub fn set_carried_job(&self, on: bool) {
        self.carried_job.set(on);
    }

    /// Whether every file in `names` gives now the bytes it gave when read before: the job
    /// reads through [`PdfCache::carried`] and each is kept there.
    pub(crate) fn files_unchanged<'n>(&self, mut names: impl Iterator<Item = &'n str>) -> bool {
        let gen = self.gen.get();
        let mut c = self.c.borrow_mut();
        self.carried_job.get()
            && names.all(|n| match c.files.map.get_mut(n) {
                Some((_, g)) => {
                    *g = gen;
                    true
                }
                None => false,
            })
    }

    pub(crate) fn stream(&self, key: u128) -> Option<std::rc::Rc<Vec<u8>>> {
        self.c.borrow_mut().streams.get(&key, self.gen.get())
    }

    pub(crate) fn put_stream(&self, key: u128, v: Vec<u8>) {
        self.c.borrow_mut().streams.map.insert(key, (std::rc::Rc::new(v), self.gen.get()));
    }

    pub(crate) fn prepared(&self, key: u128) -> Option<std::rc::Rc<PreparedFont>> {
        self.c.borrow_mut().prepared.get(&key, self.gen.get())
    }

    pub(crate) fn put_prepared(&self, key: u128, v: PreparedFont) {
        self.c.borrow_mut().prepared.map.insert(key, (std::rc::Rc::new(v), self.gen.get()));
    }

    /// A font's finished parts (`backend_ship`).
    pub(crate) fn font(&self, key: u128) -> Option<std::rc::Rc<crate::backend::backend_ship::FontDone>> {
        self.c.borrow_mut().fonts.get(&key, self.gen.get())
    }

    pub(crate) fn put_font(&self, key: u128, v: crate::backend::backend_ship::FontDone) {
        self.c.borrow_mut().fonts.map.insert(key, (std::rc::Rc::new(v), self.gen.get()));
    }

    pub(crate) fn page(&self, key: u128) -> Option<std::rc::Rc<ShippedDone>> {
        self.c.borrow_mut().pages.get(&key, self.gen.get())
    }

    pub(crate) fn put_page(&self, key: u128, v: ShippedDone) {
        self.c.borrow_mut().pages.map.insert(key, (std::rc::Rc::new(v), self.gen.get()));
    }

    pub(crate) fn image(&self, key: u128) -> Option<PreparedImage> {
        self.c.borrow_mut().images.get(&key, self.gen.get())
    }

    pub(crate) fn put_image(&self, key: u128, v: PreparedImage) {
        self.c.borrow_mut().images.map.insert(key, (v, self.gen.get()));
    }
}

/// The key of a content stream: its bytes before compression (deflate's output depends on
/// its input alone: `prepare_all`).
pub(crate) fn stream_key(data: &[u8]) -> u128 {
    let mut h = crate::backend::backend_hash::ContentHasher::new();
    h.u64(1);
    h.bytes(data);
    h.finish()
}
