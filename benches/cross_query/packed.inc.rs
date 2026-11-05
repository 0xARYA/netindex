// Appended only to the generated benchmark copy of packed.rs.
pub(crate) fn study_vector<const N: usize>(
    bytes: &[u8],
    count: usize,
    queries: &[Option<u128>],
    ipv6: bool,
    search: Search,
    directory: Option<&Directory>,
    enable_simd: bool,
) -> Result<[Option<Entry>; N], Error> {
    if (N != 4 && N != 8) || queries.len() > N {
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
    let low = crate::study_simd::upper_fences::<N>(bytes, low, high, queries, enable_simd)?;
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
