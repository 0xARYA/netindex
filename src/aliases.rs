use std::{
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use crate::{
    Error, Limits, Match, Target,
    layout::{Layout, MAGIC, add, slice, u32_at, u128_at},
    packed,
};

pub(crate) const VERSION: u32 = 5;
const HEADER: usize = 16;
const ROW: usize = 24;
const MAX_WINDOWS: usize = 64;

/// An IPv6 prefix that shares the complete IPv4 section of an index.
///
/// The next 32 bits select the IPv4 address; trailing bits are ignored. All
/// source networks, including duplicates and overlaps, are projected independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedAlias {
    /// Canonical IPv6 window address.
    pub address: Ipv6Addr,
    /// Window prefix, from 0 through 96.
    pub prefix: u8,
    /// Logical ID of the copy of physical IPv4 ID zero.
    pub first_id: u32,
    /// Use odd IDs for this window and even IDs for the original IPv4 rows.
    /// At most one window may use this mode, with first ID 1.
    pub interleaved: bool,
}

impl SharedAlias {
    pub(crate) fn mask(self) -> u128 {
        u128::MAX.checked_shr(u32::from(self.prefix)).unwrap_or(0)
    }

    pub(crate) fn start(self) -> u128 {
        u128::from(self.address)
    }

    pub(crate) fn end(self) -> u128 {
        self.start() | self.mask()
    }

    pub(crate) fn query(self, value: u128) -> Ipv4Addr {
        Ipv4Addr::from((value >> (96 - self.prefix)) as u32)
    }

    /// Project a source IPv4 network into this window.
    ///
    /// # Errors
    /// Fails for a malformed window or a target other than a canonical IPv4 network.
    pub fn project(self, target: Target) -> Result<Target, Error> {
        if self.prefix > 96 || self.start() & self.mask() != 0 {
            return Err(Error::Invalid("shared alias window"));
        }
        let Target::Network {
            address: IpAddr::V4(address),
            prefix,
        } = target
        else {
            return Err(Error::Invalid("shared alias source network"));
        };
        target.entry(0)?;
        Ok(self.project_network(address, prefix))
    }

    fn project_network(self, address: Ipv4Addr, prefix: u8) -> Target {
        Target::Network {
            address: Ipv6Addr::from(
                self.start() | (u128::from(u32::from(address)) << (96 - self.prefix)),
            )
            .into(),
            prefix: self.prefix + prefix,
        }
    }

