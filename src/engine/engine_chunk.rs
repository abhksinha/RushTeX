//! A growable array whose copies share their unchanged parts (copy-on-write in chunks), for
//! the engine's big arrays (`eqtb`'s tables, the node arena, font parameters, the token
//! arena) so a watch session's checkpoint of the job (`engine_checkpoint`) copies only what
//! changed since the last one.
//!
//! The live array is one flat `Vec`: reads are slices, as before. Every write goes through a
//! method; there is no `DerefMut`, so the compiler finds every writer. A write below the
//! array's low-water length since the last freeze sets the bit of its chunk (4 KB) in a
//! bitmap; one above it (an append, mostly) sets nothing, as every chunk reaching above the
//! low-water length is copied anyway. [`Chunked::freeze`] makes a frozen copy: each chunk
//! not written since the last freeze is the last copy's (`Arc`), each other one copied, and
//! the bitmap cleared. [`Chunked::thaw`] makes a live array of a frozen one. A one-shot run
//! never freezes: its low-water length stays 0, so a write's mark is one comparison.

use std::ops::{Deref, Range};
use std::sync::Arc;

pub struct Chunked<T> {
    v: Vec<T>,
    /// One bit per chunk written since the last freeze; empty until the first.
    dirty: Vec<u64>,
    /// The chunks of the last freeze (or of the frozen copy this was thawed from); in a
    /// frozen copy, its contents. A freeze with no chunk written shares the whole list.
    shared: Arc<Vec<Arc<[T]>>>,
    /// The least length since the last freeze (never above the length): the chunks wholly
    /// below it are the last freeze's unless marked, every other one is copied at the next.
    low: usize,
}

impl<T> Default for Chunked<T> {
    fn default() -> Self {
        Chunked { v: Vec::new(), dirty: Vec::new(), shared: Arc::default(), low: 0 }
    }
}

/// A deep copy of a live array; a frozen one's copy shares its chunks.
impl<T: Clone> Clone for Chunked<T> {
    fn clone(&self) -> Self {
        Chunked { v: self.v.clone(), dirty: self.dirty.clone(), shared: self.shared.clone(), low: self.low }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Chunked<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.v.fmt(f)
    }
}

impl<T> Deref for Chunked<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        &self.v
    }
}

impl<T, I: std::slice::SliceIndex<[T]>> std::ops::Index<I> for Chunked<T> {
    type Output = I::Output;
    #[inline]
    fn index(&self, i: I) -> &I::Output {
        &self.v[i]
    }
}

/// One element, writable (its chunk marked); a range is written through [`Chunked::slice_mut`].
impl<T: Clone> std::ops::IndexMut<usize> for Chunked<T> {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut T {
        self.get_mut(i)
    }
}

impl<'a, T> IntoIterator for &'a Chunked<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.v.iter()
    }
}

impl<T> From<Vec<T>> for Chunked<T> {
    fn from(v: Vec<T>) -> Self {
        Chunked { v, dirty: Vec::new(), shared: Arc::default(), low: 0 }
    }
}

impl<T: Clone> Chunked<T> {
    /// Elements per chunk: 4 KB of them (at least one).
    const SHIFT: u32 = {
        let n = 4096 / if std::mem::size_of::<T>() == 0 { 1 } else { std::mem::size_of::<T>() };
        if n <= 1 {
            0
        } else {
            n.ilog2()
        }
    };

    #[inline]
    fn mark(&mut self, i: usize) {
        if i < self.low {
            let c = i >> Self::SHIFT;
            if let Some(w) = self.dirty.get_mut(c >> 6) {
                *w |= 1 << (c & 63);
            }
        }
    }

