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
    width: u8,
    shift: u8,
    count: usize,
    first_id: Option<u32>,
    kind: u8,
}

impl Block {
    fn row_width(self) -> usize {
        usize::from(self.width)
            + 1
            + usize::from(self.kind > 1)
            + if self.first_id.is_some() { 0 } else { 4 }
    }
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
        length = length.checked_add(rows.len().checked_mul(block.row_width())?)?;
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
        let width = block.width
            | if block.first_id.is_some() { 0x80 } else { 0 }
            | if block.kind == 1 { 0x40 } else { 0 };
        let shift = block.shift | if block.kind <= 1 { 0x80 } else { 0 };

        output.write_all(&block.base.to_le_bytes())?;
        output.write_all(&(offset as u64).to_le_bytes())?;
        output.write_all(&[width, shift])?;
        output.write_all(&(rows.len() as u16).to_le_bytes())?;
        output.write_all(&block.first_id.unwrap_or(0).to_le_bytes())?;
        offset += rows.len() * block.row_width();
    }

    for rows in entries.chunks(BLOCK) {
        let block = describe(rows, 0);

        for entry in rows {
            let delta = (entry.start.into() - block.base) >> block.shift;
            output.write_all(slice(&delta.to_le_bytes(), 0, usize::from(block.width))?)?;
        }

        for entry in rows {
            output.write_all(&[entry.prefix])?;
        }

        if block.kind > 1 {
            for entry in rows {
                output.write_all(&[entry.kind])?;
            }
        }

        if block.first_id.is_none() {
            for entry in rows {
                output.write_all(&entry.id.to_le_bytes())?;
            }
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

    block_entry(bytes, block, local, ipv6)
}

#[derive(Debug)]
pub(crate) struct Directory {
    start: u128,
    shift: u32,
    ranges: Vec<(u32, u32)>,
}

impl Directory {
    pub(crate) fn build(
        bytes: &[u8],
        count: usize,
        start: u128,
        end: u128,
    ) -> Result<Option<Self>, Error> {
        let blocks = count.div_ceil(BLOCK);
        if blocks < 256 {
            return Ok(None);
        }

        let span = end - start;
        let bits = if blocks >= 4096 { 12 } else { 8 };
        let shift = (128 - span.leading_zeros()).saturating_sub(bits);
        let buckets = ((span >> shift) + 1) as usize;
        let mut ranges = Vec::new();
        ranges.try_reserve_exact(buckets)?;

        for bucket in 0..buckets {
            let first = start + ((bucket as u128) << shift);
            let last = first.saturating_add((1u128 << shift) - 1).min(end);
            let low = block_upper_bound(bytes, 0, blocks, first)?.saturating_sub(1);
            let high = block_upper_bound(bytes, low, blocks, last)?;
            ranges.push((
                u32::try_from(low).map_err(|_| Error::Limit("packed directory"))?,
                u32::try_from(high).map_err(|_| Error::Limit("packed directory"))?,
            ));
        }

        Ok(Some(Self {
            start,
            shift,
            ranges,
        }))
    }

    fn range(&self, query: u128) -> Result<(usize, usize), Error> {
        let bucket = query
            .checked_sub(self.start)
            .ok_or(Error::Invalid("packed directory query"))?
            >> self.shift;
        let bucket =
            usize::try_from(bucket).map_err(|_| Error::Invalid("packed directory bucket"))?;

        self.ranges
            .get(bucket)
            .copied()
            .map(|(low, high)| (low as usize, high as usize))
            .ok_or(Error::Invalid("packed directory bucket"))
    }
}

pub(crate) fn lookup(
    bytes: &[u8],
    count: usize,
    query: u128,
    ipv6: bool,
    search: Search,
    directory: Option<&Directory>,
) -> Result<Option<Entry>, Error> {
    let (low, high) = match directory {
        Some(directory) => directory.range(query)?,
        None => (0, count.div_ceil(BLOCK)),
    };

    let upper_block = block_upper_bound(bytes, low, high, query)?;

    let Some(block_index) = upper_block.checked_sub(1) else {
        return Ok(None);
    };

    let block = read_block(bytes, count, block_index)?;
    let deltas = slice(bytes, block.offset, block.count * usize::from(block.width))?;
    let delta = query
        .checked_sub(block.base)
        .ok_or(Error::Invalid("packed search base"))?;
    let upper = search.upper_bound(deltas, usize::from(block.width), delta >> block.shift)?;

    let Some(local) = upper.checked_sub(1) else {
        return Ok(None);
    };

    let entry = block_entry(bytes, block, local, ipv6)?;

    Ok((query <= entry.end).then_some(entry))
}

pub(crate) fn validate(bytes: &[u8], count: usize) -> Result<(), Error> {
    let mut offset = count.div_ceil(BLOCK) * FENCE;

    for index in 0..count.div_ceil(BLOCK) {
        let block = read_block(bytes, count, index)?;
        if block
            .first_id
            .is_some_and(|first| first.checked_add((block.count - 1) as u32).is_none())
        {
            return Err(Error::Invalid("packed record ID"));
        }

        if block.offset != offset || integer(slice(bytes, offset, usize::from(block.width))?)? != 0
        {
            return Err(Error::Invalid("packed block offset or base delta"));
        }

        offset += block.count * block.row_width();
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

fn block_entry(bytes: &[u8], block: Block, local: usize, ipv6: bool) -> Result<Entry, Error> {
    let delta = integer(slice(
        bytes,
        block.offset + local * usize::from(block.width),
        usize::from(block.width),
    )?)?;

    let shifted = delta
        .checked_shl(u32::from(block.shift))
        .ok_or(Error::Invalid("packed delta shift"))?;
    if shifted >> block.shift != delta {
        return Err(Error::Invalid("packed delta overflow"));
    }

    let start = block
        .base
        .checked_add(shifted)
        .ok_or(Error::Invalid("packed address overflow"))?;

    let columns = block.offset + block.count * usize::from(block.width);
    let prefix = *bytes
        .get(columns + local)
        .ok_or(Error::Invalid("packed prefix"))?;
    let kind = if block.kind <= 1 {
        block.kind
    } else {
        *bytes
            .get(columns + block.count + local)
            .ok_or(Error::Invalid("packed kind"))?
    };

    let bits = if ipv6 { 128 } else { 32 };
    if prefix > bits || kind > 1 || (!ipv6 && start > u128::from(u32::MAX)) {
        return Err(Error::Invalid("packed target"));
    }

    let host = u128::MAX
        .checked_shr(128 - u32::from(bits - prefix))
        .unwrap_or(0);
    let end = start | host;

    let id = match block.first_id {
        Some(first) => first
            .checked_add(local as u32)
            .ok_or(Error::Invalid("packed record ID"))?,
        None => u32_at(
            bytes,
            columns + block.count * (1 + usize::from(block.kind > 1)) + local * 4,
        )?,
    };

    Ok(Entry {
        start,
        end,
        maximum: end,
        id,
        prefix,
        kind,
    })
}

fn block_upper_bound(bytes: &[u8], low: usize, high: usize, query: u128) -> Result<usize, Error> {
    let rows = slice(bytes, low * FENCE, (high - low) * FENCE)?;
    let (fences, _) = rows.as_chunks::<FENCE>();
    let position = fences.partition_point(|row| {
        row.first_chunk::<16>()
            .is_some_and(|base| u128::from_le_bytes(*base) <= query)
    });

    Ok(low + position)
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

    let first_id = entries.first().map(|entry| entry.id).filter(|&first| {
        entries
            .iter()
            .enumerate()
            .all(|(position, entry)| first.checked_add(position as u32) == Some(entry.id))
    });

    let kind = entries.first().map_or(2, |entry| entry.kind);
    let kind = if entries.iter().all(|entry| entry.kind == kind) {
        kind
    } else {
        2
    };

    Block {
        base,
        offset,
        width: width as u8,
        shift: shift as u8,
        count: entries.len(),
        first_id,
        kind,
    }
}

fn read_block(bytes: &[u8], count: usize, index: usize) -> Result<Block, Error> {
    if index >= count.div_ceil(BLOCK) {
        return Err(Error::Invalid("packed block position"));
    }

    let row = slice(bytes, index * FENCE, FENCE)?;
    let flags = *row.get(24).ok_or(Error::Invalid("packed width"))?;
    let width = usize::from(flags & 0x3f);
    let first_id = if flags & 0x80 != 0 {
        Some(u32_at(row, 28)?)
    } else {
        None
    };
    let shift_flags = *row.get(25).ok_or(Error::Invalid("packed shift"))?;
    let shift = u32::from(shift_flags & 0x7f);
    let kind = if shift_flags & 0x80 != 0 {
        u8::from(flags & 0x40 != 0)
    } else {
        2
    };
    let rows = u16::from_le_bytes(
        slice(row, 26, 2)?
            .try_into()
            .map_err(|_| Error::Invalid("packed count"))?,
    ) as usize;

    if !matches!(width, 1 | 2 | 4 | 8 | 16)
        || (kind == 2 && flags & 0x40 != 0)
        || rows != (count - index * BLOCK).min(BLOCK)
        || (first_id.is_none() && u32_at(row, 28)? != 0)
    {
        return Err(Error::Invalid("packed block fields"));
    }

    let offset = usize_at(row, 16)?;

    let block = Block {
        base: u128_at(row, 0)?,
        offset,
        width: width as u8,
        shift: shift as u8,
        count: rows,
        first_id,
        kind,
    };
    slice(bytes, offset, rows * block.row_width())?;

    Ok(block)
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], Error> {
    bytes
        .try_into()
        .map_err(|_| Error::Invalid("packed integer width"))
}
