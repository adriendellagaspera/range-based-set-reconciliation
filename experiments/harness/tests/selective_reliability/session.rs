// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gossip::netem::{Link, Netem, NetemTransport, Probability, Rtt, Seed};
use parking_lot::Mutex;
use reconcile::persistence::{InMemoryPersistence, PersistedState, Persistence};
use reconcile::replicated_map::Config;
use reconcile::{InMemoryNetwork, ReplicatedMap};
use tokio_util::sync::CancellationToken;

use super::super::harness::{Fixture, Sample};
use super::super::wire::{FaultProfile, Metrics, Trace, BITMAP_ACK_BASE_LEN, BITMAP_ACK_ENTRY_LEN};
use super::{AckLedger, BitmapShared, BitmapTransport, DataWire, MAX_ACK_FLIGHTS_PER_FRAME};

struct ReplicaSpec<'a> {
    ip: IpAddr,
    epoch: u64,
    peer_epoch: u64,
    direction: u8,
    entries: &'a [(u64, reconcile::Entry<reconcile::Timestamp, u64>)],
}

type ReplicaProbe = (
    ReplicatedMap<u64, u64>,
    Arc<Mutex<Metrics>>,
    Arc<Mutex<Trace>>,
);

fn replica(
    network: &InMemoryNetwork,
    budget: usize,
    profile: FaultProfile,
    wire: DataWire,
    spec: ReplicaSpec<'_>,
) -> ReplicaProbe {
    let port = 24_100;
    let link = Link::at(Rtt::from_millis(10.0)).with_loss(Probability::percent(0.0));
    let raw = Arc::new(network.bind(SocketAddr::new(spec.ip, port)));
    let inner = Arc::new(NetemTransport::new(
        Arc::clone(&raw),
        Netem::uniform(
            link,
            Seed::new(0x6269_746d_6170_0000 + u64::from(spec.direction)),
        ),
    ));
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let trace = Arc::new(Mutex::new(Trace::default()));
    let ack_batch_entries = ((budget.saturating_sub(BITMAP_ACK_BASE_LEN)) / BITMAP_ACK_ENTRY_LEN)
        .clamp(1, MAX_ACK_FLIGHTS_PER_FRAME);
    let shared = Arc::new(BitmapShared {
        raw,
        inner,
        profile,
        direction: spec.direction,
        attempts: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        ack_ledger: Mutex::new(AckLedger::default()),
        ack_batch_entries,
        frame_budget: budget,
        data_header_len: wire.header_len(),
        metrics: Arc::clone(&metrics),
        trace: Arc::clone(&trace),
    });
    let transport = BitmapTransport {
        shared,
        budget,
        wire,
        epoch: spec.epoch,
        peer_epoch: spec.peer_epoch,
        next_flight: AtomicU32::new(0),
    };
    let persistence = Arc::new(InMemoryPersistence::<u64, u64>::new());
    persistence
        .save(&PersistedState::from(spec.entries.to_vec()))
        .unwrap();
    let config = Config::default()
        .with_port(port)
        .with_listen_addr(spec.ip)
        .with_net("127.0.0.0/8".parse().unwrap())
        .unwrap()
        .with_insecure_no_key();
    (
        ReplicatedMap::new_with_transport(config, Arc::new(transport))
            .unwrap()
            .with_persistence(persistence)
            .unwrap(),
        metrics,
        trace,
    )
}

pub(crate) async fn run_bitmap_sample(
    fixture: &Fixture,
    budget: usize,
    profile: FaultProfile,
) -> Sample {
    run_bitmap_sample_with_wire(fixture, budget, profile, DataWire::Full).await
}

pub(crate) async fn run_minimal_bitmap_sample(
    fixture: &Fixture,
    budget: usize,
    profile: FaultProfile,
) -> Sample {
    run_bitmap_sample_with_wire(fixture, budget, profile, DataWire::Minimal).await
}

async fn run_bitmap_sample_with_wire(
    fixture: &Fixture,
    budget: usize,
    profile: FaultProfile,
    wire: DataWire,
) -> Sample {
    let network = InMemoryNetwork::new();
    let (left_ip, right_ip): (IpAddr, IpAddr) = match wire {
        DataWire::Full => ("127.9.10.1".parse().unwrap(), "127.9.10.2".parse().unwrap()),
        DataWire::Minimal => ("127.9.11.1".parse().unwrap(), "127.9.11.2".parse().unwrap()),
    };
    let left_epoch = 0x4c45_4654_0000_0001;
    let right_epoch = 0x5249_4748_0000_0001;
    let (left, left_metrics, left_trace) = replica(
        &network,
        budget,
        profile,
        wire,
        ReplicaSpec {
            ip: left_ip,
            epoch: left_epoch,
            peer_epoch: right_epoch,
            direction: 0,
            entries: &fixture.full,
        },
    );
    let (right, right_metrics, right_trace) = replica(
        &network,
        budget,
        profile,
        wire,
        ReplicaSpec {
            ip: right_ip,
            epoch: right_epoch,
            peer_epoch: left_epoch,
            direction: 1,
            entries: &fixture.shared,
        },
    );
    let target = left.fingerprint(..);
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
        for key in 0..100_000u64 {
            assert_eq!(
                right.get_cloned(&key),
                Some(key.wrapping_mul(2_654_435_761)),
                "bitmap wire={wire:?} scenario={:#x} key={key}",
                profile.scenario
            );
        }
    }

    let mut metrics = left_metrics.lock().clone();
    metrics.add(&right_metrics.lock());
    let mut drops = left_trace.lock().drops.clone();
    drops.extend(right_trace.lock().drops.iter().copied());
    Sample {
        elapsed,
        converged,
        metrics,
        drops,
    }
}
