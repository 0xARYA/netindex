use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use tempfile::NamedTempFile;

use crate::{Error, target::Entry};

use super::{DiskBudget, temporary};

pub(super) const ROW_BYTES: u64 = 40;

#[derive(Debug, Clone, Copy)]
pub(super) struct Row {
    start: u128,
    end: u128,
    id: u32,
    prefix: u8,
    kind: u8,
    pub section: u8,
}

impl Row {
    pub(super) fn new(entry: Entry, section: u8) -> Self {
        Self {
            start: entry.start,
            end: entry.end,
            id: entry.id,
            prefix: entry.prefix,
            kind: entry.kind,
            section,
        }
    }

    pub(super) fn into_entry(self) -> Entry {
        Entry {
            start: self.start,
            end: self.end,
            maximum: self.end,
            id: self.id,
            prefix: self.prefix,
            kind: self.kind,
        }
    }

    pub(super) fn write(self, output: &mut impl Write) -> Result<(), Error> {
        output
            .write_all(&self.start.to_le_bytes())
            .map_err(temporary("write run"))?;
        output
            .write_all(&self.end.to_le_bytes())
            .map_err(temporary("write run"))?;
        output
            .write_all(&self.id.to_le_bytes())
            .map_err(temporary("write run"))?;
        output
            .write_all(&[self.prefix, self.kind, self.section, 0])
            .map_err(temporary("write run"))?;

        Ok(())
    }

    pub(super) fn read(input: &mut impl Read) -> Result<Option<Self>, Error> {
        let mut bytes = [0; ROW_BYTES as usize];
        let first = bytes
            .get_mut(..1)
            .ok_or(Error::Invalid("temporary row buffer"))?;
        loop {
            match input.read(first) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(temporary("read run")(error)),
            }
        }

        input
            .read_exact(
                bytes
                    .get_mut(1..)
                    .ok_or(Error::Invalid("temporary row buffer"))?,
            )
            .map_err(temporary("read run"))?;

        let start = crate::layout::u128_at(&bytes, 0)?;
        let end = crate::layout::u128_at(&bytes, 16)?;

        let tail = bytes.get(36..).ok_or(Error::Invalid("temporary row"))?;
        let [prefix, kind, section, reserved] = tail else {
            return Err(Error::Invalid("temporary row"));
        };

        if *reserved != 0 || *section > 2 {
            return Err(Error::Invalid("temporary row fields"));
        }

        Ok(Some(Self {
            section: *section,
            start,
            end,
            id: crate::layout::u32_at(&bytes, 32)?,
            prefix: *prefix,
            kind: *kind,
        }))
    }

    fn key(self) -> (u8, u128, u128, u32) {
        (self.section, self.start, self.end, self.id)
    }
}

#[derive(Debug)]
pub(super) struct Run {
    file: NamedTempFile,
    rows: u64,
}

impl Run {
    pub(super) fn file(&mut self) -> &mut File {
        self.file.as_file_mut()
    }

