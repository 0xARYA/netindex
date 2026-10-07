use std::io::Write;

use crate::{
    layout::{slice, u128_at, u32_at, usize_at},
    search::Search,
    target::Entry,
    Error,
};

pub(crate) const BLOCK: usize = 256;
pub(crate) const FENCE: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Block {
    base: u128,
    offset: usize,
    width: usize,
    shift: u32,
    count: usize,
}

/// Only nonoverlapping addresses and networks can use predecessor lookup.
pub(crate) fn size<N: Copy + Ord + Into<u128>>(entries: &[Entry<N>]) -> Option<usize> {
    if entries.is_empty()
        || entries.iter().any(|entry| entry.kind > 1)
        || entries
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if left.end >= right.start))
    {
        return None;
    }

    let mut length = entries.len().div_ceil(BLOCK).checked_mul(FENCE)?;
    for rows in entries.chunks(BLOCK) {
        let block = describe(rows, 0);
        length = length.checked_add(rows.len().checked_mul(block.width + 6)?)?;
    }

    Some(length)
}

pub(crate) fn write<N: Copy + Into<u128>>(
    entries: &[Entry<N>],
    output: &mut impl Write,
) -> Result<(), Error> {
    let mut offset = entries.len().div_ceil(BLOCK) * FENCE;

    for rows in entries.chunks(BLOCK) {
        let block = describe(rows, offset);
        output.write_all(&block.base.to_le_bytes())?;
        output.write_all(&(offset as u64).to_le_bytes())?;
        output.write_all(&[block.width as u8, block.shift as u8])?;
        output.write_all(&(rows.len() as u16).to_le_bytes())?;
        output.write_all(&[0; 4])?;
        offset += rows.len() * (block.width + 6);
    }

    for rows in entries.chunks(BLOCK) {
        let block = describe(rows, 0);

        for entry in rows {
            let delta = (entry.start.into() - block.base) >> block.shift;
            output.write_all(slice(&delta.to_le_bytes(), 0, block.width)?)?;
        }

        for entry in rows {
            output.write_all(&[entry.prefix])?;
        }

        for entry in rows {
            output.write_all(&[entry.kind])?;
        }

        for entry in rows {
            output.write_all(&entry.id.to_le_bytes())?;
        }
    }

    Ok(())
}

pub(crate) fn entry(
    bytes: &[u8],
    count: usize,
    position: usize,
    ipv6: bool,
) -> Result<Entry, Error> {
    if position >= count {
        return Err(Error::Invalid("packed index position"));
    }

    let block = read_block(bytes, count, position / BLOCK)?;
    let local = position % BLOCK;
    let delta = integer(slice(
        bytes,
        block.offset + local * block.width,
        block.width,
    )?)?;

    let shifted = delta
        .checked_shl(block.shift)
        .ok_or(Error::Invalid("packed delta shift"))?;
    if shifted >> block.shift != delta {
        return Err(Error::Invalid("packed delta overflow"));
    }

    let start = block
        .base
        .checked_add(shifted)
        .ok_or(Error::Invalid("packed address overflow"))?;

    let columns = block.offset + block.count * block.width;
    let prefix = *bytes
        .get(columns + local)
        .ok_or(Error::Invalid("packed prefix"))?;
    let kind = *bytes
        .get(columns + block.count + local)
        .ok_or(Error::Invalid("packed kind"))?;

    let bits = if ipv6 { 128 } else { 32 };
    if prefix > bits || kind > 1 || (!ipv6 && start > u128::from(u32::MAX)) {
        return Err(Error::Invalid("packed target"));
    }

    let host = u128::MAX
        .checked_shr(128 - u32::from(bits - prefix))
        .unwrap_or(0);
    let end = start | host;

    Ok(Entry {
        start,
        end,
        maximum: end,
        id: u32_at(bytes, columns + block.count * 2 + local * 4)?,
        prefix,
        kind,
    })
}

