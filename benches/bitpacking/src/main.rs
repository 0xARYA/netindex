//! Column qualification only; candidate bytes are not a netindex file format.
use std::{collections::BTreeMap, hint::black_box, time::Instant};

use bitpacking::{BitPacker, BitPacker8x};
use netindex::Error;
use serde_json::json;
use sha2::{Digest, Sha256};

mod search {
    include!("../../../src/search.rs");
    pub(crate) fn scalar(bytes: &[u8], width: usize, target: u128) -> usize {
        if width < 16 && target >= (1u128 << (width * 8)) - 1 {
            return bytes.len() / width;
        }
        binary(bytes, width, target).unwrap()
    }
}

struct Column {
    values: Vec<u128>,
    byte: Vec<u8>,
    packed: Vec<u8>,
    simd: Vec<u8>,
    differential: Vec<u8>,
    bits: u32,
    gap_bits: u8,
    width: usize,
    bp: BitPacker8x,
}

impl Column {
    fn new(values: Vec<u128>, width: usize) -> Self {
        assert!(!values.is_empty() && values.len() <= 256);
        assert!(values.windows(2).all(|v| v[0] <= v[1]));
        let bits = 128 - values.last().unwrap().leading_zeros();
        let mut packed = vec![0; (values.len() * bits as usize).div_ceil(8) + 16];
        let mut byte = Vec::new();
        for (index, &value) in values.iter().enumerate() {
            byte.extend_from_slice(&value.to_le_bytes()[..width]);
            for bit in 0..bits as usize {
                if value & (1u128 << bit) != 0 {
                    let at = index * bits as usize + bit;
                    packed[at / 8] |= 1 << (at % 8);
                }
            }
        }
        let mut simd = Vec::new();
        let mut differential = Vec::new();
        let mut gap_bits = 0;
        if bits <= 32 && values.len() == 256 {
            let input: Vec<_> = values.iter().map(|&v| v as u32).collect();
            let bp = BitPacker8x::new();
            simd.resize(1024, 0);
            let used = bp.compress(&input, &mut simd, bits as u8);
            simd.truncate(used);
            gap_bits = bp.num_bits_sorted(0, &input);
            differential.resize(1024, 0);
            let used = bp.compress_sorted(0, &input, &mut differential, gap_bits);
            differential.truncate(used);
        }
        Self {
            values,
            byte,
            packed,
            simd,
            differential,
            bits,
            gap_bits,
            width,
            bp: BitPacker8x::new(),
        }
    }

    #[inline]
    fn get(&self, index: usize) -> u128 {
        if self.bits == 0 {
            return 0;
        }
        let position = index * self.bits as usize;
        let at = position / 8;
        let shift = position % 8;
        let mask = u128::MAX >> (128 - self.bits);
        if self.bits <= 57 {
            let word = u64::from_le_bytes(self.packed[at..at + 8].try_into().unwrap());
            return u128::from(word >> shift) & mask;
        }
        let word = u128::from_le_bytes(self.packed[at..at + 16].try_into().unwrap());
        let mut value = word >> shift;
        if self.bits as usize + shift > 128 {
            value |= u128::from(self.packed[at + 16]) << (128 - shift);
        }
        value & mask
    }

