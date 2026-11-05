//! Research-only RAM mirrors; no format change, prefetch or extra SIMD.
use crate::Error;

pub(crate) struct Tree<K> {
    sorted: Vec<K>,
    heap: Vec<K>,
    ranks: Vec<u32>,
}

impl<K: Copy + Default + Ord> Tree<K> {
    pub(crate) fn new(sorted: Vec<K>) -> Result<Self, Error> {
        if sorted.len() > u32::MAX as usize || sorted.len() > (usize::MAX / 2 - 1) {
            return Err(Error::Limit("research tree positions"));
        }
        let count = sorted.len() + 1;
        let mut heap = Vec::new();
        let mut ranks = Vec::new();
        heap.try_reserve_exact(count)?;
        ranks.try_reserve_exact(count)?;
        heap.resize(count, K::default());
        ranks.resize(count, 0);
        fn fill<K: Copy>(
            node: usize,
            next: &mut usize,
            sorted: &[K],
            heap: &mut [K],
            ranks: &mut [u32],
        ) {
            if node >= heap.len() {
                return;
            }
            fill(node * 2, next, sorted, heap, ranks);
            heap[node] = sorted[*next];
            ranks[node] = *next as u32;
            *next += 1;
            fill(node * 2 + 1, next, sorted, heap, ranks);
        }
        fill(1, &mut 0, &sorted, &mut heap, &mut ranks);
        Ok(Self {
            sorted,
            heap,
            ranks,
        })
    }

    pub(crate) fn allocation_bytes(&self) -> [usize; 3] {
        [
            self.sorted.capacity() * size_of::<K>(),
            self.heap.capacity() * size_of::<K>(),
            self.ranks.capacity() * 4,
        ]
    }

    #[inline(never)]
    pub(crate) fn bound(&self, query: K, upper: bool, method: usize) -> usize {
        let right = |key| if upper { key <= query } else { key < query };
        match method {
            0 => self.sorted.partition_point(|&key| right(key)),
            1 => {
                let mut node = 1;
                let mut best = self.sorted.len();
                while node < self.heap.len() {
                    if right(self.heap[node]) {
                        node = node * 2 + 1;
                    } else {
                        best = self.ranks[node] as usize;
                        node *= 2;
                    }
                }
                best
            }
            _ => {
                let mut node = 1usize;
                while node < self.heap.len() {
                    node = node * 2 + usize::from(right(self.heap[node]));
                }
                // Recover the last left turn from the traversal path, then map
                // its heap position back to original sorted rank. All-right -> 0.
                let candidate = node.checked_shr(node.trailing_ones() + 1).unwrap_or(0);
                if candidate == 0 {
                    self.sorted.len()
                } else {
                    self.ranks[candidate] as usize
                }
            }
        }
    }
}

pub(crate) struct Cache {
    pub(crate) asns: Tree<u32>,
    pub(crate) v4: Tree<u128>,
    pub(crate) v6: Tree<u128>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_linear_oracle_for_incomplete_trees_duplicates_and_unsigned_boundaries() {
        for count in (0..130).chain([255, 256, 257, 511, 512, 513, 1023, 1024, 1025]) {
            for duplicate in [false, true] {
                let keys: Vec<u128> = (0..count)
                    .map(|i| {
                        (u128::MAX / count.max(1) as u128)
                            * (if duplicate { i / 3 } else { i }) as u128
                    })
                    .collect();
                let tree = Tree::new(keys.clone()).unwrap();
                let mut queries = vec![0, 1, u128::MAX, 1 << 127, (1 << 64) - 1, 1 << 64];
                for &key in &keys {
                    queries.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
                }
                for q in queries {
                    for upper in [false, true] {
                        let expected = keys
                            .iter()
                            .take_while(|&&key| if upper { key <= q } else { key < q })
                            .count();
                        for method in 0..3 {
                            assert_eq!(tree.bound(q, upper, method), expected);
                        }
                    }
                }
                let small = Tree::new(keys.iter().map(|&k| (k >> 96) as u32).collect()).unwrap();
                for q in [0, 1, u32::MAX, 1 << 31] {
                    for upper in [false, true] {
                        let expected = small
                            .sorted
                            .iter()
                            .take_while(|&&k| if upper { k <= q } else { k < q })
                            .count();
                        for method in 0..3 {
                            assert_eq!(small.bound(q, upper, method), expected);
                        }
                    }
                }
            }
        }
    }
}
