use std::mem::size_of;

use crate::Error;

pub(crate) const BUDGET: usize = 128 * 1024 * 1024;

pub(crate) trait Key: Copy + Ord {
    fn number(self) -> u64;
}
impl Key for u32 {
    fn number(self) -> u64 {
        u64::from(self)
    }
}
impl Key for u64 {
    fn number(self) -> u64 {
        self
    }
}

#[derive(Clone, Copy)]
struct Segment {
    key: u64,
    rank: usize,
    end_key: u64,
    end_rank: usize,
    slope: f64,
}

pub(crate) struct Model<K> {
    keys: Vec<K>,
    segments: Vec<Segment>,
    radix: Vec<u32>,
    shift: u32,
    epsilon: usize,
}

pub(crate) struct Bound {
    pub(crate) index: usize,
    pub(crate) fallback: bool,
    pub(crate) window: usize,
}

impl<K: Key> Model<K> {
    pub(crate) fn new(keys: Vec<K>, epsilon: usize, budget: usize) -> Result<Self, Error> {
        let key_bytes = keys
            .capacity()
            .checked_mul(size_of::<K>())
            .ok_or(Error::Limit("learned key bytes"))?;
        if key_bytes > budget || epsilon > 128 {
            return Err(Error::Limit("learned model budget/error"));
        }
        let mut segments = Vec::new();
        let mut i = 0;
        let (mut lower, mut upper) = (0.0_f64, f64::INFINITY);
        let mut current: Option<Segment> = None;
        while i < keys.len() {
            let x = keys[i].number();
            let mut end = i + 1;
            while end < keys.len() && keys[end].number() == x {
                end += 1;
            }
            if end < keys.len() && keys[end].number() < x {
                return Err(Error::Invalid("learned unsorted keys"));
            }
            if let Some(mut s) = current {
                let dx = (x - s.key) as f64;
                let dy = (end - s.rank) as f64;
                let lo = lower.max((dy - epsilon as f64) / dx);
                let hi = upper.min((dy + epsilon as f64) / dx);
                if lo <= hi {
                    lower = lo;
                    upper = hi;
                    s.end_key = x;
                    s.end_rank = end;
                    current = Some(s);
                } else {
                    s.slope = if upper.is_finite() {
                        (lower + upper) * 0.5
                    } else {
                        0.0
                    };
                    push_segment(&mut segments, s, key_bytes, budget)?;
                    current = None;
                }
            }
            if current.is_none() {
                current = Some(Segment {
                    key: x,
                    rank: end,
                    end_key: x,
                    end_rank: end,
                    slope: 0.0,
                });
                lower = 0.0;
                upper = f64::INFINITY;
            }
            i = end;
        }
        if let Some(mut s) = current {
            s.slope = if upper.is_finite() {
                (lower + upper) * 0.5
            } else {
                0.0
            };
            push_segment(&mut segments, s, key_bytes, budget)?;
        }
        let mut model = Self {
            keys,
            segments,
            radix: Vec::new(),
            shift: 0,
            epsilon,
        };
        if model.segments.len() > 1 {
            let span = model.keys.last().unwrap().number() - model.keys[0].number();
            let bits = (usize::BITS - (model.segments.len() - 1).leading_zeros()).min(12);
            model.shift = (64 - span.leading_zeros()).saturating_sub(bits);
            let buckets = (span >> model.shift) as usize + 1;
            let required = (buckets + 1) * 4;
            if model.allocation_bytes().iter().sum::<usize>() + required > budget {
                return Err(Error::Limit("learned radix budget"));
            }
            model.radix.try_reserve_exact(buckets + 1)?;
            let mut index = 0;
            for prefix in 0..=buckets {
                while index < model.segments.len()
                    && ((model.segments[index].key - model.keys[0].number()) >> model.shift)
                        < prefix as u64
                {
                    index += 1;
                }
                model.radix.push(
                    u32::try_from(index).map_err(|_| Error::Limit("learned segment positions"))?,
                );
            }
        }
        if model.allocation_bytes().iter().sum::<usize>() > budget {
            return Err(Error::Limit("learned capacity"));
        }
        Ok(model)
    }

    pub(crate) fn allocation_bytes(&self) -> [usize; 3] {
        [
            self.keys.capacity() * size_of::<K>(),
            self.segments.capacity() * size_of::<Segment>(),
            self.radix.capacity() * 4,
        ]
    }
    pub(crate) fn dimensions(&self) -> [usize; 3] {
        [self.keys.len(), self.segments.len(), self.radix.len()]
    }

    #[inline(never)]
    fn estimate(&self, query: u64, method: usize) -> usize {
        let index = if self.segments.len() == 1 {
            0
        } else if method == 2 {
            let prefix = ((query - self.keys[0].number()) >> self.shift) as usize;
            let low = (self.radix[prefix] as usize).saturating_sub(1);
            let high = self.radix[prefix + 1] as usize;
            low + self.segments[low..high].partition_point(|s| s.key <= query) - 1
        } else {
            self.segments.partition_point(|s| s.key <= query) - 1
        };
        let s = self.segments[index];
        if query > s.end_key {
            return s.end_rank;
        }
        // Subtract integers before conversion, retaining small differences near u64::MAX.
        ((query - s.key) as f64 * s.slope + s.rank as f64) as usize
    }

