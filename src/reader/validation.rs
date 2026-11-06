use crate::{
    Coverage, Error, Limits, Reader,
    aliases::{self, Aliases},
    broad_routes::BroadRoutes,
    layout::{Encoding, Layout, MAGIC, Section, add, slice, u32_at},
    packed,
    payload::{Slots, directory_span},
    search::Search,
    target::Entry,
};

impl<B: AsRef<[u8]>> Reader<B> {
    pub(super) fn open_inner(bytes: B, limits: Limits, broad_routes: bool) -> Result<Self, Error> {
        let artifact = bytes.as_ref();
        if artifact.len() > limits.bytes {
            return Err(Error::Limit("artifact bytes"));
        }

        let (windows, offset) =
            if artifact.starts_with(MAGIC) && u32_at(artifact, 8)? == aliases::VERSION {
                Aliases::read(artifact)?
            } else {
                (Vec::new(), 0)
            };

        let data = artifact
            .get(offset..)
            .ok_or(Error::Invalid("shared alias base"))?;
        let mut layout = Layout::read(data, limits)?;

        let (mut coverage, broad_routes) = validate_index(data, layout, broad_routes)?;
        let bounds = [ip_bounds(data, layout.v4)?, ip_bounds(data, layout.v6)?];
        let ipv4_directory = ip_directory(data, layout.v4, bounds[0])?;

        let aliases = if offset == 0 {
            None
        } else {
            let aliases = Aliases::validate(&windows, data, layout, limits)?;
            for window in &aliases.windows {
                coverage.insert(window.start(), window.end(), true);
            }
            layout.rebase(offset);
            Some(aliases)
        };

        Ok(Self {
            bytes,
            layout,
            search: Search::detect(),
            bounds,
            ipv4_directory,
            coverage,
            broad_routes,
            aliases,
        })
    }
}

fn ip_bounds(bytes: &[u8], section: Section) -> Result<(u128, u128), Error> {
    if section.count == 0 {
        return Ok((0, 0));
    }

    let start = section.entry(bytes, 0)?.start;
    let end = if section.encoding == Encoding::Packed {
        section.entry(bytes, section.count - 1)?.end
    } else {
        section.maximum(bytes, section.count / 2)?
    };

    Ok((start, end))
}

fn ip_directory(
    bytes: &[u8],
    section: Section,
    (start, end): (u128, u128),
) -> Result<Option<packed::Directory>, Error> {
    if section.encoding != Encoding::Packed {
        return Ok(None);
    }

    packed::Directory::build(
        slice(bytes, section.offset, section.length)?,
        section.count,
        start,
        end,
    )
}

fn validate_index(
    bytes: &[u8],
    layout: Layout,
    broad_routes: bool,
) -> Result<(Coverage, Option<BroadRoutes>), Error> {
    let (coverage, routes) = validate_targets(bytes, layout, broad_routes)?;
    validate_payloads(bytes, layout)?;

    Ok((coverage, routes))
}

fn validate_targets(
    bytes: &[u8],
    layout: Layout,
    broad_routes: bool,
) -> Result<(Coverage, Option<BroadRoutes>), Error> {
    let mut seen = Vec::new();
    let words = layout.records.div_ceil(64);
    seen.try_reserve_exact(words)?;
    seen.resize(words, 0u64);

    let mut coverage = Coverage::empty();
    let mut routes = broad_routes.then(BroadRoutes::empty);

    for section in [layout.v4, layout.v6] {
        let ipv6 = section.ipv6;
        if section.encoding == Encoding::Packed {
            packed::validate(
                slice(bytes, section.offset, section.length)?,
                section.count,
                layout.bitpacking,
            )?;
        }

        let mut previous = None;
        let mut previous_end = None;

        let mut check = |entry: Entry| -> Result<(), Error> {
            entry.validate(ipv6)?;

            let key = (entry.start, entry.end, entry.id);
            if previous.is_some_and(|previous| previous >= key) {
                return Err(Error::Invalid("IP entry order"));
            }

            if section.encoding == Encoding::Packed
                && previous_end.is_some_and(|end| end >= entry.start)
            {
                return Err(Error::Invalid("packed overlapping targets"));
            }

            mark_record_id(&mut seen, layout.records, entry.id)?;

            coverage.insert(entry.start, entry.end, ipv6);

            if section.encoding == Encoding::Packed
                && let Some(routes) = &mut routes
            {
                routes.insert(entry, ipv6)?;
            }

            previous = Some(key);
            previous_end = Some(entry.end);

            Ok(())
        };

        section.visit_entries(bytes, &mut check)?;

        if section.encoding == Encoding::Interval {
            validate_interval_maximum(bytes, section, 0, section.count)?;
        }
    }

    let mut previous = None;

    for position in 0..layout.asns.count {
        let row = layout.asns.row(bytes, position)?;
        let key = (u32_at(row, 0)?, u32_at(row, 4)?);

        if key.0 == 0 || previous.is_some_and(|previous| previous >= key) {
            return Err(Error::Invalid("ASN entry order or key"));
        }

        mark_record_id(&mut seen, layout.records, key.1)?;
        previous = Some(key);
    }

    Ok((coverage, routes.filter(|routes| !routes.is_empty())))
}

