// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! #26/#69: actual RBSR sessions with complete wire messages in small datagrams.
//! The research transport is for unkeyed u64/u64 traffic only, and does not implement
//! production authentication, network MTU discovery, or selective reliability.

#![forbid(unsafe_code)]

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gossip::netem::{Impairments, Link, Netem, NetemTransport, Probability, Rtt, Seed};
use parking_lot::Mutex;
use rbsr::RangeAggregate;
use reconcile::persistence::{InMemoryPersistence, PersistedState, Persistence};
use reconcile::replicated_map::Config;
use reconcile::{
    Entry, InMemoryNetwork, InMemoryTransport, ReplicatedMap, State, Timestamp, Transport,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

// Variant order mirrors the pinned public implementation. Byte equality below guards drift.
#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
enum WireMessage {
    EntryFingerprint(RangeAggregate<u64>),
    EntryUpdate((u64, Entry<Timestamp, u64>)),
    TombstoneAck((u64, u64)),
    StateFingerprint(RangeAggregate<u64>),
    StateUpdate((u64, State<u64>)),
    ConvergenceAck,
    Reserved6(Vec<u8>),
}

struct SmallFrameTransport {
    inner: NetemTransport<InMemoryTransport>,
    budget: usize,
    sent_lengths: Arc<Mutex<Vec<usize>>>,
}

#[async_trait::async_trait]
impl Transport for SmallFrameTransport {
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    async fn send_to(&self, buf: &[u8], dst: &SocketAddr) -> io::Result<usize> {
        let Some((&version, payload)) = buf.split_first() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "empty wire frame",
            ));
        };
        let messages: Vec<WireMessage> = gossip::bincode::decode_stream(payload, 65_507)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let mut reencoded = Vec::new();
        let mut frame = vec![version];
        let mut frames = Vec::new();
        for message in messages {
            let start = reencoded.len();
            gossip::bincode::encode(&message, &mut reencoded)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            let item = &reencoded[start..];
            if item.len() + 1 > self.budget {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "atomic message exceeds budget",
                ));
            }
            if frame.len() + item.len() > self.budget {
                frames.push(frame);
                frame = vec![version];
            }
            frame.extend_from_slice(item);
        }
        if reencoded != payload {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "wire mirror changed bytes",
            ));
        }
        if frame.len() > 1 {
            frames.push(frame);
        }
        for frame in frames {
            let sent = self.inner.send_to(&frame, dst).await?;
            if sent != frame.len() {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "short frame send"));
            }
            self.sent_lengths.lock().push(sent);
        }
        // The caller submitted this whole logical datagram; Netem applies loss to each
        // emitted small frame and reports it through its own impairment counters.
        Ok(buf.len())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

type SentLengths = Arc<Mutex<Vec<usize>>>;

fn replica(
    network: &InMemoryNetwork,
    ip: IpAddr,
    link: Link,
    seed: Seed,
    budget: usize,
) -> (ReplicatedMap<u64, u64>, SentLengths, Impairments) {
    let port = 24_100;
    let netem = NetemTransport::new(
        Arc::new(network.bind(SocketAddr::new(ip, port))),
        Netem::uniform(link, seed),
    );
    let impairments = netem.impairments();
    let sent_lengths = Arc::new(Mutex::new(Vec::new()));
    let transport = SmallFrameTransport {
        inner: netem,
        budget,
        sent_lengths: Arc::clone(&sent_lengths),
    };
    let config = Config::default()
        .with_port(port)
        .with_listen_addr(ip)
        .with_net("127.0.0.0/8".parse().unwrap())
        .unwrap()
        .with_insecure_no_key();
    (
        ReplicatedMap::new_with_transport(config, Arc::new(transport)).unwrap(),
        sent_lengths,
        impairments,
    )
}

struct Sample {
    elapsed: Duration,
    converged: bool,
    offered: u64,
    dropped: u64,
    bytes: usize,
    largest: usize,
}

