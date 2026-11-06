use std::io::Write;

use crate::{Error, layout::slice};

use super::{BLOCK, Block, FENCE, Width, block_delta, read_block};

pub(super) fn integer(bytes: &[u8], position: usize, bits: u8) -> Result<u128, Error> {
    if bits == 0 {
        return Ok(0);
    }
    let position = position * usize::from(bits);
    let at = position / 8;
    let shift = position % 8;
    let mask = u128::MAX >> (128 - bits);
    if bits <= 57 {
        let word = u64::from_le_bytes(
            slice(bytes, at, 8)?
                .try_into()
                .map_err(|_| Error::Invalid("bitpacked integer"))?,
        );
        return Ok(u128::from(word >> shift) & mask);
    }
    let word = u128::from_le_bytes(
        slice(bytes, at, 16)?
            .try_into()
            .map_err(|_| Error::Invalid("bitpacked integer"))?,
    );
    let mut value = word >> shift;
    if usize::from(bits) + shift > 128 {
        value |= u128::from(
            *bytes
                .get(at + 16)
                .ok_or(Error::Invalid("bitpacked integer"))?,
        ) << (128 - shift);
    }
    Ok(value & mask)
}

pub(super) fn upper_bound(
    bytes: &[u8],
    count: usize,
    bits: u8,
    target: u128,
) -> Result<usize, Error> {
    let (mut low, mut high) = (0, count);
    while low < high {
        let mid = low + (high - low) / 2;
        if integer(bytes, mid, bits)? <= target {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    Ok(low)
}

fn plan(bytes: &[u8], block: Block) -> Result<Block, Error> {
    let maximum = block_delta(bytes, block, block.count - 1)?;
    let bits = (128 - maximum.leading_zeros()) as u8;
    let mut selected = block;
    selected.width = Width::Bytes(block.width.bytes());
    if bits < 128
        && (block.count * usize::from(bits)).div_ceil(8) + 16
            < block.count * usize::from(block.width.bytes())
    {
        selected.width = Width::Bits(bits);
    }
    Ok(selected)
}

pub(crate) fn compacted_size(bytes: &[u8], count: usize) -> Result<usize, Error> {
    let mut length = count.div_ceil(BLOCK) * FENCE;
    for index in 0..count.div_ceil(BLOCK) {
        length += plan(bytes, read_block(bytes, count, index)?)?.data_bytes();
    }
    Ok(length)
}

fn encode(bytes: &[u8], original: Block, selected: Block, buffer: &mut [u8]) -> Result<(), Error> {
    for local in 0..original.count {
        let value = block_delta(bytes, original, local)?;
        if let Some(bits) = selected.width.bits() {
            let position = local * usize::from(bits);
            let at = position / 8;
            let shift = position % 8;
            let length = (usize::from(bits) + shift).div_ceil(8);
            if length == 0 {
                continue;
            }
            let word = (value << shift).to_le_bytes();
            *buffer
                .get_mut(at)
                .ok_or(Error::Invalid("bitpacking buffer"))? |= word
                .first()
                .copied()
                .ok_or(Error::Invalid("bitpacking word"))?;
            let tail = length.min(16) - 1;
            buffer
                .get_mut(at + 1..at + 1 + tail)
                .ok_or(Error::Invalid("bitpacking buffer"))?
                .copy_from_slice(slice(&word, 1, tail)?);
            if length > 16 {
                *buffer
                    .get_mut(at + 16)
                    .ok_or(Error::Invalid("bitpacking buffer"))? = (value >> (128 - shift)) as u8;
            }
        } else {
            let width = usize::from(selected.width.bytes());
            buffer
                .get_mut(local * width..(local + 1) * width)
                .ok_or(Error::Invalid("bitpacking buffer"))?
                .copy_from_slice(slice(&value.to_le_bytes(), 0, width)?);
        }
    }
    Ok(())
}

pub(crate) fn write_compacted(
    bytes: &[u8],
    count: usize,
    output: &mut impl Write,
) -> Result<(), Error> {
    let mut offset = count.div_ceil(BLOCK) * FENCE;
    for index in 0..count.div_ceil(BLOCK) {
        let selected = plan(bytes, read_block(bytes, count, index)?)?;
        let mut fence = [0u8; FENCE];
        fence.copy_from_slice(slice(bytes, index * FENCE, FENCE)?);
        fence
            .get_mut(16..24)
            .ok_or(Error::Invalid("bitpacking fence"))?
            .copy_from_slice(&(offset as u64).to_le_bytes());
        let width = fence
            .get_mut(24)
            .ok_or(Error::Invalid("bitpacking fence"))?;
        *width = (*width & !31)
            | if selected.width.bits().is_some() {
                0
            } else {
                selected.width.bytes()
            };
        let count = selected.count as u16 | (u16::from(selected.width.bits().unwrap_or(0)) << 9);
        fence
            .get_mut(26..28)
            .ok_or(Error::Invalid("bitpacking fence"))?
            .copy_from_slice(&count.to_le_bytes());
        output.write_all(&fence)?;
        offset += selected.data_bytes();
    }
    for index in 0..count.div_ceil(BLOCK) {
        let original = read_block(bytes, count, index)?;
        let selected = plan(bytes, original)?;
        if selected.width == original.width {
            output.write_all(slice(bytes, original.offset, original.data_bytes())?)?;
        } else {
            let mut buffer = [0u8; BLOCK * 16 + 16];
            encode(bytes, original, selected, &mut buffer)?;
            output.write_all(slice(&buffer, 0, selected.delta_bytes())?)?;
            output.write_all(slice(
                bytes,
                original.offset + original.delta_bytes(),
                original.data_bytes() - original.delta_bytes(),
            )?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bit_width_and_word_crossing_matches_independent_values() {
        for bits in 0..128u8 {
            for count in [1, 3, 17, 255, 256] {
                let maximum = if bits == 0 {
                    0
                } else {
                    u128::MAX >> (128 - bits)
                };
                let width = [1u8, 2, 4, 8, 16]
                    .into_iter()
                    .find(|width| bits <= width * 8)
                    .unwrap();
                let values: Vec<_> = (0..count)
                    .map(|i| {
                        if count == 1 {
                            0
                        } else if i == count - 1 {
                            maximum
                        } else {
                            maximum / (count as u128 - 1) * i as u128
                        }
                    })
                    .collect();
                let mut source = Vec::new();
                for value in &values {
                    source.extend_from_slice(&value.to_le_bytes()[..usize::from(width)]);
                }
                let original = Block {
                    base: 0,
                    offset: 0,
                    width: Width::Bytes(width),
                    shift: 0,
                    count,
                    first_id: Some(0),
                    id_step: 1,
                    kind: 0,
                };
                let selected = Block {
                    width: Width::Bits(bits),
                    ..original
                };
                let mut packed = vec![0; selected.delta_bytes()];
                encode(&source, original, selected, &mut packed).unwrap();
                for (i, &value) in values.iter().enumerate() {
                    assert_eq!(integer(&packed, i, bits).unwrap(), value);
                    for query in [
                        value,
                        value.saturating_sub(1),
                        value.saturating_add(1),
                        u128::MAX,
                    ] {
                        assert_eq!(
                            upper_bound(&packed, count, bits, query).unwrap(),
                            values.partition_point(|&v| v <= query)
                        );
                    }
                }
            }
        }
    }
}
