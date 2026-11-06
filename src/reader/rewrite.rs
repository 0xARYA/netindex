use std::io::{BufWriter, Write};

use crate::{
    Error, Limits, Reader, SharedAlias,
    aliases::Aliases,
    layout::{Encoding, Section, slice},
    packed,
};

impl<B: AsRef<[u8]>> Reader<B> {
    /// Write opt-in format-6 bitpacked columns when they reduce the file size.
    ///
    /// Preserves IDs, original targets, metadata, payloads and shared aliases.
    /// Existing builders keep their ordinary output policy. Upgrade readers before
    /// deploying this output: earlier readers reject format 6, including inside
    /// format-5 alias envelopes. If no space is saved, copies the original bytes.
    /// Uses fixed block buffers and creates no threads. Output failure can leave
    /// partial bytes; stage the result before publishing.
    ///
    /// # Errors
    /// Fails on output I/O or invalid backing bytes. Keep the backing immutable.
    pub fn write_bitpacked_to(&self, output: impl Write) -> Result<usize, Error> {
        let bytes = self.bytes.as_ref();
        let [v4, v6] = [self.layout.v4, self.layout.v6];
        let v4_length = bitpacked_section_length(bytes, v4)?;
        let v6_length = bitpacked_section_length(bytes, v6)?;
        let length = bytes
            .len()
            .checked_sub(v4.length)
            .and_then(|remaining| remaining.checked_sub(v6.length))
            .and_then(|remaining| remaining.checked_add(v4_length))
            .and_then(|remaining| remaining.checked_add(v6_length))
            .ok_or(Error::Invalid("bitpacking length"))?;

        let mut output = BufWriter::new(output);
        if length >= bytes.len() {
            output.write_all(bytes)?;
            output.flush()?;
            return Ok(bytes.len());
        }

        output.write_all(slice(bytes, 0, self.layout.header_offset())?)?;
        let header = self
            .layout
            .bitpacked_header(bytes, [v4_length, v6_length])?;
        output.write_all(&header)?;
        output.write_all(self.metadata()?)?;

        for section in [v4, v6] {
            let data = slice(bytes, section.offset, section.length)?;
            if section.encoding == Encoding::Packed {
                packed::write_compacted(data, section.count, &mut output)?;
            } else {
                output.write_all(data)?;
            }
        }

        let tail = bytes
            .get(self.layout.asns.offset..)
            .ok_or(Error::Invalid("bitpacking tail"))?;
        output.write_all(tail)?;
        output.flush()?;

        Ok(length)
    }

    /// Write an opt-in format-5 artifact over this plain format-3, -4 or -6 index.
    ///
    /// Windows must be sorted by address and disjoint from each other and from
    /// native IPv6 assertions. The physical IPv4 section must contain networks
    /// with IDs starting at zero in sorted row order. Each window receives a
    /// contiguous ID block, or one window may interleave odd IDs with IPv4.
    /// Remaining physical IDs fill the remaining logical IDs in order.
    ///
    /// # Errors
    /// Fails on invalid windows, incompatible base indexes, resource limits or
    /// output I/O. I/O failure may leave partial output.
    pub fn write_shared_aliases(
        &self,
        windows: &[SharedAlias],
        output: impl Write,
        limits: Limits,
    ) -> Result<(), Error> {
        if self.aliases.is_some() {
            return Err(Error::Invalid("nested shared aliases"));
        }

        let bytes = self.bytes.as_ref();
        if Aliases::total_bytes(windows, bytes.len())? > limits.bytes {
            return Err(Error::Limit("artifact bytes"));
        }

        Aliases::validate(windows, bytes, self.layout, limits)?;
        Aliases::write(windows, output, bytes)
    }
}

fn bitpacked_section_length(bytes: &[u8], section: Section) -> Result<usize, Error> {
    if section.encoding != Encoding::Packed {
        return Ok(section.length);
    }

    packed::compacted_size(slice(bytes, section.offset, section.length)?, section.count)
}
