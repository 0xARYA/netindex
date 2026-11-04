use std::{fs::File, io};

/// The Windows operations update the file cursor. Callers must use a separate
/// handle when another operation depends on a sequential cursor.
pub(super) fn read_exact(file: &File, output: &mut [u8], offset: u64) -> io::Result<()> {
    read_all(output, offset, |bytes, position| {
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            file.seek_read(bytes, position)
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            file.read_at(bytes, position)
        }
        #[cfg(not(any(unix, windows)))]
        {
            use std::io::{Read, Seek, SeekFrom};
            let mut file = file;
            file.seek(SeekFrom::Start(position))?;
            file.read(bytes)
        }
    })
}

pub(super) fn write_all(file: &File, mut input: &[u8], mut offset: u64) -> io::Result<()> {
    while !input.is_empty() {
        #[cfg(windows)]
        let result = {
            use std::os::windows::fs::FileExt;
            file.seek_write(input, offset)
        };
        #[cfg(unix)]
        let result = {
            use std::os::unix::fs::FileExt;
            file.write_at(input, offset)
        };
        #[cfg(not(any(unix, windows)))]
        let result = {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = file;
            file.seek(SeekFrom::Start(offset))?;
            file.write(input)
        };
        match result {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(length) => {
                offset = offset
                    .checked_add(length as u64)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
                input = input
                    .get(length..)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_all(
    mut output: &mut [u8],
    mut offset: u64,
    mut read: impl FnMut(&mut [u8], u64) -> io::Result<usize>,
) -> io::Result<()> {
    while !output.is_empty() {
        match read(output, offset) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(length) => {
                offset = offset
                    .checked_add(length as u64)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
                output = output
                    .get_mut(length..)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Seek, SeekFrom};

    use super::*;

    #[test]
    fn short_and_interrupted_reads_resume_at_the_correct_offset() {
        let mut output = [0; 5];
        let mut calls = 0;
        read_all(&mut output, 7, |bytes, offset| {
            calls += 1;
            if calls == 1 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            bytes[0] = offset as u8;
            Ok(1)
        })
        .unwrap();
        assert_eq!(output, [7, 8, 9, 10, 11]);
        assert_eq!(calls, 6);
        assert_eq!(
            read_all(&mut output, 0, |_, _| Ok(0)).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn positioned_io_preserves_contents_and_rejects_short_files() {
        let mut file = tempfile::tempfile().unwrap();
        write_all(&file, b"payload", 11).unwrap();
        let mut output = [0; 7];
        read_exact(&file, &mut output, 11).unwrap();
        assert_eq!(&output, b"payload");
        assert_eq!(
            read_exact(&file, &mut output, 17).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut all = Vec::new();
        file.read_to_end(&mut all).unwrap();
        assert_eq!(all, [vec![0; 11], b"payload".to_vec()].concat());
    }
}