    #[inline(never)]
    fn direct(&self, target: u128) -> usize {
        let (mut low, mut high) = (0, self.values.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if self.get(mid) <= target {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low
    }

    #[inline(never)]
    fn decoded(&self, target: u128) -> usize {
        let mut decoded = [0u128; 256];
        for (index, value) in decoded.iter_mut().take(self.values.len()).enumerate() {
            *value = self.get(index);
        }
        decoded[..self.values.len()].partition_point(|&v| v <= target)
    }

    #[inline(never)]
    fn vector(&self, target: u128, sorted: bool) -> usize {
        let mut decoded = [0u32; 256];
        let bp = self.bp;
        if sorted {
            bp.decompress_sorted(0, &self.differential, &mut decoded, self.gap_bits);
        } else {
            bp.decompress(&self.simd, &mut decoded, self.bits as u8);
        }
        decoded.partition_point(|&v| u128::from(v) <= target)
    }

    fn verify(&self) {
        assert_eq!(
            (0..self.values.len())
                .map(|i| self.get(i))
                .collect::<Vec<_>>(),
            self.values
        );
        for query in self
            .values
            .iter()
            .flat_map(|&v| [v, v.saturating_sub(1), v.saturating_add(1)])
            .chain([0, u128::MAX])
        {
            let expected = self.values.partition_point(|&v| v <= query);
            assert_eq!(self.direct(query), expected);
            assert_eq!(self.decoded(query), expected);
            assert_eq!(search::scalar(&self.byte, self.width, query), expected);
            assert_eq!(
                search::Search::detect()
                    .upper_bound(&self.byte, self.width, query)
                    .unwrap(),
                expected
            );
            if !self.simd.is_empty() || self.bits == 0 && self.values.len() == 256 {
                assert_eq!(self.vector(query, false), expected);
                assert_eq!(self.vector(query, true), expected);
            }
        }
    }
}

fn number(bytes: &[u8]) -> u128 {
    let mut value = [0; 16];
    value[..bytes.len()].copy_from_slice(bytes);
    u128::from_le_bytes(value)
}

fn width(bits: u32) -> usize {
    [1, 2, 4, 8, 16]
        .into_iter()
        .find(|w| bits <= w * 8)
        .unwrap() as usize
}

fn measure(name: &str, columns: &[Column], iterations: usize) {
    if columns.is_empty() {
        return;
    }
    let queries: Vec<_> = (0..65536usize)
        .map(|i| {
            let column = i.wrapping_mul(2654435761) % columns.len();
            let values = &columns[column].values;
            let at = i.wrapping_mul(2246822519) % values.len();
            (column, values[at].saturating_add((i % 2) as u128))
        })
        .collect();
    let detected = search::Search::detect();
    let vector = columns
        .iter()
        .all(|c| c.values.len() == 256 && c.bits <= 32);
    for round in 0..3 {
        let mut methods = vec![
            "byte-scalar",
            "byte-detected",
            "bit-direct",
            "bit-decode",
            "hybrid-direct",
        ];
        if vector {
            methods.extend(["simd8-decode", "simd8-delta-decode"]);
        }
        if round == 1 {
            methods.reverse();
        }
        for method in methods {
            let (elapsed, checksum) = match method {
                "byte-scalar" => sample(columns, &queries, iterations, |c, q| {
                    search::scalar(&c.byte, c.width, q)
                }),
                "byte-detected" => sample(columns, &queries, iterations, |c, q| {
                    detected.upper_bound(&c.byte, c.width, q).unwrap()
                }),
                "bit-direct" => sample(columns, &queries, iterations, Column::direct),
                "bit-decode" => sample(columns, &queries, iterations, Column::decoded),
                "hybrid-direct" => sample(columns, &queries, iterations, |c, q| {
                    if c.packed.len() < c.byte.len() {
                        c.direct(q)
                    } else {
                        detected.upper_bound(&c.byte, c.width, q).unwrap()
                    }
                }),
                "simd8-decode" => sample(columns, &queries, iterations, |c, q| c.vector(q, false)),
                _ => sample(columns, &queries, iterations, |c, q| c.vector(q, true)),
            };
            println!(
                "{}",
                json!({"case":name,"round":round,"method":method,"queries":iterations,"ns_per_query":elapsed as f64 / iterations as f64,"checksum":checksum,"columns":columns.len()})
            );
        }
    }
}

fn sample(
    columns: &[Column],
    queries: &[(usize, u128)],
    iterations: usize,
    mut kernel: impl FnMut(&Column, u128) -> usize,
) -> (u128, usize) {
    let started = Instant::now();
    let mut checksum = 0usize;
    for index in 0..iterations {
        let &(column, query) = black_box(&queries[index % queries.len()]);
        checksum = checksum.wrapping_add(kernel(black_box(&columns[column]), query));
    }
    black_box(checksum);
    (started.elapsed().as_nanos(), checksum)
}

fn real(path: &str, iterations: usize) -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let limits = netindex::Limits {
        records: 30_000_000,
        bytes: 2 * 1024 * 1024 * 1024,
    };
    let reader = netindex::Reader::open(data.as_slice(), limits)?;
    if !matches!(number(&data[8..12]), 3 | 4) {
        return Err("column study requires plain format 3 or 4".into());
    }
    let count = reader.len();
    let mut offset = 80 + number(&data[48..56]) as usize;
    let flags = number(&data[12..16]);
    for (family, count_at, size_at, interval_width, flag) in
        [("v4", 24, 64, 20, 1), ("v6", 32, 72, 56, 2)]
    {
        let rows = number(&data[count_at..count_at + 8]) as usize;
        if flags & flag == 0 {
            offset += rows * interval_width;
            continue;
        }
        let size = number(&data[size_at..size_at + 8]) as usize;
        let section = &data[offset..offset + size];
        let blocks = rows.div_ceil(256);
        let mut histogram = BTreeMap::<String, usize>::new();
        let mut tight = 0;
        let mut old = 0;
        let mut sampled = Vec::new();
        let stride = blocks.div_ceil(2048).max(1);
        for index in 0..blocks {
            let descriptor = &section[index * 32..index * 32 + 32];
            let width = usize::from(descriptor[24] & 31);
            let start = number(&descriptor[16..24]) as usize;
            let count = number(&descriptor[26..28]) as usize;
            let raw = &section[start..start + count * width];
            let values: Vec<_> = raw.chunks_exact(width).map(number).collect();
            let bits = 128 - values.last().unwrap().leading_zeros();
            *histogram
                .entry(format!("{width}-byte/{bits}-bit"))
                .or_default() += count;
            old += raw.len();
            tight += (count * bits as usize).div_ceil(8);
            if index % stride == 0 {
                sampled.push(Column::new(values, width));
            }
        }
        for column in &sampled {
            column.verify();
        }
        println!(
            "{}",
            json!({"phase":"distribution","input":path,"input_sha256":format!("{:x}",Sha256::digest(&data)),"assertions":count,"family":family,"rows":rows,"blocks":blocks,"section_bytes":size,"delta_bytes":old,"tight_delta_bytes":tight,"saving_bytes":old-tight,"sampled_blocks":sampled.len(),"histogram":histogram})
        );
        measure(&format!("{path}/{family}"), &sampled, iterations);
        let vector: Vec<_> = sampled
            .into_iter()
            .filter(|c| c.values.len() == 256 && c.bits <= 32)
            .collect();
        measure(&format!("{path}/{family}/u32"), &vector, iterations);
        offset += size;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|v| v == "reader") {
        for path in args.iter().skip(1) {
            reader_measure(path)?;
        }
        return Ok(());
    }
    let iterations = args
        .first()
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(262144);
    for bits in [
        0, 1, 7, 8, 9, 12, 15, 16, 17, 24, 25, 31, 32, 33, 48, 63, 64, 65, 96, 127, 128,
    ] {
        let maximum = if bits == 0 {
            0
        } else {
            u128::MAX >> (128 - bits)
        };
        for count in [1, 17, 127, 128, 255, 256] {
            let values = (0..count)
                .map(|i| {
                    if count == 1 {
                        0
                    } else if i == count - 1 {
                        maximum
                    } else {
                        maximum / (count as u128 - 1) * i as u128
                    }
                })
                .collect();
            Column::new(values, width(bits)).verify();
        }
        if bits >= 8 {
            let values = (0..256)
                .map(|i| if i == 255 { maximum } else { maximum / 255 * i })
                .collect();
            let columns = vec![Column::new(values, width(bits))];
            measure(&format!("synthetic-{bits}"), &columns, iterations);
        }
    }
    for path in args.iter().skip(1) {
        real(path, iterations)?;
    }
    Ok(())
}

fn reader_measure(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use netindex::{Limits, Reader, Target};
    let bytes = std::fs::read(path)?;
    let limits = Limits {
        records: 30_000_000,
        bytes: 2 * 1024 * 1024 * 1024,
    };
    let reader = Reader::open(bytes.as_slice(), limits)?;
    let mut queries = Vec::new();
    let stride = reader.len().div_ceil(4096);
    let mut index = 0usize;
    reader.visit_all(|row| {
        if index.is_multiple_of(stride) {
            match row.target {
                Target::Address(address) | Target::Network { address, .. } => queries.push(address),
                Target::Range { start, .. } => queries.push(start),
                Target::Asn(_) => (),
            }
        }
        index += 1;
        Ok::<_, Error>(())
    })?;
    for round in 0..5 {
        let started = Instant::now();
        black_box(Reader::open(black_box(bytes.as_slice()), limits)?);
        println!(
            "{}",
            json!({"input":path,"phase":"reader-open","round":round,"ms":started.elapsed().as_secs_f64()*1000.0})
        );
        let started = Instant::now();
        let mut checksum = 0u64;
        reader.visit_all(|row| {
            checksum = checksum
                .wrapping_add(u64::from(row.id))
                .wrapping_add(row.payload.len() as u64);
            black_box(row.target);
            Ok::<_, Error>(())
        })?;
        black_box(checksum);
        println!(
            "{}",
            json!({"input":path,"phase":"reader-visit","round":round,"ms":started.elapsed().as_secs_f64()*1000.0,"checksum":checksum})
        );
        let started = Instant::now();
        let mut checksum = 0u64;
        for i in 0..131072usize {
            reader.visit_ip(
                black_box(queries[i.wrapping_mul(2654435761) % queries.len()]),
                |row| {
                    checksum = checksum.wrapping_add(u64::from(row.id));
                    black_box(row.target);
                    black_box(row.payload);
                    Ok::<_, Error>(())
                },
            )?;
        }
        black_box(checksum);
        println!(
            "{}",
            json!({"input":path,"phase":"reader-lookup","round":round,"queries":131072,"ms":started.elapsed().as_secs_f64()*1000.0,"checksum":checksum})
        );
    }
    Ok(())
}
