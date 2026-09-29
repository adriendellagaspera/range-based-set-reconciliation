// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gossip::netem::{Link, Netem, NetemTransport, Probability, Rtt, Seed};
use parking_lot::Mutex;
use reconcile::persistence::{InMemoryPersistence, PersistedState, Persistence};
use reconcile::replicated_map::Config;
use reconcile::{Entry, InMemoryNetwork, InMemoryTransport, ReplicatedMap, Timestamp, Transport};
use tokio_util::sync::CancellationToken;

use super::wire::{
    decode_envelope, encode_envelope, pack_messages, stable_id, AttemptMap, FaultKey, FaultProfile,
    Metrics, Mode, SeenState, Trace, WireKey, ACK, DATA, HEADER_LEN, MAX_FRAMES_PER_FLIGHT,
    MAX_PENDING_FRAMES,
};

const MAX_RETRIES: usize = 3;
const RETRY_AFTER: Duration = Duration::from_millis(80);
const REORDER_EXTRA: Duration = Duration::from_millis(25);
const DUPLICATE_EXTRA: Duration = Duration::from_millis(2);

#[derive(Clone)]
struct PendingFrame {
    key: WireKey,
    bytes: Arc<Vec<u8>>,
    dst: SocketAddr,
}

struct Shared {
    raw: Arc<InMemoryTransport>,
    inner: Arc<NetemTransport<InMemoryTransport>>,
    profile: FaultProfile,
    direction: u8,
    attempts: AttemptMap,
    pending: Mutex<HashMap<WireKey, PendingFrame>>,
    metrics: Arc<Mutex<Metrics>>,
    trace: Arc<Mutex<Trace>>,
}

impl Shared {
    fn next_fault_key(&self, kind: u8, semantic: u64) -> FaultKey {
        let mut attempts = self.attempts.lock();
        let attempt = attempts.entry((kind, semantic)).or_default();
        let key = FaultKey {
            direction: self.direction,
            kind,
            semantic,
            attempt: *attempt,
        };
        *attempt = attempt.saturating_add(1);
        key
    }

    async fn emit_candidate(
        self: &Arc<Self>,
        kind: u8,
        key: WireKey,
        bytes: Arc<Vec<u8>>,
        dst: SocketAddr,
        retry: bool,
    ) -> io::Result<()> {
        {
            let mut metrics = self.metrics.lock();
            if kind == ACK {
                metrics.control_frames += 1;
                metrics.control_bytes += bytes.len();
            } else if retry {
                metrics.retry_frames += 1;
                metrics.retry_bytes += bytes.len();
            } else {
                metrics.useful_bytes += bytes.len() - HEADER_LEN;
                metrics.first_data_frames += 1;
                metrics.first_data_bytes += bytes.len();
            }
        }

        let fault_key = self.next_fault_key(kind, key.semantic);
        let decision = self.profile.decision(fault_key);
        if decision.drop {
            let mut metrics = self.metrics.lock();
            metrics.fault_drops += 1;
            if kind == DATA {
                metrics.data_fault_drops += 1;
            } else {
                metrics.control_fault_drops += 1;
            }
            drop(metrics);
            self.trace.lock().drops.insert(fault_key);
            return Ok(());
        }

        if decision.stale {
            self.metrics.lock().stale_injected += 1;
            let mut stale = bytes.as_ref().clone();
            let stale_epoch = key.epoch ^ 0xa5a5_a5a5_a5a5_a5a5;
            stale[1..9].copy_from_slice(&stale_epoch.to_le_bytes());
            self.raw.send_to(&stale, &dst).await?;
        }

        if decision.reorder {
            self.metrics.lock().fault_reorders += 1;
            let inner = Arc::clone(&self.inner);
            let delayed = Arc::clone(&bytes);
            tokio::spawn(async move {
                tokio::time::sleep(REORDER_EXTRA).await;
                let _ = inner.send_to(delayed.as_slice(), &dst).await;
            });
        } else {
            self.inner.send_to(bytes.as_slice(), &dst).await?;
        }

        if decision.duplicate {
            self.metrics.lock().fault_duplicates += 1;
            let inner = Arc::clone(&self.inner);
            let duplicate = Arc::clone(&bytes);
            tokio::spawn(async move {
                tokio::time::sleep(DUPLICATE_EXTRA).await;
                let _ = inner.send_to(duplicate.as_slice(), &dst).await;
            });
        }
        Ok(())
    }

