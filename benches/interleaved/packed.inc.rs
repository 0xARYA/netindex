// Appended only to the generated benchmark copy of packed.rs.
pub(crate) fn study_interleaved<const N: usize>(
    bytes: &[u8],
    count: usize,
    queries: &[Option<u128>],
    ipv6: bool,
    search: Search,
    directory: Option<&Directory>,
) -> Result<[Option<Entry>; N], Error> {
    if N == 0 || N > 16 || queries.len() > N {
        return Err(Error::Invalid("interleave lanes"));
    }
    let mut low = [0usize; N];
    let mut high = [0usize; N];
    for (lane, &query) in queries.iter().enumerate() {
        if let Some(query) = query {
            (low[lane], high[lane]) = match directory {
                Some(directory) => directory.range(query)?,
                None => (0, count.div_ceil(BLOCK)),
            };
        }
    }
    loop {
        let mut pending = false;
        for lane in 0..queries.len() {
            if low[lane] < high[lane] {
                pending = true;
                let mid = low[lane] + (high[lane] - low[lane]) / 2;
                let base = u128_at(bytes, mid * FENCE)?;
                if base <= queries[lane].ok_or(Error::Invalid("inactive packed lane"))? {
                    low[lane] = mid + 1;
                } else {
                    high[lane] = mid;
                }
            }
        }
        if !pending {
            break;
        }
    }
    let mut entries = [None; N];
    for (lane, &query) in queries.iter().enumerate() {
        let Some(query) = query else {
            continue;
        };
        let Some(index) = low[lane].checked_sub(1) else {
            continue;
        };
        let block = read_block(bytes, count, index)?;
        let deltas = slice(bytes, block.offset, block.count * usize::from(block.width))?;
        let delta = query
            .checked_sub(block.base)
            .ok_or(Error::Invalid("packed search base"))?;
        let upper = search.upper_bound(deltas, usize::from(block.width), delta >> block.shift)?;
        if let Some(local) = upper.checked_sub(1) {
            let entry = block_entry(bytes, block, local, ipv6)?;
            entries[lane] = (query <= entry.end).then_some(entry);
        }
    }
    Ok(entries)
}
