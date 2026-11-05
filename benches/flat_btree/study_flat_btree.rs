//! Research-only flat B+ rank search with original sorted leaves.
use std::ops::Range;

use crate::{Error, study_eytzinger::Tree as Eyt};

pub(crate) trait Key: Copy + Default + Ord {
    const WIDTH: usize;
    fn vector_count(keys: &[Self], query: Self, upper: bool) -> usize;
    fn ordered_rank(keys: &[Self], query: Self, upper: bool) -> usize;
}

impl Key for u32 {
    const WIDTH: usize = 16;
    fn vector_count(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 detected; helper loads only complete eight-key groups.
            return unsafe { count32(keys, query, upper) };
        }
        count(keys, query, upper)
    }

    fn ordered_rank(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 detected, complete-group loads; caller supplies sorted keys.
            return unsafe { rank32(keys, query, upper) };
        }
        count(keys, query, upper)
    }
}

impl Key for u128 {
    const WIDTH: usize = 4;
    fn vector_count(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 detected; helper loads only complete two-key groups.
            return unsafe { count128(keys, query, upper) };
        }
        count(keys, query, upper)
    }

    fn ordered_rank(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 detected, complete-group loads; caller supplies sorted keys.
            return unsafe { rank128(keys, query, upper) };
        }
        count(keys, query, upper)
    }
}

