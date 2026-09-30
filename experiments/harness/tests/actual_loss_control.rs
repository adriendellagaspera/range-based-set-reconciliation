// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! #69's current-implementation control: actual `ReplicatedMap` convergence under seeded,
//! per-datagram Netem loss on the 100k/100 scattered corpus. The fragment totals are a
//! separate *projection* over sent application datagrams, not observed IP fragmentation.

#![forbid(unsafe_code)]

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gossip::netem::{Impairments, Link, Netem, NetemTransport, Probability, Rtt, Seed};
use parking_lot::Mutex;
use reconcile::persistence::{InMemoryPersistence, PersistedState, Persistence};
use reconcile::replicated_map::Config;
use reconcile::{InMemoryNetwork, InMemoryTransport, ReplicatedMap, Transport};
use tokio_util::sync::CancellationToken;

struct RecordingTransport {
    inner: NetemTransport<InMemoryTransport>,
    sent_lengths: Arc<Mutex<Vec<usize>>>,
}

#[async_trait::async_trait]
impl Transport for RecordingTransport {
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    async fn send_to(&self, buf: &[u8], dst: &SocketAddr) -> io::Result<usize> {
        let sent = self.inner.send_to(buf, dst).await?;
        self.sent_lengths.lock().push(sent);
        Ok(sent)
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
) -> (ReplicatedMap<u64, u64>, SentLengths, Impairments) {
    let port = 24_100;
    let netem = NetemTransport::new(
        Arc::new(network.bind(SocketAddr::new(ip, port))),
        Netem::uniform(link, seed),
    );
    let impairments = netem.impairments();
    let sent_lengths = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        inner: netem,
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
    seed: u64,
    loss: f64,
    elapsed: Duration,
    converged: bool,
    offered: u64,
    dropped: u64,
    datagrams: usize,
    bytes: usize,
    largest: usize,
    fragments_1200: usize,
    fragments_1472: usize,
}

async fn run_sample(corpus: &[(u64, u64)], loss: f64, raw_seed: u64) -> Sample {
    let network = InMemoryNetwork::new();
    let left_ip: IpAddr = "127.9.9.1".parse().unwrap();
    let right_ip: IpAddr = "127.9.9.2".parse().unwrap();
    let link = Link::at(Rtt::from_millis(10.0)).with_loss(Probability::percent(loss));
    let seed = Seed::new(raw_seed);
    let (left, left_lengths, left_loss) = replica(&network, left_ip, link, seed);
    let (right, right_lengths, right_loss) = replica(&network, right_ip, link, seed);

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
    assert!(lengths.iter().all(|&size| size <= 65_507));
    let offered = left_loss.offered() + right_loss.offered();
    assert_eq!(lengths.len(), offered as usize);
    Sample {
        seed: raw_seed,
        loss,
        elapsed,
        converged,
        offered,
        dropped: left_loss.dropped() + right_loss.dropped(),
        datagrams: lengths.len(),
        bytes: lengths.iter().sum(),
        largest: lengths.iter().copied().max().unwrap_or(0),
        fragments_1200: lengths.iter().map(|&size| size.div_ceil(1_200)).sum(),
        fragments_1472: lengths.iter().map(|&size| size.div_ceil(1_472)).sum(),
    }
}

#[tokio::test]
#[ignore = "seeded 100k-entry Netem sweep; run explicitly with --ignored --nocapture"]
async fn current_reconciliation_under_datagram_loss() {
    let corpus: Vec<_> = (0..100_000u64)
        .map(|key| (key, key.wrapping_mul(2_654_435_761)))
        .collect();
    for &(loss, samples) in &[(0.0, 16u64), (0.1, 128), (1.0, 32), (5.0, 32)] {
        let mut results = Vec::new();
        for index in 0..samples {
            let seed = Seed::DEFAULT.get().wrapping_add(index);
            let result = run_sample(&corpus, loss, seed).await;
            println!(
                "[control] loss={:.1}% seed={:#x} converged={} elapsed_ms={} offered={} dropped={} datagrams={} bytes={} largest={} projected_fragments_1200={} projected_fragments_1472={}",
                result.loss, result.seed, result.converged, result.elapsed.as_millis(),
                result.offered, result.dropped, result.datagrams, result.bytes, result.largest,
                result.fragments_1200, result.fragments_1472,
            );
            results.push(result);
        }
        let mut times: Vec<_> = results
            .iter()
            .map(|sample| sample.elapsed.as_millis())
            .collect();
        times.sort_unstable();
        println!(
            "[summary] loss={loss:.1}% trials={samples} converged={} dropped={} elapsed_p50_ms={} elapsed_p95_ms={} bytes_total={}",
            results.iter().filter(|sample| sample.converged).count(),
            results.iter().map(|sample| sample.dropped).sum::<u64>(),
            times[times.len() / 2], times[(times.len() * 95 / 100).min(times.len() - 1)],
            results.iter().map(|sample| sample.bytes).sum::<usize>(),
        );
    }
}
