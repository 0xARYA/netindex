//! Research-only, runtime-guarded packed-column tails.
#![deny(unsafe_op_in_unsafe_fn)]
use crate::{Error, search::Search};

pub fn study_available(mode: u8) -> bool {
    match mode {
        0 | 1 => true,
        #[cfg(target_arch = "x86_64")]
        2 => std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw"),
        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        3 | 4 => std::arch::is_aarch64_feature_detected!("neon"),
        _ => false,
    }
}

pub fn study_kernel(bytes: &[u8], width: usize, target: u128, mode: u8) -> Result<usize, Error> {
    if mode > 4 {
        return Err(Error::Invalid("study mode"));
    }
    Search::study(mode).upper_bound(bytes, width, target)
}

pub(crate) fn upper(bytes: &[u8], width: usize, target: u128, mode: u8) -> Result<usize, Error> {
    #[cfg(target_arch = "x86_64")]
    if mode == 2 && study_available(2) && bytes.len() >= 64 {
        // SAFETY: feature detection established F/BW; Search validated column and width.
        return unsafe {
            match width {
                1 => avx512::<1>(bytes, target),
                2 => avx512::<2>(bytes, target),
                4 => avx512::<4>(bytes, target),
                8 => avx512::<8>(bytes, target),
                16 => avx512::<16>(bytes, target),
                _ => Err(Error::Invalid("packed integer width")),
            }
        };
    }
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    if matches!(mode, 3 | 4) && study_available(mode) && width <= 8 && bytes.len() >= 16 {
        // SAFETY: NEON detected; Search validated width and complete vector length.
        return unsafe {
            match (width, mode == 4) {
                (1, false) => neon::<1, false>(bytes, target),
                (2, false) => neon::<2, false>(bytes, target),
                (4, false) => neon::<4, false>(bytes, target),
                (8, false) => neon::<8, false>(bytes, target),
                (1, true) => neon::<1, true>(bytes, target),
                (2, true) => neon::<2, true>(bytes, target),
                (4, true) => neon::<4, true>(bytes, target),
                (8, true) => neon::<8, true>(bytes, target),
                _ => Err(Error::Invalid("packed integer width")),
            }
        };
    }
    let _ = mode;
    Search::study(1).upper_bound(bytes, width, target)
}

fn start<const W: usize>(bytes: &[u8], target: u128, lanes: usize) -> Result<usize, Error> {
    let count = bytes.len() / W;
    let (mut low, mut high) = (0, count);
    while high - low > lanes {
        let mid = low + (high - low) / 2;
        let row = bytes
            .get(mid * W..(mid + 1) * W)
            .ok_or(Error::Invalid("packed search column"))?;
        let mut value = [0; 16];
        value[..W].copy_from_slice(row);
        if u128::from_le_bytes(value) <= target {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    Ok(low.min(count - lanes))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f,avx512bw")]
unsafe fn avx512<const W: usize>(bytes: &[u8], target: u128) -> Result<usize, Error> {
    use std::arch::x86_64::*;
    let lanes = 64 / W;
    let first = start::<W>(bytes, target, lanes)?;
    // SAFETY: first <= count - lanes, hence the complete 64-byte load is in bounds.
    let values = unsafe { _mm512_loadu_si512(bytes.as_ptr().add(first * W).cast()) };
    let mask: u64 = match W {
        1 => _mm512_cmp_epu8_mask::<_MM_CMPINT_NLE>(values, _mm512_set1_epi8(target as i8)),
        2 => u64::from(_mm512_cmp_epu16_mask::<_MM_CMPINT_NLE>(
            values,
            _mm512_set1_epi16(target as i16),
        )),
        4 => u64::from(_mm512_cmp_epu32_mask::<_MM_CMPINT_NLE>(
            values,
            _mm512_set1_epi32(target as i32),
        )),
        8 => u64::from(_mm512_cmp_epu64_mask::<_MM_CMPINT_NLE>(
            values,
            _mm512_set1_epi64(target as i64),
        )),
        16 => {
            let lo = target as i64;
            let hi = (target >> 64) as i64;
            let q = _mm512_set_epi64(hi, lo, hi, lo, hi, lo, hi, lo);
            let greater = _mm512_cmp_epu64_mask::<_MM_CMPINT_NLE>(values, q);
            let equal = _mm512_cmp_epu64_mask::<_MM_CMPINT_EQ>(values, q);
            // Each u128 is [lo, hi]; compare hi first, then lo only when hi equals.
            u64::from(((greater >> 1) | ((equal >> 1) & greater)) & 0x55)
        }
        _ => return Err(Error::Invalid("packed integer width")),
    };
    let offset = if mask == 0 {
        lanes
    } else {
        mask.trailing_zeros() as usize / if W == 16 { 2 } else { 1 }
    };
    Ok(first + offset)
}

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
#[target_feature(enable = "neon")]
unsafe fn neon<const W: usize, const PACKED: bool>(
    bytes: &[u8],
    target: u128,
) -> Result<usize, Error> {
    use std::arch::aarch64::*;
    let lanes = 16 / W;
    let first = start::<W>(bytes, target, lanes)?;
    // SAFETY: first <= count - lanes, bounding this complete 16-byte load.
    let values = unsafe { vld1q_u8(bytes.as_ptr().add(first * W)) };
    let greater = match W {
        1 => vcgtq_u8(values, vdupq_n_u8(target as u8)),
        2 => vreinterpretq_u8_u16(vcgtq_u16(
            vreinterpretq_u16_u8(values),
            vdupq_n_u16(target as u16),
        )),
        4 => vreinterpretq_u8_u32(vcgtq_u32(
            vreinterpretq_u32_u8(values),
            vdupq_n_u32(target as u32),
        )),
        8 => vreinterpretq_u8_u64(vcgtq_u64(
            vreinterpretq_u64_u8(values),
            vdupq_n_u64(target as u64),
        )),
        _ => return Err(Error::Invalid("packed integer width")),
    };
    if PACKED {
        // Each all-zero/all-one byte becomes four bits in the narrowed word.
        // A zero word has 64 trailing zeros, yielding the one-past-vector index.
        let mask = vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(vreinterpretq_u16_u8(
            greater,
        ))));
        return Ok(first + mask.trailing_zeros() as usize / (4 * W));
    }
    // AArch64 is little endian on the qualified runner. Store masks in lane order;
    // this explicit reduction avoids assuming a non-existent NEON movemask.
    let mut mask = [0u8; 16];
    // SAFETY: mask has space for the complete 16-byte vector.
    unsafe { vst1q_u8(mask.as_mut_ptr(), greater) };
    Ok(first + mask.iter().position(|&byte| byte != 0).unwrap_or(16) / W)
}
