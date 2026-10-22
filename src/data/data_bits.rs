//! A set of IDs as one bit per ID (`loaded`, `needs`): one load and a mask per test, and a
//! plain array, so a checkpoint copies it and an edit diffs two of them word by word.

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Bits {
    words: Vec<u64>,
}

impl Bits {
    /// The empty set over IDs `0..n`, allocated once.
    pub fn new(n: usize) -> Bits {
        Bits { words: vec![0; n.div_ceil(64)] }
    }

    #[inline]
    pub fn test(&self, id: u32) -> bool {
        self.words.get(id as usize >> 6).is_some_and(|w| w >> (id & 63) & 1 != 0)
    }

    /// Adds `id`; true when it was not in the set. Panics past the set's range.
    #[inline]
    pub fn insert(&mut self, id: u32) -> bool {
        let w = &mut self.words[id as usize >> 6];
        let m = 1u64 << (id & 63);
        let new = *w & m == 0;
        *w |= m;
        new
    }

    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// `self ∪= other` over the common range.
    pub fn union_with(&mut self, other: &Bits) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    /// The IDs in `self` and not in `other`, ascending: what an edit newly needs.
    pub fn difference<'a>(&'a self, other: &'a Bits) -> impl Iterator<Item = u32> + 'a {
        self.words.iter().enumerate().flat_map(move |(i, &w)| ones(i, w & !other.words.get(i).copied().unwrap_or(0)))
    }

    /// The IDs in the set, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(i, &w)| ones(i, w))
    }

    pub fn count(&self) -> u32 {
        self.words.iter().map(|w| w.count_ones()).sum()
    }

    /// The words themselves, for a checkpoint.
    pub fn words(&self) -> &[u64] {
        &self.words
    }
}

/// The set bits of word `i`, as IDs, lowest first (one `trailing_zeros` per ID).
fn ones(i: usize, mut w: u64) -> impl Iterator<Item = u32> {
    std::iter::from_fn(move || {
        (w != 0).then(|| {
            let b = w.trailing_zeros();
            w &= w - 1;
            (i as u32) << 6 | b
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_operations() {
        let mut a = Bits::new(200);
        assert!(a.insert(3) && a.insert(64) && a.insert(199) && !a.insert(3));
        assert!(a.test(64) && !a.test(65) && !a.test(10_000));
        let mut b = Bits::new(200);
        b.insert(64);
        b.insert(100);
        assert_eq!(a.difference(&b).collect::<Vec<_>>(), [3, 199]);
        a.union_with(&b);
        assert_eq!(a.iter().collect::<Vec<_>>(), [3, 64, 100, 199]);
        assert_eq!(a.count(), 4);
        a.clear();
        assert_eq!(a.count(), 0);
    }
}
