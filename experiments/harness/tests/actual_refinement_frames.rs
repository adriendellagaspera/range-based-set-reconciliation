// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! #26: actual application datagrams on the 100k/100 scattered corpus. Run manually with
//! `cargo test --release --test actual_refinement_frames -- --ignored --nocapture`.
//! This does not observe kernel IP fragmentation or model a lossy path.

#![forbid(unsafe_code)]

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rbsr::RangeAggregate;
use reconcile::persistence::{InMemoryPersistence, PersistedState, Persistence};
use reconcile::replicated_map::Config;
use reconcile::{
    Entry, InMemoryNetwork, InMemoryTransport, ReplicatedMap, State, Timestamp, Transport,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

struct RecordingTransport {
    inner: InMemoryTransport,
    datagrams: Arc<Mutex<Vec<Vec<u8>>>>,
}

#[async_trait::async_trait]
impl Transport for RecordingTransport {
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    async fn send_to(&self, buf: &[u8], dst: &SocketAddr) -> io::Result<usize> {
        let sent = self.inner.send_to(buf, dst).await?;
        self.datagrams.lock().push(buf.to_vec());
        Ok(sent)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

fn replica(
    network: &InMemoryNetwork,
    ip: IpAddr,
    datagrams: Arc<Mutex<Vec<Vec<u8>>>>,
) -> ReplicatedMap<u64, u64> {
    let port = 24_100;
    let transport = RecordingTransport {
        inner: network.bind(SocketAddr::new(ip, port)),
        datagrams,
    };
    let config = Config::default()
        .with_port(port)
        .with_listen_addr(ip)
        .with_net("127.0.0.0/8".parse().unwrap())
        .unwrap()
        .with_insecure_no_key();
    ReplicatedMap::new_with_transport(config, Arc::new(transport)).unwrap()
}

// The enum mirrors the current wire variant order. Exact re-encoding checks that the mirror
// consumed every byte before using it to price a smaller frame budget.
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

fn repacked_at(datagrams: &[Vec<u8>], budget: usize) -> (usize, usize, usize) {
    let mut frames = 0;
    let mut bytes = 0;
    let mut largest_item = 0;
    for datagram in datagrams {
        assert!(!datagram.is_empty());
        let messages: Vec<WireMessage> =
            gossip::bincode::decode_stream(&datagram[1..], 65_507).unwrap();
        let mut reencoded = Vec::new();
        let mut current = 1usize; // wire-version byte per frame
        for message in messages {
            let before = reencoded.len();
            gossip::bincode::encode(&message, &mut reencoded).unwrap();
            let size = reencoded.len() - before;
            largest_item = largest_item.max(size);
            assert!(size < budget, "atomic message needs {} bytes", size + 1);
            if current + size > budget {
                frames += 1;
                bytes += current;
                current = 1;
            }
            current += size;
        }
        assert_eq!(
            &reencoded,
            &datagram[1..],
            "wire mirror must round-trip exactly"
        );
        if current > 1 {
            frames += 1;
            bytes += current;
        }
    }
    (frames, bytes, largest_item)
}

#[tokio::test]
#[ignore = "100k-entry live corpus; run explicitly to capture application datagrams"]
async fn scattered_refinement_is_bounded_and_converges_exactly() {
    let network = InMemoryNetwork::new();
    let left_ip: IpAddr = "127.9.9.1".parse().unwrap();
    let right_ip: IpAddr = "127.9.9.2".parse().unwrap();
    let left_datagrams = Arc::new(Mutex::new(Vec::new()));
    let right_datagrams = Arc::new(Mutex::new(Vec::new()));
    let left = replica(&network, left_ip, Arc::clone(&left_datagrams));
    let right = replica(&network, right_ip, Arc::clone(&right_datagrams));

    let corpus: Vec<_> = (0..100_000u64)
        .map(|key| (key, key.wrapping_mul(2_654_435_761)))
        .collect();
    left.load_bulk(&corpus);
    let target = left.fingerprint(..);

    // Preserve the same dated cells on both sides: independent load_bulk calls would mint
    // different LWW timestamps on all 99,900 shared keys and change the difference shape.
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

    let tasks = [
        tokio::spawn(left.clone().run(CancellationToken::new())),
        tokio::spawn(right.clone().run(CancellationToken::new())),
    ];
    tokio::time::timeout(Duration::from_secs(120), async {
        while right.fingerprint(..) != target {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the live exchange did not converge within 120 seconds");
    for task in tasks {
        task.abort();
    }

    for &(key, value) in &corpus {
        assert_eq!(right.get_cloned(&key), Some(value), "key {key}");
    }
    for (side, samples) in [("left", left_datagrams), ("right", right_datagrams)] {
        let datagrams = samples.lock();
        let lengths: Vec<_> = datagrams.iter().map(Vec::len).collect();
        assert!(!lengths.is_empty(), "{side} sent no datagrams");
        assert!(lengths.iter().all(|&length| length <= 65_507));
        println!(
            "{side}: {} application datagrams, {} bytes, largest {} bytes; individual sizes: {lengths:?}",
            lengths.len(),
            lengths.iter().sum::<usize>(),
            lengths.iter().max().unwrap(),
        );
        for budget in [256, 512, 1_200, 1_472] {
            let (frames, bytes, largest_item) = repacked_at(&datagrams, budget);
            println!(
                "{side}: projected complete-frame budget={budget} frames={frames} bytes={bytes} largest_atomic_item={largest_item}"
            );
        }
    }
}
