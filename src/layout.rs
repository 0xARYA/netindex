use std::io::Write;

use crate::{packed, target::Entry, Error, Limits};

pub(crate) const HEADER: usize = 80;
pub(crate) const MAGIC: &[u8; 8] = b"IPINDEX\0";
pub(crate) const VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Interval,
    Packed,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub offset: usize,
    pub count: usize,
    pub width: usize,
    pub length: usize,
    pub ipv6: bool,
    pub encoding: Encoding,
}

impl Section {
    pub(crate) fn end(self) -> Result<usize, Error> {
        add(self.offset, self.length)
    }

    pub(crate) fn row(self, bytes: &[u8], position: usize) -> Result<&[u8], Error> {
        if position >= self.count {
            return Err(Error::Invalid("index position"));
        }

        slice(
            bytes,
            add(self.offset, multiply(position, self.width)?)?,
            self.width,
        )
    }

    pub(crate) fn entry(self, bytes: &[u8], position: usize) -> Result<Entry, Error> {
        if self.encoding == Encoding::Packed {
            return packed::entry(
                slice(bytes, self.offset, self.length)?,
                self.count,
                position,
                self.ipv6,
            );
        }

        let row = self.row(bytes, position)?;
        let (start, end, maximum, id, prefix_offset) = if self.width == 20 {
            (
                u128::from(u32_at(row, 0)?),
                u128::from(u32_at(row, 4)?),
                u128::from(u32_at(row, 8)?),
                u32_at(row, 12)?,
                16,
            )
        } else {
            (
                u128_at(row, 0)?,
                u128_at(row, 16)?,
                u128_at(row, 32)?,
                u32_at(row, 48)?,
                52,
            )
        };

        if slice(row, prefix_offset + 2, 2)?
            .iter()
            .any(|&byte| byte != 0)
        {
            return Err(Error::Invalid("reserved entry fields"));
        }

        Ok(Entry {
            start,
            end,
            maximum,
            id,
            prefix: byte_at(row, prefix_offset)?,
            kind: byte_at(row, prefix_offset + 1)?,
        })
    }

    pub(crate) fn start(self, bytes: &[u8], position: usize) -> Result<u128, Error> {
        let row = self.row(bytes, position)?;
        if self.ipv6 {
            u128_at(row, 0)
        } else {
            u32_at(row, 0).map(u128::from)
        }
    }

    pub(crate) fn maximum(self, bytes: &[u8], position: usize) -> Result<u128, Error> {
        let row = self.row(bytes, position)?;
        if self.ipv6 {
            u128_at(row, 32)
        } else {
            u32_at(row, 8).map(u128::from)
        }
    }

