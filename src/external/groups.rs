use crate::Error;

/// Resident fingerprints for the private disk table, charged to its cache budget.
#[derive(Default)]
pub(super) struct Groups {
    bytes: Vec<u8>,
}

pub(super) struct Probe {
    pub(super) base: u64,
    pub(super) candidates: u16,
    pub(super) empty: Option<u64>,
    pub(super) count: u64,
}

impl Groups {
    pub(super) fn bytes(&self) -> usize {
        self.bytes.capacity()
    }

    pub(super) fn enabled(&self) -> bool {
        !self.bytes.is_empty()
    }

    pub(super) fn reset(&mut self, capacity: u64, budget: usize) -> Result<(), Error> {
        // Old metadata is unnecessary during sequential rehash. Drop it before
        // reserving the replacement so both allocations cannot consume the budget.
        self.bytes = Vec::new();
        let Ok(capacity) = usize::try_from(capacity) else {
            return Ok(());
        };
        if capacity > budget {
            return Ok(());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity)?;
        if bytes.capacity() > budget {
            return Err(Error::Limit("disk table cache bytes"));
        }
        bytes.resize(capacity, 0);
        self.bytes = bytes;
        Ok(())
    }

    pub(super) fn set(&mut self, position: u64, hash: u64) -> Result<(), Error> {
        if self.enabled() {
            *self
                .bytes
                .get_mut(position as usize)
                .ok_or(Error::Invalid("payload table fingerprint"))? = fingerprint(hash);
        }
        Ok(())
    }

    pub(super) fn probe(&self, position: u64, hash: u64) -> Result<Probe, Error> {
        let base = position & !15;
        let lane = (position & 15) as u32;
        let bytes: &[u8; 16] = self
            .bytes
            .get(base as usize..base as usize + 16)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(Error::Invalid("payload table fingerprints"))?;
        let tag = fingerprint(hash);
        let first = *bytes
            .get(lane as usize)
            .ok_or(Error::Invalid("payload table fingerprint"))?;
        // Hits and empty home slots are common. Scan the whole group only
        // when this slot is occupied by an unrelated fingerprint.
        if first == 0 || first == tag {
            return Ok(Probe {
                base,
                candidates: if first == tag { 1 << lane } else { 0 },
                empty: (first == 0).then_some(position),
                count: 1,
            });
        }
        let (matches, empty) = masks(bytes, tag);
        let remaining = u16::MAX << lane;
        let empty = empty & remaining;
        let stop = if empty == 0 {
            16
        } else {
            empty.trailing_zeros()
        };
        Ok(Probe {
            base,
            candidates: matches & remaining & ((1u32 << stop) - 1) as u16,
            empty: (stop < 16).then_some(base + u64::from(stop)),
            count: u64::from(16 - lane),
        })
    }
}

fn fingerprint(hash: u64) -> u8 {
    (hash >> 57) as u8 | 0x80
}

fn masks(bytes: &[u8; 16], tag: u8) -> (u16, u16) {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    {
        use std::arch::x86_64::{
            _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8, _mm_setzero_si128,
        };
        // SAFETY: SSE2 is baseline on x86-64 and the input contains 16 readable bytes.
        unsafe {
            let group = _mm_loadu_si128(bytes.as_ptr().cast());
            (
                _mm_movemask_epi8(_mm_cmpeq_epi8(group, _mm_set1_epi8(tag as i8))) as u16,
                _mm_movemask_epi8(_mm_cmpeq_epi8(group, _mm_setzero_si128())) as u16,
            )
        }
    }
    #[cfg(not(all(feature = "simd", target_arch = "x86_64")))]
    {
        let mut matches = 0;
        let mut empty = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            matches |= u16::from(byte == tag) << index;
            empty |= u16::from(byte == 0) << index;
        }
        (matches, empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tags_match_independent_lane_masks() {
        for value in 0..=u8::MAX {
            for tag in 0x80..=u8::MAX {
                for lane in 0..16 {
                    let mut bytes = [0; 16];
                    bytes[lane] = value;
                    let expected_matches = if value == tag { 1 << lane } else { 0 };
                    let expected_empty = if value == 0 {
                        u16::MAX
                    } else {
                        u16::MAX ^ (1 << lane)
                    };
                    assert_eq!(masks(&bytes, tag), (expected_matches, expected_empty));
                }
            }
        }
    }

    #[test]
    fn probe_ignores_earlier_lanes_and_candidates_after_first_empty() {
        let mut groups = Groups::default();
        groups.reset(64, 64).unwrap();
        // Same fingerprint, different full hashes.
        for position in [0, 1, 3, 15, 63] {
            groups.set(position, position * 16).unwrap();
        }
        let probe = groups.probe(1, 7).unwrap();
        assert_eq!(probe.base, 0);
        assert_eq!(probe.candidates, 1 << 1);
        assert_eq!(probe.empty, None);
        assert_eq!(probe.count, 1);
        assert_eq!(groups.probe(2, 7).unwrap().empty, Some(2));
        groups.set(3, 1 << 57).unwrap();
        let probe = groups.probe(1, 1 << 57).unwrap();
        assert_eq!(probe.candidates, 0);
        assert_eq!(probe.empty, Some(2));
        let probe = groups.probe(63, 7).unwrap();
        assert_eq!(probe.candidates, 1 << 15);
        assert_eq!(probe.empty, None);
        assert_eq!(probe.count, 1);

        groups.reset(128, 64).unwrap();
        assert!(!groups.enabled());
        assert_eq!(groups.bytes(), 0);
    }
}