    async fn emit_control(
        self: &Arc<Self>,
        bytes: Arc<Vec<u8>>,
        dst: SocketAddr,
    ) -> io::Result<()> {
        {
            let mut metrics = self.metrics.lock();
            metrics.useful_bytes += bytes.len();
            metrics.first_data_frames += 1;
            metrics.first_data_bytes += bytes.len();
        }

        let semantic = stable_id(bytes.as_slice());
        let fault_key = self.next_fault_key(DATA, semantic);
        let decision = self.profile.decision(fault_key);
        if decision.drop {
            let mut metrics = self.metrics.lock();
            metrics.fault_drops += 1;
            metrics.data_fault_drops += 1;
            drop(metrics);
            self.trace.lock().drops.insert(fault_key);
            return Ok(());
        }

        if decision.reorder {
            self.metrics.lock().fault_reorders += 1;
            let inner = Arc::clone(&self.inner);
            let delayed = Arc::clone(&bytes);
            tokio::spawn(async move {
                tokio::time::sleep(REORDER_EXTRA).await;
                let _ = inner.send_to(delayed.as_slice(), &dst).await;
            });
        } else {
            self.inner.send_to(bytes.as_slice(), &dst).await?;
        }

        if decision.duplicate {
            self.metrics.lock().fault_duplicates += 1;
            let inner = Arc::clone(&self.inner);
            let duplicate = Arc::clone(&bytes);
            tokio::spawn(async move {
                tokio::time::sleep(DUPLICATE_EXTRA).await;
                let _ = inner.send_to(duplicate.as_slice(), &dst).await;
            });
        }
        Ok(())
    }

    fn insert_pending(&self, frames: &[PendingFrame]) -> io::Result<()> {
        let mut pending = self.pending.lock();
        if pending.len() + frames.len() > MAX_PENDING_FRAMES {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "selective retry state bound exceeded",
            ));
        }
        for frame in frames {
            pending.insert(frame.key, frame.clone());
        }
        let pending_bytes = pending.values().map(|frame| frame.bytes.len()).sum();
        let mut metrics = self.metrics.lock();
        metrics.max_pending_frames = metrics.max_pending_frames.max(pending.len());
        metrics.max_pending_bytes = metrics.max_pending_bytes.max(pending_bytes);
        Ok(())
    }

    fn spawn_retries(self: &Arc<Self>, flight: u32) {
        let shared = Arc::clone(self);
        tokio::spawn(async move {
            for _ in 0..MAX_RETRIES {
                tokio::time::sleep(RETRY_AFTER).await;
                let frames: Vec<_> = shared
                    .pending
                    .lock()
                    .values()
                    .filter(|frame| frame.key.flight == flight)
                    .cloned()
                    .collect();
                if frames.is_empty() {
                    return;
                }
                for frame in frames {
                    let _ = shared
                        .emit_candidate(DATA, frame.key, frame.bytes, frame.dst, true)
                        .await;
                }
            }
            tokio::time::sleep(RETRY_AFTER).await;
            let exhausted = {
                let mut pending = shared.pending.lock();
                let before = pending.len();
                pending.retain(|key, _| key.flight != flight);
                before - pending.len()
            };
            shared.metrics.lock().retry_exhausted += exhausted;
        });
    }
}

struct ReplayTransport {
    shared: Arc<Shared>,
    mode: Mode,
    budget: usize,
    epoch: u64,
    peer_epoch: u64,
    next_flight: AtomicU32,
    seen: Mutex<SeenState>,
}

#[async_trait::async_trait]
impl Transport for ReplayTransport {
    async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        if self.mode == Mode::Control {
            return self.shared.inner.recv_from(buf).await;
        }
        let mut raw = vec![0; 65_507];
        loop {
            let (n, src) = self.shared.inner.recv_from(&mut raw).await?;
            let (kind, key, payload) = decode_envelope(&raw[..n])?;
            match kind {
                ACK => {
                    if self.mode == Mode::Selective && key.epoch == self.epoch {
                        self.shared.pending.lock().remove(&key);
                    }
                }
                DATA => {
                    if key.epoch != self.peer_epoch {
                        self.shared.metrics.lock().stale_rejected += 1;
                        continue;
                    }
                    if self.mode == Mode::Selective {
                        let ack = Arc::new(encode_envelope(ACK, key, &[]));
                        self.shared
                            .emit_candidate(ACK, key, ack, src, false)
                            .await?;
                        let (duplicate, seen) = self.seen.lock().record(key.flight, key.slot);
                        let mut metrics = self.shared.metrics.lock();
                        metrics.max_seen_ids = metrics.max_seen_ids.max(seen);
                        drop(metrics);
                        if duplicate {
                            continue;
                        }
                    }
                    if payload.len() > buf.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "decoded frame exceeds receive buffer",
                        ));
                    }
                    buf[..payload.len()].copy_from_slice(payload);
                    return Ok((payload.len(), src));
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unknown research envelope kind",
                    ));
                }
            }
        }
    }

    async fn send_to(&self, buf: &[u8], dst: &SocketAddr) -> io::Result<usize> {
        let payload_budget = match self.mode {
            Mode::Control => self.budget,
            Mode::Selective => self.budget.checked_sub(HEADER_LEN).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "frame budget below identity header size",
                )
            })?,
        };
        let frames = pack_messages(buf, payload_budget)?;
        if frames.len() > MAX_FRAMES_PER_FLIGHT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "logical datagram exceeds bounded flight size",
            ));
        }
        if self.mode == Mode::Control {
            for frame in frames {
                self.shared.emit_control(Arc::new(frame), *dst).await?;
            }
            return Ok(buf.len());
        }

        let flight = self.next_flight.fetch_add(1, Ordering::Relaxed);
        let encoded: Vec<_> = frames
            .into_iter()
            .enumerate()
            .map(|(slot, payload)| {
                let key = WireKey {
                    epoch: self.epoch,
                    flight,
                    slot: slot as u16,
                    semantic: stable_id(&payload),
                };
                PendingFrame {
                    key,
                    bytes: Arc::new(encode_envelope(DATA, key, &payload)),
                    dst: *dst,
                }
            })
            .collect();
        debug_assert!(encoded.iter().all(|frame| frame.bytes.len() <= self.budget));

        self.shared.insert_pending(&encoded)?;
        for frame in &encoded {
            self.shared
                .emit_candidate(DATA, frame.key, Arc::clone(&frame.bytes), frame.dst, false)
                .await?;
        }
        self.shared.spawn_retries(flight);
        Ok(buf.len())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.shared.inner.local_addr()
    }
}