    fn mark_range(&mut self, r: Range<usize>) {
        let end = r.end.min(self.low);
        if r.start < end {
            for c in r.start >> Self::SHIFT..=(end - 1) >> Self::SHIFT {
                if let Some(w) = self.dirty.get_mut(c >> 6) {
                    *w |= 1 << (c & 63);
                }
            }
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Vec::with_capacity(n).into()
    }

    #[inline]
    pub fn set(&mut self, i: usize, x: T) {
        self.v[i] = x;
        self.mark(i);
    }

    #[inline]
    pub fn get_mut(&mut self, i: usize) -> &mut T {
        self.mark(i);
        &mut self.v[i]
    }

    #[inline]
    pub fn last_mut(&mut self) -> Option<&mut T> {
        let n = self.v.len().checked_sub(1)?;
        Some(self.get_mut(n))
    }

    #[inline]
    pub fn slice_mut(&mut self, r: Range<usize>) -> &mut [T] {
        self.mark_range(r.clone());
        &mut self.v[r]
    }

    /// The whole array, writable (every chunk marked).
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        let n = self.v.len();
        self.slice_mut(0..n)
    }

    #[inline]
    // Appends write above the low-water length (never above the length): nothing to mark.
    pub fn push(&mut self, x: T) {
        self.v.push(x);
    }

    pub fn extend_from_slice(&mut self, s: &[T]) {
        self.v.extend_from_slice(s);
    }

    pub fn extend_from_within(&mut self, r: Range<usize>) {
        self.v.extend_from_within(r);
    }

    pub fn extend(&mut self, it: impl IntoIterator<Item = T>) {
        self.v.extend(it);
    }

    pub fn resize(&mut self, n: usize, x: T) {
        self.v.resize(n, x);
        self.low = self.low.min(n);
    }

    pub fn truncate(&mut self, n: usize) {
        self.v.truncate(n);
        self.low = self.low.min(self.v.len());
    }

    pub fn clear(&mut self) {
        self.v.clear();
        self.low = 0;
    }

    pub fn pop(&mut self) -> Option<T> {
        let x = self.v.pop();
        self.low = self.low.min(self.v.len());
        x
    }

    pub fn reserve(&mut self, n: usize) {
        self.v.reserve(n);
    }

    pub fn capacity(&self) -> usize {
        self.v.capacity()
    }

    /// Copies `src` to `dest` inside the array (`slice::copy_within`).
    pub fn copy_within(&mut self, src: Range<usize>, dest: usize)
    where
        T: Copy,
    {
        let n = src.len();
        self.v.copy_within(src, dest);
        self.mark_range(dest..dest + n);
    }

    /// Replaces the contents with `v`.
    pub fn replace(&mut self, v: Vec<T>) {
        self.v = v;
        self.low = 0;
    }

    /// The contents, taken out (a live array is left empty).
    pub fn take(&mut self) -> Vec<T> {
        self.low = 0;
        std::mem::take(&mut self.v)
    }

    /// A frozen copy: the chunks unchanged since the last freeze shared with it, the others
    /// copied; this array's chunks become those of the copy and its marks are cleared.
    pub fn freeze(&mut self) -> Chunked<T> {
        let size = 1usize << Self::SHIFT;
        let n = self.v.len().div_ceil(size);
        // Nothing written and the same length: the last freeze's chunks, whole.
        let same = n == self.shared.len() && self.low == self.v.len() && self.dirty.len() > (n >> 6) && self.dirty.iter().all(|&w| w == 0) && self.shared.last().is_none_or(|s| s.len() == self.v.len() - (n - 1) * size);
        if !same {
            let mut out = Vec::with_capacity(n);
            for c in 0..n {
                let r = c * size..self.v.len().min((c + 1) * size);
                let clean = r.end <= self.low && self.dirty.get(c >> 6).is_some_and(|w| w & (1 << (c & 63)) == 0) && self.shared.get(c).is_some_and(|s| s.len() == r.len());
                out.push(if clean { self.shared[c].clone() } else { Arc::from(&self.v[r]) });
            }
            self.shared = Arc::new(out);
            self.dirty.clear();
            self.dirty.resize(n / 64 + 1, 0);
        }
        self.low = self.v.len();
        Chunked { v: Vec::new(), dirty: Vec::new(), shared: self.shared.clone(), low: 0 }
    }

