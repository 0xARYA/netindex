#[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
use std::arch::aarch64::*;
#[cfg(all(feature = "simd", target_arch = "x86_64"))]
use std::arch::x86_64::*;

use crate::Error;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Search {
    #[cfg(all(feature = "simd", target_arch = "x86_64"))]
    avx2: bool,
    #[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
    neon: bool,
}

impl Search {
    pub(crate) fn detect() -> Self {
        Self {
            #[cfg(all(feature = "simd", target_arch = "x86_64"))]
            avx2: std::is_x86_feature_detected!("avx2"),
            #[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
            neon: std::arch::is_aarch64_feature_detected!("neon"),
        }
    }

    pub(crate) fn upper_bound(
        self,
        bytes: &[u8],
        width: usize,
        target: u128,
    ) -> Result<usize, Error> {
        if !matches!(width, 1 | 2 | 4 | 8 | 16) || !bytes.len().is_multiple_of(width) {
            return Err(Error::Invalid("packed search column"));
        }

        let count = bytes.len() / width;
        if width < 16 && target >= (1u128 << (width * 8)) - 1 {
            return Ok(count);
        }

        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
        if self.avx2 && width <= 8 && count >= 32 / width {
            // SAFETY: detect() established AVX2 support; the complete column has
            // at least one vector and the search bounds its unaligned load.
            return unsafe {
                match width {
                    1 => avx2_tail::<1>(bytes, target),
                    2 => avx2_tail::<2>(bytes, target),
                    4 => avx2_tail::<4>(bytes, target),
                    8 => avx2_tail::<8>(bytes, target),
                    _ => binary(bytes, width, target),
                }
            };
        }

        #[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
        if self.neon && width <= 8 && bytes.len() >= 16 {
            // SAFETY: detect() established NEON support and upper_bound
            // validated the complete column and the minimum load length.
            return unsafe {
                match width {
                    1 => neon_tail::<1>(bytes, target),
                    2 => neon_tail::<2>(bytes, target),
                    4 => neon_tail::<4>(bytes, target),
                    8 => neon_tail::<8>(bytes, target),
                    _ => binary(bytes, width, target),
                }
            };
        }

        binary(bytes, width, target)
    }
}

fn binary(bytes: &[u8], width: usize, target: u128) -> Result<usize, Error> {
    match width {
        1 => binary_width::<1>(bytes, target),
        2 => binary_width::<2>(bytes, target),
        4 => binary_width::<4>(bytes, target),
        8 => binary_width::<8>(bytes, target),
        16 => binary_width::<16>(bytes, target),
        _ => Err(Error::Invalid("packed integer width")),
    }
}

