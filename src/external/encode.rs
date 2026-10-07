use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom, Write},
};

use super::{
    runs::{Row, ROW_BYTES},
    temporary,
};
use crate::{
    builder::{augment, write_entry},
    layout::slice,
    packed::{self, BLOCK, FENCE},
    target::Entry,
    Error,
};

pub(super) fn sizes(input: &mut File, counts: [usize; 3]) -> Result<[Option<usize>; 2], Error> {
    input
        .seek(SeekFrom::Start(0))
        .map_err(temporary("rewind sorted run"))?;

    let mut input = BufReader::new(input);
    let mut result = [None, None];

    for (family, count) in counts.into_iter().take(2).enumerate() {
        let mut length = 0;
        let mut previous_end = None;
        let mut eligible = count > 0;
        let mut remaining = count;

        while remaining > 0 {
            let rows = block(&mut input, remaining.min(BLOCK), family as u8)?;

            for entry in &rows {
                if entry.kind > 1 || previous_end.is_some_and(|end| end >= entry.start) {
                    eligible = false;
                }

                previous_end = Some(entry.end);
            }

            match packed::size(&rows) {
                Some(size) => length += size,
                None => eligible = false,
            }

            remaining -= rows.len();
        }

        if eligible && length < count * if family == 0 { 20 } else { 56 } {
            *result
                .get_mut(family)
                .ok_or(Error::Invalid("packed family"))? = Some(length);
        }
    }

    Ok(result)
}

pub(super) fn write_sections(
    input: &mut File,
    output: &mut (impl Write + Seek),
    counts: [usize; 3],
    packed: [Option<usize>; 2],
) -> Result<(), Error> {
    let mut offset = 0;

    for (family, count) in counts.into_iter().take(2).enumerate() {
        input
            .seek(SeekFrom::Start(offset))
            .map_err(temporary("seek sorted section"))?;

        if packed.get(family).is_some_and(Option::is_some) {
            write_packed(input, output, offset, count, family as u8)?;
        } else {
            let mut reader = BufReader::new(&mut *input);
            write_interval(&mut reader, output, count, family as u8)?;
        }

        offset += count as u64 * ROW_BYTES;
    }

    input
        .seek(SeekFrom::Start(offset))
        .map_err(temporary("seek sorted ASN section"))?;

    let mut input = BufReader::new(input);

    for _ in 0..counts.get(2).copied().ok_or(Error::Invalid("ASN count"))? {
        let row = next(&mut input, 2)?;
        let number = u32::try_from(row.start).map_err(|_| Error::Invalid("temporary ASN"))?;

        output.write_all(&number.to_le_bytes())?;
        output.write_all(&row.id.to_le_bytes())?;
    }

    Ok(())
}

fn write_packed(
    input: &mut File,
    output: &mut impl Write,
    offset: u64,
    count: usize,
    family: u8,
) -> Result<(), Error> {
    let mut data_offset = count.div_ceil(BLOCK) * FENCE;

    for fences in [true, false] {
        input
            .seek(SeekFrom::Start(offset))
            .map_err(temporary("rewind packed section"))?;

        let mut reader = BufReader::new(&mut *input);
        let mut remaining = count;

        while remaining > 0 {
            let rows = block(&mut reader, remaining.min(BLOCK), family)?;

            let mut bytes = Vec::new();
            bytes.try_reserve_exact(FENCE + rows.len() * 22)?;
            packed::write(&rows, &mut bytes)?;

            if fences {
                bytes
                    .get_mut(16..24)
                    .ok_or(Error::Invalid("packed fence offset"))?
                    .copy_from_slice(&(data_offset as u64).to_le_bytes());
                output.write_all(slice(&bytes, 0, FENCE)?)?;
                data_offset += bytes.len() - FENCE;
            } else {
                output.write_all(slice(&bytes, FENCE, bytes.len() - FENCE)?)?;
            }

            remaining -= rows.len();
        }
    }

    Ok(())
}

fn write_interval(
    input: &mut impl Read,
    output: &mut (impl Write + Seek),
    count: usize,
    family: u8,
) -> Result<u128, Error> {
    if count <= BLOCK {
        let mut rows = block(input, count, family)?;
        let maximum = augment(&mut rows);

        for entry in &rows {
            write_entry(output, entry, family == 1)?;
        }

        return Ok(maximum);
    }

    let left = write_interval(input, output, count / 2, family)?;
    let mut root = next(input, family)?;
    let position = output.stream_position()?;

    write_entry(output, &root, family == 1)?;

    let right = write_interval(input, output, count - count / 2 - 1, family)?;
    root.maximum = root.end.max(left).max(right);

    let end = output.stream_position()?;
    output.seek(SeekFrom::Start(position))?;
    write_entry(output, &root, family == 1)?;
    output.seek(SeekFrom::Start(end))?;

    Ok(root.maximum)
}

fn block(input: &mut impl Read, count: usize, family: u8) -> Result<Vec<Entry>, Error> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)?;

    for _ in 0..count {
        rows.push(next(input, family)?);
    }

    Ok(rows)
}

fn next(input: &mut impl Read, family: u8) -> Result<Entry, Error> {
    let row = Row::read(input)?.ok_or(Error::Invalid("truncated sorted run"))?;

    if row.section != family {
        return Err(Error::Invalid("sorted run section"));
    }

    Ok(row.into_entry())
}