fn validate_payloads(bytes: &[u8], layout: Layout) -> Result<(), Error> {
    match layout.slot_encoding {
        Slots::Indexed { values, .. } => validate_indexed_payloads(bytes, layout, values),
        Slots::Fixed { .. } => validate_fixed_payloads(bytes, layout),
        Slots::Spans(_) => validate_payload_spans(bytes, layout),
    }
}

fn validate_indexed_payloads(bytes: &[u8], layout: Layout, values: usize) -> Result<(), Error> {
    let mut end = 0;
    for ordinal in 0..values {
        let (start, length) = directory_span(bytes, add(layout.slots, 8)?, ordinal)?;
        if start != end {
            return Err(Error::Invalid("payload directory order"));
        }

        end = add(start, length)?;
    }

    if end != layout.bytes - layout.payloads {
        return Err(Error::Invalid("payload length"));
    }

    let mut next = 0;
    for id in 0..layout.records {
        let ordinal = layout.slot_encoding.ordinal(bytes, layout.slots, id)?;
        if ordinal > next {
            return Err(Error::Invalid("payload reference order"));
        }

        if ordinal == next {
            next += 1;
        }
    }

    if next != values {
        return Err(Error::Invalid("unused payload directory entry"));
    }

    Ok(())
}

fn validate_fixed_payloads(bytes: &[u8], layout: Layout) -> Result<(), Error> {
    let mut end = 0;

    for id in 0..layout.records {
        let (start, length) = layout.span(bytes, id as u32)?;
        if start > end {
            return Err(Error::Invalid("payload reference order"));
        }

        slice(bytes, add(layout.payloads, start)?, length)?;

        if start == end {
            end = end
                .checked_add(length)
                .ok_or(Error::Limit("payload bytes"))?;
        }
    }

    if end != layout.bytes - layout.payloads {
        return Err(Error::Invalid("payload length"));
    }

    Ok(())
}

fn validate_payload_spans(bytes: &[u8], layout: Layout) -> Result<(), Error> {
    // First-seen contiguous spans stay sorted, including empty spans before new bytes.
    let mut offset = 0usize;
    let mut spans = Vec::new();
    let mut previous_span = None;

    for id in 0..layout.records {
        let (start, length) = layout.span(bytes, id as u32)?;
        if previous_span == Some((start, length)) {
            continue;
        }

        if (start < offset || length == 0) && spans.binary_search(&(start, length)).is_ok() {
            previous_span = Some((start, length));
            continue;
        }

        if start != offset {
            return Err(Error::Invalid("payload offset"));
        }

        slice(bytes, add(layout.payloads, offset)?, length)?;

        spans.try_reserve(1)?;
        spans.push((start, length));
        previous_span = Some((start, length));
        offset = offset
            .checked_add(length)
            .ok_or(Error::Limit("payload bytes"))?;
    }

    if offset != layout.bytes - layout.payloads {
        return Err(Error::Invalid("payload length"));
    }

    Ok(())
}

fn mark_record_id(seen: &mut [u64], records: usize, id: u32) -> Result<(), Error> {
    let id = id as usize;
    if id >= records {
        return Err(Error::Invalid("record ID"));
    }

    let seen = seen.get_mut(id / 64).ok_or(Error::Invalid("record ID"))?;
    let mask = 1u64 << (id % 64);
    if *seen & mask != 0 {
        return Err(Error::Invalid("duplicate record ID"));
    }

    *seen |= mask;

    Ok(())
}

fn validate_interval_maximum(
    bytes: &[u8],
    section: Section,
    low: usize,
    high: usize,
) -> Result<u128, Error> {
    if low == high {
        return Ok(0);
    }

    let mid = low + (high - low) / 2;
    let expected = section
        .endpoint(bytes, mid)?
        .max(validate_interval_maximum(bytes, section, low, mid)?)
        .max(validate_interval_maximum(bytes, section, mid + 1, high)?);

    if expected != section.maximum(bytes, mid)? {
        return Err(Error::Invalid("interval maximum"));
    }

    Ok(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_marks_reject_duplicates_word_boundaries_and_padding_ids() {
        let mut seen = [0; 2];

        for id in [0, 63, 64] {
            mark_record_id(&mut seen, 65, id).unwrap();
        }

        for id in [0, 63, 64] {
            assert!(matches!(
                mark_record_id(&mut seen, 65, id),
                Err(Error::Invalid("duplicate record ID"))
            ));
        }

        for id in [65, 127, u32::MAX] {
            assert!(matches!(
                mark_record_id(&mut seen, 65, id),
                Err(Error::Invalid("record ID"))
            ));
        }

        assert!(matches!(
            mark_record_id(&mut [], 0, 0),
            Err(Error::Invalid("record ID"))
        ));
        assert!(matches!(
            mark_record_id(&mut [], 1, 0),
            Err(Error::Invalid("record ID"))
        ));
    }
}
