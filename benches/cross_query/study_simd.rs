//! Research-only cross-query AVX2 search; backing bytes are already validated.
use crate::{Error, layout::u128_at};

pub(crate) fn lower_asns32(
    bytes: &[u8],
    queries: &[u32],
    enable: bool,
) -> Result<[usize; 8], Error> {
    if queries.len() > 8 || !bytes.len().is_multiple_of(8) {
        return Err(Error::Invalid("vector ASN rows"));
    }
    #[cfg(target_arch = "x86_64")]
    if enable && bytes.len() / 8 <= i32::MAX as usize && std::is_x86_feature_detected!("avx2") {
        // SAFETY: checked complete rows and signed-index range; AVX2 detected.
        return Ok(unsafe { vector32(bytes, queries) });
    }
    lower_asns::<8>(bytes, queries, false)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn vector32(bytes: &[u8], queries: &[u32]) -> [usize; 8] {
    use std::arch::x86_64::*;
    let keys: [u32; 8] = std::array::from_fn(|i| queries.get(i).copied().unwrap_or(0));
    let high: [u32; 8] = std::array::from_fn(|i| {
        if i < queries.len() {
            (bytes.len() / 8) as u32
        } else {
            0
        }
    });
    let mut output = [0u32; 8];
    // SAFETY: arrays contain eight u32s; active midpoint indices are nonnegative
    // and strictly below row count <= i32::MAX. Scale eight addresses the key
    // half of a complete ASN row; masked-off lanes perform no memory access.
    unsafe {
        let zero = _mm256_setzero_si256();
        let one = _mm256_set1_epi32(1);
        let sign = _mm256_set1_epi32(i32::MIN);
        let query = _mm256_xor_si256(_mm256_loadu_si256(keys.as_ptr().cast()), sign);
        let mut lo = zero;
        let mut hi = _mm256_loadu_si256(high.as_ptr().cast());
        loop {
            let active = _mm256_cmpgt_epi32(hi, lo);
            if _mm256_movemask_epi8(active) == 0 {
                break;
            }
            let mid = _mm256_add_epi32(lo, _mm256_srli_epi32::<1>(_mm256_sub_epi32(hi, lo)));
            let key = _mm256_mask_i32gather_epi32::<8>(zero, bytes.as_ptr().cast(), mid, active);
            let go = _mm256_and_si256(
                active,
                _mm256_cmpgt_epi32(query, _mm256_xor_si256(key, sign)),
            );
            lo = _mm256_blendv_epi8(lo, _mm256_add_epi32(mid, one), go);
            hi = _mm256_blendv_epi8(hi, mid, _mm256_andnot_si256(go, active));
        }
        _mm256_storeu_si256(output.as_mut_ptr().cast(), lo);
    }
    output.map(|value| value as usize)
}

pub(crate) fn lower_asns<const N: usize>(
    bytes: &[u8],
    queries: &[u32],
    enable: bool,
) -> Result<[usize; N], Error> {
    if queries.len() > N || !bytes.len().is_multiple_of(8) {
        return Err(Error::Invalid("vector ASN rows"));
    }
    let keys = std::array::from_fn(|i| queries.get(i).copied().unwrap_or(0) as u128);
    let high = std::array::from_fn(|i| {
        if i < queries.len() {
            bytes.len() / 8
        } else {
            0
        }
    });
    bounds::<N, false>(bytes, [0; N], high, keys, enable)
}

pub(crate) fn upper_fences<const N: usize>(
    bytes: &[u8],
    low: [usize; N],
    high: [usize; N],
    queries: &[Option<u128>],
    enable: bool,
) -> Result<[usize; N], Error> {
    if queries.len() > N {
        return Err(Error::Invalid("vector fence queries"));
    }
    let keys = std::array::from_fn(|i| queries.get(i).copied().flatten().unwrap_or(0));
    bounds::<N, true>(bytes, low, high, keys, enable)
}

fn bounds<const N: usize, const FENCES: bool>(
    bytes: &[u8],
    mut low: [usize; N],
    high: [usize; N],
    keys: [u128; N],
    enable: bool,
) -> Result<[usize; N], Error> {
    let stride = if FENCES { 32 } else { 8 };
    if (N != 4 && N != 8)
        || low
            .iter()
            .zip(high)
            .any(|(&lo, hi)| lo > hi || hi > bytes.len() / stride)
    {
        return Err(Error::Invalid("vector search bounds"));
    }
    if high.iter().all(|&hi| hi == 0) {
        return Ok(low);
    }
    #[cfg(target_arch = "x86_64")]
    if enable && std::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 is detected; N is 4/8, every half-open bound is within
        // complete rows, and masked-off lanes never access an indexed address.
        // Slice lengths fit isize, so scaled nonnegative gather offsets fit i64.
        return Ok(unsafe { vector::<N, FENCES>(bytes, low, high, keys) });
    }
    let _ = enable;
    for lane in 0..N {
        let mut hi = high[lane];
        while low[lane] < hi {
            let mid = low[lane] + (hi - low[lane]) / 2;
            let key = if FENCES {
                u128_at(bytes, mid * stride)?
            } else {
                u32::from_le_bytes(
                    bytes[mid * 8..mid * 8 + 4]
                        .try_into()
                        .map_err(|_| Error::Invalid("vector ASN key"))?,
                ) as u128
            };
            if if FENCES {
                key <= keys[lane]
            } else {
                key < keys[lane]
            } {
                low[lane] = mid + 1;
            } else {
                hi = mid;
            }
        }
    }
    Ok(low)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn vector<const N: usize, const FENCES: bool>(
    bytes: &[u8],
    mut low: [usize; N],
    high: [usize; N],
    keys: [u128; N],
) -> [usize; N] {
    use std::arch::x86_64::*;
    // SAFETY: caller validates AVX2, N, complete rows and search bounds. Each
    // active midpoint stays within its original half-open bound. Masked gathers
    // read one full 8-byte ASN row, or the two 8-byte halves of a 32-byte fence.
    // Array loads/stores cover four 64-bit elements at offsets zero/four only.
    // The second fence base pointer is formed only for nonempty complete rows.
    unsafe {
        let zero = _mm256_setzero_si256();
        let one = _mm256_set1_epi64x(1);
        let sign = _mm256_set1_epi64x(i64::MIN);
        let keylo: [u64; N] = keys.map(|q| q as u64);
        let keyhi: [u64; N] = keys.map(|q| (q >> 64) as u64);
        let mut lows = [zero; 2];
        let mut highs = [zero; 2];
        let mut qlo = [zero; 2];
        let mut qhi = [zero; 2];
        for group in 0..N / 4 {
            lows[group] = _mm256_loadu_si256(low.as_ptr().add(group * 4).cast());
            highs[group] = _mm256_loadu_si256(high.as_ptr().add(group * 4).cast());
            qlo[group] = _mm256_xor_si256(
                _mm256_loadu_si256(keylo.as_ptr().add(group * 4).cast()),
                sign,
            );
            qhi[group] = _mm256_xor_si256(
                _mm256_loadu_si256(keyhi.as_ptr().add(group * 4).cast()),
                sign,
            );
        }
        loop {
            let mut pending = false;
            for group in 0..N / 4 {
                let lo = lows[group];
                let hi = highs[group];
                let active = _mm256_cmpgt_epi64(hi, lo);
                if _mm256_movemask_epi8(active) == 0 {
                    continue;
                }
                pending = true;
                let mid = _mm256_add_epi64(lo, _mm256_srli_epi64::<1>(_mm256_sub_epi64(hi, lo)));
                let indices = if FENCES {
                    _mm256_slli_epi64::<2>(mid)
                } else {
                    mid
                };
                let first =
                    _mm256_mask_i64gather_epi64::<8>(zero, bytes.as_ptr().cast(), indices, active);
                let right = if FENCES {
                    let second = _mm256_mask_i64gather_epi64::<8>(
                        zero,
                        bytes.as_ptr().add(8).cast(),
                        indices,
                        active,
                    );
                    let less_hi = _mm256_cmpgt_epi64(qhi[group], _mm256_xor_si256(second, sign));
                    let equal_hi = _mm256_cmpeq_epi64(qhi[group], _mm256_xor_si256(second, sign));
                    let greater_lo = _mm256_cmpgt_epi64(_mm256_xor_si256(first, sign), qlo[group]);
                    _mm256_or_si256(less_hi, _mm256_andnot_si256(greater_lo, equal_hi))
                } else {
                    let value = _mm256_and_si256(first, _mm256_set1_epi64x(u32::MAX as i64));
                    _mm256_cmpgt_epi64(qlo[group], _mm256_xor_si256(value, sign))
                };
                let go = _mm256_and_si256(active, right);
                lows[group] = _mm256_blendv_epi8(lo, _mm256_add_epi64(mid, one), go);
                highs[group] = _mm256_blendv_epi8(hi, mid, _mm256_andnot_si256(go, active));
            }
            if !pending {
                break;
            }
        }
        for (group, &value) in lows.iter().take(N / 4).enumerate() {
            _mm256_storeu_si256(low.as_mut_ptr().add(group * 4).cast(), value);
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check<const N: usize>() {
        for count in [0, 1, 2, 3, 4, 7, 8, 9, 17, 64, 511] {
            let asns: Vec<u32> = (0..count)
                .map(|i| (u32::MAX as u64 * i as u64 / count.max(1) as u64) as u32)
                .collect();
            let fences: Vec<u128> = (0..count)
                .map(|i| (u128::MAX / count.max(1) as u128) * i as u128)
                .collect();
            let mut numbers = vec![0, 1, u32::MAX, 1 << 31];
            let mut addresses = vec![0, 1, u128::MAX, 1 << 127, 1 << 64, (1 << 64) - 1];
            for (&asn, &fence) in asns.iter().zip(&fences) {
                numbers.extend([asn.saturating_sub(1), asn, asn.saturating_add(1)]);
                addresses.extend([fence.saturating_sub(1), fence, fence.saturating_add(1)]);
            }
            for offset in 0..32 {
                let mut a = vec![0; offset];
                let mut f = vec![0; offset];
                for (id, (&asn, &fence)) in asns.iter().zip(&fences).enumerate() {
                    a.extend(asn.to_le_bytes());
                    a.extend((id as u32).to_le_bytes());
                    f.extend(fence.to_le_bytes());
                    f.extend([0; 16]);
                }
                for q in numbers.chunks(N) {
                    let expected: [usize; N] = std::array::from_fn(|i| {
                        q.get(i)
                            .map(|key| asns.iter().position(|v| v >= key).unwrap_or(count))
                            .unwrap_or(0)
                    });
                    for enable in [false, true] {
                        assert_eq!(lower_asns::<N>(&a[offset..], q, enable).unwrap(), expected);
                        let native = lower_asns32(&a[offset..], q, enable).unwrap();
                        assert_eq!(&native[..N], &expected);
                        assert!(native[q.len()..].iter().all(|&value| value == 0));
                    }
                }
                for (batch, q) in addresses.chunks(N).enumerate() {
                    let queries: Vec<_> = q
                        .iter()
                        .enumerate()
                        .map(|(i, &key)| {
                            if (i + batch) % 5 == 0 {
                                None
                            } else {
                                Some(key)
                            }
                        })
                        .collect();
                    let low = std::array::from_fn(|i| {
                        if queries.get(i).is_some_and(Option::is_some) && count > 2 {
                            1
                        } else {
                            0
                        }
                    });
                    let high = std::array::from_fn(|i| {
                        if queries.get(i).is_some_and(Option::is_some) {
                            count
                        } else {
                            0
                        }
                    });
                    let expected: [usize; N] = std::array::from_fn(|i| {
                        low[i]
                            + queries
                                .get(i)
                                .copied()
                                .flatten()
                                .map(|key| {
                                    fences[low[i]..high[i]]
                                        .iter()
                                        .take_while(|&&v| v <= key)
                                        .count()
                                })
                                .unwrap_or(0)
                    });
                    for enable in [false, true] {
                        assert_eq!(
                            upper_fences::<N>(&f[offset..], low, high, &queries, enable).unwrap(),
                            expected
                        );
                    }
                }
            }
        }
        assert!(lower_asns::<N>(&[0; 7], &[0], true).is_err());
        assert!(upper_fences::<N>(&[], [0; N], [usize::MAX; N], &[Some(0)], true).is_err());
        assert!(upper_fences::<N>(&[0; 32], [1; N], [0; N], &[Some(0)], true).is_err());
    }

    #[test]
    fn vector_four_matches_linear_oracles_at_every_alignment() {
        check::<4>();
    }
    #[test]
    fn vector_eight_matches_linear_oracles_at_every_alignment() {
        check::<8>();
    }
}
