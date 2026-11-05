use std::{
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::{BufReader, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Instant,
};

use netindex::{ExternalBuilder, ExternalOptions, Limits, Reader, Target};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Failure = Box<dyn std::error::Error>;
const MAX_RECORDS: usize = 30_000_000;
const WORKSPACE: usize = 512 * 1024 * 1024;
const BYTE_BUDGETS: [u64; 5] = [4096, 65536, 1 << 20, 4 << 20, 64 << 20];

fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("analyze") if args.len() == 3 => {
            let file = File::open(&args[2])?;
            // SAFETY: runner privately copies the input and holds it immutable.
            let mapping = unsafe { memmap2::Mmap::map(&file) }?;
            println!("{}", analyze(&mapping)?);
        }
        Some("ingest") if args.len() == 5 => ingest(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        )?,
        _ => return Err("expected analyze FILE, or ingest ROWS OUTPUT TEMP_DIRECTORY".into()),
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Counts {
    references: u64,
    distinct: u64,
    physical: u64,
}

#[derive(Default)]
struct Histogram(BTreeMap<u64, u64>);
impl Histogram {
    fn add(&mut self, value: u64) {
        let bucket = if value == 0 {
            0
        } else {
            u64::from(64 - value.leading_zeros())
        };
        *self.0.entry(bucket).or_default() += 1;
    }
    fn json(&self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|(&bucket, &count)| {
                    let low = if bucket == 0 { 0 } else { 1u64 << (bucket - 1) };
                    let high = if bucket == 0 {
                        0
                    } else {
                        u64::MAX >> (64 - bucket)
                    };
                    json!({"low":low,"high":high,"count":count})
                })
                .collect(),
        )
    }
}

struct Tree {
    counts: Vec<u32>,
    bytes: Vec<u64>,
}
impl Tree {
    fn new(count: usize) -> Result<Self, Failure> {
        let mut counts = Vec::new();
        let mut bytes = Vec::new();
        counts.try_reserve_exact(count + 1)?;
        bytes.try_reserve_exact(count + 1)?;
        counts.resize(count + 1, 0);
        bytes.resize(count + 1, 0);
        Ok(Self { counts, bytes })
    }
    fn update(&mut self, index: usize, length: u64, insert: bool) {
        let mut at = index + 1;
        while at < self.counts.len() {
            if insert {
                self.counts[at] += 1;
                self.bytes[at] += length;
            } else {
                self.counts[at] -= 1;
                self.bytes[at] -= length;
            }
            at += at & at.wrapping_neg();
        }
    }
    fn prefix(&self, through: usize) -> (u64, u64) {
        let (mut count, mut bytes) = (0, 0);
        let mut at = through;
        while at > 0 {
            count += u64::from(self.counts[at]);
            bytes += self.bytes[at];
            at &= at - 1;
        }
        (count, bytes)
    }
    fn allocated(&self) -> usize {
        self.counts.capacity() * 4 + self.bytes.capacity() * 8
    }
}

