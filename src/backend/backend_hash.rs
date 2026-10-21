//! 128-bit content hashing for the recompile caches.
//!
//! A cache entry is reused when the hash of its inputs matches, so the hash stands in for
//! comparing the inputs themselves. Two independent 64-bit lanes, each mixed with a folded
//! 64×64→128 multiply (as wyhash does), make an accidental collision between two different
//! paragraphs or pages about as likely as a 2^-128 event. It is not meant to resist inputs
//! crafted to collide.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

const K_A: u64 = 0xa076_1d64_78bd_642f;
const K_B: u64 = 0xe703_7ed1_a0b4_28db;
const K_C: u64 = 0x8ebc_6af0_9c88_c6e3;

#[inline(always)]
fn fold_mul(a: u64, b: u64) -> u64 {
    let m = (a as u128).wrapping_mul(b as u128);
    (m as u64) ^ ((m >> 64) as u64)
}

/// Streaming 128-bit hash of a sequence of words and byte strings.
#[derive(Clone, Copy)]
pub struct ContentHasher {
    a: u64,
    b: u64,
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher {
    #[inline]
    pub const fn new() -> Self {
        Self { a: K_A, b: K_B }
    }

    #[inline(always)]
    pub fn u64(&mut self, v: u64) {
        self.a = fold_mul(self.a ^ v, K_B);
        self.b = fold_mul(self.b ^ v.rotate_left(32), K_C);
    }

    #[inline(always)]
    pub fn u32(&mut self, v: u32) {
        self.u64(v as u64);
    }

    #[inline(always)]
    pub fn f32(&mut self, v: f32) {
        self.u64(v.to_bits() as u64);
    }

    /// Hashes `bytes` with their length, so consecutive strings cannot run together.
    #[inline]
    pub fn bytes(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for &c in chunks {
            self.u64(u64::from_le_bytes(c));
        }
        let mut tail = [0u8; 8];
        tail[..rest.len()].copy_from_slice(rest);
        self.u64(u64::from_le_bytes(tail) ^ ((bytes.len() as u64) << 56));
        self.u64(bytes.len() as u64);
    }

    #[inline]
    pub fn finish(self) -> u128 {
        let a = fold_mul(self.a ^ K_C, self.b ^ K_A);
        let b = fold_mul(self.b ^ K_B, self.a.rotate_left(17) ^ K_C);
        ((a as u128) << 64) | b as u128
    }
}

/// Hasher for maps keyed by a [`ContentHasher`] result: the key is already uniformly
/// distributed, so its low half is used as it is.
#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | b as u64;
        }
    }

    #[inline]
    fn write_u128(&mut self, v: u128) {
        self.0 = v as u64;
    }
}

/// A map from content hashes to cached results.
pub type ContentMap<V> = HashMap<u128, V, BuildHasherDefault<KeyHasher>>;

/// Entries of one compile generation. A recompile marks the entries it reuses or adds;
/// [`GenerationCache::sweep`] then drops the rest, so the cache holds what the current
/// document needs and no more.
pub struct GenerationCache<V> {
    map: ContentMap<(u32, V)>,
    generation: u32,
}

impl<V> Default for GenerationCache<V> {
    fn default() -> Self {
        Self { map: ContentMap::default(), generation: 0 }
    }
}

impl<V> GenerationCache<V> {
    /// Starts a compile.
    pub fn begin(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    #[inline]
    pub fn get(&mut self, key: u128) -> Option<&V> {
        let generation = self.generation;
        self.map.get_mut(&key).map(|e| {
            e.0 = generation;
            &e.1
        })
    }

    #[inline]
    pub fn insert(&mut self, key: u128, value: V) {
        self.map.insert(key, (self.generation, value));
    }

    /// Drops the entries the compile just finished did not use.
    pub fn sweep(&mut self) {
        let generation = self.generation;
        self.map.retain(|_, e| e.0 == generation);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_do_not_run_together() {
        let mut a = ContentHasher::new();
        a.bytes(b"ab");
        a.bytes(b"c");
        let mut b = ContentHasher::new();
        b.bytes(b"a");
        b.bytes(b"bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn sweep_keeps_only_this_generation() {
        let mut c = GenerationCache::default();
        c.begin();
        c.insert(1, "one");
        c.insert(2, "two");
        c.sweep();
        c.begin();
        assert_eq!(c.get(1), Some(&"one"));
        c.sweep();
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(2), None);
    }
}
