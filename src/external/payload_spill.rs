//! Temporary payload references in insertion order and the distinct-value directory.

use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::Error;

use super::temporary;

const SLOT_BYTES: usize = 16;
const WIDE_LENGTH: u32 = u32::MAX;

/// A payload's byte span and its ordinal in the relevant value directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PayloadSpan {
    pub(super) offset: u64,
    pub(super) length: u64,
    pub(super) ordinal: u32,
}

/// Compact builds reuse the 16-byte spill slot for offset, length and ordinal.
/// A sentinel length adds an eight-byte escape, charged before writing the slot.
pub(super) fn slot_bytes(length: u64, with_ordinals: bool) -> u64 {
    SLOT_BYTES as u64
        + if with_ordinals && length >= u64::from(WIDE_LENGTH) {
            8
        } else {
            0
        }
}

pub(super) fn write_slot(
    output: &mut impl Write,
    span: PayloadSpan,
    with_ordinals: bool,
) -> Result<(), Error> {
    output
        .write_all(&span.offset.to_le_bytes())
        .map_err(temporary("write payload slots"))?;
    if with_ordinals {
        let length = u32::try_from(span.length).unwrap_or(WIDE_LENGTH);
        output
            .write_all(&length.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
        output
            .write_all(&span.ordinal.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
        if length == WIDE_LENGTH {
            output
                .write_all(&span.length.to_le_bytes())
                .map_err(temporary("write payload slots"))?;
        }
    } else {
        output
            .write_all(&span.length.to_le_bytes())
            .map_err(temporary("write payload slots"))?;
    }

    Ok(())
}

pub(super) fn read_slot(input: &mut impl Read, with_ordinals: bool) -> Result<PayloadSpan, Error> {
    let mut bytes = [0; SLOT_BYTES];
    input
        .read_exact(&mut bytes)
        .map_err(temporary("read payload slots"))?;
    let offset = crate::layout::usize_at(&bytes, 0)? as u64;
    if !with_ordinals {
        return Ok(PayloadSpan {
            offset,
            length: crate::layout::usize_at(&bytes, 8)? as u64,
            ordinal: 0,
        });
    }
    let length = crate::layout::u32_at(&bytes, 8)?;
    let ordinal = crate::layout::u32_at(&bytes, 12)?;
    let length = if length == WIDE_LENGTH {
        let mut word = [0; 8];
        input
            .read_exact(&mut word)
            .map_err(temporary("read wide payload slot"))?;
        u64::from_le_bytes(word)
    } else {
        u64::from(length)
    };

    Ok(PayloadSpan {
        offset,
        length,
        ordinal,
    })
}

pub(super) struct PayloadDirectory {
    directory: BufWriter<File>,
    values: u32,
}

impl PayloadDirectory {
    pub(super) fn new(directory: &Path) -> Result<Self, Error> {
        Ok(Self {
            directory: BufWriter::new(
                tempfile::tempfile_in(directory).map_err(temporary("create payload directory"))?,
            ),
            values: 0,
        })
    }

    pub(super) fn value_count(&self) -> u32 {
        self.values
    }

    pub(super) fn record_new_span(&mut self, span: PayloadSpan) -> Result<(), Error> {
        self.directory
            .write_all(&span.offset.to_le_bytes())
            .map_err(temporary("write payload directory"))?;
        self.directory
            .write_all(&span.length.to_le_bytes())
            .map_err(temporary("write payload directory"))?;
        self.values = self
            .values
            .checked_add(1)
            .ok_or(Error::Limit("payload values"))?;

        Ok(())
    }

    pub(super) fn write(
        &mut self,
        output: &mut impl Write,
        values: usize,
        width: u8,
    ) -> Result<(), Error> {
        self.directory
            .flush()
            .map_err(temporary("flush payload directory"))?;
        self.directory
            .get_mut()
            .seek(SeekFrom::Start(0))
            .map_err(temporary("rewind payload directory"))?;
        output.write_all(&(values as u32).to_le_bytes())?;
        output.write_all(&[width, 0, 0, 0])?;
        let mut directory = BufReader::new(self.directory.get_mut());
        for _ in 0..values {
            let mut span = [0; 16];
            directory
                .read_exact(&mut span)
                .map_err(temporary("read payload directory"))?;
            crate::payload::write_directory(
                output,
                crate::layout::usize_at(&span, 0)? as u64,
                crate::layout::usize_at(&span, 8)? as u64,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_slots_preserve_wide_lengths_and_ids_with_exact_framing() {
        for length in [
            0,
            1,
            u64::from(u32::MAX) - 1,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
        ] {
            let first = PayloadSpan {
                offset: 8,
                length,
                ordinal: 513,
            };
            let second = PayloadSpan {
                offset: 12,
                length: 1,
                ordinal: 9,
            };
            let mut bytes = Vec::new();
            write_slot(&mut bytes, first, true).unwrap();
            assert_eq!(
                bytes.len(),
                if length >= u64::from(u32::MAX) {
                    24
                } else {
                    16
                }
            );
            write_slot(&mut bytes, second, true).unwrap();
            let mut input = bytes.as_slice();
            assert_eq!(read_slot(&mut input, true).unwrap(), first);
            assert_eq!(read_slot(&mut input, true).unwrap(), second);
            assert!(input.is_empty());
            for end in 0..bytes.len() - 16 {
                assert!(read_slot(&mut &bytes[..end], true).is_err());
            }
        }
    }
}
