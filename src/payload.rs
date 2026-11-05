use std::io::Write;

use crate::{
    Error,
    layout::{add, multiply, slice, u32_at, usize_at},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Slots {
    Spans(usize),
    Fixed { length: usize, width: u8 },
    Indexed { values: usize, width: u8 },
}

impl Slots {
    pub(crate) fn indexed(
        self,
        records: usize,
        payloads: usize,
        values: usize,
    ) -> Result<Self, Error> {
        if values == 0 || payloads > u32::MAX as usize {
            return Ok(self);
        }
        let indexed = Self::Indexed {
            values,
            width: ordinal_width(values),
        };
        Ok(if indexed.size(records)? < self.size(records)? {
            indexed
        } else {
            self
        })
    }

    pub(crate) fn select(
        records: usize,
        payloads: usize,
        length: Option<usize>,
    ) -> Result<Self, Error> {
        let spans = Self::Spans(if payloads <= u32::MAX as usize { 8 } else { 16 });
        let Some(length) = length.filter(|&length| length <= u32::MAX as usize) else {
            return Ok(spans);
        };

        if records == 0 || payloads > u32::MAX as usize {
            return Ok(spans);
        }

        let width = reference_width(payloads, length)?;
        let fixed = Self::Fixed { length, width };

        Ok(if fixed.size(records)? < spans.size(records)? {
            fixed
        } else {
            spans
        })
    }

    pub(crate) fn size(self, records: usize) -> Result<usize, Error> {
        match self {
            Self::Spans(width) => multiply(records, width),
            Self::Fixed { width, .. } => add(8, multiply(records, usize::from(width))?),
            Self::Indexed { values, width } => add(
                add(8, multiply(values, 8)?)?,
                multiply(records, usize::from(width))?,
            ),
        }
    }

    pub(crate) fn read(bytes: &[u8], offset: usize, payloads: usize) -> Result<Self, Error> {
        let length = u32_at(bytes, offset)? as usize;
        let width = *bytes
            .get(add(offset, 4)?)
            .ok_or(Error::Invalid("payload reference width"))?;

        if slice(bytes, add(offset, 5)?, 3)?
            .iter()
            .any(|&byte| byte != 0)
            || width != reference_width(payloads, length)?
        {
            return Err(Error::Invalid("fixed payload fields"));
        }

        Ok(Self::Fixed { length, width })
    }

    pub(crate) fn read_indexed(
        bytes: &[u8],
        offset: usize,
        records: usize,
        payloads: usize,
    ) -> Result<Self, Error> {
        let values = u32_at(bytes, offset)? as usize;
        let descriptor = slice(bytes, add(offset, 4)?, 4)?;
        if values == 0
            || values > records
            || payloads > u32::MAX as usize
            || descriptor != [ordinal_width(values), 0, 0, 0]
        {
            return Err(Error::Invalid("indexed payload fields"));
        }
        Ok(Self::Indexed {
            values,
            width: ordinal_width(values),
        })
    }

    #[inline]
    pub(crate) fn ordinal(self, bytes: &[u8], slots: usize, id: usize) -> Result<usize, Error> {
        let Self::Indexed { values, width } = self else {
            return Err(Error::Invalid("indexed payload fields"));
        };
        let start = add(
            add(add(slots, 8)?, multiply(values, 8)?)?,
            multiply(id, usize::from(width))?,
        )?;
        let ordinal = read_ordinal(bytes, start, width)? as usize;
        if ordinal >= values {
            return Err(Error::Invalid("payload reference"));
        }
        Ok(ordinal)
    }

    #[inline]
    pub(crate) fn span(
        self,
        bytes: &[u8],
        slots: usize,
        id: usize,
    ) -> Result<(usize, usize), Error> {
        match self {
            Self::Indexed { .. } => {
                let ordinal = self.ordinal(bytes, slots, id)?;
                directory_span(bytes, add(slots, 8)?, ordinal)
            }
            Self::Spans(width) => {
                let slot = add(slots, multiply(id, width)?)?;
                if width == 8 {
                    Ok((
                        u32_at(bytes, slot)? as usize,
                        u32_at(bytes, add(slot, 4)?)? as usize,
                    ))
                } else {
                    Ok((usize_at(bytes, slot)?, usize_at(bytes, add(slot, 8)?)?))
                }
            }
            Self::Fixed { length, width } => {
                if width == 0 {
                    return Ok((0, length));
                }

                let start = add(add(slots, 8)?, multiply(id, usize::from(width))?)?;
                let index = read_ordinal(bytes, start, width)?;

                Ok((multiply(index as usize, length)?, length))
            }
        }
    }
}

pub(crate) struct SlotWriter<'a, W> {
    output: &'a mut W,
    slots: Slots,
}

impl<'a, W: Write> SlotWriter<'a, W> {
    pub(crate) fn new(output: &'a mut W, slots: Slots) -> Result<Self, Error> {
        if matches!(slots, Slots::Indexed { .. }) {
            return Err(Error::Invalid("indexed slot writer"));
        }
        if let Slots::Fixed { length, width } = slots {
            output.write_all(&(length as u32).to_le_bytes())?;
            output.write_all(&[width, 0, 0, 0])?;
        }

        Ok(Self { output, slots })
    }

