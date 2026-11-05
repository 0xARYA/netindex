use std::{
    collections::hash_map::RandomState,
    fs::File,
    hash::BuildHasher,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use tempfile::NamedTempFile;

use crate::Error;

use super::{DiskBudget, groups::Groups, positioned, temporary};

const CELL_BYTES: u64 = 24;
const PAGE_CELLS: u64 = 16;
const PAGE_BYTES: usize = PAGE_CELLS as usize * 32;

/// An on-disk open-addressed table for values that do not fit in the key cache.
/// Hashes only select candidates; equality always compares the complete bytes.
pub(super) struct DiskTable {
    file: Option<NamedTempFile>,
    capacity: u64,
    entries: u64,
    state: RandomState,
    cell_bytes: u64,
    pages: PageCache,
    groups: Groups,
}

impl DiskTable {
    pub(super) fn new() -> Self {
        Self {
            file: None,
            capacity: 0,
            entries: 0,
            state: RandomState::new(),
            cell_bytes: CELL_BYTES,
            pages: PageCache::default(),
            groups: Groups::default(),
        }
    }

    pub(super) fn with_references() -> Self {
        Self {
            cell_bytes: 32,
            ..Self::new()
        }
    }

    pub(super) fn set_cache_budget(&mut self, budget: usize) {
        self.pages.budget = budget;
    }

    pub(super) fn cache_bytes(&self) -> usize {
        self.pages.slots.capacity() * size_of::<Page>() + self.groups.bytes()
    }

    pub(super) fn hash(&self, payload: &[u8]) -> u64 {
        self.state.hash_one(payload)
    }

    #[cfg(test)]
    fn find(
        &mut self,
        hash: u64,
        payload: &[u8],
        pool: &mut BufWriter<File>,
        reader: &File,
        end: u64,
    ) -> Result<Option<(u64, u64)>, Error> {
        Ok(self
            .find_reference(hash, payload, pool, reader, end)?
            .map(|(offset, length, _)| (offset, length)))
    }

    pub(super) fn find_reference(
        &mut self,
        hash: u64,
        payload: &[u8],
        pool: &mut BufWriter<File>,
        reader: &File,
        end: u64,
    ) -> Result<Option<(u64, u64, u32)>, Error> {
        if self.groups.enabled() {
            return self.find_grouped_reference(hash, payload, pool, reader, end);
        }
        let Some(file) = self.file.as_mut() else {
            return Ok(None);
        };

        let mut position = hash & (self.capacity - 1);
        for _ in 0..self.capacity {
            let cell = self
                .pages
                .read(file.as_file_mut(), position, self.cell_bytes)?;
            if cell.length_plus_one == 0 {
                return Ok(None);
            }
            let length = cell.length_plus_one - 1;
            if cell.hash == hash
                && length == payload.len() as u64
                && equals(pool, reader, end, cell.offset, payload)?
            {
                return Ok(Some((cell.offset, length, cell.ordinal as u32)));
            }
            position = (position + 1) & (self.capacity - 1);
        }

        Err(Error::Invalid("full payload table"))
    }

    #[inline(never)]
    fn find_grouped_reference(
        &mut self,
        hash: u64,
        payload: &[u8],
        pool: &mut BufWriter<File>,
        reader: &File,
        end: u64,
    ) -> Result<Option<(u64, u64, u32)>, Error> {
        let file = self.file.as_mut().ok_or(Error::Invalid("payload table"))?;
        let mut position = hash & (self.capacity - 1);
        let mut remaining = self.capacity;
        while remaining > 0 {
            let probe = self.groups.probe(position, hash)?;
            let mut candidates = probe.candidates;
            while candidates != 0 {
                let lane = candidates.trailing_zeros();
                candidates &= candidates - 1;
                let cell = self.pages.read(
                    file.as_file_mut(),
                    probe.base + u64::from(lane),
                    self.cell_bytes,
                )?;
                let length = cell
                    .length_plus_one
                    .checked_sub(1)
                    .ok_or(Error::Invalid("payload table fingerprint"))?;
                if cell.hash == hash
                    && length == payload.len() as u64
                    && equals(pool, reader, end, cell.offset, payload)?
                {
                    return Ok(Some((cell.offset, length, cell.ordinal as u32)));
                }
            }
            if probe.empty.is_some() {
                return Ok(None);
            }
            remaining = remaining.saturating_sub(probe.count);
            position = (position + probe.count) & (self.capacity - 1);
        }
        Err(Error::Invalid("full payload table"))
    }

    #[cfg(test)]
    fn insert(
        &mut self,
        hash: u64,
        span: (u64, u64),
        directory: &Path,
        budget: &mut DiskBudget,
    ) -> Result<(), Error> {
        self.insert_reference(hash, (span.0, span.1, 0), directory, budget)
    }

    pub(super) fn insert_reference(
        &mut self,
        hash: u64,
        span: (u64, u64, u32),
        directory: &Path,
        budget: &mut DiskBudget,
    ) -> Result<(), Error> {
        if self.entries >= self.capacity / 2 {
            self.grow(directory, budget)?;
        }
        let cell = Cell {
            hash,
            ordinal: u64::from(span.2),
            offset: span.0,
            length_plus_one: span
                .1
                .checked_add(1)
                .ok_or(Error::Limit("payload length"))?,
        };
        let file = self.file.as_mut().ok_or(Error::Invalid("payload table"))?;
        place(
            file.as_file_mut(),
            self.capacity,
            cell,
            self.cell_bytes,
            &mut self.pages,
            &mut self.groups,
        )?;
        self.entries += 1;

        Ok(())
    }

    pub(super) fn close(mut self, budget: &mut DiskBudget) -> Result<(), Error> {
        // The temporary table is no longer consulted, so dirty pages can be discarded.
        if let Some(file) = self.file.take() {
            file.close().map_err(temporary("remove payload table"))?;
            budget.release(self.capacity * self.cell_bytes)?;
        }

        Ok(())
    }

    fn grow(&mut self, directory: &Path, budget: &mut DiskBudget) -> Result<(), Error> {
        let capacity = self
            .capacity
            .max(32)
            .checked_mul(2)
            .ok_or(Error::Limit("payload table bytes"))?;
        let bytes = capacity
            .checked_mul(self.cell_bytes)
            .ok_or(Error::Limit("payload table bytes"))?;
        budget.reserve(bytes)?;
        let mut file =
            NamedTempFile::new_in(directory).map_err(temporary("create payload table"))?;
        file.as_file_mut()
            .set_len(bytes)
            .map_err(temporary("size payload table"))?;

        self.pages.allocate()?;
        self.groups.reset(
            capacity,
            self.pages
                .budget
                .saturating_sub(self.pages.slots.capacity() * size_of::<Page>()),
        )?;
        if let Some(previous) = self.file.as_mut() {
            // Rehash reads the old table sequentially; the cache then belongs to the new file.
            self.pages
                .flush_and_invalidate(previous.as_file_mut(), self.cell_bytes)?;
            previous
                .as_file_mut()
                .seek(SeekFrom::Start(0))
                .map_err(temporary("rewind payload table"))?;
            let mut input = BufReader::new(previous.as_file_mut());
            for _ in 0..self.capacity {
                let cell = Cell::read(&mut input, self.cell_bytes)?;
                if cell.length_plus_one != 0 {
                    place(
                        file.as_file_mut(),
                        capacity,
                        cell,
                        self.cell_bytes,
                        &mut self.pages,
                        &mut self.groups,
                    )?;
                }
            }
        }
        if let Some(previous) = self.file.take() {
            previous
                .close()
                .map_err(temporary("remove payload table"))?;
            budget.release(self.capacity * self.cell_bytes)?;
        }
        self.file = Some(file);
        self.capacity = capacity;

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Cell {
    hash: u64,
    ordinal: u64,
    offset: u64,
    // Zero denotes a vacant cell, including when the payload itself is empty.
    length_plus_one: u64,
}

impl Cell {
    fn at(file: &mut File, position: u64, cell_bytes: u64) -> Result<Self, Error> {
        let mut bytes = [0; 32];
        positioned::read_exact(
            file,
            bytes
                .get_mut(..cell_bytes as usize)
                .ok_or(Error::Invalid("payload table cell"))?,
            position * cell_bytes,
        )
        .map_err(temporary("read payload table"))?;
        Self::decode(bytes)
    }

    fn read(input: &mut impl Read, cell_bytes: u64) -> Result<Self, Error> {
        let mut bytes = [0; 32];
        input
            .read_exact(
                bytes
                    .get_mut(..cell_bytes as usize)
                    .ok_or(Error::Invalid("payload table cell"))?,
            )
            .map_err(temporary("read payload table"))?;
        Self::decode(bytes)
    }

    fn decode(bytes: [u8; 32]) -> Result<Self, Error> {
        let mut words = bytes.chunks_exact(8);
        let mut word = || {
            let bytes = words.next().ok_or(Error::Invalid("payload table cell"))?;
            Ok::<_, Error>(u64::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::Invalid("payload table word"))?,
            ))
        };
        Ok(Self {
            hash: word()?,
            offset: word()?,
            length_plus_one: word()?,
            ordinal: word()?,
        })
    }

    fn encode(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (chunk, value) in bytes.chunks_exact_mut(8).zip([
            self.hash,
            self.offset,
            self.length_plus_one,
            self.ordinal,
        ]) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn write(self, file: &mut File, position: u64, cell_bytes: u64) -> Result<(), Error> {
        let bytes = self.encode();
        positioned::write_all(
            file,
            bytes
                .get(..cell_bytes as usize)
                .ok_or(Error::Invalid("payload table cell"))?,
            position * cell_bytes,
        )
        .map_err(temporary("write payload table"))?;
        Ok(())
    }
}

#[derive(Clone)]
struct Page {
    number: u64,
    dirty: bool,
    bytes: [u8; PAGE_BYTES],
}

impl Page {
    fn flush(&mut self, file: &File, width: usize) -> Result<(), Error> {
        if self.dirty {
            positioned::write_all(
                file,
                self.bytes
                    .get(..width)
                    .ok_or(Error::Invalid("payload table page"))?,
                self.number * width as u64,
            )
            .map_err(temporary("write payload table page"))?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[derive(Default)]
struct PageCache {
    slots: Vec<Page>,
    budget: usize,
}

impl PageCache {
    fn allocate(&mut self) -> Result<(), Error> {
        let slots = self.budget / size_of::<Page>();
        if slots == 0 || !self.slots.is_empty() {
            return Ok(());
        }
        let slots = 1 << slots.ilog2();
        let mut pages = Vec::new();
        pages.try_reserve_exact(slots)?;
        if pages.capacity() > self.budget / size_of::<Page>() {
            return Err(Error::Limit("disk table cache bytes"));
        }
        pages.resize(
            slots,
            Page {
                number: u64::MAX,
                dirty: false,
                bytes: [0; PAGE_BYTES],
            },
        );
        self.slots = pages;
        Ok(())
    }

    fn page(&mut self, file: &File, position: u64, cell_bytes: u64) -> Result<&mut Page, Error> {
        let number = position / PAGE_CELLS;
        let width = (PAGE_CELLS * cell_bytes) as usize;
        let at = number as usize & (self.slots.len() - 1);
        let slot = self
            .slots
            .get_mut(at)
            .ok_or(Error::Invalid("payload table page"))?;
        if slot.number != number {
            slot.flush(file, width)?;
            slot.number = u64::MAX;
            positioned::read_exact(
                file,
                slot.bytes
                    .get_mut(..width)
                    .ok_or(Error::Invalid("payload table page"))?,
                number * width as u64,
            )
            .map_err(temporary("read payload table page"))?;
            slot.number = number;
        }
        Ok(slot)
    }

    fn read(&mut self, file: &mut File, position: u64, cell_bytes: u64) -> Result<Cell, Error> {
        if self.slots.is_empty() {
            return Cell::at(file, position, cell_bytes);
        }
        let start = ((position % PAGE_CELLS) * cell_bytes) as usize;
        let page = self.page(file, position, cell_bytes)?;
        let mut bytes = [0; 32];
        bytes
            .get_mut(..cell_bytes as usize)
            .ok_or(Error::Invalid("payload table cell"))?
            .copy_from_slice(
                page.bytes
                    .get(start..start + cell_bytes as usize)
                    .ok_or(Error::Invalid("payload table page cell"))?,
            );
        Cell::decode(bytes)
    }

    fn write(
        &mut self,
        file: &mut File,
        position: u64,
        cell_bytes: u64,
        cell: Cell,
    ) -> Result<(), Error> {
        if self.slots.is_empty() {
            return cell.write(file, position, cell_bytes);
        }
        let start = ((position % PAGE_CELLS) * cell_bytes) as usize;
        let page = self.page(file, position, cell_bytes)?;
        page.bytes
            .get_mut(start..start + cell_bytes as usize)
            .ok_or(Error::Invalid("payload table page cell"))?
            .copy_from_slice(
                cell.encode()
                    .get(..cell_bytes as usize)
                    .ok_or(Error::Invalid("payload table cell"))?,
            );
        page.dirty = true;
        Ok(())
    }

    fn flush_and_invalidate(&mut self, file: &File, cell_bytes: u64) -> Result<(), Error> {
        let width = (PAGE_CELLS * cell_bytes) as usize;
        for slot in &mut self.slots {
            slot.flush(file, width)?;
            slot.number = u64::MAX;
        }
        Ok(())
    }
}

fn place(
    file: &mut File,
    capacity: u64,
    cell: Cell,
    cell_bytes: u64,
    pages: &mut PageCache,
    groups: &mut Groups,
) -> Result<(), Error> {
    let mut position = cell.hash & (capacity - 1);
    if groups.enabled() {
        let mut remaining = capacity;
        while remaining > 0 {
            let probe = groups.probe(position, cell.hash)?;
            if let Some(position) = probe.empty {
                pages.write(file, position, cell_bytes, cell)?;
                return groups.set(position, cell.hash);
            }
            remaining = remaining.saturating_sub(probe.count);
            position = (position + probe.count) & (capacity - 1);
        }
        return Err(Error::Invalid("full payload table"));
    }
    for _ in 0..capacity {
        if pages.read(file, position, cell_bytes)?.length_plus_one == 0 {
            return pages.write(file, position, cell_bytes, cell);
        }
        position = (position + 1) & (capacity - 1);
    }
    Err(Error::Invalid("full payload table"))
}

fn equals(
    pool: &mut BufWriter<File>,
    reader: &File,
    end: u64,
    offset: u64,
    payload: &[u8],
) -> Result<bool, Error> {
    let span_end = offset
        .checked_add(payload.len() as u64)
        .filter(|&value| value <= end)
        .ok_or(Error::Invalid("payload comparison span"))?;
    let buffered_start = end
        .checked_sub(pool.buffer().len() as u64)
        .ok_or(Error::Invalid("payload buffer position"))?;
    if offset >= buffered_start {
        let start = (offset - buffered_start) as usize;
        return Ok(pool
            .buffer()
            .get(start..start + payload.len())
            .ok_or(Error::Invalid("buffered payload span"))?
            == payload);
    }
    if span_end > buffered_start {
        pool.flush().map_err(temporary("flush payload pool"))?;
    }

    let mut buffer = [0; 8192];
    let mut position = offset;
    for chunk in payload.chunks(buffer.len()) {
        let stored = buffer
            .get_mut(..chunk.len())
            .ok_or(Error::Invalid("payload compare buffer"))?;
        positioned::read_exact(reader, stored, position)
            .map_err(temporary("compare payload pool"))?;
        if stored != chunk {
            return Ok(false);
        }
        position += chunk.len() as u64;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_comparisons_need_no_read_access_and_cross_buffer_spans_flush() {
        let file = NamedTempFile::new().unwrap();
        let reader = file.reopen().unwrap();
        let mut pool = BufWriter::with_capacity(8, file.reopen().unwrap());
        pool.write_all(b"abc").unwrap();
        let no_read_access = std::fs::OpenOptions::new()
            .write(true)
            .open(file.path())
            .unwrap();
        assert!(equals(&mut pool, &no_read_access, 3, 0, b"abc").unwrap());
        assert!(!equals(&mut pool, &no_read_access, 3, 0, b"abx").unwrap());
        assert!(equals(&mut pool, &no_read_access, 3, 3, b"").unwrap());

        pool.write_all(b"defghij").unwrap();
        pool.write_all(b"klm").unwrap();
        assert!(equals(&mut pool, &reader, 13, 8, b"ijklm").unwrap());
        pool.write_all(b"no").unwrap();
        pool.flush().unwrap();
        let mut bytes = [0; 15];
        positioned::read_exact(&reader, &mut bytes, 0).unwrap();
        assert_eq!(&bytes, b"abcdefghijklmno");
        assert!(matches!(
            equals(&mut pool, &reader, 15, 14, b"no"),
            Err(Error::Invalid("payload comparison span"))
        ));
    }

    #[test]
    fn colliding_hashes_compare_complete_payloads_and_restore_the_append_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let payload_file = NamedTempFile::new().unwrap();
        let mut pool = BufWriter::new(payload_file.reopen().unwrap());
        let reader = payload_file.reopen().unwrap();
        let mut table = DiskTable::new();
        table.set_cache_budget(size_of::<Page>());
        let mut budget = DiskBudget {
            used: 0,
            peak: 0,
            limit: 1 << 20,
        };
        let mut end = 0;
        let mut values = Vec::new();
        for id in 0..100u8 {
            let mut payload = vec![0x5a; 16_385];
            *payload.last_mut().unwrap() = id;
            assert_eq!(
                table.find(7, &payload, &mut pool, &reader, end).unwrap(),
                None
            );
            let span = (end, payload.len() as u64);
            table
                .insert(7, span, directory.path(), &mut budget)
                .unwrap();
            pool.write_all(&payload).unwrap();
            values.push((payload, span));
            end += span.1;
        }
        table
            .insert(7, (end, 0), directory.path(), &mut budget)
            .unwrap();
        assert_eq!(
            table.find(7, b"", &mut pool, &reader, end).unwrap(),
            Some((end, 0))
        );
        for (payload, span) in values.iter().rev() {
            assert_eq!(
                table.find(7, payload, &mut pool, &reader, end).unwrap(),
                Some(*span)
            );
        }
        assert_eq!(pool.stream_position().unwrap(), end);
        assert_eq!(pool.get_ref().metadata().unwrap().len(), end);
        table.close(&mut budget).unwrap();
        assert_eq!(budget.used, 0);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn reference_ids_survive_collisions_and_growth_without_entering_payload_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let payload_file = NamedTempFile::new().unwrap();
        let mut pool = BufWriter::new(payload_file.reopen().unwrap());
        let reader = payload_file.reopen().unwrap();
        let mut table = DiskTable::with_references();
        table.set_cache_budget(size_of::<Page>());
        let mut budget = DiskBudget {
            used: 0,
            peak: 0,
            limit: 1 << 20,
        };
        for id in 0..160u32 {
            let payload = id.to_le_bytes();
            table
                .insert_reference(
                    7,
                    (u64::from(id) * 4, 4, id * 3),
                    directory.path(),
                    &mut budget,
                )
                .unwrap();
            pool.write_all(&payload).unwrap();
        }
        for id in (0..160u32).rev() {
            assert_eq!(
                table
                    .find_reference(7, &id.to_le_bytes(), &mut pool, &reader, 640)
                    .unwrap(),
                Some((u64::from(id) * 4, 4, id * 3))
            );
        }
        assert_eq!(budget.used, 512 * 32);
        table.close(&mut budget).unwrap();
        assert_eq!(budget.used, 0);
    }

    #[test]
    fn growth_charges_simultaneous_tables_and_releases_the_old_one() {
        let directory = tempfile::tempdir().unwrap();
        let mut table = DiskTable::new();
        let mut budget = DiskBudget {
            used: 0,
            peak: 0,
            limit: 4607,
        };
        for id in 0..32u64 {
            table
                .insert(id, (id, 1), directory.path(), &mut budget)
                .unwrap();
        }
        assert_eq!(budget.used, 1536);
        assert!(matches!(
            table.insert(32, (32, 1), directory.path(), &mut budget),
            Err(Error::Limit("temporary bytes"))
        ));
        assert_eq!(budget.used, 1536);

        budget.limit = 4608;
        table
            .insert(32, (32, 1), directory.path(), &mut budget)
            .unwrap();
        assert_eq!(budget.peak, 4608);
        assert_eq!(budget.used, 3072);
        table.close(&mut budget).unwrap();
        assert_eq!(budget.used, 0);
    }

    #[test]
    fn grouped_collisions_wrap_grow_and_fall_back_with_exact_payload_equality() {
        let directory = tempfile::tempdir().unwrap();
        for cached in [false, true] {
            for compact in [false, true] {
                let mut table = if compact {
                    DiskTable::with_references()
                } else {
                    DiskTable::new()
                };
                let cache_budget = if cached { size_of::<Page>() + 512 } else { 0 };
                table.set_cache_budget(cache_budget);
                let payload_file = NamedTempFile::new().unwrap();
                let mut pool = BufWriter::new(payload_file.reopen().unwrap());
                let reader = payload_file.reopen().unwrap();
                let mut budget = DiskBudget {
                    used: 0,
                    peak: 0,
                    limit: 1 << 20,
                };
                for id in 0..300u32 {
                    // All hashes have the same fingerprint and initial bucket.
                    let hash = u64::MAX - (u64::from(id % 4) << 10);
                    let payload = id.to_le_bytes();
                    assert_eq!(
                        table
                            .find_reference(hash, &payload, &mut pool, &reader, u64::from(id) * 4)
                            .unwrap(),
                        None
                    );
                    table
                        .insert_reference(
                            hash,
                            (u64::from(id) * 4, 4, id),
                            directory.path(),
                            &mut budget,
                        )
                        .unwrap();
                    pool.write_all(&payload).unwrap();
                    assert!(table.cache_bytes() <= cache_budget);
                    if cached && id == 200 {
                        assert!(table.groups.enabled());
                    }
                }
                // A one-page cache leaves only 512 bytes for fingerprints. At
                // 1024 cells the directory is dropped and ordinary probing resumes.
                assert!(!table.groups.enabled());
                for id in (0..300u32).rev() {
                    let hash = u64::MAX - (u64::from(id % 4) << 10);
                    assert_eq!(
                        table
                            .find_reference(hash, &id.to_le_bytes(), &mut pool, &reader, 1200)
                            .unwrap(),
                        Some((u64::from(id) * 4, 4, if compact { id } else { 0 }))
                    );
                }
                table.close(&mut budget).unwrap();
                assert_eq!(budget.used, 0);
            }
        }
    }

    #[test]
    fn dirty_page_eviction_preserves_the_write_error() {
        let file = NamedTempFile::new().unwrap();
        file.as_file().set_len(2 * PAGE_CELLS * CELL_BYTES).unwrap();
        let mut cache = PageCache {
            budget: size_of::<Page>(),
            ..PageCache::default()
        };
        cache.allocate().unwrap();
        cache.page(file.as_file(), 0, CELL_BYTES).unwrap().dirty = true;
        let read_only = File::open(file.path()).unwrap();

        assert!(matches!(
            cache.page(&read_only, PAGE_CELLS, CELL_BYTES),
            Err(Error::TemporaryIo {
                operation: "write payload table page",
                ..
            })
        ));
        assert!(cache.page(file.as_file(), 0, CELL_BYTES).unwrap().dirty);
        cache.page(file.as_file(), PAGE_CELLS, CELL_BYTES).unwrap();
    }

    #[test]
    fn truncated_table_reads_retain_the_io_cause() {
        let directory = tempfile::tempdir().unwrap();
        let mut table = DiskTable::new();
        let mut pool = BufWriter::new(tempfile::tempfile().unwrap());
        let reader = tempfile::tempfile().unwrap();
        let mut budget = DiskBudget {
            used: 0,
            peak: 0,
            limit: 4096,
        };
        table
            .insert(7, (0, 1), directory.path(), &mut budget)
            .unwrap();
        table
            .file
            .as_mut()
            .unwrap()
            .as_file_mut()
            .set_len(0)
            .unwrap();
        assert!(
            matches!(table.find(7, b"a", &mut pool, &reader, 1), Err(Error::TemporaryIo { operation: "read payload table", error })
            if error.kind() == std::io::ErrorKind::UnexpectedEof)
        );
    }
}
