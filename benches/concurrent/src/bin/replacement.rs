//! Application-owned publication of immutable ReaderSet snapshots under load.
use std::{
    fs::{File, OpenOptions},
    hint::black_box,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use netindex::{Builder, Error, Limits, MappedReader, ReaderSet, Target};

type Failure = Box<dyn std::error::Error + Send + Sync>;
type Row = (usize, u32, Target, Vec<u8>);
type Oracle = Vec<Vec<Vec<Row>>>;
#[derive(Clone, Copy)]
enum Query {
    Ip(IpAddr),
    Asn(u32),
}
struct Snapshot {
    generation: usize,
    set: ReaderSet<memmap2::Mmap>,
}
struct Stop<'a>(&'a AtomicBool, &'a AtomicU8);
impl Drop for Stop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
        self.1.store(2, Ordering::Release);
    }
}

fn main() -> Result<(), Failure> {
    let args: Vec<_> = std::env::args().collect();
    let directory = Path::new(args.get(1).ok_or("expected private fixture directory")?);
    pin(31)?;
    let (old, old_data) = fixture(directory, 0)?;
    let (new, new_data) = fixture(directory, 1)?;
    let (unchanged, unchanged_data) = fixture(directory, 2)?;
    let invalid = directory.join("invalid.nidx");
    std::fs::write(&invalid, [0u8; 80])?;
    let queries = queries();
    let oracle: Oracle = [&old_data, &new_data]
        .iter()
        .map(|data| {
            queries
                .iter()
                .map(|&query| {
                    [*data, &unchanged_data]
                        .iter()
                        .enumerate()
                        .flat_map(|(source, data)| {
                            data.iter()
                                .filter(move |(_, _, target, _)| contains(*target, query))
                                .map(move |(_, id, target, payload)| {
                                    (source, *id, *target, payload.clone())
                                })
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    drop(old);
    drop(new);
    for trial in 1..=2 {
        for fine in [false, true] {
            let counts = if trial == 1 {
                vec![1, 4, 16]
            } else {
                vec![16, 4, 1]
            };
            for count in counts {
                let mut staged = std::collections::VecDeque::new();
                let mut weak = Vec::new();
                for round in 0..=50 {
                    let file = File::open(directory.join(format!("snapshot-{}.nidx", round % 2)))?;
                    // SAFETY: each generation file remains private and immutable.
                    let reader =
                        Arc::new(unsafe { MappedReader::map_file(&file, Limits::default()) }?);
                    weak.push(Arc::downgrade(&reader));
                    staged.push_back(reader);
                }
                let readers = vec![
                    staged.pop_front().ok_or("initial reader missing")?,
                    Arc::clone(&unchanged),
                ];
                let set = if fine {
                    ReaderSet::with_fine_routing(readers)?
                } else {
                    ReaderSet::new(readers)?
                };
                let initial = Arc::new(Snapshot { generation: 0, set });
                for index in 0..queries.len() {
                    verify(&initial, index, &queries, &oracle)?;
                }
                let next = Snapshot {
                    generation: 1,
                    set: initial.set.replaced(
                        0,
                        Arc::clone(staged.front().ok_or("staged reader missing")?),
                    )?,
                };
                for index in 0..queries.len() {
                    verify(&next, index, &queries, &oracle)?;
                }
                drop(next);
                let mut borrowed = Vec::new();
                initial.set.visit_ip("0.0.0.0".parse()?, |_, row| {
                    borrowed.push((row.payload, row.payload.to_vec()));
                    Ok::<_, Error>(())
                })?;
                let published = RwLock::new(Arc::clone(&initial));
                let stop = AtomicBool::new(false);
                let gate = AtomicU8::new(0);
                let result = thread::scope(|scope| -> Result<_, Failure> {
                    let _stop = Stop(&stop, &gate);
                    let (sender, ready) = mpsc::channel();
                    let mut handles = Vec::new();
                    for worker in 0..count {
                        let sender = sender.clone();
                        let published = &published;
                        let stop = &stop;
                        let gate = &gate;
                        let queries = &queries;
                        let oracle = &oracle;
                        let initial = &initial;
                        handles.push(scope.spawn(move || -> Result<_, Failure> {
                            let setup = pin(worker * 2);
                            let mut samples = Vec::with_capacity(16_384);
                            sender.send(setup.is_ok())?;
                            drop(sender);
                            setup?;
                            let mut batches = 0u64;
                            let mut generations = [0u64; 2];
                            while gate.load(Ordering::Acquire) == 0 {
                                std::hint::spin_loop();
                            }
                            if gate.load(Ordering::Acquire) == 2 {
                                return Err("publication setup cancelled".into());
                            }
                            while !stop.load(Ordering::Acquire) {
                                let timed = (batches.is_multiple_of(17)).then(Instant::now);
                                // Keep an old request alive regularly while newer requests
                                // acquire one coherent snapshot for the whole 32-query batch.
                                let snapshot = if batches.is_multiple_of(16) {
                                    Arc::clone(initial)
                                } else {
                                    Arc::clone(
                                        &*published
                                            .read()
                                            .map_err(|_| "publication lock poisoned")?,
                                    )
                                };
                                for item in 0..32 {
                                    let index = ((batches * 32 + item + worker as u64 * 97)
                                        % queries.len() as u64)
                                        as usize;
                                    verify(&snapshot, index, queries, oracle)?;
                                }
                                generations[snapshot.generation] += 32;
                                batches += 1;
                                if let Some(start) = timed {
                                    samples.push(start.elapsed().as_nanos() as u64);
                                }
                            }
                            let finished = Instant::now();
                            assert!(generations.iter().all(|&n| n > 0));
                            Ok((finished, batches, generations, samples))
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
                    if failed {
                        gate.store(2, Ordering::Release);
                        return Err("publication worker setup failed".into());
                    }
                    let started = Instant::now();
                    gate.store(1, Ordering::Release);
                    let mut constructions = Vec::new();
                    let mut publications = Vec::new();
                    let mut rejected = 0;
                    for round in 0..50 {
                        let active = Arc::clone(
                            &*published.read().map_err(|_| "publication lock poisoned")?,
                        );
                        if round % 10 == 0 {
                            // SAFETY: this private malformed file is immutable, and rejected mappings are dropped immediately.
                            assert!(matches!(
                                unsafe {
                                    MappedReader::map_file(
                                        &File::open(&invalid)?,
                                        Limits::default(),
                                    )
                                },
                                Err(Error::Invalid(_))
                            ));
                            assert!(
                                active
                                    .set
                                    .replaced(
                                        2,
                                        Arc::clone(staged.front().ok_or("staged reader missing")?)
                                    )
                                    .is_err()
                            );
                            assert!(Arc::ptr_eq(
                                &active,
                                &*published.read().map_err(|_| "publication lock poisoned")?
                            ));
                            rejected += 2;
                        }
                        let generation = 1 - active.generation;
                        let start = Instant::now();
                        let candidate = Arc::new(Snapshot {
                            generation,
                            set: active
                                .set
                                .replaced(0, staged.pop_front().ok_or("staged reader missing")?)?,
                        });
                        constructions.push(start.elapsed().as_nanos() as u64);
                        assert_eq!(
                            Arc::strong_count(
                                candidate.set.reader(0).ok_or("replacement missing")?
                            ),
                            1
                        );
                        assert!(Arc::ptr_eq(
                            candidate.set.reader(1).ok_or("unchanged reader missing")?,
                            &unchanged
                        ));
                        assert_eq!(
                            candidate
                                .set
                                .reader(0)
                                .ok_or("replacement reader missing")?
                                .metadata()?,
                            [generation as u8]
                        );
                        let start = Instant::now();
                        let previous = {
                            let mut guard =
                                published.write().map_err(|_| "publication lock poisoned")?;
                            std::mem::replace(&mut *guard, candidate)
                        };
                        publications.push(start.elapsed().as_nanos() as u64);
                        drop(previous);
                        for (live, copy) in &borrowed {
                            assert_eq!(*live, copy.as_slice());
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    stop.store(true, Ordering::Release);
                    let mut workers = Vec::new();
                    for handle in handles {
                        workers.push(handle.join().map_err(|_| "publication worker panicked")??);
                    }
                    let ended = workers
                        .iter()
                        .map(|w| w.0)
                        .max()
                        .ok_or("no publication workers")?;
                    let wall_ns = ended.duration_since(started).as_nanos();
                    let queries = workers.iter().map(|w| w.1 * 32).sum::<u64>();
                    let mut samples: Vec<_> =
                        workers.iter().flat_map(|w| w.3.iter().copied()).collect();
                    let histogram = histogram(&samples);
                    samples.sort_unstable();
                    constructions.sort_unstable();
                    publications.sort_unstable();
                    let released = weak
                        .iter()
                        .filter(|reader| reader.upgrade().is_none())
                        .count();
                    assert_eq!(released, 49);
                    Ok((
                        serde_json::json!({"trial":trial,"routing":if fine{"fine"}else{"coarse"},"workers":count,
                        "queries":queries,"wall_ns":wall_ns,"throughput_queries_per_second":queries as f64/(wall_ns as f64/1e9),
                        "publications":50,"rejected_candidates":rejected,"retired_mappings_released":released,"retained_borrowed_payloads":borrowed.len(),
                        "construct_p50_ns":constructions[25],"construct_p95_ns":constructions[47],"construct_p99_ns":constructions[49],
                        "publication_lock_p50_ns":publications[25],"publication_lock_p95_ns":publications[47],"publication_lock_p99_ns":publications[49],
                        "batch32_p50_ns":samples[samples.len()/2],"batch32_p95_ns":samples[samples.len()*95/100],"batch32_p99_ns":samples[samples.len()*99/100],
                        "sampled_batches":samples.len(),"batch32_histogram_ns":histogram,
                        "worker_generations":workers.iter().map(|w|w.2).collect::<Vec<_>>()}),
                        weak,
                    ))
                })?;
                for (live, copy) in &borrowed {
                    assert_eq!(*live, copy.as_slice());
                }
                drop(borrowed);
                drop(published);
                drop(initial);
                let (mut result, weak) = result;
                assert!(weak.iter().all(|reader| reader.upgrade().is_none()));
                result["all_mappings_released_after_snapshots"] = serde_json::json!(true);
                println!("{result}");
            }
        }
    }
    Ok(())
}

fn fixture(directory: &Path, generation: u8) -> Result<(Arc<MappedReader>, Vec<Row>), Failure> {
    let mut builder = Builder::new(Limits::default());
    let mut data = Vec::new();
    let mut push = |target: Target| -> Result<(), Error> {
        let id = data.len() as u32;
        let mut payload = vec![generation];
        payload.extend_from_slice(&id.to_le_bytes());
        builder.push(target, &payload)?;
        data.push((0, id, target, payload));
        Ok(())
    };
    if generation == 2 {
        push(Target::Network {
            address: Ipv4Addr::UNSPECIFIED.into(),
            prefix: 0,
        })?;
        push(Target::Network {
            address: Ipv6Addr::UNSPECIFIED.into(),
            prefix: 0,
        })?;
        push(Target::Asn(64512))?;
    } else {
        for id in 0..65_536u32 {
            let target = Target::Network {
                address: Ipv4Addr::from(u32::from(generation) * 0x8000_0000 + id * 512).into(),
                prefix: 24,
            };
            push(target)?;
            if id % 1024 == 0 {
                push(target)?;
            }
            push(Target::Network {
                address: Ipv6Addr::from(
                    (u128::from(0x2001_0db8u32 + u32::from(generation)) << 96)
                        | (u128::from(id) << 64),
                )
                .into(),
                prefix: 72,
            })?;
            push(Target::Asn(id * 2 + 1 + u32::from(generation)))?;
        }
    }
    let path = directory.join(format!("snapshot-{generation}.nidx"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    builder.write_to(&mut file, &[generation])?;
    drop(file);
    // SAFETY: files are private and immutable until every snapshot and mapping is dropped.
    let reader = unsafe { MappedReader::map_file(&File::open(path)?, Limits::default()) }?;
    Ok((Arc::new(reader), data))
}
fn queries() -> Vec<Query> {
    let mut out = Vec::new();
    for index in 0..256u32 {
        let id = index.wrapping_mul(2_654_435_761) % 65536;
        let generation = (index / 3) % 2;
        let gap = (index / 6) % 2;
        out.push(match index % 3 {
            0 => Query::Ip(Ipv4Addr::from(generation * 0x8000_0000 + id * 512 + gap * 256).into()),
            1 => Query::Ip(
                Ipv6Addr::from(
                    (u128::from(0x2001_0db8 + generation) << 96)
                        | (u128::from(id) << 64)
                        | (u128::from(gap) << 63),
                )
                .into(),
            ),
            _ => Query::Asn(id * 2 + 1 + generation),
        });
    }
    out.extend([
        Query::Ip(Ipv4Addr::UNSPECIFIED.into()),
        Query::Ip(Ipv6Addr::UNSPECIFIED.into()),
        Query::Asn(64512),
    ]);
    out
}
fn contains(target: Target, query: Query) -> bool {
    match (target, query) {
        (Target::Asn(a), Query::Asn(b)) => a == b,
        (
            Target::Network {
                address: IpAddr::V4(a),
                prefix,
            },
            Query::Ip(IpAddr::V4(b)),
        ) => {
            let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);
            u32::from(a) & mask == u32::from(b) & mask
        }
        (
            Target::Network {
                address: IpAddr::V6(a),
                prefix,
            },
            Query::Ip(IpAddr::V6(b)),
        ) => {
            let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);
            u128::from(a) & mask == u128::from(b) & mask
        }
        _ => false,
    }
}
fn verify(
    snapshot: &Snapshot,
    index: usize,
    queries: &[Query],
    oracle: &Oracle,
) -> Result<(), Error> {
    let expected = &oracle[snapshot.generation][index];
    let mut seen = 0u64;
    let mut emit = |source: usize, row: netindex::Match<'_>| {
        let position = expected
            .iter()
            .position(|r| r.0 == source && r.1 == row.id && r.2 == row.target && r.3 == row.payload)
            .expect("snapshot result disagrees with original-target oracle");
        assert_eq!(seen & (1 << position), 0);
        seen |= 1 << position;
        black_box(row.payload);
        Ok::<_, Error>(())
    };
    match queries[index] {
        Query::Ip(ip) => snapshot.set.visit_ip(ip, &mut emit)?,
        Query::Asn(asn) => snapshot.set.visit_asn(asn, &mut emit)?,
    };
    assert_eq!(seen, (1 << expected.len()) - 1);
    Ok(())
}
fn histogram(values: &[u64]) -> std::collections::BTreeMap<u64, u64> {
    let mut out = std::collections::BTreeMap::new();
    for &value in values {
        *out.entry(value).or_insert(0) += 1;
    }
    out
}
fn pin(cpu: usize) -> std::io::Result<()> {
    // SAFETY: fixed CPUs are in 0..32, the mask is initialized, and PID zero selects this thread only.
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
