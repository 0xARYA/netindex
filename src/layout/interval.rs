//! Encoding and subtree maxima for the fixed-width interval sections.

use std::io::{Cursor, Write};

use crate::{Error, target::Entry};

use super::slice;

pub(super) const IPV4_ROW_BYTES: usize = 20;
pub(super) const IPV6_ROW_BYTES: usize = 56;

pub(crate) fn augment_maxima<N: Copy + Ord + Default>(entries: &mut [Entry<N>]) -> N {
    let mid = entries.len() / 2;
    let (left, rest) = entries.split_at_mut(mid);

    let Some((entry, right)) = rest.split_first_mut() else {
        return N::default();
    };

    entry.maximum = entry
        .end
        .max(augment_maxima(left))
        .max(augment_maxima(right));

    entry.maximum
}

pub(crate) fn write_row(output: &mut impl Write, entry: &Entry, ipv6: bool) -> Result<(), Error> {
    let mut bytes = [0; IPV6_ROW_BYTES];
    let mut row = Cursor::new(bytes.as_mut_slice());

    for value in [entry.start, entry.end, entry.maximum] {
        if ipv6 {
            row.write_all(&value.to_le_bytes())?;
        } else {
            row.write_all(&(value as u32).to_le_bytes())?;
        }
    }

    row.write_all(&entry.id.to_le_bytes())?;
    row.write_all(&[entry.prefix, entry.kind, 0, 0])?;

    output.write_all(slice(
        &bytes,
        0,
        if ipv6 { IPV6_ROW_BYTES } else { IPV4_ROW_BYTES },
    )?)?;

    Ok(())
}
