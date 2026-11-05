pub(crate) fn study_at_block(
    bytes: &[u8],
    count: usize,
    index: usize,
    query: u128,
    ipv6: bool,
    search: Search,
) -> Result<Option<Entry>, Error> {
    let block = read_block(bytes, count, index)?;
    let deltas = slice(bytes, block.offset, block.count * usize::from(block.width))?;
    let delta = query
        .checked_sub(block.base)
        .ok_or(Error::Invalid("research packed base"))?;
    let upper = search.upper_bound(deltas, usize::from(block.width), delta >> block.shift)?;
    if let Some(local) = upper.checked_sub(1) {
        let entry = block_entry(bytes, block, local, ipv6)?;
        return Ok((query <= entry.end).then_some(entry));
    }
    Ok(None)
}

impl Directory {
    pub(crate) fn study_range(&self, query: u128) -> Result<(usize, usize), Error> {
        self.range(query)
    }
}