    pub(crate) fn endpoint(self, bytes: &[u8], position: usize) -> Result<u128, Error> {
        let row = self.row(bytes, position)?;
        if self.ipv6 {
            u128_at(row, 16)
        } else {
            u32_at(row, 4).map(u128::from)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub records: usize,
    pub metadata: usize,
    pub v4: Section,
    pub v6: Section,
    pub asns: Section,
    pub slots: usize,
    pub slot_width: usize,
    pub payloads: usize,
    pub bytes: usize,
}

impl Layout {
    pub(crate) fn new(
        counts: [usize; 3],
        metadata: usize,
        payloads: usize,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::with_packed(
            counts,
            metadata,
            payloads,
            limits,
            [None, None],
            if payloads <= u32::MAX as usize { 8 } else { 16 },
        )
    }

    pub(crate) fn with_packed(
        counts: [usize; 3],
        metadata: usize,
        payloads: usize,
        limits: Limits,
        packed: [Option<usize>; 2],
        slot_width: usize,
    ) -> Result<Self, Error> {
        let [v4, v6, asns] = counts;
        let [packed_v4, packed_v6] = packed;
        let records = add(add(v4, v6)?, asns)?;
        if records > limits.records || records > u32::MAX as usize {
            return Err(Error::Limit("records"));
        }

        let v4 = Section {
            offset: add(HEADER, metadata)?,
            count: v4,
            width: 20,
            length: packed_v4.unwrap_or(multiply(v4, 20)?),
            ipv6: false,
            encoding: if packed_v4.is_some() {
                Encoding::Packed
            } else {
                Encoding::Interval
            },
        };
        let v6 = Section {
            offset: v4.end()?,
            count: v6,
            width: 56,
            length: packed_v6.unwrap_or(multiply(v6, 56)?),
            ipv6: true,
            encoding: if packed_v6.is_some() {
                Encoding::Packed
            } else {
                Encoding::Interval
            },
        };

        let asns = Section {
            offset: v6.end()?,
            count: asns,
            width: 8,
            length: multiply(asns, 8)?,
            ipv6: false,
            encoding: Encoding::Interval,
        };

        let slots = asns.end()?;
        let payload_offset = add(slots, multiply(records, slot_width)?)?;
        let bytes = add(payload_offset, payloads)?;
        if bytes > limits.bytes {
            return Err(Error::Limit("artifact bytes"));
        }

        Ok(Self {
            records,
            metadata,
            v4,
            v6,
            asns,
            slots,
            slot_width,
            payloads: payload_offset,
            bytes,
        })
    }

    pub(crate) fn write_header(&self, output: &mut impl Write) -> Result<(), Error> {
        let packed_v4 = self.v4.encoding == Encoding::Packed;
        let packed_v6 = self.v6.encoding == Encoding::Packed;
        let flags = u32::from(packed_v4)
            | (u32::from(packed_v6) << 1)
            | (u32::from(self.slot_width == 8) << 2);

        output.write_all(MAGIC)?;
        output.write_all(&VERSION.to_le_bytes())?;
        output.write_all(&flags.to_le_bytes())?;

        for value in [
            self.records,
            self.v4.count,
            self.v6.count,
            self.asns.count,
            self.metadata,
            self.bytes - self.payloads,
            if packed_v4 { self.v4.length } else { 0 },
            if packed_v6 { self.v6.length } else { 0 },
        ] {
            output.write_all(&(value as u64).to_le_bytes())?;
        }

        Ok(())
    }

    pub(crate) fn read(bytes: &[u8], limits: Limits) -> Result<Self, Error> {
        if bytes.len() > limits.bytes {
            return Err(Error::Limit("artifact bytes"));
        }

        if slice(bytes, 0, 8)? != MAGIC {
            return Err(Error::Invalid("index magic"));
        }

        let version = u32_at(bytes, 8)?;
        if version != VERSION {
            return Err(Error::Version(version));
        }

        let flags = u32_at(bytes, 12)?;
        if flags & !7 != 0 {
            return Err(Error::Invalid("reserved header fields"));
        }

        let mut packed = [None, None];

        for (index, value) in packed.iter_mut().enumerate() {
            let length = usize_at(bytes, 64 + index * 8)?;
            if flags & (1 << index) != 0 {
                if length == 0 {
                    return Err(Error::Invalid("packed section length"));
                }

                *value = Some(length);
            } else if length != 0 {
                return Err(Error::Invalid("reserved header fields"));
            }
        }

        let payloads = usize_at(bytes, 56)?;
        let slot_width = if flags & 4 != 0 { 8 } else { 16 };
        if slot_width == 8 && payloads > u32::MAX as usize {
            return Err(Error::Invalid("compact payload length"));
        }

        let layout = Self::with_packed(
            [
                usize_at(bytes, 24)?,
                usize_at(bytes, 32)?,
                usize_at(bytes, 40)?,
            ],
            usize_at(bytes, 48)?,
            payloads,
            limits,
            packed,
            slot_width,
        )?;
        if usize_at(bytes, 16)? != layout.records || bytes.len() != layout.bytes {
            return Err(Error::Invalid("artifact length or record count"));
        }

        Ok(layout)
    }

    pub(crate) fn payload<'a>(&self, bytes: &'a [u8], id: u32) -> Result<&'a [u8], Error> {
        let (offset, length) = self.span(bytes, id)?;

        slice(bytes, add(self.payloads, offset)?, length)
    }

    pub(crate) fn span(&self, bytes: &[u8], id: u32) -> Result<(usize, usize), Error> {
        let id = id as usize;
        if id >= self.records {
            return Err(Error::Invalid("record ID"));
        }

        let slot = add(self.slots, multiply(id, self.slot_width)?)?;
        if self.slot_width == 8 {
            Ok((
                u32_at(bytes, slot)? as usize,
                u32_at(bytes, add(slot, 4)?)? as usize,
            ))
        } else {
            Ok((usize_at(bytes, slot)?, usize_at(bytes, add(slot, 8)?)?))
        }
    }
}

pub(crate) fn slice(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], Error> {
    bytes
        .get(offset..add(offset, length)?)
        .ok_or(Error::Invalid("truncated section"))
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?
            .try_into()
            .map_err(|_| Error::Invalid("u32 field"))?,
    ))
}

pub(crate) fn u128_at(bytes: &[u8], offset: usize) -> Result<u128, Error> {
    Ok(u128::from_le_bytes(
        slice(bytes, offset, 16)?
            .try_into()
            .map_err(|_| Error::Invalid("u128 field"))?,
    ))
}

pub(crate) fn usize_at(bytes: &[u8], offset: usize) -> Result<usize, Error> {
    let value = u64::from_le_bytes(
        slice(bytes, offset, 8)?
            .try_into()
            .map_err(|_| Error::Invalid("u64 field"))?,
    );

    usize::try_from(value).map_err(|_| Error::Limit("addressable bytes"))
}

fn byte_at(bytes: &[u8], offset: usize) -> Result<u8, Error> {
    bytes
        .get(offset)
        .copied()
        .ok_or(Error::Invalid("truncated entry"))
}

fn add(left: usize, right: usize) -> Result<usize, Error> {
    left.checked_add(right)
        .ok_or(Error::Limit("addressable bytes"))
}

fn multiply(left: usize, right: usize) -> Result<usize, Error> {
    left.checked_mul(right)
        .ok_or(Error::Limit("addressable bytes"))
}
