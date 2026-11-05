use crate::study_flat_btree::Tree;

pub(crate) const MAX_BUDGET: usize = 128 * 1024 * 1024;

pub(crate) struct Cache {
    pub(crate) budget: usize,
    pub(crate) asns: Option<Tree<u32>>,
    pub(crate) v4: Option<Vec<u32>>,
    pub(crate) v6: Option<Vec<u128>>,
}

impl Cache {
    pub(crate) fn allocation_bytes(&self) -> [usize; 5] {
        let a = self.asns.as_ref().map_or([0; 3], Tree::allocation_bytes);
        [
            a[0],
            a[1],
            a[2],
            self.v4.as_ref().map_or(0, |v| v.capacity() * 4),
            self.v6.as_ref().map_or(0, |v| v.capacity() * 16),
        ]
    }
}

pub(crate) fn admit(remaining: &mut usize, cost: usize) -> bool {
    if cost == 0 || cost > *remaining {
        return false;
    }
    *remaining -= cost;
    true
}

#[inline(never)]
pub(crate) fn choose(length: usize, span: Option<u128>, cached: bool, v4: bool) -> usize {
    if length < 32 || span.is_none_or(|s| s <= 4096) {
        0
    } else if cached {
        if v4 { 601 } else { 600 }
    } else {
        108
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_is_exact_aggregate_and_never_wraps() {
        for budget in [0, 1, 63, 64, 65, 1 << 20, MAX_BUDGET] {
            let mut remaining = budget;
            let mut admitted = 0;
            for cost in [0, 64, 4, 16, usize::MAX, 128] {
                let before = remaining;
                if admit(&mut remaining, cost) {
                    admitted += cost;
                    assert_eq!(before - remaining, cost);
                } else {
                    assert_eq!(before, remaining);
                }
                assert_eq!(admitted + remaining, budget);
            }
        }
        for cost in [1, 64, 1024, 1 << 20] {
            assert!(!admit(&mut (cost - 1), cost));
            let mut exact = cost;
            assert!(admit(&mut exact, cost));
            assert_eq!(exact, 0);
        }
    }

    #[test]
    fn selection_uses_only_declared_observations_at_boundaries() {
        for n in [0, 1, 8, 31] {
            assert_eq!(choose(n, Some(u128::MAX), true, true), 0);
        }
        for n in [32, 33, 65536] {
            assert_eq!(choose(n, None, true, true), 0);
            assert_eq!(choose(n, Some(4096), true, true), 0);
            assert_eq!(choose(n, Some(4097), false, true), 108);
            assert_eq!(choose(n, Some(4097), true, true), 601);
            assert_eq!(choose(n, Some(u128::MAX), true, false), 600);
        }
    }
}