    #[inline(never)]
    pub(crate) fn search(&self, query: u64, method: usize) -> Bound {
        let n = self.keys.len();
        if n == 0 || query < self.keys[0].number() {
            return Bound {
                index: 0,
                fallback: false,
                window: 0,
            };
        }
        if query >= self.keys[n - 1].number() {
            return Bound {
                index: n,
                fallback: false,
                window: 0,
            };
        }
        if method == 0 {
            return Bound {
                index: self.keys.partition_point(|k| k.number() <= query),
                fallback: false,
                window: n,
            };
        }
        let estimate = if method == 3 {
            0
        } else {
            self.estimate(query, method).min(n)
        };
        let radius = self.epsilon + 2;
        let low = estimate.saturating_sub(radius);
        let high = estimate.saturating_add(radius + 1).min(n);
        let index = low + self.keys[low..high].partition_point(|k| k.number() <= query);
        // These global neighbors prove the exact upper bound even if the model is wrong.
        let valid = (index == 0 || self.keys[index - 1].number() <= query)
            && (index == n || self.keys[index].number() > query);
        Bound {
            index: if valid {
                index
            } else {
                self.keys.partition_point(|k| k.number() <= query)
            },
            fallback: !valid,
            window: high - low,
        }
    }
}

fn push_segment(
    segments: &mut Vec<Segment>,
    segment: Segment,
    key_bytes: usize,
    budget: usize,
) -> Result<(), Error> {
    let bytes = (segments.len() + 1)
        .checked_mul(size_of::<Segment>())
        .and_then(|n| n.checked_add(key_bytes))
        .ok_or(Error::Limit("learned segment bytes"))?;
    if bytes > budget {
        return Err(Error::Limit("learned segment budget"));
    }
    if segments.len() == segments.capacity() {
        let maximum = (budget - key_bytes) / size_of::<Segment>();
        let capacity = segments.capacity().max(1).saturating_mul(2).min(maximum);
        if capacity <= segments.len() {
            return Err(Error::Limit("learned segment capacity"));
        }
        segments.try_reserve_exact(capacity - segments.len())?;
    }
    segments.push(segment);
    Ok(())
}

pub(crate) struct Cache {
    pub(crate) epsilon: usize,
    pub(crate) asns: Option<Model<u32>>,
    pub(crate) v4: Option<Model<u32>>,
    pub(crate) v6: Option<(u128, Model<u64>)>,
}
impl Cache {
    pub(crate) fn allocation_bytes(&self) -> [usize; 9] {
        let a = self.asns.as_ref().map_or([0; 3], Model::allocation_bytes);
        let b = self.v4.as_ref().map_or([0; 3], Model::allocation_bytes);
        let c = self
            .v6
            .as_ref()
            .map_or([0; 3], |(_, m)| m.allocation_bytes());
        [a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]
    }
    pub(crate) fn dimensions(&self) -> [[usize; 3]; 3] {
        [
            self.asns.as_ref().map_or([0; 3], Model::dimensions),
            self.v4.as_ref().map_or([0; 3], Model::dimensions),
            self.v6.as_ref().map_or([0; 3], |(_, m)| m.dimensions()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predictions_match_independent_linear_ranks_and_guards_recover_bad_models() {
        let mut fixtures = vec![
            vec![],
            vec![0],
            vec![7; 513],
            vec![0, 0, 1, u64::MAX - 1, u64::MAX],
        ];
        fixtures.push((0..4097).map(|i| i * i + i).collect());
        fixtures.push((0..4097).map(|i| (i / 64) * 1_000_000 + i % 64).collect());
        fixtures.push((0..4097).map(|i| u64::MAX - 100_000 + i * 13).collect());
        fixtures.push(
            (0..2048)
                .map(|i| if i < 1024 { 0 } else { u64::MAX })
                .collect(),
        );
        for keys in fixtures {
            let mut queries = vec![0, 1, u64::MAX / 2, u64::MAX - 1, u64::MAX];
            for &key in keys.iter().step_by(17) {
                queries.extend([key.saturating_sub(1), key, key.saturating_add(1)]);
            }
            for epsilon in [0, 1, 4, 32, 128] {
                let mut model = Model::new(keys.clone(), epsilon, BUDGET).unwrap();
                for &q in &queries {
                    let want = keys.iter().filter(|&&k| k <= q).count();
                    for method in 0..=3 {
                        assert_eq!(
                            model.search(q, method).index,
                            want,
                            "q={q} e={epsilon} m={method}"
                        );
                    }
                }
                for segment in &mut model.segments {
                    segment.slope = f64::NAN;
                }
                for &q in &queries {
                    assert_eq!(
                        model.search(q, 2).index,
                        keys.iter().filter(|&&k| k <= q).count()
                    );
                }
            }
        }
        let keys: Vec<u32> = (0..4097).map(|i| i * 3).collect();
        let model = Model::new(keys, 4, BUDGET).unwrap();
        assert!(model.search(8000, 3).fallback);
    }
    #[test]
    fn allocation_failures_sortedness_and_unsigned_widths() {
        assert!(matches!(
            Model::new(vec![2u32, 1], 4, BUDGET),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            Model::new(vec![1u32; 10], 4, 39),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            Model::new(vec![1u32; 10], 4, 40),
            Err(Error::Limit(_))
        ));
        for keys in [vec![0u32, u32::MAX], vec![0, 1, 1, 2, 2, 2, u32::MAX]] {
            let model = Model::new(keys.clone(), 4, BUDGET).unwrap();
            for q in [0, 1, 2, u32::MAX / 2, u32::MAX] {
                for method in 0..=3 {
                    assert_eq!(
                        model.search(q as u64, method).index,
                        keys.iter().filter(|&&k| k <= q).count()
                    );
                }
            }
        }
    }
}
