#[derive(Default)]
#[repr(align(64))]
pub(crate) struct Aligned<T>(T);

impl<K, T: AsRef<[K]>> AsRef<[K]> for Aligned<T> {
    #[inline]
    fn as_ref(&self) -> &[K] {
        self.0.as_ref()
    }
}

impl<K, T: AsMut<[K]>> AsMut<[K]> for Aligned<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut [K] {
        self.0.as_mut()
    }
}

pub(crate) trait GroupKey: Copy + Default + Ord {
    type Natural: Default + AsRef<[Self]> + AsMut<[Self]>;
    type Aligned: Default + AsRef<[Self]> + AsMut<[Self]>;
    const LANES: usize;
}

impl GroupKey for u32 {
    type Natural = [u32; 16];
    type Aligned = Aligned<[u32; 16]>;
    const LANES: usize = 16;
}

impl GroupKey for u128 {
    type Natural = [u128; 4];
    type Aligned = Aligned<[u128; 4]>;
    const LANES: usize = 4;
}

pub(crate) struct Mirror<K: GroupKey> {
    natural: Vec<K::Natural>,
    aligned: Vec<K::Aligned>,
}

impl<K: GroupKey> Mirror<K> {
    pub(crate) fn requested_bytes(count: usize) -> Result<usize, Error> {
        count
            .checked_add(1)
            .and_then(|n| n.div_ceil(K::LANES).checked_mul(128))
            .ok_or(Error::Limit("research grouped mirrors"))
    }

    fn new(heap: &[K]) -> Result<Self, Error> {
        let groups = heap.len().div_ceil(K::LANES);
        let mut natural = Vec::<K::Natural>::new();
        let mut aligned = Vec::<K::Aligned>::new();
        natural.try_reserve_exact(groups)?;
        aligned.try_reserve_exact(groups)?;
        natural.resize_with(groups, Default::default);
        aligned.resize_with(groups, Default::default);
        for (index, &key) in heap.iter().enumerate() {
            natural[index / K::LANES].as_mut()[index % K::LANES] = key;
            aligned[index / K::LANES].as_mut()[index % K::LANES] = key;
        }
        Ok(Self { natural, aligned })
    }

    pub(crate) fn allocation_bytes(&self) -> [usize; 2] {
        [
            self.natural.capacity() * size_of::<K::Natural>(),
            self.aligned.capacity() * size_of::<K::Aligned>(),
        ]
    }

    pub(crate) fn alignment(&self) -> [usize; 2] {
        [
            self.natural.as_ptr() as usize % 64,
            self.aligned.as_ptr() as usize % 64,
        ]
    }

    #[inline(always)]
    fn group<const ALIGN: bool>(&self, index: usize) -> &[K] {
        if ALIGN {
            self.aligned[index].as_ref()
        } else {
            self.natural[index].as_ref()
        }
    }
}

impl<K: GroupKey> Tree<K> {
    pub(crate) fn study_mirror(&self) -> Result<Mirror<K>, Error> {
        Mirror::new(&self.heap)
    }

    #[inline]
    pub(crate) fn traverse(
        &self,
        mirror: &Mirror<K>,
        query: K,
        upper: bool,
        method: usize,
    ) -> usize {
        match method {
            0 => self.walk::<false, false, false>(mirror, query, upper),
            1 => self.walk::<true, false, false>(mirror, query, upper),
            2 => self.walk::<false, true, false>(mirror, query, upper),
            3 => self.walk::<true, true, false>(mirror, query, upper),
            4 => self.walk::<false, false, true>(mirror, query, upper),
            5 => self.walk::<true, false, true>(mirror, query, upper),
            6 => self.walk::<false, true, true>(mirror, query, upper),
            _ => self.walk::<true, true, true>(mirror, query, upper),
        }
    }

    #[inline(never)]
    fn walk<const ARITH: bool, const PREFETCH: bool, const ALIGN: bool>(
        &self,
        mirror: &Mirror<K>,
        query: K,
        upper: bool,
    ) -> usize {
        let mut node = 1usize;
        let mut best = self.sorted.len();
        while node < self.heap.len() {
            if PREFETCH {
                #[cfg(target_arch = "x86_64")]
                if let Some(target) = node.checked_mul(K::LANES).filter(|&i| i < self.heap.len()) {
                    let group = mirror.group::<ALIGN>(target / K::LANES);
                    // SAFETY: x86-64 includes SSE; pointer is within a live complete
                    // 64-byte group. No out-of-allocation pointer is formed.
                    unsafe {
                        std::arch::x86_64::_mm_prefetch(
                            group.as_ptr().cast(),
                            std::arch::x86_64::_MM_HINT_T0,
                        )
                    };
                }
            }
            let key = mirror.group::<ALIGN>(node / K::LANES)[node % K::LANES];
            let right = if upper { key <= query } else { key < query };
            if ARITH {
                node = node * 2 + usize::from(right);
            } else if right {
                node = node * 2 + 1;
            } else {
                best = self.ranks[node] as usize;
                node *= 2;
            }
        }
        if !ARITH {
            return best;
        }
        let candidate = node.checked_shr(node.trailing_ones() + 1).unwrap_or(0);
        if candidate == 0 {
            self.sorted.len()
        } else {
            self.ranks[candidate] as usize
        }
    }
}

#[cfg(test)]
mod traversal_tests {
    use super::*;

    fn check<K: GroupKey + std::fmt::Debug>(keys: Vec<K>, queries: &[K]) {
        let tree = Tree::new(keys.clone()).unwrap();
        let mirror = tree.study_mirror().unwrap();
        assert_eq!(mirror.alignment()[1], 0);
        assert_eq!(
            mirror.allocation_bytes().iter().sum::<usize>(),
            Mirror::<K>::requested_bytes(keys.len()).unwrap()
        );
        for &query in queries {
            for upper in [false, true] {
                let expected = keys
                    .iter()
                    .take_while(|&&k| if upper { k <= query } else { k < query })
                    .count();
                for method in 0..8 {
                    assert_eq!(tree.traverse(&mirror, query, upper, method), expected);
                }
            }
        }
    }

    #[test]
    fn all_traversal_factors_match_linear_oracles_and_alignment_contract() {
        for n in (0..130).chain([255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097]) {
            for duplicate in [false, true] {
                let keys: Vec<u128> = (0..n)
                    .map(|i| {
                        (u128::MAX / n.max(1) as u128) * (if duplicate { i / 3 } else { i }) as u128
                    })
                    .collect();
                let mut queries = vec![0, 1, 1 << 63, 1 << 64, 1 << 127, u128::MAX];
                for &key in keys.iter().step_by((n / 64).max(1)) {
                    queries.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
                }
                check(keys.clone(), &queries);
                let small: Vec<u32> = keys.iter().map(|&k| (k >> 96) as u32).collect();
                let mut queries = vec![0, 1, 1 << 31, u32::MAX];
                for &key in small.iter().step_by((n / 64).max(1)) {
                    queries.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
                }
                check(small, &queries);
            }
        }
        assert!(matches!(
            Mirror::<u128>::requested_bytes(usize::MAX),
            Err(Error::Limit(_))
        ));
    }
}
