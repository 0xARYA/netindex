pub(crate) trait DirectKey: Key {
    /// Caller must detect AVX2 before entering this implementation.
    unsafe fn direct_rank(keys: &[Self], query: Self, upper: bool) -> usize;
}

impl DirectKey for u32 {
    #[cfg_attr(target_arch = "x86_64", target_feature(enable = "avx2"))]
    #[inline]
    unsafe fn direct_rank(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: caller detects AVX2; sorted keys come from validated tree levels.
        return unsafe { rank32(keys, query, upper) };
        #[cfg(not(target_arch = "x86_64"))]
        count(keys, query, upper)
    }
}

impl DirectKey for u128 {
    #[cfg_attr(target_arch = "x86_64", target_feature(enable = "avx2"))]
    #[inline]
    unsafe fn direct_rank(keys: &[Self], query: Self, upper: bool) -> usize {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: caller detects AVX2; sorted keys come from validated tree levels.
        return unsafe { rank128(keys, query, upper) };
        #[cfg(not(target_arch = "x86_64"))]
        count(keys, query, upper)
    }
}

impl<K: DirectKey> Tree<K> {
    pub(crate) fn bound_specialized(
        &self,
        query: K,
        upper: bool,
        ordered: bool,
        enable: bool,
    ) -> usize {
        #[cfg(target_arch = "x86_64")]
        if ordered && enable && std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 detected once per query, outside every node loop.
            return unsafe { self.walk_vector(query, upper) };
        }
        let _ = (ordered, enable);
        self.walk_count(query, upper)
    }

    #[inline(never)]
    fn walk_count(&self, query: K, upper: bool) -> usize {
        let mut base = 0;
        for level in self.levels.iter().rev() {
            let keys = &self.nodes[level.clone()];
            let end = (base + K::WIDTH).min(keys.len());
            let offset = count(&keys[base..end], query, upper);
            if offset == end - base {
                return self.leaves.len();
            }
            base = (base + offset) * K::WIDTH;
        }
        base + count(
            &self.leaves[base..(base + K::WIDTH).min(self.leaves.len())],
            query,
            upper,
        )
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[inline(never)]
    unsafe fn walk_vector(&self, query: K, upper: bool) -> usize {
        let mut base = 0;
        for level in self.levels.iter().rev() {
            let keys = &self.nodes[level.clone()];
            let end = (base + K::WIDTH).min(keys.len());
            // SAFETY: this AVX2 entry was gated once; levels are sorted, and
            // complete in-bounds slices retain partial-node lengths.
            let offset = unsafe { K::direct_rank(&keys[base..end], query, upper) };
            if offset == end - base {
                return self.leaves.len();
            }
            base = (base + offset) * K::WIDTH;
        }
        // SAFETY: same AVX2 entry; leaf slice lies within original sorted leaves.
        base + unsafe {
            K::direct_rank(
                &self.leaves[base..(base + K::WIDTH).min(self.leaves.len())],
                query,
                upper,
            )
        }
    }
}

#[cfg(test)]
mod specialized_tests {
    use super::*;

    fn check<K: DirectKey>(keys: Vec<K>, queries: &[K]) {
        let tree = Tree::new(keys.clone()).unwrap();
        for &query in queries {
            for upper in [false, true] {
                let expected = keys
                    .iter()
                    .take_while(|&&k| if upper { k <= query } else { k < query })
                    .count();
                for ordered in [false, true] {
                    for enable in [false, true] {
                        assert_eq!(
                            tree.bound_specialized(query, upper, ordered, enable),
                            expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn specialized_traversal_and_forced_scalar_match_linear_bounds() {
        for n in (0..130).chain([255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097]) {
            let keys: Vec<u128> = (0..n)
                .map(|i| (u128::MAX / n.max(1) as u128) * (i / 3) as u128)
                .collect();
            let mut q = vec![0, 1, 1 << 64, 1 << 127, u128::MAX];
            for &k in keys.iter().step_by((n / 64).max(1)) {
                q.extend([k.saturating_sub(1), k, k.saturating_add(1)]);
            }
            check(keys.clone(), &q);
            let small: Vec<u32> = keys.iter().map(|&k| (k >> 96) as u32).collect();
            let mut q = vec![0, 1, 1 << 31, u32::MAX];
            for &k in small.iter().step_by((n / 64).max(1)) {
                q.extend([k.saturating_sub(1), k, k.saturating_add(1)]);
            }
            check(small, &q);
        }
    }
}