    pub(crate) fn push(&mut self, offset: u64, length: u64) -> Result<(), Error> {
        match self.slots {
            Slots::Indexed { .. } => return Err(Error::Invalid("indexed slot writer")),
            Slots::Spans(8) => {
                let offset =
                    u32::try_from(offset).map_err(|_| Error::Invalid("compact payload span"))?;
                let length =
                    u32::try_from(length).map_err(|_| Error::Invalid("compact payload span"))?;

                self.output.write_all(&offset.to_le_bytes())?;
                self.output.write_all(&length.to_le_bytes())?;
            }
            Slots::Spans(_) => {
                self.output.write_all(&offset.to_le_bytes())?;
                self.output.write_all(&length.to_le_bytes())?;
            }
            Slots::Fixed {
                length: expected,
                width,
            } => {
                if length != expected as u64
                    || (length == 0 && offset != 0)
                    || (length != 0 && !offset.is_multiple_of(length))
                {
                    return Err(Error::Invalid("fixed payload span"));
                }

                let index = offset.checked_div(length).unwrap_or(0);
                if index >= (1u64 << (width * 8)) {
                    return Err(Error::Invalid("payload reference"));
                }

                self.output.write_all(slice(
                    &(index as u32).to_le_bytes(),
                    0,
                    usize::from(width),
                )?)?;
            }
        }

        Ok(())
    }
}

#[inline]
fn read_ordinal(bytes: &[u8], start: usize, width: u8) -> Result<u32, Error> {
    Ok(match slice(bytes, start, usize::from(width))? {
        [] => 0,
        [a] => u32::from(*a),
        [a, b] => u32::from(u16::from_le_bytes([*a, *b])),
        [a, b, c] => u32::from_le_bytes([*a, *b, *c, 0]),
        [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
        _ => return Err(Error::Invalid("payload reference width")),
    })
}

pub(crate) fn directory_span(
    bytes: &[u8],
    start: usize,
    ordinal: usize,
) -> Result<(usize, usize), Error> {
    let start = add(start, multiply(ordinal, 8)?)?;
    Ok((
        u32_at(bytes, start)? as usize,
        u32_at(bytes, add(start, 4)?)? as usize,
    ))
}

pub(crate) fn write_directory(
    output: &mut impl Write,
    offset: u64,
    length: u64,
) -> Result<(), Error> {
    output.write_all(
        &u32::try_from(offset)
            .map_err(|_| Error::Invalid("compact payload span"))?
            .to_le_bytes(),
    )?;
    output.write_all(
        &u32::try_from(length)
            .map_err(|_| Error::Invalid("compact payload span"))?
            .to_le_bytes(),
    )?;
    Ok(())
}

pub(crate) fn write_ordinal(output: &mut impl Write, ordinal: u32, width: u8) -> Result<(), Error> {
    output.write_all(slice(&ordinal.to_le_bytes(), 0, usize::from(width))?)?;
    Ok(())
}

fn ordinal_width(values: usize) -> u8 {
    (usize::BITS - values.saturating_sub(1).leading_zeros()).div_ceil(8) as u8
}

fn reference_width(payloads: usize, length: usize) -> Result<u8, Error> {
    if length == 0 {
        return if payloads == 0 {
            Ok(0)
        } else {
            Err(Error::Invalid("fixed payload length"))
        };
    }

    if payloads == 0 || payloads > u32::MAX as usize || !payloads.is_multiple_of(length) {
        return Err(Error::Invalid("fixed payload length"));
    }

    let maximum = payloads / length - 1;

    Ok((usize::BITS - maximum.leading_zeros()).div_ceil(8) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_selection_uses_all_widths_and_preserves_wide_pool_fallback() {
        for (values, width) in [
            (1usize, 0),
            (256, 1),
            (257, 2),
            (65_536, 2),
            (65_537, 3),
            (16_777_216, 3),
            (16_777_217, 4),
        ] {
            assert_eq!(ordinal_width(values), width);
        }
        if let Some(payloads) = (u32::MAX as usize).checked_add(1) {
            assert!(matches!(
                Slots::Spans(16).indexed(100, payloads, 2).unwrap(),
                Slots::Spans(16)
            ));
        }
    }

    #[test]
    fn references_roundtrip_every_width_and_tail() {
        for width in 1..=4u8 {
            let slots = Slots::Fixed { length: 1, width };
            let maximum = (1u64 << (width * 8)) - 1;
            let expected = [0, maximum, maximum / 2, 1, 0, maximum, 1, maximum / 2, 0];
            let mut bytes = Vec::new();
            let mut writer = SlotWriter::new(&mut bytes, slots).unwrap();

            for &value in &expected {
                writer.push(value, 1).unwrap();
            }

            assert_eq!(bytes.len(), slots.size(expected.len()).unwrap());

            for (id, &value) in expected.iter().enumerate() {
                assert_eq!(slots.span(&bytes, 0, id).unwrap(), (value as usize, 1));
            }
        }
    }
}