pub(crate) fn predecessor(
    bytes: &[u8],
    count: usize,
    query: u128,
    search: Search,
) -> Result<Option<usize>, Error> {
    let (mut low, mut high) = (0, count.div_ceil(BLOCK));

    while low < high {
        let mid = low + (high - low) / 2;
        if u128_at(bytes, mid * FENCE)? <= query {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    let Some(block_index) = low.checked_sub(1) else {
        return Ok(None);
    };

    let block = read_block(bytes, count, block_index)?;
    let deltas = slice(bytes, block.offset, block.count * block.width)?;
    let delta = query
        .checked_sub(block.base)
        .ok_or(Error::Invalid("packed search base"))?;
    let upper = search.upper_bound(deltas, block.width, delta >> block.shift)?;

    Ok(upper
        .checked_sub(1)
        .map(|local| block_index * BLOCK + local))
}

pub(crate) fn validate(bytes: &[u8], count: usize) -> Result<(), Error> {
    let mut offset = count.div_ceil(BLOCK) * FENCE;

    for index in 0..count.div_ceil(BLOCK) {
        let block = read_block(bytes, count, index)?;
        if block.offset != offset || integer(slice(bytes, offset, block.width)?)? != 0 {
            return Err(Error::Invalid("packed block offset or base delta"));
        }

        offset += block.count * (block.width + 6);
        slice(bytes, block.offset, offset - block.offset)?;
    }

    if offset != bytes.len() {
        return Err(Error::Invalid("packed section length"));
    }

    Ok(())
}

pub(crate) fn integer(bytes: &[u8]) -> Result<u128, Error> {
    match bytes.len() {
        1 => bytes
            .first()
            .copied()
            .map(u128::from)
            .ok_or(Error::Invalid("packed integer")),
        2 => Ok(u128::from(u16::from_le_bytes(array(bytes)?))),
        4 => Ok(u128::from(u32::from_le_bytes(array(bytes)?))),
        8 => Ok(u128::from(u64::from_le_bytes(array(bytes)?))),
        16 => Ok(u128::from_le_bytes(array(bytes)?)),
        _ => Err(Error::Invalid("packed integer width")),
    }
}

fn describe<N: Copy + Into<u128>>(entries: &[Entry<N>], offset: usize) -> Block {
    let base = entries.first().map_or(0, |entry| entry.start.into());
    let mut common = 0u128;
    let mut maximum = 0u128;

    for entry in entries {
        let delta = entry.start.into() - base;
        common |= delta;
        maximum = delta;
    }

    let shift = common.trailing_zeros().min(127);
    let maximum = maximum >> shift;
    let width = [1, 2, 4, 8, 16]
        .into_iter()
        .find(|width| maximum.checked_shr(width * 8).unwrap_or(0) == 0)
        .unwrap_or(16);

    Block {
        base,
        offset,
        width: width as usize,
        shift,
        count: entries.len(),
    }
}

fn read_block(bytes: &[u8], count: usize, index: usize) -> Result<Block, Error> {
    if index >= count.div_ceil(BLOCK) {
        return Err(Error::Invalid("packed block position"));
    }

    let row = slice(bytes, index * FENCE, FENCE)?;
    let width = usize::from(*row.get(24).ok_or(Error::Invalid("packed width"))?);
    let shift = u32::from(*row.get(25).ok_or(Error::Invalid("packed shift"))?);
    let rows = u16::from_le_bytes(
        slice(row, 26, 2)?
            .try_into()
            .map_err(|_| Error::Invalid("packed count"))?,
    ) as usize;
    if !matches!(width, 1 | 2 | 4 | 8 | 16)
        || shift > 127
        || rows != (count - index * BLOCK).min(BLOCK)
        || slice(row, 28, 4)?.iter().any(|&byte| byte != 0)
    {
        return Err(Error::Invalid("packed block fields"));
    }

    let offset = usize_at(row, 16)?;

    slice(bytes, offset, rows * (width + 6))?;

    Ok(Block {
        base: u128_at(row, 0)?,
        offset,
        width,
        shift,
        count: rows,
    })
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], Error> {
    bytes
        .try_into()
        .map_err(|_| Error::Invalid("packed integer width"))
}
