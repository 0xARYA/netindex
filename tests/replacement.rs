//! In-flight snapshots and borrowed answers survive concurrent source replacement.

#![cfg(test)]

use std::{
    net::IpAddr,
    sync::{Arc, RwLock, mpsc},
    thread,
    time::Duration,
};

use netindex::{Builder, Error, Reader, ReaderSet, Target};

struct Snapshot {
    generation: usize,
    set: ReaderSet<Vec<u8>>,
}

#[test]
fn publication_preserves_in_flight_results_and_rejects_invalid_replacements() {
    let unchanged = reader(2);
    let sources = [reader(0), reader(1)];

    for fine in [false, true] {
        let readers = vec![Arc::clone(&sources[0]), Arc::clone(&unchanged)];
        let set = if fine {
            ReaderSet::with_fine_routing(readers)
        } else {
            ReaderSet::new(readers)
        }
        .unwrap();
        let published = RwLock::new(Arc::new(Snapshot { generation: 0, set }));

        thread::scope(|scope| {
            let mut controls = Vec::new();
            let mut workers = Vec::new();
            for _ in 0..4 {
                let (command, receive) = mpsc::channel();
                let (acknowledge, received) = mpsc::channel();
                controls.push((command, received));
                let published = &published;
                let unchanged = &unchanged;

                workers.push(scope.spawn(move || {
                    for round in 0..32 {
                        receive.recv_timeout(Duration::from_secs(10)).unwrap();
                        let held = Arc::clone(&published.read().unwrap());
                        assert_eq!(held.generation, round % 2);
                        let mut borrowed = collect(&held);
                        acknowledge.send(()).unwrap();

                        receive.recv_timeout(Duration::from_secs(10)).unwrap();
                        borrowed.sort_unstable_by_key(|row| (row.0, row.1, row.2));
                        assert_eq!(borrowed, expected(held.generation));
                        let fresh = Arc::clone(&published.read().unwrap());
                        assert_eq!(fresh.generation, 1 - held.generation);
                        let mut actual = collect(&fresh);
                        actual.sort_unstable_by_key(|row| (row.0, row.1, row.2));
                        assert_eq!(actual, expected(fresh.generation));
                        assert!(Arc::ptr_eq(fresh.set.reader(1).unwrap(), unchanged));
                        acknowledge.send(()).unwrap();
                    }
                }));
            }

            for round in 0..32 {
                for (command, _) in &controls {
                    command.send(()).unwrap();
                }
                for (_, received) in &controls {
                    received.recv_timeout(Duration::from_secs(10)).unwrap();
                }

                let current = Arc::clone(&published.read().unwrap());
                assert_eq!(current.generation, round % 2);
                assert!(matches!(
                    current.set.replaced(2, Arc::clone(&sources[1])),
                    Err(Error::Invalid("reader position"))
                ));
                assert!(Arc::ptr_eq(&current, &published.read().unwrap()));
                let generation = 1 - current.generation;
                let replacement = current
                    .set
                    .replaced(0, Arc::clone(&sources[generation]))
                    .unwrap();
                *published.write().unwrap() = Arc::new(Snapshot {
                    generation,
                    set: replacement,
                });

                for (command, _) in &controls {
                    command.send(()).unwrap();
                }
                for (_, received) in &controls {
                    received.recv_timeout(Duration::from_secs(10)).unwrap();
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
    }
}

type Row<'a> = (usize, usize, u32, Target, &'a [u8]);

fn collect(snapshot: &Snapshot) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    for (query, address) in queries().iter().enumerate() {
        snapshot
            .set
            .visit_ip(*address, |source, row| {
                rows.push((query, source, row.id, row.target, row.payload));
                Ok::<_, Error>(())
            })
            .unwrap();
    }
    rows
}

fn expected(generation: usize) -> Vec<Row<'static>> {
    let mut rows = Vec::new();
    for (query, address) in queries().iter().enumerate() {
        let active: IpAddr = if generation == 0 {
            "203.0.113.1"
        } else {
            "198.51.100.2"
        }
        .parse()
        .unwrap();
        if *address == active {
            for id in [0, 1] {
                rows.push((query, 0, id, Target::Address(active), payload(generation)));
            }
        }
        rows.push((
            query,
            1,
            0,
            Target::Network {
                address: "0.0.0.0".parse().unwrap(),
                prefix: 0,
            },
            payload(2),
        ));
    }
    rows
}

fn reader(generation: usize) -> Arc<Reader<Vec<u8>>> {
    let mut builder = Builder::new(Default::default());
    if generation == 2 {
        builder
            .push(
                Target::Network {
                    address: "0.0.0.0".parse().unwrap(),
                    prefix: 0,
                },
                payload(generation),
            )
            .unwrap();
    } else {
        let address = queries()[generation];
        for _ in 0..2 {
            builder
                .push(Target::Address(address), payload(generation))
                .unwrap();
        }
    }
    Arc::new(Reader::open(builder.into_bytes(b"snapshot").unwrap(), Default::default()).unwrap())
}

fn queries() -> [IpAddr; 2] {
    [
        "203.0.113.1".parse().unwrap(),
        "198.51.100.2".parse().unwrap(),
    ]
}

fn payload(generation: usize) -> &'static [u8] {
    match generation {
        0 => b"old",
        1 => b"new",
        _ => b"unchanged",
    }
}