struct Columns<'a> {
    bytes: &'a [u8],
    count: usize,
    slots: usize,
    pool: usize,
    pool_bytes: usize,
    slot_width: usize,
    fixed: Option<(usize, usize)>,
    metadata_bytes: usize,
    index_bytes: usize,
}
impl<'a> Columns<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, Failure> {
        if bytes.len() < 80
            || &bytes[..8] != b"NETINDEX"
            || u32::from_le_bytes(bytes[8..12].try_into()?) != 3
        {
            return Err("unsupported file header".into());
        }
        let flags = u32::from_le_bytes(bytes[12..16].try_into()?);
        let word = |at| -> Result<usize, Failure> {
            Ok(usize::try_from(u64::from_le_bytes(
                bytes[at..at + 8].try_into()?,
            ))?)
        };
        let count = word(16)?;
        if count > MAX_RECORDS {
            return Err("record bound".into());
        }
        let metadata_bytes = word(48)?;
        let pool_bytes = word(56)?;
        let index_bytes = if flags & 1 != 0 {
            word(64)?
        } else {
            word(24)?.checked_mul(20).ok_or("v4 size")?
        }
        .checked_add(if flags & 2 != 0 {
            word(72)?
        } else {
            word(32)?.checked_mul(56).ok_or("v6 size")?
        })
        .and_then(|n| n.checked_add(word(40).ok()?.checked_mul(8)?))
        .ok_or("index size")?;
        let slots = 80usize
            .checked_add(metadata_bytes)
            .and_then(|n| n.checked_add(index_bytes))
            .ok_or("slots offset")?;
        let slot_width = if flags & 4 != 0 { 8 } else { 16 };
        let fixed = if flags & 8 != 0 {
            let header = bytes
                .get(slots..slots.checked_add(8).ok_or("fixed header")?)
                .ok_or("fixed header")?;
            Some((
                u32::from_le_bytes(header[..4].try_into()?) as usize,
                usize::from(header[4]),
            ))
        } else {
            None
        };
        let slot_bytes = if let Some((_, width)) = fixed {
            if width > 4 {
                return Err("fixed width".into());
            }
            count.checked_mul(width).and_then(|n| n.checked_add(8))
        } else {
            count.checked_mul(slot_width)
        }
        .ok_or("slot size")?;
        let pool = slots.checked_add(slot_bytes).ok_or("pool offset")?;
        if pool.checked_add(pool_bytes) != Some(bytes.len()) {
            return Err("file length".into());
        }
        Ok(Self {
            bytes,
            count,
            slots,
            pool,
            pool_bytes,
            slot_width,
            fixed,
            metadata_bytes,
            index_bytes,
        })
    }
    fn span(&self, id: usize) -> Result<(usize, usize), Failure> {
        if id >= self.count {
            return Err("payload ID".into());
        }
        let (offset, length) = if let Some((length, width)) = self.fixed {
            let start = self.slots + 8 + id * width;
            let mut ordinal = [0u8; 4];
            ordinal[..width].copy_from_slice(&self.bytes[start..start + width]);
            (
                (u32::from_le_bytes(ordinal) as usize)
                    .checked_mul(length)
                    .ok_or("fixed offset")?,
                length,
            )
        } else {
            let start = self.slots + id * self.slot_width;
            let word = |at| -> Result<usize, Failure> {
                Ok(if self.slot_width == 8 {
                    u32::from_le_bytes(self.bytes[at..at + 4].try_into()?) as usize
                } else {
                    usize::try_from(u64::from_le_bytes(self.bytes[at..at + 8].try_into()?))?
                })
            };
            (word(start)?, word(start + self.slot_width / 2)?)
        };
        if offset
            .checked_add(length)
            .is_none_or(|end| end > self.pool_bytes)
        {
            return Err("payload span".into());
        }
        Ok((offset, length))
    }
    fn payload(&self, id: usize) -> Result<&'a [u8], Failure> {
        let (offset, length) = self.span(id)?;
        Ok(&self.bytes[self.pool + offset..self.pool + offset + length])
    }
}

#[derive(Default)]
struct Payload {
    references: u64,
    last: usize,
    physical: u64,
    families: u8,
    cached: bool,
}

