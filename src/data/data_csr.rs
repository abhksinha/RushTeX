//! Lists by ID in compressed sparse rows: `offs[n + 1]` and one `ids` array, an array of
//! arrays with no allocation per list (the `\RequirePackage` edges, the names a macro body
//! mentions). Read in place from a blob.

use super::data_bits::Bits;
use super::data_blob::{Blob, U32s};

#[derive(Clone, Copy)]
pub struct Csr<'a> {
    offs: U32s<'a>,
    ids: U32s<'a>,
}

impl<'a> Csr<'a> {
    /// The lists stored as sections `{name}.o` and `{name}.i`, when their offsets are sound.
    pub fn from_blob(b: &Blob<'a>, name: &str) -> Option<Csr<'a>> {
        let offs = b.u32s(&format!("{name}.o"))?;
        let ids = b.u32s(&format!("{name}.i"))?;
        let ok = !offs.is_empty() && offs.get(0) == 0 && offs.get(offs.len() - 1) as usize == ids.len() && (1..offs.len()).all(|i| offs.get(i - 1) <= offs.get(i));
        ok.then_some(Csr { offs, ids })
    }

    /// The number of lists.
    pub fn len(&self) -> usize {
        self.offs.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// List `i`, in stored order.
    pub fn row(&self, i: u32) -> impl Iterator<Item = u32> + 'a {
        let ids = self.ids;
        (self.offs.get(i as usize) as usize..self.offs.get(i as usize + 1) as usize).map(move |k| ids.get(k))
    }

    /// Adds to `set` every ID reachable from `seeds` (seeds included). `stack` is the
    /// caller's buffer, reused across calls; each ID is pushed at most once, since an ID is
    /// pushed only when `insert` reports it new.
    pub fn closure(&self, seeds: impl IntoIterator<Item = u32>, set: &mut Bits, stack: &mut Vec<u32>) {
        stack.clear();
        stack.extend(seeds.into_iter().filter(|&s| set.insert(s)));
        while let Some(i) = stack.pop() {
            if (i as usize) < self.len() {
                for j in self.row(i) {
                    if set.insert(j) {
                        stack.push(j);
                    }
                }
            }
        }
    }
}

/// Writes `rows` as the sections `Csr::from_blob` reads.
#[cfg(any(test, feature = "gen"))]
pub fn write(w: &mut super::data_blob::BlobWriter, name: &str, rows: &[Vec<u32>]) {
    let mut offs = Vec::with_capacity(rows.len() + 1);
    offs.push(0u32);
    for r in rows {
        offs.push(offs[offs.len() - 1] + r.len() as u32);
    }
    let ids: Vec<u32> = rows.iter().flatten().copied().collect();
    w.u32s(&format!("{name}.o"), &offs);
    w.u32s(&format!("{name}.i"), &ids);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::data_blob::BlobWriter;

    #[test]
    fn rows_and_closure() {
        // 0 → 1, 2;  1 → 3;  2 → 1;  3 → (none);  4 → 0 (not reachable from 1)
        let mut w = BlobWriter::default();
        write(&mut w, "req", &[vec![1, 2], vec![3], vec![1], vec![], vec![0]]);
        let bytes = w.finish();
        let b = Blob::new(&bytes).unwrap();
        let c = Csr::from_blob(&b, "req").unwrap();
        assert_eq!(c.len(), 5);
        assert_eq!(c.row(0).collect::<Vec<_>>(), [1, 2]);
        let (mut set, mut stack) = (Bits::new(5), Vec::new());
        c.closure([1], &mut set, &mut stack);
        assert_eq!(set.iter().collect::<Vec<_>>(), [1, 3]);
        c.closure([0], &mut set, &mut stack);
        assert_eq!(set.iter().collect::<Vec<_>>(), [0, 1, 2, 3]);
    }
}
