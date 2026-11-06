use crate::{
    Error, Reader,
    broad_routes::BroadRoutes,
    layout::{Encoding, Section, slice, u32_at},
    packed,
    payload::Slots,
};

/// Physical target encoding; shared aliases do not expand these sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionEncoding {
    /// Augmented interval rows supporting overlaps and inclusive ranges.
    Interval,
    /// Disjoint address/network columns with a fence per 256 assertions.
    Packed,
    /// Sorted exact-ASN key and insertion-ID pairs.
    Asn,
}

/// Physical rows, bytes and column types for one encoded section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionInspection {
    /// Observed encoding, without assumptions about alternative representations.
    pub encoding: SectionEncoding,
    /// Physical assertion count, excluding projected aliases.
    pub records: usize,
    /// Encoded section bytes, including packed fences when present.
    pub bytes: usize,
    /// Packed blocks, zero for other encodings.
    pub blocks: usize,
    /// Blocks using exact-bit deltas; remaining blocks use byte widths.
    pub bitpacked_blocks: usize,
}

/// Reference encoding; opaque payload contents are never decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadReferences {
    /// Offset/length spans of this many bytes per assertion.
    Spans {
        /// Bytes per span.
        width: usize,
    },
    /// Fixed payload length and compact offsets.
    Fixed {
        /// Bytes in each payload.
        length: usize,
        /// Bytes per offset; zero for empty payloads.
        width: u8,
    },
    /// Shared span directory with compact ordinals.
    Indexed {
        /// Number of shared payload spans.
        values: usize,
        /// Bytes per ordinal.
        width: u8,
    },
}

/// Encoded accounting and reader allocation capacities, not process RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inspection {
    /// Outer format version (5 for an alias envelope).
    pub format: u32,
    /// Embedded format for aliases, otherwise equal to `format`.
    pub base_format: u32,
    /// Logical assertions including projected aliases.
    pub records: usize,
    /// Physical assertions with payload references.
    pub physical_records: usize,
    /// Complete backing length; ownership and residency are caller-dependent.
    pub artifact_bytes: usize,
    /// Base header plus any alias header/descriptors.
    pub header_bytes: usize,
    /// Opaque metadata bytes.
    pub metadata_bytes: usize,
    /// Physical IPv4 section.
    pub ipv4: SectionInspection,
    /// Physical IPv6 section.
    pub ipv6: SectionInspection,
    /// Exact-ASN section.
    pub asn: SectionInspection,
    /// Reference region bytes, including shared directory/descriptors.
    pub reference_bytes: usize,
    /// Complete opaque payload region, counting shared values once.
    pub payload_bytes: usize,
    /// Observed reference encoding.
    pub references: PayloadReferences,
    /// Explicit IPv6 alias windows.
    pub alias_windows: usize,
    /// Reader object size, including its inline backing object/descriptor.
    pub reader_struct_bytes: usize,
    /// Allocated IPv4 directory capacity, excluding its inline descriptor.
    pub directory_heap_bytes: usize,
    /// Allocated broad-route table capacities, excluding inline descriptors.
    pub broad_route_heap_bytes: usize,
    /// Allocated alias vector capacities, excluding inline descriptors.
    pub alias_heap_bytes: usize,
    /// Requested transient ID-bitmap bytes during validation, already released.
    /// Other validation allocations are not included.
    pub validation_id_bitmap_bytes: usize,
}

impl<B: AsRef<[u8]>> Reader<B> {
    /// Inspect representation and allocation capacities without allocating.
    ///
    /// Visits packed descriptors only: O(blocks), without scanning targets or
    /// payloads. Header + metadata + section + reference + payload bytes sum to
    /// artifact bytes. Resource fields are separate measures, not an additive RSS
    /// estimate: backing may be inline, borrowed, owned or mapped. This reports
    /// selected representations, not hypothetical writer choices or schemas.
    ///
    /// # Errors
    /// Returns invalid-backing errors if required headers/descriptors changed.
    pub fn inspect(&self) -> Result<Inspection, Error> {
        let bytes = self.bytes.as_ref();
        if bytes.len() != self.layout.bytes {
            return Err(Error::Invalid("inspection backing length"));
        }
        let base = self.layout.header_offset();
        Ok(Inspection {
            format: u32_at(bytes, 8)?,
            base_format: u32_at(bytes, base + 8)?,
            records: self.len(),
            physical_records: self.layout.records,
            artifact_bytes: bytes.len(),
            header_bytes: base + crate::layout::HEADER,
            metadata_bytes: self.layout.metadata,
            ipv4: section(bytes, self.layout.v4)?,
            ipv6: section(bytes, self.layout.v6)?,
            asn: SectionInspection {
                encoding: SectionEncoding::Asn,
                records: self.layout.asns.count,
                bytes: self.layout.asns.length,
                blocks: 0,
                bitpacked_blocks: 0,
            },
            reference_bytes: self.layout.payloads - self.layout.slots,
            payload_bytes: bytes.len() - self.layout.payloads,
            references: match self.layout.slot_encoding {
                Slots::Spans(width) => PayloadReferences::Spans { width },
                Slots::Fixed { length, width } => PayloadReferences::Fixed { length, width },
                Slots::Indexed { values, width } => PayloadReferences::Indexed { values, width },
            },
            alias_windows: self
                .aliases
                .as_ref()
                .map_or(0, |aliases| aliases.windows.len()),
            reader_struct_bytes: size_of_val(self),
            directory_heap_bytes: self
                .ipv4_directory
                .as_ref()
                .map_or(0, packed::Directory::heap_bytes),
            broad_route_heap_bytes: self
                .broad_routes
                .as_ref()
                .map_or(0, BroadRoutes::heap_bytes),
            alias_heap_bytes: self
                .aliases
                .as_ref()
                .map_or(0, crate::aliases::Aliases::heap_bytes),
            validation_id_bitmap_bytes: self.layout.records.div_ceil(64) * size_of::<u64>(),
        })
    }
}

fn section(bytes: &[u8], section: Section) -> Result<SectionInspection, Error> {
    let packed = section.encoding == Encoding::Packed;
    Ok(SectionInspection {
        encoding: if packed {
            SectionEncoding::Packed
        } else {
            SectionEncoding::Interval
        },
        records: section.count,
        bytes: section.length,
        blocks: if packed {
            section.count.div_ceil(packed::BLOCK)
        } else {
            0
        },
        bitpacked_blocks: if packed {
            packed::bitpacked_blocks(slice(bytes, section.offset, section.length)?, section.count)?
        } else {
            0
        },
    })
}