fn analyze(bytes: &[u8]) -> Result<Value, Failure> {
    let started = Instant::now();
    // Validation precedes interpreting private format offsets.
    let reader = Reader::open(
        bytes,
        Limits {
            records: MAX_RECORDS,
            bytes: 2 * 1024 * 1024 * 1024,
        },
    )?;
    let columns = Columns::new(bytes)?;
    if columns.count != reader.len() {
        return Err("header/reader count mismatch".into());
    }
    let validated_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut tree = Tree::new(columns.count)?;
    let mut values: HashMap<&[u8], Payload> = HashMap::new();
    let (mut logical_bytes, mut unique_bytes, mut written, mut physical, mut repeats) =
        (0u64, 0u64, 0usize, 0u64, 0u64);
    let (mut gap, mut distinct_gap, mut byte_footprint) = (
        Histogram::default(),
        Histogram::default(),
        Histogram::default(),
    );
    let mut within_bytes = [0u64; 5];
    let mut lengths: BTreeMap<usize, Counts> = BTreeMap::new();
    let mut digest = Sha256::new();
    digest.update(b"netindex-payload-stream-v1\0");
    let (mut admission_charge, mut cache_hits, mut cache_misses) = (0usize, 0u64, 0u64);
    let mut cache_saturated_at = None;
    for id in 0..columns.count {
        let (offset, length) = columns.span(id)?;
        let payload = columns.payload(id)?;
        digest.update((length as u64).to_le_bytes());
        digest.update(payload);
        logical_bytes += length as u64;
        lengths.entry(length).or_default().references += 1;
        let new_span = length > 0 && offset == written;
        if new_span {
            written += length;
            physical += 1;
            lengths.entry(length).or_default().physical += 1;
        }
        let distinct = values.len() as u64;
        if !values.contains_key(payload) && values.len() == values.capacity() {
            // Conservative allowance for the next table growth (including control
            // bytes). This bounds modeled table+tree state, not total process RSS.
            let next = values
                .capacity()
                .max(4)
                .checked_mul(2)
                .ok_or("table capacity")?;
            let estimate = tree.allocated() + next * 2 * std::mem::size_of::<(&[u8], Payload)>();
            if estimate > WORKSPACE {
                return Err("512 MiB modeled workspace bound".into());
            }
            values.try_reserve(1)?;
        }
        let state = values.entry(payload).or_default();
        if state.references == 0 {
            unique_bytes += length as u64;
            lengths.entry(length).or_default().distinct += 1;
            let charge = length.checked_add(64).ok_or("cache charge")?;
            if charge <= (4 * 1024 * 1024usize).saturating_sub(admission_charge) {
                state.cached = true;
                admission_charge += charge;
            } else if cache_saturated_at.is_none() {
                cache_saturated_at = Some(id);
            }
        } else {
            repeats += 1;
            gap.add((id - state.last) as u64);
            let (before_count, before_bytes) = tree.prefix(state.last + 1);
            let distance = distinct - before_count;
            let footprint = unique_bytes - before_bytes + length as u64;
            distinct_gap.add(distance);
            byte_footprint.add(footprint);
            for (count, budget) in within_bytes.iter_mut().zip(BYTE_BUDGETS) {
                if footprint <= budget {
                    *count += 1;
                }
            }
            tree.update(state.last, length as u64, false);
        }
        // The current builder's first-seen cache never evicts. Count a miss on
        // first occurrence even if this value was admitted by that occurrence.
        if state.references > 0 && state.cached {
            cache_hits += 1;
        } else {
            cache_misses += 1;
        }
        if new_span {
            state.physical += 1;
        }
        state.references += 1;
        state.last = id;
        tree.update(id, length as u64, true);
    }
    if written != columns.pool_bytes
        || logical_bytes < unique_bytes
        || unique_bytes > columns.pool_bytes as u64
    {
        return Err("payload accounting mismatch".into());
    }
    let (count, total_bytes) = tree.prefix(columns.count);
    if count != values.len() as u64 || total_bytes != unique_bytes {
        return Err("reuse tree mismatch".into());
    }
    let mut family_counts = [[0u64; 2]; 3];
    let mut visited = 0u64;
    reader.visit_all(|row| -> Result<(), Failure> {
        let expected = columns.payload(row.id as usize)?;
        if row.payload != expected || row.payload.as_ptr() != expected.as_ptr() {
            return Err("reader/span/pointer mismatch".into());
        }
        let family = match row.target {
            Target::Asn(_) => 2,
            Target::Address(ip) => usize::from(ip.is_ipv6()),
            Target::Network { address, .. } => usize::from(address.is_ipv6()),
            Target::Range { start, .. } => usize::from(start.is_ipv6()),
        };
        let state = values.get_mut(row.payload).ok_or("missing payload value")?;
        state.families |= 1 << family;
        family_counts[family][0] += 1;
        family_counts[family][1] += row.payload.len() as u64;
        visited += 1;
        Ok(())
    })?;
    if visited != columns.count as u64 {
        return Err("incomplete visit".into());
    }
    let mut frequencies: BTreeMap<u64, u64> = BTreeMap::new();
    let mut family_masks: BTreeMap<u8, u64> = BTreeMap::new();
    let (mut singleton_values, mut nonempty_values, mut recomputed_bytes, mut recomputed_records) =
        (0u64, 0u64, 0u64, 0u64);
    for (payload, state) in &values {
        *frequencies.entry(state.references).or_default() += 1;
        *family_masks.entry(state.families).or_default() += 1;
        if state.references == 1 {
            singleton_values += 1;
        }
        if !payload.is_empty() {
            nonempty_values += 1;
        }
        recomputed_bytes += payload.len() as u64 * state.references;
        recomputed_records += state.references;
    }
    if recomputed_records != columns.count as u64 || recomputed_bytes != logical_bytes {
        return Err("frequency accounting".into());
    }
    let lengths_json: Vec<_>=lengths.iter().map(|(length,c)|json!({"length":length,"references":c.references,"distinct":c.distinct,"physical_spans":c.physical})).collect();
    let index_width = match values.len().saturating_sub(1) {
        0 => 0,
        n => ((usize::BITS - n.leading_zeros()) as usize).div_ceil(8),
    };
    let modeled_workspace =
        tree.allocated() + values.capacity() * 2 * std::mem::size_of::<(&[u8], Payload)>();
    Ok(
        json!({"records":columns.count,"validated_and_crosschecked_records":visited,"input_bytes":bytes.len(),
        "physical_layout":{"header_bytes":80,"metadata_bytes":columns.metadata_bytes,"index_bytes":columns.index_bytes,
            "reference_bytes":columns.pool-columns.slots,"pool_bytes":columns.pool_bytes,
            "reference_encoding":if columns.fixed.is_some(){"fixed"}else if columns.slot_width==8{"compact_spans"}else{"wide_spans"}},
        "payloads":{"distinct_values":values.len(),"nonempty_distinct_values":nonempty_values,"singleton_values":singleton_values,
            "references_to_repeated_values":columns.count as u64-singleton_values,"logical_bytes":logical_bytes,"unique_bytes":unique_bytes,
            "physical_nonempty_spans":physical,"duplicate_stored_bytes":columns.pool_bytes as u64-unique_bytes,
            "duplicate_logical_bytes":logical_bytes-unique_bytes,"lengths":lengths_json,"frequency_histogram":frequencies,
            "family_unique_value_masks":family_masks,"family_references_and_logical_bytes":family_counts},
        "insertion_order_reuse":{"cold":values.len(),"repeats":repeats,"reference_gap_histogram":gap.json(),
            "distinct_intervening_histogram":distinct_gap.json(),"distinct_byte_footprint_histogram":byte_footprint.json(),
            "within_byte_budget":BYTE_BUDGETS.into_iter().zip(within_bytes).collect::<BTreeMap<_,_>>()},
        "first_seen_4mib_cache_model":{"charge_bytes":admission_charge,"first_failed_admission_id":cache_saturated_at,
            "hits":cache_hits,"misses":cache_misses,"includes_key_plus_64_byte_charge":true,"eviction":false},
        "reference_model":{"minimum_whole_byte_id_width":index_width,
            "id_array_plus_u32_span_directory_bytes":columns.count as u64*index_width as u64+values.len() as u64*8,
            "excludes_headers_validation_memory_and_lookup_cost":true},
        "logical_payload_sha256":format!("{:x}",digest.finalize()),"validation_ms":validated_ms,
        "analysis_total_ms":started.elapsed().as_secs_f64()*1000.0,"modeled_workspace_bytes":modeled_workspace,
        "workspace_limit_bytes":WORKSPACE,"tree_bytes":tree.allocated(),"table_capacity":values.capacity()}),
    )
}