    /// A live array of a frozen copy (an array that was never frozen is copied whole).
    pub fn thaw(&self) -> Chunked<T> {
        if !self.v.is_empty() || self.shared.is_empty() {
            return self.clone();
        }
        let mut v = Vec::with_capacity(self.shared.iter().map(|s| s.len()).sum::<usize>() + 64);
        for s in self.shared.iter() {
            v.extend_from_slice(s);
        }
        let n = self.shared.len();
        let low = v.len();
        Chunked { v, dirty: vec![0; n / 64 + 1], shared: self.shared.clone(), low }
    }

    /// The number of elements, live or frozen.
    pub fn count(&self) -> usize {
        if self.v.is_empty() {
            self.shared.iter().map(|c| c.len()).sum()
        } else {
            self.v.len()
        }
    }

    /// Element `i`, live or frozen.
    pub fn at(&self, i: usize) -> Option<&T> {
        if self.v.is_empty() {
            self.shared.get(i >> Self::SHIFT)?.get(i & ((1 << Self::SHIFT) - 1))
        } else {
            self.v.get(i)
        }
    }

    /// The indices where two frozen copies of one length differ (chunks they share are
    /// skipped); `None` when their lengths differ.
    pub fn differences(&self, other: &Self) -> Option<Vec<usize>>
    where
        T: PartialEq,
    {
        if self.count() != other.count() || !self.v.is_empty() || !other.v.is_empty() {
            return (self == other).then(Vec::new);
        }
        let mut out = Vec::new();
        for (c, (a, b)) in self.shared.iter().zip(other.shared.iter()).enumerate() {
            if !Arc::ptr_eq(a, b) && a[..] != b[..] {
                out.extend(a.iter().zip(b.iter()).enumerate().filter(|(_, (x, y))| x != y).map(|(k, _)| (c << Self::SHIFT) + k));
            }
        }
        Some(out)
    }

    /// A frozen copy with element `i` replaced by `x` for each `(i, x)` of `edits` (the
    /// chunks holding them copied; a live array is written in place).
    pub fn patched(&self, edits: &[(usize, T)]) -> Self {
        if !self.v.is_empty() {
            let mut c = self.clone();
            for (i, x) in edits {
                c.set(*i, x.clone());
            }
            return c;
        }
        let mut chunks: Vec<Arc<[T]>> = self.shared.to_vec();
        let mut k = 0;
        while k < edits.len() {
            let c = edits[k].0 >> Self::SHIFT;
            let mut chunk: Vec<T> = chunks[c].to_vec();
            while k < edits.len() && edits[k].0 >> Self::SHIFT == c {
                chunk[edits[k].0 & ((1 << Self::SHIFT) - 1)] = edits[k].1.clone();
                k += 1;
            }
            chunks[c] = Arc::from(chunk);
        }
        Chunked { v: Vec::new(), dirty: Vec::new(), shared: Arc::new(chunks), low: 0 }
    }

    /// The bytes a freeze would copy now (chunks written since the last one).
    pub fn dirty_bytes(&self) -> usize {
        let size = 1usize << Self::SHIFT;
        let n = self.v.len().div_ceil(size);
        (0..n).filter(|&c| !(self.dirty.get(c >> 6).is_some_and(|w| w & (1 << (c & 63)) == 0) && self.shared.get(c).is_some_and(|s| s.len() == self.v.len().min((c + 1) * size) - c * size))).count() * size * std::mem::size_of::<T>()
    }
}

/// Two frozen copies are equal when their chunks are (shared chunks compared by address);
/// live arrays by their contents.
impl<T: PartialEq> PartialEq for Chunked<T> {
    fn eq(&self, other: &Self) -> bool {
        let frozen = |c: &Self| c.v.is_empty() && !c.shared.is_empty();
        if frozen(self) && frozen(other) {
            return self.shared.len() == other.shared.len() && self.shared.iter().zip(other.shared.iter()).all(|(a, b)| Arc::ptr_eq(a, b) || a[..] == b[..]);
        }
        fn items<T>(c: &Chunked<T>) -> Vec<&T> {
            if c.v.is_empty() {
                c.shared.iter().flat_map(|s| s.iter()).collect()
            } else {
                c.v.iter().collect()
            }
        }
        items(self) == items(other)
    }
}
