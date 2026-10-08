use std::io::Write;

use crate::{
    layout::{add, multiply, slice, u32_at, usize_at},
    Error,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Slots {
    Spans(usize),
    Fixed { length: usize, width: u8 },
}

impl Slots {
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

    #[inline]
    pub(crate) fn span(
        self,
        bytes: &[u8],
        slots: usize,
        id: usize,
    ) -> Result<(usize, usize), Error> {
        match self {
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
                let row = slice(bytes, start, usize::from(width))?;
                let index = match row {
                    [a] => u32::from(*a),
                    [a, b] => u32::from(u16::from_le_bytes([*a, *b])),
                    [a, b, c] => u32::from_le_bytes([*a, *b, *c, 0]),
                    [a, b, c, d] => u32::from_le_bytes([*a, *b, *c, *d]),
                    _ => return Err(Error::Invalid("payload reference width")),
                };

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
        if let Slots::Fixed { length, width } = slots {
            output.write_all(&(length as u32).to_le_bytes())?;
            output.write_all(&[width, 0, 0, 0])?;
        }

        Ok(Self { output, slots })
    }

    pub(crate) fn push(&mut self, offset: u64, length: u64) -> Result<(), Error> {
        match self.slots {
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