fn read_row(input: &mut impl Read) -> Result<Option<(Target, Vec<u8>)>, Failure> {
    let mut family = [0];
    if input.read(&mut family)? == 0 {
        return Ok(None);
    }
    let mut data = [0u8; 36];
    input.read_exact(&mut data)?;
    let first = u128::from_le_bytes(data[..16].try_into()?);
    let last = u128::from_le_bytes(data[16..32].try_into()?);
    let length = u32::from_le_bytes(data[32..36].try_into()?) as usize;
    if length > 4096 {
        return Err("ASN row payload limit".into());
    }
    let ip = |n| -> Result<IpAddr, Failure> {
        Ok(match family[0] {
            4 => Ipv4Addr::from(u32::try_from(n)?).into(),
            6 => Ipv6Addr::from(n).into(),
            _ => return Err("ASN family".into()),
        })
    };
    let mut payload = vec![0; length];
    input.read_exact(&mut payload)?;
    Ok(Some((
        Target::Range {
            start: ip(first)?,
            end: ip(last)?,
        },
        payload,
    )))
}

fn ingest(rows: &Path, output: &Path, temp: &Path) -> Result<(), Failure> {
    let mut builder = ExternalBuilder::new(
        temp,
        Limits {
            records: MAX_RECORDS,
            bytes: 2 * 1024 * 1024 * 1024,
        },
        ExternalOptions {
            temporary_bytes: 8 * 1024 * 1024 * 1024,
            ..ExternalOptions::default()
        },
    )?;
    let mut input = BufReader::new(File::open(rows)?);
    let mut count = 0usize;
    while let Some((target, payload)) = read_row(&mut input)? {
        builder.push(target, &payload)?;
        count += 1;
    }
    let output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(output)?;
    builder.write_to(&output, b"payload-distribution-asn-v1")?;
    // SAFETY: this process owns the closed writer's immutable output contents.
    let mapping = unsafe { memmap2::Mmap::map(&output) }?;
    let reader = Reader::open(
        &mapping,
        Limits {
            records: MAX_RECORDS,
            bytes: 2 * 1024 * 1024 * 1024,
        },
    )?;
    let mut original = BufReader::new(File::open(rows)?);
    let mut id = 0usize;
    reader.visit_all(|row| -> Result<(), Failure> {
        let (target, payload) = read_row(&mut original)?.ok_or("short original ASN rows")?;
        if row.id as usize != id || row.target != target || row.payload != payload {
            return Err("ASN full-assertion mismatch".into());
        }
        id += 1;
        Ok(())
    })?;
    if id != count || read_row(&mut original)?.is_some() {
        return Err("ASN record count".into());
    }
    println!(
        "{}",
        json!({"records":count,"full_original_assertions_checked":id})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use netindex::Builder;

    fn fixture(payloads: &[&[u8]]) -> Result<Vec<u8>, Failure> {
        let mut builder = Builder::new(Limits::default());
        for (id, payload) in payloads.iter().enumerate() {
            // Reverse targets so visit_all order differs from insertion order.
            builder.push(
                Target::Address(Ipv4Addr::from((payloads.len() - id) as u32).into()),
                payload,
            )?;
        }
        Ok(builder.into_bytes(b"distribution-test")?)
    }

    fn buckets(values: impl IntoIterator<Item = u64>) -> Value {
        let mut histogram = Histogram::default();
        for value in values {
            histogram.add(value);
        }
        histogram.json()
    }

    #[test]
    fn reuse_matches_independent_naive_history_with_variable_and_empty_values()
    -> Result<(), Failure> {
        let payloads: [&[u8]; 10] = [
            b"a", b"long", b"a", b"", b"long", b"x", b"", b"a", b"x", b"x",
        ];
        let actual = analyze(&fixture(&payloads)?)?;
        let (mut gaps, mut distances, mut footprints) = (Vec::new(), Vec::new(), Vec::new());
        for (at, payload) in payloads.iter().enumerate() {
            if let Some(previous) = payloads[..at].iter().rposition(|p| p == payload) {
                let distinct: std::collections::HashSet<_> =
                    payloads[previous + 1..at].iter().copied().collect();
                gaps.push((at - previous) as u64);
                distances.push(distinct.len() as u64);
                footprints
                    .push((distinct.iter().map(|p| p.len()).sum::<usize>() + payload.len()) as u64);
            }
        }
        assert_eq!(
            actual["insertion_order_reuse"]["reference_gap_histogram"],
            buckets(gaps)
        );
        assert_eq!(
            actual["insertion_order_reuse"]["distinct_intervening_histogram"],
            buckets(distances)
        );
        assert_eq!(
            actual["insertion_order_reuse"]["distinct_byte_footprint_histogram"],
            buckets(footprints)
        );
        assert_eq!(actual["payloads"]["distinct_values"], 4);
        assert_eq!(actual["payloads"]["unique_bytes"], 6);
        assert_eq!(actual["payloads"]["logical_bytes"], 14);
        assert_eq!(actual["first_seen_4mib_cache_model"]["hits"], 6);
        assert_eq!(actual["payloads"]["duplicate_stored_bytes"], 0);
        Ok(())
    }

    #[test]
    fn fixed_empty_and_malformed_inputs_are_accounted_for() -> Result<(), Failure> {
        for payloads in [
            vec![],
            vec![b"".as_slice(); 5],
            vec![b"same".as_slice(); 5],
            vec![b"aaaa".as_slice(), b"bbbb".as_slice()],
        ] {
            let bytes = fixture(&payloads)?;
            let actual = analyze(&bytes)?;
            assert_eq!(actual["records"], payloads.len());
            assert_eq!(
                actual["payloads"]["logical_bytes"],
                payloads.iter().map(|p| p.len()).sum::<usize>()
            );
            assert_eq!(actual["payloads"]["duplicate_stored_bytes"], 0);
            assert!(analyze(&bytes[..bytes.len() - 1]).is_err());
        }
        assert!(analyze(b"not an index").is_err());
        Ok(())
    }

    #[test]
    fn distinct_physical_spans_can_have_identical_bytes() -> Result<(), Failure> {
        let mut bytes = fixture(&[b"a", b"bb", b"a"])?;
        let columns = Columns::new(&bytes)?;
        assert_eq!(columns.slot_width, 8);
        assert!(columns.fixed.is_none());
        let third = columns.slots + 16;
        // Independent legacy layout: append a second physical "a" span.
        bytes[third..third + 4].copy_from_slice(&3u32.to_le_bytes());
        bytes[56..64].copy_from_slice(&4u64.to_le_bytes());
        bytes.push(b'a');
        let actual = analyze(&bytes)?;
        assert_eq!(actual["payloads"]["distinct_values"], 2);
        assert_eq!(actual["payloads"]["physical_nonempty_spans"], 3);
        assert_eq!(actual["payloads"]["duplicate_stored_bytes"], 1);
        Ok(())
    }

    #[test]
    fn non_evicting_cache_and_byte_reuse_have_different_meanings() -> Result<(), Failure> {
        let a = vec![b'a'; 5 * 1024 * 1024 / 2];
        let b = vec![b'b'; a.len()];
        let actual = analyze(&fixture(&[&a, &b, b"", &a, &b, b""])?)?;
        assert_eq!(actual["first_seen_4mib_cache_model"]["hits"], 2);
        assert_eq!(actual["first_seen_4mib_cache_model"]["misses"], 4);
        assert_eq!(
            actual["first_seen_4mib_cache_model"]["first_failed_admission_id"],
            1
        );
        assert_eq!(
            actual["insertion_order_reuse"]["within_byte_budget"]["4194304"],
            0
        );
        Ok(())
    }
}