    pub(crate) fn matched(self, row: Match<'_>) -> Result<Match<'_>, Error> {
        let Target::Network {
            address: IpAddr::V4(address),
            prefix,
        } = row.target
        else {
            return Err(Error::Invalid("shared alias source network"));
        };
        Ok(Match {
            id: self.first_id + row.id * if self.interleaved { 2 } else { 1 },
            target: self.project_network(address, prefix),
            payload: row.payload,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Aliases {
    pub(crate) windows: Vec<SharedAlias>,
    blocks: Vec<usize>,
    source: usize,
    interleaved: bool,
    pub(crate) records: usize,
}

impl Aliases {
    pub(crate) fn read(bytes: &[u8]) -> Result<(Vec<SharedAlias>, usize), Error> {
        let count = u32_at(bytes, 12)? as usize;
        if !(1..=MAX_WINDOWS).contains(&count) {
            return Err(Error::Invalid("shared alias count"));
        }
        let mut windows = Vec::new();
        windows.try_reserve_exact(count)?;
        let (rows, _) = slice(bytes, HEADER, count * ROW)?.as_chunks::<ROW>();
        for row in rows {
            let [prefix, stride, zero_a, zero_b] = slice(row, 20, 4)? else {
                return Err(Error::Invalid("shared alias descriptor"));
            };
            if !matches!(stride, 1 | 2) || *zero_a != 0 || *zero_b != 0 {
                return Err(Error::Invalid("shared alias descriptor"));
            }
            windows.push(SharedAlias {
                address: u128_at(row, 0)?.into(),
                first_id: u32_at(row, 16)?,
                prefix: *prefix,
                interleaved: *stride == 2,
            });
        }
        Ok((windows, HEADER + count * ROW))
    }

    pub(crate) fn validate(
        windows: &[SharedAlias],
        data: &[u8],
        layout: Layout,
        limits: Limits,
    ) -> Result<Self, Error> {
        let source = layout.v4.count;
        if source == 0 || !(1..=MAX_WINDOWS).contains(&windows.len()) {
            return Err(Error::Invalid("shared alias count or source"));
        }
        let records = add(
            layout.records,
            crate::layout::multiply(source, windows.len())?,
        )?;
        if records > limits.records || records > u32::MAX as usize {
            return Err(Error::Limit("records"));
        }

        let mut blocks = Vec::new();
        blocks.try_reserve_exact(windows.len())?;
        let mut previous = None;
        let mut interleaved = false;
        for &window in windows {
            if window.prefix > 96
                || window.start() & window.mask() != 0
                || previous.is_some_and(|end| end >= window.start())
            {
                return Err(Error::Invalid("shared alias windows"));
            }
            previous = Some(window.end());
            if window.interleaved {
                if interleaved || window.first_id != 1 {
                    return Err(Error::Invalid("shared alias interleaved IDs"));
                }
                interleaved = true;
            } else {
                blocks.push(window.first_id as usize);
            }
        }
        blocks.sort_unstable();
        let mut end = source * (1 + usize::from(interleaved));
        for &start in &blocks {
            if start < end || start > records - source {
                return Err(Error::Invalid("shared alias ID blocks"));
            }
            end = start + source;
        }

        let mut position = 0;
        visit(layout.v4, data, |entry| {
            if entry.id as usize != position || entry.kind != 1 {
                return Err(Error::Invalid("shared alias IPv4 order or kind"));
            }
            position += 1;
            Ok(())
        })?;
        visit(layout.v6, data, |entry| {
            let at = windows.partition_point(|window| window.end() < entry.start);
            if windows
                .get(at)
                .is_some_and(|window| window.start() <= entry.end)
            {
                return Err(Error::Invalid("shared alias overlaps native IPv6"));
            }
            Ok(())
        })?;

        let mut owned = Vec::new();
        owned.try_reserve_exact(windows.len())?;
        owned.extend_from_slice(windows);
        Ok(Self {
            windows: owned,
            blocks,
            source,
            interleaved,
            records,
        })
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.windows.capacity() * size_of::<SharedAlias>()
            + self.blocks.capacity() * size_of::<usize>()
    }

    pub(crate) fn logical(&self, id: u32) -> u32 {
        let mut id = id as usize;
        if id < self.source {
            return id as u32 * (1 + u32::from(self.interleaved));
        }
        id += self.source * usize::from(self.interleaved);
        for &start in &self.blocks {
            if id < start {
                break;
            }
            id += self.source;
        }
        id as u32
    }

    pub(crate) fn lookup(&self, value: u128) -> Option<SharedAlias> {
        let at = self.windows.partition_point(|window| window.end() < value);
        self.windows
            .get(at)
            .copied()
            .filter(|window| window.start() <= value)
    }

    pub(crate) fn write(
        windows: &[SharedAlias],
        mut output: impl Write,
        data: &[u8],
    ) -> Result<(), Error> {
        output.write_all(MAGIC)?;
        output.write_all(&VERSION.to_le_bytes())?;
        output.write_all(&(windows.len() as u32).to_le_bytes())?;
        for &window in windows {
            output.write_all(&window.start().to_le_bytes())?;
            output.write_all(&window.first_id.to_le_bytes())?;
            output.write_all(&[window.prefix, if window.interleaved { 2 } else { 1 }, 0, 0])?;
        }
        output.write_all(data)?;
        output.flush()?;
        Ok(())
    }

    pub(crate) fn total_bytes(windows: &[SharedAlias], base: usize) -> Result<usize, Error> {
        add(
            add(HEADER, crate::layout::multiply(windows.len(), ROW)?)?,
            base,
        )
    }
}

pub(crate) fn visit<E: From<Error>>(
    section: crate::layout::Section,
    bytes: &[u8],
    mut emit: impl FnMut(crate::target::Entry) -> Result<(), E>,
) -> Result<(), E> {
    if section.encoding == crate::layout::Encoding::Packed {
        packed::visit_entries(
            slice(bytes, section.offset, section.length)?,
            section.count,
            section.ipv6,
            &mut emit,
        )?;
    } else {
        for position in 0..section.count {
            emit(section.entry(bytes, position)?)?;
        }
    }
    Ok(())
}
