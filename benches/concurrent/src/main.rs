//! Shared immutable mappings with fixed worker placement and sampled latency.
use std::{
    collections::BTreeMap,
    fs::File,
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

use netindex::{Error, Limits, Reader, Target};

type Failure = Box<dyn std::error::Error + Send + Sync>;

struct Worker {
    cpu: usize,
    finished: Instant,
    elapsed_ns: u128,
    checksum: u64,
    hits: u64,
    samples: Vec<u64>,
}

// Release waiting workers even if spawning, setup or the coordinator fails.
struct Gate<'a>(&'a AtomicU8);
impl Drop for Gate<'_> {
    fn drop(&mut self) {
        self.0.store(2, Ordering::Release);
    }
}

fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 || !["v4", "v6", "asn"].contains(&args[2].as_str()) {
        return Err("expected FILE FAMILY RECORDS QUERIES_PER_WORKER CPU_LIST".into());
    }
    let family = &args[2];
    let records: u32 = args[3].parse()?;
    let queries: u32 = args[4].parse()?;
    let cpus: Vec<usize> = args[5]
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    if records == 0
        || records > 20_000_000
        || !(10_000..=10_000_000).contains(&queries)
        || !queries.is_multiple_of(2)
        || cpus.len() < 16
        || cpus.iter().any(|&cpu| cpu >= libc::CPU_SETSIZE as usize)
    {
        return Err(
            "invalid records, even query count, or CPU list (need 16 unique workers)".into(),
        );
    }
    let unique: std::collections::BTreeSet<_> = cpus.iter().copied().collect();
    if unique.len() != cpus.len() || cpus.contains(&31) {
        return Err("duplicate CPU or coordinator CPU 31 in worker list".into());
    }
    for &cpu in cpus.iter().chain(std::iter::once(&31)) {
        pin(cpu)?;
    }
    let file = File::open(&args[1])?;
    // SAFETY: the runner owns this immutable private file until the process exits.
    let mapping = unsafe { memmap2::MmapOptions::new().map(&file) }?;
    let reader = Reader::open(
        mapping.as_ref(),
        Limits {
            bytes: 2 * 1024 * 1024 * 1024,
            records: records as usize,
        },
    )?;
    for page in mapping.chunks(4096) {
        black_box(page[0]);
    }
    let mut clock = Vec::new();
    for _ in 0..10_000 {
        let now = Instant::now();
        clock.push(now.elapsed().as_nanos() as u64);
    }
    clock.sort_unstable();
    println!(
        "{}",
        serde_json::json!({"phase":"opened","family":family,"records":records,
        "artifact_bytes":mapping.len(),"worker_cpus":cpus,"coordinator_cpu":31,
        "clock_p50_ns":clock[5000],"clock_p95_ns":clock[9500],"clock_min_ns":clock[0]})
    );
    for trial in 1..=2 {
        let counts = if trial == 1 {
            vec![1, 2, 4, 8, 16]
        } else {
            vec![16, 8, 4, 2, 1]
        };
        for count in counts {
            let gate = AtomicU8::new(0);
            let (started, workers) = thread::scope(|scope| -> Result<_, Failure> {
                let _release = Gate(&gate);
                let (sender, ready) = mpsc::channel();
                let mut handles = Vec::new();
                for (worker, &cpu) in cpus.iter().take(count).enumerate() {
                    let sender = sender.clone();
                    let gate = &gate;
                    let reader = &reader;
                    handles.push(scope.spawn(move || -> Result<Option<Worker>, Failure> {
                        let setup = (|| -> Result<u64, Failure> {
                            pin(cpu)?;
                            let expected = verify(reader, family, records, worker as u32, queries)?;
                            for index in 0..100_000 {
                                let (key, _) = key(index, worker as u32, records);
                                visit(reader, family, key, |row| {
                                    black_box(row);
                                    Ok(())
                                })?;
                            }
                            Ok(expected)
                        })();
                        let mut samples = Vec::with_capacity(queries.div_ceil(1021) as usize);
                        sender.send(setup.is_ok())?;
                        drop(sender);
                        let expected = setup?;
                        while gate.load(Ordering::Acquire) == 0 {
                            std::hint::spin_loop();
                        }
                        if gate.load(Ordering::Acquire) == 2 {
                            return Ok(None);
                        }
                        let start = Instant::now();
                        let mut checksum = 0u64;
                        let mut hits = 0u64;
                        for index in 0..queries {
                            let (key, _) = key(index, worker as u32, records);
                            let sample = (index % 1021 == 0).then(Instant::now);
                            visit(reader, family, black_box(key), |row| {
                                checksum = checksum.wrapping_add(u64::from(row.id));
                                hits += 1;
                                black_box(row.target);
                                black_box(row.payload);
                                Ok(())
                            })?;
                            if let Some(start) = sample {
                                samples.push(start.elapsed().as_nanos() as u64);
                            }
                        }
                        let finished = Instant::now();
                        let elapsed_ns = finished.duration_since(start).as_nanos();
                        assert_eq!(hits, u64::from(queries / 2));
                        assert_eq!(checksum, expected);
                        Ok(Some(Worker {
                            cpu,
                            finished,
                            elapsed_ns,
                            checksum,
                            hits,
                            samples,
                        }))
                    }));
                }
                drop(sender);
                let mut failed = false;
                for _ in 0..count {
                    match ready.recv() {
                        Ok(ok) => failed |= !ok,
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                let started = Instant::now();
                gate.store(if failed { 2 } else { 1 }, Ordering::Release);
                let mut workers = Vec::new();
                for handle in handles {
                    let result = handle
                        .join()
                        .map_err(|_| std::io::Error::other("reader worker panicked"))??;
                    if let Some(worker) = result {
                        workers.push(worker);
                    }
                }
                if failed {
                    return Err("worker setup failed".into());
                }
                Ok((started, workers))
            })?;
            let finished = workers
                .iter()
                .map(|w| w.finished)
                .max()
                .ok_or("no workers")?;
            let wall_ns = finished.duration_since(started).as_nanos();
            let mut samples: Vec<_> = workers
                .iter()
                .flat_map(|w| w.samples.iter().copied())
                .collect();
            samples.sort_unstable();
            let mut histogram = BTreeMap::new();
            for &value in &samples {
                *histogram.entry(value).or_insert(0u64) += 1;
            }
            let details:Vec<_>=workers.iter().map(|w|serde_json::json!({"cpu":w.cpu,"queries":queries,
                "elapsed_ns":w.elapsed_ns,"checksum":w.checksum,"hits":w.hits,"latency_samples":w.samples.len()})).collect();
            println!(
                "{}",
                serde_json::json!({"phase":"concurrent","family":family,"trial":trial,"workers":count,
                "queries":u64::from(queries)*count as u64,"wall_ns":wall_ns,
                "throughput_queries_per_second":queries as f64*count as f64/(wall_ns as f64/1e9),
                "latency_samples":samples.len(),"p50_ns":samples[samples.len()/2],
                "p95_ns":samples[samples.len()*95/100],"p99_ns":samples[samples.len()*99/100],
                "latency_histogram_ns":histogram,"worker_details":details})
            );
        }
    }
    Ok(())
}

fn pin(cpu: usize) -> std::io::Result<()> {
    // SAFETY: cpu is checked against CPU_SETSIZE; set is initialized and lives
    // for this syscall. PID zero changes only the calling thread's affinity.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        if libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
fn key(index: u32, worker: u32, records: u32) -> (u32, u32) {
    let id = index
        .wrapping_mul(2_654_435_761)
        .wrapping_add(worker.wrapping_mul(0x85eb_ca6b))
        % records;
    (id * 2 + 2 + index % 2, id)
}
fn address(family: &str, key: u32) -> IpAddr {
    if family == "v6" {
        Ipv6Addr::from((0x2001_0db8u128 << 96) | u128::from(key)).into()
    } else {
        Ipv4Addr::from(key).into()
    }
}
fn visit(
    reader: &Reader<&[u8]>,
    family: &str,
    key: u32,
    mut emit: impl FnMut(netindex::Match<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    if family == "asn" {
        reader.visit_asn(key, &mut emit)
    } else {
        reader.visit_ip(address(family, key), &mut emit)
    }
}
fn verify(
    reader: &Reader<&[u8]>,
    family: &str,
    records: u32,
    worker: u32,
    queries: u32,
) -> Result<u64, Failure> {
    let mut checksum = 0u64;
    for index in 0..queries {
        let (key, id) = key(index, worker, records);
        let target = if family == "asn" {
            Target::Asn(key)
        } else {
            Target::Address(address(family, key))
        };
        let mut seen = 0;
        visit(reader, family, key, |row| {
            assert_eq!(row.id, id);
            assert_eq!(row.target, target);
            assert_eq!(row.payload, (id % 4096).to_le_bytes());
            seen += 1;
            Ok(())
        })?;
        assert_eq!(seen, usize::from(index % 2 == 0));
        if index % 2 == 0 {
            checksum = checksum.wrapping_add(u64::from(id));
        }
    }
    Ok(checksum)
}