async fn run_sample(corpus: &[(u64, u64)], budget: usize, loss: f64, raw_seed: u64) -> Sample {
    let network = InMemoryNetwork::new();
    let left_ip: IpAddr = "127.9.9.1".parse().unwrap();
    let right_ip: IpAddr = "127.9.9.2".parse().unwrap();
    let link = Link::at(Rtt::from_millis(10.0)).with_loss(Probability::percent(loss));
    let seed = Seed::new(raw_seed);
    let (left, left_lengths, left_loss) = replica(&network, left_ip, link, seed, budget);
    let (right, right_lengths, right_loss) = replica(&network, right_ip, link, seed, budget);

    left.load_bulk(corpus);
    let target = left.fingerprint(..);
    let shared: Vec<_> = left
        .snapshot()
        .iter()
        .filter(|(key, _)| **key % 1_000 != 500)
        .map(|(key, entry)| (*key, entry.clone()))
        .collect();
    assert_eq!(shared.len(), 99_900);
    let persistence = Arc::new(InMemoryPersistence::<u64, u64>::new());
    persistence.save(&PersistedState::from(shared)).unwrap();
    let right = right.with_persistence(persistence).unwrap();
    assert_ne!(right.fingerprint(..), target);
    right.seed_peer(left_ip);

    let start = Instant::now();
    let tasks = [
        tokio::spawn(left.clone().run(CancellationToken::new())),
        tokio::spawn(right.clone().run(CancellationToken::new())),
    ];
    let converged = tokio::time::timeout(Duration::from_secs(12), async {
        while right.fingerprint(..) != target {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .is_ok();
    let elapsed = start.elapsed();
    for task in tasks {
        task.abort();
    }
    if converged {
        for &(key, value) in corpus {
            assert_eq!(
                right.get_cloned(&key),
                Some(value),
                "seed {raw_seed}, key {key}"
            );
        }
    }

    let left_sent = left_lengths.lock();
    let right_sent = right_lengths.lock();
    let lengths: Vec<usize> = left_sent.iter().chain(right_sent.iter()).copied().collect();
    assert!(!lengths.is_empty());
    assert!(lengths.iter().all(|&size| size <= budget));
    let offered = left_loss.offered() + right_loss.offered();
    assert_eq!(lengths.len(), offered as usize);
    Sample {
        elapsed,
        converged,
        offered,
        dropped: left_loss.dropped() + right_loss.dropped(),
        bytes: lengths.iter().sum(),
        largest: lengths.iter().copied().max().unwrap_or(0),
    }
}

#[tokio::test]
#[ignore = "seeded 100k-entry live small-frame sweep; run explicitly with --ignored --nocapture"]
async fn complete_small_frames_under_datagram_loss() {
    let corpus: Vec<_> = (0..100_000u64)
        .map(|key| (key, key.wrapping_mul(2_654_435_761)))
        .collect();
    for budget in [1_200, 1_472] {
        for &(loss, samples) in &[(0.0, 8u64), (0.1, 64), (1.0, 24), (5.0, 24)] {
            let mut results = Vec::new();
            for index in 0..samples {
                let seed = Seed::DEFAULT.get().wrapping_add(index);
                let result = run_sample(&corpus, budget, loss, seed).await;
                println!(
                    "[small] budget={budget} loss={loss:.1}% seed={seed:#x} converged={} elapsed_ms={} offered={} dropped={} bytes={} largest={}",
                    result.converged, result.elapsed.as_millis(), result.offered,
                    result.dropped, result.bytes, result.largest,
                );
                results.push(result);
            }
            let mut times: Vec<_> = results
                .iter()
                .map(|sample| sample.elapsed.as_millis())
                .collect();
            times.sort_unstable();
            println!(
                "[summary] budget={budget} loss={loss:.1}% trials={samples} converged={} offered={} dropped={} elapsed_p50_ms={} elapsed_p95_ms={} bytes_total={}",
                results.iter().filter(|sample| sample.converged).count(),
                results.iter().map(|sample| sample.offered).sum::<u64>(),
                results.iter().map(|sample| sample.dropped).sum::<u64>(),
                times[times.len() / 2], times[(times.len() * 95 / 100).min(times.len() - 1)],
                results.iter().map(|sample| sample.bytes).sum::<usize>(),
            );
            assert!(
                results.iter().all(|sample| sample.converged),
                "convergence regression at {budget} bytes and {loss}% loss"
            );
        }
    }
}