fn count<K: Ord + Copy>(keys: &[K], query: K, upper: bool) -> usize {
    keys.iter()
        .filter(|&&k| if upper { k <= query } else { k < query })
        .count()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn count32(keys: &[u32], query: u32, upper: bool) -> usize {
    use std::arch::x86_64::*;
    let mut total = 0;
    let mut groups = keys.chunks_exact(8);
    // SAFETY: every unaligned load covers one complete eight-key chunk.
    unsafe {
        let sign = _mm256_set1_epi32(i32::MIN);
        let q = _mm256_xor_si256(_mm256_set1_epi32(query as i32), sign);
        for group in &mut groups {
            let key = _mm256_xor_si256(_mm256_loadu_si256(group.as_ptr().cast()), sign);
            let bits = if upper {
                !_mm256_movemask_ps(_mm256_castsi256_ps(_mm256_cmpgt_epi32(key, q))) & 255
            } else {
                _mm256_movemask_ps(_mm256_castsi256_ps(_mm256_cmpgt_epi32(q, key)))
            };
            total += bits.count_ones() as usize;
        }
    }
    total + count(groups.remainder(), query, upper)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn count128(keys: &[u128], query: u128, upper: bool) -> usize {
    use std::arch::x86_64::*;
    let mut total = 0;
    let mut groups = keys.chunks_exact(2);
    // SAFETY: each load covers two complete native little-endian u128 values on
    // x86-64. Even lanes are low halves, odd lanes high halves; no padded reads.
    unsafe {
        let sign = _mm256_set1_epi64x(i64::MIN);
        let q = _mm256_xor_si256(
            _mm256_set_epi64x(
                (query >> 64) as i64,
                query as i64,
                (query >> 64) as i64,
                query as i64,
            ),
            sign,
        );
        for group in &mut groups {
            let key = _mm256_xor_si256(_mm256_loadu_si256(group.as_ptr().cast()), sign);
            let less = _mm256_movemask_pd(_mm256_castsi256_pd(_mm256_cmpgt_epi64(q, key)));
            let equal = _mm256_movemask_pd(_mm256_castsi256_pd(_mm256_cmpeq_epi64(q, key)));
            let mut bits = ((less >> 1) & 5) | ((equal >> 1) & less & 5);
            if upper {
                bits |= (equal >> 1) & equal & 5;
            }
            total += bits.count_ones() as usize;
        }
    }
    total + count(groups.remainder(), query, upper)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn rank32(keys: &[u32], query: u32, upper: bool) -> usize {
    use std::arch::x86_64::*;
    let mut total = 0;
    let mut groups = keys.chunks_exact(8);
    // SAFETY: each unaligned load covers a complete eight-key group. Sorted keys
    // make true comparisons a low-bit prefix, so trailing ones give its rank.
    unsafe {
        let sign = _mm256_set1_epi32(i32::MIN);
        let q = _mm256_xor_si256(_mm256_set1_epi32(query as i32), sign);
        for group in &mut groups {
            let key = _mm256_xor_si256(_mm256_loadu_si256(group.as_ptr().cast()), sign);
            let bits = if upper {
                !_mm256_movemask_ps(_mm256_castsi256_ps(_mm256_cmpgt_epi32(key, q))) & 255
            } else {
                _mm256_movemask_ps(_mm256_castsi256_ps(_mm256_cmpgt_epi32(q, key)))
            } as u32;
            let rank = bits.trailing_ones() as usize;
            total += rank;
            if rank != 8 {
                return total;
            }
        }
    }
    total + count(groups.remainder(), query, upper)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn rank128(keys: &[u128], query: u128, upper: bool) -> usize {
    use std::arch::x86_64::*;
    let mut total = 0;
    let mut groups = keys.chunks_exact(2);
    // SAFETY: complete two-key loads on little-endian x86-64. Even/odd lanes
    // are low/high halves. Sorted keys make the compressed mask a true prefix.
    unsafe {
        let sign = _mm256_set1_epi64x(i64::MIN);
        let q = _mm256_xor_si256(
            _mm256_set_epi64x(
                (query >> 64) as i64,
                query as i64,
                (query >> 64) as i64,
                query as i64,
            ),
            sign,
        );
        for group in &mut groups {
            let key = _mm256_xor_si256(_mm256_loadu_si256(group.as_ptr().cast()), sign);
            let less = _mm256_movemask_pd(_mm256_castsi256_pd(_mm256_cmpgt_epi64(q, key)));
            let equal = _mm256_movemask_pd(_mm256_castsi256_pd(_mm256_cmpeq_epi64(q, key)));
            let mut bits = ((less >> 1) & 5) | ((equal >> 1) & less & 5);
            if upper {
                bits |= (equal >> 1) & equal & 5;
            }
            let rank = (((bits & 1) | ((bits >> 1) & 2)) as u32).trailing_ones() as usize;
            total += rank;
            if rank != 2 {
                return total;
            }
        }
    }
    total + count(groups.remainder(), query, upper)
}

pub(crate) struct Tree<K> {
    leaves: Vec<K>,
    nodes: Vec<K>,
    levels: Vec<Range<usize>>,
}

impl<K: Key> Tree<K> {
    pub(crate) fn requested_bytes(mut n: usize) -> Result<usize, Error> {
        if n > u32::MAX as usize {
            return Err(Error::Limit("research B+ positions"));
        }
        let mut bytes = n
            .checked_mul(size_of::<K>())
            .ok_or(Error::Limit("research B+ bytes"))?;
        while n > K::WIDTH {
            n = n.div_ceil(K::WIDTH);
            let level_bytes = n
                .checked_mul(size_of::<K>())
                .and_then(|b| b.checked_add(size_of::<Range<usize>>()))
                .ok_or(Error::Limit("research B+ bytes"))?;
            bytes = bytes
                .checked_add(level_bytes)
                .ok_or(Error::Limit("research B+ bytes"))?;
        }
        Ok(bytes)
    }

    pub(crate) fn new(leaves: Vec<K>) -> Result<Self, Error> {
        Self::requested_bytes(leaves.len())?;
        let (mut n, mut keys, mut depth) = (leaves.len(), 0, 0);
        while n > K::WIDTH {
            n = n.div_ceil(K::WIDTH);
            keys += n;
            depth += 1;
        }
        let mut nodes = Vec::new();
        let mut levels = Vec::new();
        nodes.try_reserve_exact(keys)?;
        levels.try_reserve_exact(depth)?;
        let mut previous = 0..leaves.len();
        for level in 0..depth {
            let start = nodes.len();
            for end in (K::WIDTH..previous.len())
                .step_by(K::WIDTH)
                .chain([previous.len()])
            {
                let key = if level == 0 {
                    leaves[end - 1]
                } else {
                    nodes[previous.start + end - 1]
                };
                nodes.push(key);
            }
            previous = start..nodes.len();
            levels.push(previous.clone());
        }
        Ok(Self {
            leaves,
            nodes,
            levels,
        })
    }

    pub(crate) fn allocation_bytes(&self) -> [usize; 3] {
        [
            self.leaves.capacity() * size_of::<K>(),
            self.nodes.capacity() * size_of::<K>(),
            self.levels.capacity() * size_of::<Range<usize>>(),
        ]
    }

    #[inline(never)]
    pub(crate) fn bound(&self, query: K, upper: bool, method: usize, enable: bool) -> usize {
        let search = |keys: &[K]| match method {
            0 => keys.partition_point(|&k| if upper { k <= query } else { k < query }),
            2 if enable => K::vector_count(keys, query, upper),
            3 if enable => K::ordered_rank(keys, query, upper),
            _ => count(keys, query, upper),
        };
        let mut base = 0;
        for level in self.levels.iter().rev() {
            let keys = &self.nodes[level.clone()];
            let end = (base + K::WIDTH).min(keys.len());
            let offset = search(&keys[base..end]);
            if offset == end - base {
                return self.leaves.len();
            }
            base = (base + offset) * K::WIDTH;
        }
        base + search(&self.leaves[base..(base + K::WIDTH).min(self.leaves.len())])
    }
}

pub(crate) struct Cache {
    pub(crate) asns: Eyt<u32>,
    pub(crate) v4: Eyt<u128>,
    pub(crate) v6: Eyt<u128>,
    pub(crate) b_asns: Tree<u32>,
    pub(crate) b_v4: Tree<u128>,
    pub(crate) b_v6: Tree<u128>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check<K: Key + std::fmt::Debug>(keys: Vec<K>, queries: Vec<K>) {
        let tree = Tree::new(keys.clone()).unwrap();
        assert_eq!(
            tree.allocation_bytes().iter().sum::<usize>(),
            Tree::<K>::requested_bytes(keys.len()).unwrap()
        );
        for q in queries {
            for upper in [false, true] {
                let expected = keys
                    .iter()
                    .take_while(|&&k| if upper { k <= q } else { k < q })
                    .count();
                for method in 0..4 {
                    for enable in [false, true] {
                        assert_eq!(
                            tree.bound(q, upper, method, enable),
                            expected,
                            "q={q:?} upper={upper} method={method}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bounds_match_linear_oracles_at_partial_node_and_level_boundaries() {
        for n in (0..130).chain([
            255, 256, 257, 511, 512, 513, 1023, 1024, 1025, 4095, 4096, 4097,
        ]) {
            for duplicates in [false, true] {
                let keys: Vec<u128> = (0..n)
                    .map(|i| {
                        (u128::MAX / n.max(1) as u128)
                            * (if duplicates { i / 3 } else { i }) as u128
                    })
                    .collect();
                let mut q = vec![0, 1, 1 << 63, 1 << 64, 1 << 127, u128::MAX];
                for &key in keys.iter().step_by((n / 64).max(1)) {
                    q.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
                }
                check(keys.clone(), q);
                let small: Vec<u32> = keys.iter().map(|&k| (k >> 96) as u32).collect();
                let mut q = vec![0, 1, 1 << 31, u32::MAX];
                for &key in small.iter().step_by((n / 64).max(1)) {
                    q.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
                }
                check(small, q);
            }
        }
        assert!(matches!(
            Tree::<u32>::requested_bytes(u32::MAX as usize + 1),
            Err(Error::Limit(_))
        ));
    }

    #[test]
    fn node_counts_match_scalar_at_each_alignment_and_unsigned_half_boundary() {
        let wide = [
            0,
            1,
            (1 << 63) - 1,
            1 << 63,
            (1 << 64) - 1,
            1 << 64,
            (1 << 64) + 1,
            1 << 127,
            u128::MAX - 1,
            u128::MAX,
        ];
        let small = [0, 1, (1 << 31) - 1, 1 << 31, u32::MAX - 1, u32::MAX];
        for start in 0..8 {
            let a: Vec<_> = wide.into_iter().cycle().skip(start).take(32).collect();
            let b: Vec<_> = small.into_iter().cycle().skip(start).take(32).collect();
            let mut ordered_a = a.clone();
            let mut ordered_b = b.clone();
            ordered_a.sort_unstable();
            ordered_b.sort_unstable();
            for length in 0..17 {
                for upper in [false, true] {
                    for q in wide {
                        assert_eq!(
                            u128::ordered_rank(&ordered_a[start..start + length], q, upper),
                            count(&ordered_a[start..start + length], q, upper)
                        );
                        assert_eq!(
                            u128::vector_count(&a[start..start + length], q, upper),
                            count(&a[start..start + length], q, upper)
                        );
                    }
                    for q in small {
                        assert_eq!(
                            u32::ordered_rank(&ordered_b[start..start + length], q, upper),
                            count(&ordered_b[start..start + length], q, upper)
                        );
                        assert_eq!(
                            u32::vector_count(&b[start..start + length], q, upper),
                            count(&b[start..start + length], q, upper)
                        );
                    }
                }
            }
        }
    }
}
