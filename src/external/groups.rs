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
        let (matches, empty) = masks(bytes, fingerprint(hash));
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