pub(super) struct Fixture {
    pub(super) full: Vec<(u64, Entry<Timestamp, u64>)>,
    pub(super) shared: Vec<(u64, Entry<Timestamp, u64>)>,
}

pub(super) fn fixture() -> Fixture {
    let corpus: Vec<_> = (0..100_000u64)
        .map(|key| (key, key.wrapping_mul(2_654_435_761)))
        .collect();
    let network = InMemoryNetwork::new();
    let ip: IpAddr = "127.9.8.1".parse().unwrap();
    let config = Config::default()
        .with_port(24_100)
        .with_listen_addr(ip)
        .with_net("127.0.0.0/8".parse().unwrap())
        .unwrap()
        .with_insecure_no_key();
    let map = ReplicatedMap::new_with_transport(
        config,
        Arc::new(network.bind(SocketAddr::new(ip, 24_100))),
    )
    .unwrap();
    map.load_bulk(&corpus);
    let full: Vec<_> = map
        .snapshot()
        .iter()
        .map(|(key, entry)| (*key, entry.clone()))
        .collect();
    let shared = full
        .iter()
        .filter(|(key, _)| *key % 1_000 != 500)
        .cloned()
        .collect();
    Fixture { full, shared }
}

struct ReplicaSpec<'a> {
    ip: IpAddr,
    epoch: u64,
    peer_epoch: u64,
    direction: u8,
    entries: &'a [(u64, Entry<Timestamp, u64>)],
}

type ReplicaProbe = (
    ReplicatedMap<u64, u64>,
    Arc<Mutex<Metrics>>,
    Arc<Mutex<Trace>>,
);

fn replica(
    network: &InMemoryNetwork,
    mode: Mode,
    budget: usize,
    profile: FaultProfile,
    spec: ReplicaSpec<'_>,
) -> ReplicaProbe {
    let ReplicaSpec {
        ip,
        epoch,
        peer_epoch,
        direction,
        entries,
    } = spec;
    let port = 24_100;
    let link = Link::at(Rtt::from_millis(10.0)).with_loss(Probability::percent(0.0));
    let raw = Arc::new(network.bind(SocketAddr::new(ip, port)));
    let inner = Arc::new(NetemTransport::new(
        Arc::clone(&raw),
        Netem::uniform(link, Seed::new(0x6e65_7465_6d00 + u64::from(direction))),
    ));
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let trace = Arc::new(Mutex::new(Trace::default()));
    let shared = Arc::new(Shared {
        raw,
        inner,
        profile,
        direction,
        attempts: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        metrics: Arc::clone(&metrics),
        trace: Arc::clone(&trace),
    });
    let transport = ReplayTransport {
        shared,
        mode,
        budget,
        epoch,
        peer_epoch,
        next_flight: AtomicU32::new(0),
        seen: Mutex::new(SeenState::default()),
    };
    let persistence = Arc::new(InMemoryPersistence::<u64, u64>::new());
    persistence
        .save(&PersistedState::from(entries.to_vec()))
        .unwrap();
    let config = Config::default()
        .with_port(port)
        .with_listen_addr(ip)
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

pub(super) struct Sample {
    pub(super) elapsed: Duration,
    pub(super) converged: bool,
    pub(super) metrics: Metrics,
    pub(super) drops: HashSet<FaultKey>,
}

pub(super) async fn run_sample(
    fixture: &Fixture,
    mode: Mode,
    budget: usize,
    profile: FaultProfile,
) -> Sample {
    let network = InMemoryNetwork::new();
    let left_ip: IpAddr = "127.9.9.1".parse().unwrap();
    let right_ip: IpAddr = "127.9.9.2".parse().unwrap();
    let left_epoch = 0x4c45_4654_0000_0001;
    let right_epoch = 0x5249_4748_0000_0001;
    let (left, left_metrics, left_trace) = replica(
        &network,
        mode,
        budget,
        profile,
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
        mode,
        budget,
        profile,
        ReplicaSpec {
            ip: right_ip,
            epoch: right_epoch,
            peer_epoch: left_epoch,
            direction: 1,
            entries: &fixture.shared,
        },
    );
    assert_eq!(fixture.shared.len(), 99_900);
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
                "mode={mode:?} scenario={:#x} key={key}",
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