fn binary_width<const WIDTH: usize>(bytes: &[u8], target: u128) -> Result<usize, Error> {
    let (values, _) = bytes.as_chunks::<WIDTH>();
    let (mut low, mut high) = (0, values.len());

    while low < high {
        let mid = low + (high - low) / 2;
        let row = values
            .get(mid)
            .ok_or(Error::Invalid("packed search column"))?;
        let mut value = [0u8; 16];
        value
            .get_mut(..WIDTH)
            .ok_or(Error::Invalid("packed integer width"))?
            .copy_from_slice(row);

        let value = u128::from_le_bytes(value);
        // upper_bound returns early when the target exceeds this width.
        let less_or_equal = if WIDTH <= 8 {
            value as u64 <= target as u64
        } else {
            value <= target
        };

        if less_or_equal {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    Ok(low)
}

#[cfg(all(feature = "simd", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn avx2_tail<const WIDTH: usize>(bytes: &[u8], target: u128) -> Result<usize, Error> {
    let lanes = 32 / WIDTH;
    let (column, _) = bytes.as_chunks::<WIDTH>();
    let count = column.len();
    let target = target as u64;
    let (mut low, mut high) = (0, count);

    while high - low > lanes {
        let mid = low + (high - low) / 2;
        let row = column
            .get(mid)
            .ok_or(Error::Invalid("packed search column"))?;
        let mut value = [0; 8];
        value
            .get_mut(..WIDTH)
            .ok_or(Error::Invalid("packed integer width"))?
            .copy_from_slice(row);
        if u64::from_le_bytes(value) <= target {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    let start = low.min(count - lanes);

    // SAFETY: start <= count - lanes, lanes * WIDTH == 32, and bytes is a
    // complete column. The load is within the slice even for a partial block.
    let values = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(start * WIDTH).cast()) };

    let greater = match WIDTH {
        1 => _mm256_cmpgt_epi8(
            _mm256_xor_si256(values, _mm256_set1_epi8(i8::MIN)),
            _mm256_set1_epi8((target as u8 ^ 0x80) as i8),
        ),
        2 => _mm256_cmpgt_epi16(
            _mm256_xor_si256(values, _mm256_set1_epi16(i16::MIN)),
            _mm256_set1_epi16((target as u16 ^ 0x8000) as i16),
        ),
        4 => _mm256_cmpgt_epi32(
            _mm256_xor_si256(values, _mm256_set1_epi32(i32::MIN)),
            _mm256_set1_epi32((target as u32 ^ 0x8000_0000) as i32),
        ),
        8 => _mm256_cmpgt_epi64(
            _mm256_xor_si256(values, _mm256_set1_epi64x(i64::MIN)),
            _mm256_set1_epi64x((target ^ 0x8000_0000_0000_0000) as i64),
        ),
        _ => return Err(Error::Invalid("packed integer width")),
    };
    let mask = _mm256_movemask_epi8(greater) as u32;

    Ok(start + mask.trailing_zeros() as usize / WIDTH)
}

#[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
#[target_feature(enable = "neon")]
unsafe fn neon_tail<const WIDTH: usize>(bytes: &[u8], target: u128) -> Result<usize, Error> {
    let lanes = 16 / WIDTH;
    let (column, _) = bytes.as_chunks::<WIDTH>();
    let count = column.len();
    let target = target as u64;
    let (mut low, mut high) = (0, count);

    while high - low > lanes {
        let mid = low + (high - low) / 2;
        let row = column
            .get(mid)
            .ok_or(Error::Invalid("packed search column"))?;
        let mut value = [0; 8];
        value
            .get_mut(..WIDTH)
            .ok_or(Error::Invalid("packed integer width"))?
            .copy_from_slice(row);
        if u64::from_le_bytes(value) <= target {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    let start = low.min(count - lanes);

    // SAFETY: start <= count - lanes and lanes * WIDTH == 16.
    let values = unsafe { vld1q_u8(bytes.as_ptr().add(start * WIDTH)) };

    let greater = match WIDTH {
        1 => vcgtq_u8(values, vdupq_n_u8(target as u8)),
        2 => vreinterpretq_u8_u16(vcgtq_u16(
            vreinterpretq_u16_u8(values),
            vdupq_n_u16(target as u16),
        )),
        4 => vreinterpretq_u8_u32(vcgtq_u32(
            vreinterpretq_u32_u8(values),
            vdupq_n_u32(target as u32),
        )),
        8 => vreinterpretq_u8_u64(vcgtq_u64(vreinterpretq_u64_u8(values), vdupq_n_u64(target))),
        _ => return Err(Error::Invalid("packed integer width")),
    };
    // Narrow each all-one or all-zero byte into four bits in lane order.
    // A zero mask yields the one-past-vector index.
    let mask = vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(vreinterpretq_u16_u8(
        greater,
    ))));
    Ok(start + mask.trailing_zeros() as usize / (4 * WIDTH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
    #[test]
    fn detected_neon_on_qualification_runner() {
        if std::env::var_os("NETINDEX_REQUIRE_NEON").is_some() {
            assert!(Search::detect().neon);
        }
    }

    #[test]
    fn randomized_columns_match_scalar_and_detected_search_at_every_alignment() {
        let scalar = Search {
            #[cfg(all(feature = "simd", target_arch = "x86_64"))]
            avx2: false,
            #[cfg(all(feature = "simd", target_arch = "aarch64", target_endian = "little"))]
            neon: false,
        };
        let mut state = 0x2026_1006_ef24_u64;

        for width in [1, 2, 4, 8, 16] {
            let mask = u128::MAX.checked_shr(128 - (width * 8) as u32).unwrap_or(0);
            for count in (0..=512).step_by(7) {
                let mut values = Vec::new();
                for _ in 0..count {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let upper = state;
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    values.push(((u128::from(upper) << 64) | u128::from(state)) & mask);
                }

                values.sort_unstable();

                for alignment in 0..32 {
                    let mut bytes = vec![0xa5; alignment];
                    for value in &values {
                        bytes.extend_from_slice(&value.to_le_bytes()[..width]);
                    }

                    for query in
                        values
                            .iter()
                            .step_by(13)
                            .copied()
                            .chain([0, mask / 2, mask, u128::MAX])
                    {
                        let expected = values.iter().filter(|&&value| value <= query).count();
                        for search in [scalar, Search::detect()] {
                            assert_eq!(
                                search
                                    .upper_bound(&bytes[alignment..], width, query)
                                    .unwrap(),
                                expected,
                                "width={width} count={count} alignment={alignment} query={query}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn scalar_and_detected_search_match_linear_unsigned_oracle() {
        for width in [1, 2, 4, 8, 16] {
            let maximum = u128::MAX.checked_shr(128 - (width * 8) as u32).unwrap_or(0);
            for count in [0, 1, 3, 4, 7, 8, 15, 16, 31, 32, 33, 127, 255, 256] {
                let mut values: Vec<_> = (0..count).map(|index| (index / 2) as u128).collect();
                if let [.., previous, last] = values.as_mut_slice() {
                    *previous = maximum / 2 + 1;
                    *last = maximum;
                }

                values.sort_unstable();

                let mut storage = vec![0xa5];
                for value in &values {
                    storage.extend_from_slice(&value.to_le_bytes()[..width]);
                }

                for query in values.iter().copied().chain([
                    0,
                    1,
                    128,
                    maximum / 2,
                    maximum / 2 + 1,
                    maximum,
                    u128::MAX,
                ]) {
                    let expected = values.iter().filter(|&&value| value <= query).count();

                    let scalar = Search {
                        #[cfg(all(feature = "simd", target_arch = "x86_64"))]
                        avx2: false,
                        #[cfg(all(
                            feature = "simd",
                            target_arch = "aarch64",
                            target_endian = "little"
                        ))]
                        neon: false,
                    };
                    for search in [scalar, Search::detect()] {
                        assert_eq!(
                            search.upper_bound(&storage[1..], width, query).unwrap(),
                            expected,
                            "width={width} count={count} query={query}"
                        );
                    }
                }
            }
        }

        assert!(Search::detect().upper_bound(&[0; 3], 2, 0).is_err());
        assert!(Search::detect().upper_bound(&[], 0, 0).is_err());
    }
}