    fn close(self, budget: &mut DiskBudget) -> Result<(), Error> {
        self.file.close().map_err(temporary("remove run"))?;
        budget.release(self.rows * ROW_BYTES)?;

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct Sorter {
    buffer: Vec<Row>,
    run_records: usize,
    levels: Vec<Option<Run>>,
}

impl Sorter {
    pub(super) fn new(run_records: usize) -> Result<Self, Error> {
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(run_records)?;

        Ok(Self {
            buffer,
            run_records,
            levels: Vec::new(),
        })
    }

    pub(super) fn push(
        &mut self,
        row: Row,
        directory: &Path,
        budget: &mut DiskBudget,
    ) -> Result<(), Error> {
        if self.buffer.len() == self.run_records {
            self.flush(directory, budget)?;
        }

        self.buffer.push(row);

        Ok(())
    }

    pub(super) fn finish(
        mut self,
        directory: &Path,
        budget: &mut DiskBudget,
    ) -> Result<Run, Error> {
        self.flush(directory, budget)?;

        let mut result = None;
        for run in self.levels.into_iter().flatten() {
            result = Some(match result {
                Some(previous) => merge(previous, run, directory, budget)?,
                None => run,
            });
        }

        match result {
            Some(run) => Ok(run),
            None => Ok(Run {
                file: NamedTempFile::new_in(directory).map_err(temporary("create empty run"))?,
                rows: 0,
            }),
        }
    }

    fn flush(&mut self, directory: &Path, budget: &mut DiskBudget) -> Result<(), Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        self.buffer.sort_unstable_by_key(|row| row.key());

        let rows = self.buffer.len() as u64;
        budget.reserve(rows * ROW_BYTES)?;
        let mut file = NamedTempFile::new_in(directory).map_err(temporary("create run"))?;

        {
            let mut output = BufWriter::new(file.as_file_mut());
            for &row in &self.buffer {
                row.write(&mut output)?;
            }

            output.flush().map_err(temporary("flush run"))?;
        }

        self.buffer.clear();

        let mut run = Run { file, rows };
        let mut level = 0;
        loop {
            if level == self.levels.len() {
                self.levels.try_reserve(1)?;
                self.levels.push(Some(run));
                break;
            }

            let slot = self
                .levels
                .get_mut(level)
                .ok_or(Error::Invalid("run level"))?;
            let Some(previous) = slot.take() else {
                *slot = Some(run);
                break;
            };

            run = merge(previous, run, directory, budget)?;
            level += 1;
        }

        Ok(())
    }
}

fn merge(
    mut left: Run,
    mut right: Run,
    directory: &Path,
    budget: &mut DiskBudget,
) -> Result<Run, Error> {
    let rows = left.rows + right.rows;
    budget.reserve(rows * ROW_BYTES)?;
    let mut file = NamedTempFile::new_in(directory).map_err(temporary("create merged run"))?;

    left.file()
        .seek(SeekFrom::Start(0))
        .map_err(temporary("rewind run"))?;
    right
        .file()
        .seek(SeekFrom::Start(0))
        .map_err(temporary("rewind run"))?;

    {
        let mut left = BufReader::new(left.file());
        let mut right = BufReader::new(right.file());
        let mut output = BufWriter::new(file.as_file_mut());

        let mut a = Row::read(&mut left)?;
        let mut b = Row::read(&mut right)?;
        let mut written = 0;

        while a.is_some() || b.is_some() {
            if written == rows {
                return Err(Error::Invalid("temporary run record count"));
            }

            match (a, b) {
                (Some(row), other) if other.is_none_or(|other| row.key() <= other.key()) => {
                    row.write(&mut output)?;
                    a = Row::read(&mut left)?;
                }
                (_, Some(row)) => {
                    row.write(&mut output)?;
                    b = Row::read(&mut right)?;
                }
                _ => return Err(Error::Invalid("run merge state")),
            }

            written += 1;
        }

        if written != rows {
            return Err(Error::Invalid("temporary run record count"));
        }

        output.flush().map_err(temporary("flush merged run"))?;
    }

    left.close(budget)?;
    right.close(budget)?;

    Ok(Run { file, rows })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn interrupted_reads_retry_without_losing_the_row() {
        struct InterruptedOnce<'a> {
            bytes: &'a [u8],
            interrupted: bool,
        }

        impl Read for InterruptedOnce<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::Error::from(io::ErrorKind::Interrupted));
                }

                self.bytes.read(output)
            }
        }

        let mut bytes = [0; ROW_BYTES as usize];
        bytes[0] = 7;
        bytes[16] = 9;
        bytes[32] = 3;
        bytes[37] = 2;
        let mut input = InterruptedOnce {
            bytes: &bytes,
            interrupted: false,
        };

        let row = Row::read(&mut input).unwrap().unwrap();

        assert_eq!(row.key(), (0, 7, 9, 3));
        assert!(Row::read(&mut input).unwrap().is_none());
    }

    #[test]
    fn buffered_rows_reconstruct_maxima_without_changing_run_bytes() {
        let entry = Entry {
            start: 3,
            end: 9,
            maximum: 99,
            id: 7,
            prefix: 0,
            kind: 2,
        };
        let row = Row::new(entry, 1);
        let mut bytes = Vec::new();

        row.write(&mut bytes).unwrap();
        let decoded = Row::read(&mut Cursor::new(&bytes)).unwrap().unwrap();
        let restored = decoded.into_entry();

        assert_eq!(bytes.len(), ROW_BYTES as usize);
        assert_eq!(decoded.key(), row.key());
        assert_eq!(restored.target(true).unwrap(), entry.target(true).unwrap());
        assert_eq!(restored.maximum, entry.end);
        assert!(size_of::<Row>() < size_of::<Entry>());
    }

    #[test]
    fn truncated_rows_and_runs_fail_instead_of_losing_assertions() {
        let directory = tempfile::tempdir().unwrap();
        let row = Row::new(
            Entry {
                start: 1,
                end: 1,
                maximum: 1,
                id: 0,
                prefix: 0,
                kind: 0,
            },
            2,
        );

        let mut bytes = Vec::new();
        row.write(&mut bytes).unwrap();

        for length in 1..bytes.len() {
            assert!(
                matches!(Row::read(&mut Cursor::new(&bytes[..length])), Err(Error::TemporaryIo { error, .. }) if error.kind() == io::ErrorKind::UnexpectedEof)
            );
        }

        let left = Run {
            file: NamedTempFile::new_in(directory.path()).unwrap(),
            rows: 1,
        };
        let right = Run {
            file: NamedTempFile::new_in(directory.path()).unwrap(),
            rows: 1,
        };
        let mut budget = DiskBudget {
            used: 80,
            peak: 80,
            limit: 1000,
        };

        assert!(matches!(
            merge(left, right, directory.path(), &mut budget),
            Err(Error::Invalid("temporary run record count"))
        ));

        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
