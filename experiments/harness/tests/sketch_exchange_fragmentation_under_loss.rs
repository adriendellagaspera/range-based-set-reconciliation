// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Issue #7: "a sketch sends fewer datagrams, so it lowers the probability an exchange loses
//! one" is arithmetically the wrong way round — datagrams are not packets, and a 256-cell
//! sketch's single ~12 KiB blob is nine IP fragments at a 1 472 B MTU
//! ([`devkit::protocol_cost::MTU_FRAGMENT_PAYLOAD`], stated here rather than assumed), delivered
//! all-or-nothing. This file measures the completion probability of both exchanges under real
//! injected loss instead of trusting that arithmetic.
//!
//! **No sketch implementation exists in this workspace** (it is #5's job, blocked on this issue).
//! [`sketch_exchange_shape`] is therefore a modeled hypothesis, not a measurement of shipped code:
//! an IBLT cell is priced as `count: i64` (8 B) + a 256-bit key-sum digest (32 B, the width
//! [`rsos::Fingerprint`] already uses) + an 8 B check-sum = 48 B/cell, which reproduces #13's own
//! "256 cells ≈ 12 kB" figure exactly (256 × 48 B = 12 KiB) and its "64 cells" row (64 × 48 B =
//! 3 KiB) — both checked below against the same [`devkit::protocol_cost`] datagram/fragment
//! ceilings the real RBSR side is priced through, so the two are commensurable rather than one
//! measured and one asserted.
//!
//! RBSR's own side is not modeled: [`rbsr_chain_shape`] drives the real, unmodified
//! `devkit::protocol_cost::reconcile` over a real [`FingerprintTreeMap`], the same instrument
//! `benches/protocol.rs`'s `reconciliation_cost` uses.
//!
//! **Fast, always-on (this file's part of the standard `cargo test --workspace` gate):** the
//! sketch-shape arithmetic against the stated sizing, RBSR's real shape at a small scale, and a
//! determinism check on the loss-simulation helper.
//!
//! **`#[ignore]`d, run manually for the headline numbers** (mirrors
//! `aggregate_and_truncation_collision_rates.rs`'s arms A/B): the `n = 10⁶, d = 1` case #7's own
//! table is stated at, and the loss-rate crossover sweep — both Monte Carlo, both requiring a
//! release build to finish in reasonable time:
//! `cargo test --release -p set-reconciliation-experiments --test sketch_exchange_fragmentation_under_loss -- \
//! --ignored --nocapture`.
//!
//! **What "measured, not derived" means here.** The completion probability of an exchange is not
//! computed as `(1 - p)^fragments` — it is drawn from `gossip::netem`'s real seeded loss model
//! ([`exchange_completion_rate`]), one fragment at a time, through the same `NetemTransport` /
//! `Impairments` instrument `benches/system.rs`'s `probe_link` calibrates against. The closed form
//! is what this validates, not what it assumes.

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use devkit::protocol_cost::{reconcile, Cost, MAX_DATAGRAM_PAYLOAD, MTU_FRAGMENT_PAYLOAD};
use gossip::netem::{Link, Netem, NetemTransport, Probability, Seed};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rbsr::{FanOut, FixedFanOut};
use reconcile::{InMemoryNetwork, Transport};
use rsos::FingerprintTreeMap;
use tokio::runtime::Runtime;

/// One IBLT cell's assumed wire size — see the module doc for the derivation. A free parameter of
/// the *model*, not a width this workspace ships; #8 owns whether a cell can even be resolved once
/// peeled.
const SKETCH_CELL_BYTES: usize = 48;

/// The two capacities #7's and #13's own tables are stated at.
const SKETCH_CAPACITIES: &[usize] = &[64, 256];

/// The miss list and the update batch — #13's "offer, miss, updates" three-message shape, both
/// assumed to fit one fragment: at `d` within capacity neither carries more than a handful of
/// 32-byte digests.
const SKETCH_SMALL_MESSAGE_BYTES: usize = 64;

/// `(messages, datagrams, fragments)` for one sketch exchange at `capacity` cells, priced through
/// the same ceilings [`devkit::protocol_cost::reconcile`] prices RBSR's refinement traffic
/// through.
fn sketch_exchange_shape(capacity: usize) -> (usize, usize, usize) {
    let offer_bytes = capacity * SKETCH_CELL_BYTES;
    let small = SKETCH_SMALL_MESSAGE_BYTES;
    let datagrams_of = |bytes: usize| bytes.div_ceil(MAX_DATAGRAM_PAYLOAD).max(1);
    let fragments_of = |bytes: usize| bytes.div_ceil(MTU_FRAGMENT_PAYLOAD).max(1);
    let messages = 3;
    let datagrams = datagrams_of(offer_bytes) + datagrams_of(small) + datagrams_of(small);
    let fragments = fragments_of(offer_bytes) + fragments_of(small) + fragments_of(small);
    (messages, datagrams, fragments)
}

/// A store of `n` sequential keys, `missing` withheld — [`store_of`] rather than importing
/// `benches/protocol.rs`'s private helper of the same shape, since a bench binary's items are not
/// visible to a test binary.
fn store_of(n: usize, missing: &[u64]) -> FingerprintTreeMap<u64, u64> {
    let mut map = FingerprintTreeMap::new();
    for key in 0..n as u64 {
        if !missing.contains(&key) {
            map.insert(key, key);
        }
    }
    map
}

/// RBSR's real, measured `(messages, datagrams, fragments, refinement_bytes)` reconciling `n`
/// sequential keys against the same set missing one key at `n / 2`, under the shipped default
/// policy (`FixedFanOut(b = 16)`) — #13's and #7's own operating point.
fn rbsr_chain_shape(n: usize) -> Cost {
    let full = store_of(n, &[]);
    let holed = store_of(n, &[n as u64 / 2]);
    let policy = FixedFanOut::new(FanOut::NEGENTROPY);
    reconcile(
        &full,
        &holed,
        &policy,
        None::<&mut dyn FnMut(u64) -> Vec<usize>>,
        &mut StdRng::seed_from_u64(42),
    )
}

/// Two fixed loopback endpoints. Each caller builds its own [`InMemoryNetwork`], which isolates
/// its own address space, so a fixed pair never collides across calls.
fn endpoints(port: u16) -> (SocketAddr, SocketAddr) {
    (
        SocketAddr::new("127.9.9.1".parse().unwrap(), port),
        SocketAddr::new("127.9.9.2".parse().unwrap(), port),
    )
}

/// The fraction of `trials` in which every one of `total_fragments` independent per-fragment sends
/// survived `link` — the empirical completion probability of an exchange whose IP-level footprint
/// is `total_fragments` fragments, drawn one at a time from `gossip::netem`'s real seeded loss
/// model (mirrors `benches/system.rs`'s `probe_link`) rather than composed as `(1 - p)^k`.
async fn exchange_completion_rate(
    link: Link,
    total_fragments: usize,
    trials: u64,
    port: u16,
) -> f64 {
    let network = InMemoryNetwork::new();
    let (src, dst) = endpoints(port);
    let sender = NetemTransport::new(
        Arc::new(network.bind(src)),
        Netem::uniform(link, Seed::DEFAULT),
    );
    let receiver = network.bind(dst);
    let impairments = sender.impairments();
    let mut buf = [0u8; 8];
    let mut completed = 0u64;
    for trial in 0..trials {
        let mut all_survived = true;
        for fragment in 0..total_fragments as u64 {
            let dropped_before = impairments.dropped();
            let payload = (trial.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ fragment).to_le_bytes();
            sender
                .send_to(&payload, &dst)
                .await
                .expect("an in-memory send cannot fail");
            if impairments.dropped() != dropped_before {
                all_survived = false;
            } else {
                receiver
                    .recv_from(&mut buf)
                    .await
                    .expect("a datagram the model did not drop is always delivered");
            }
        }
        if all_survived {
            completed += 1;
        }
    }
    completed as f64 / trials as f64
}

/// The arithmetic behind #7's and #13's tables, checked against the same ceilings the real RBSR
/// side is priced through — not a magic literal, [`sketch_exchange_shape`]'s formula run at the
/// two stated capacities.
#[test]
fn modeled_sketch_shape_matches_the_stated_12kb_and_3kb_sizing() {
    assert_eq!(
        sketch_exchange_shape(256),
        (3, 3, 11),
        "256 cells x 48 B/cell = 12 KiB: one 9-fragment offer plus two 1-fragment small messages"
    );
    assert_eq!(
        sketch_exchange_shape(64),
        (3, 3, 5),
        "64 cells x 48 B/cell = 3 KiB: one 3-fragment offer plus two 1-fragment small messages"
    );
}

/// At a scale two orders of magnitude below #7's headline `n`, RBSR's real refinement batches must
/// still stay under [`MTU_FRAGMENT_PAYLOAD`] every round, so `datagrams == fragments`. A future
/// change that grew a round past that ceiling — moving this file's ranking — would flip this
/// first, fast and in the standard gate rather than only in the `#[ignore]`d headline case.
#[test]
fn rbsr_real_chain_at_small_scale_is_never_fragmented() {
    let cost = rbsr_chain_shape(10_000);
    assert!(
        cost.messages > 0,
        "a holed store must exchange at least one message"
    );
    assert_eq!(
        cost.datagrams, cost.fragments,
        "n=10_000 refinement rounds are expected to stay under the {MTU_FRAGMENT_PAYLOAD} B \
         fragment ceiling; a widened round here changes the headline ranking too"
    );
}

/// `Netem::uniform` seeds each directed link from the fixed [`Seed::DEFAULT`] mixed with the
/// (local, destination) socket pair (`gossip::netem` module docs), so two runs against the same
/// fixed [`endpoints`] must replay to the same completion rate — `.claude/rules/tests.md`'s
/// determinism requirement, checked rather than assumed.
#[test]
fn exchange_completion_rate_is_deterministic_given_the_same_endpoints() {
    let rt = Runtime::new().unwrap();
    let link = Link::PERFECT.with_loss(Probability::percent(2.0));
    let a = rt.block_on(exchange_completion_rate(link, 11, 300, 19_101));
    let b = rt.block_on(exchange_completion_rate(link, 11, 300, 19_101));
    assert_eq!(
        a, b,
        "same link, same fragment count, same trial count, same port -> byte-identical draws"
    );
}

/// The headline case: #7's and #13's own `n = 10⁶, d = 1, scattered` operating point, measured
/// rather than quoted, ranked against both sketch capacities at #13's own calibration loss rates
/// (`benches/README.md`'s "Results: loss, at rtt=1ms"), then swept finely to find the loss rate at
/// which the 256-cell sketch first completes less often than the chain it would replace.
///
/// Cluster-adjacent, not cluster-scale: a `10⁶`-entry `FingerprintTreeMap` plus tens of thousands
/// of in-memory sends. Run with `--release` or budget minutes rather than seconds.
#[test]
#[ignore = "10^6-entry store + Monte Carlo loss sweep; run manually with --release --nocapture"]
fn headline_case_ranks_the_sketch_against_rbsr_under_loss() {
    const N: usize = 1_000_000;
    const TRIALS: u64 = 4_000;

    let rbsr = rbsr_chain_shape(N);
    println!(
        "[sketch-vs-rbsr] n={N} d=1 scattered, FixedFanOut(b=16): {msgs} messages, {dg} \
         datagrams, {fr} fragments, {bytes} B refinement (devkit::protocol_cost::reconcile, \
         measured not quoted; MTU assumed = {MTU_FRAGMENT_PAYLOAD} B)",
        msgs = rbsr.messages,
        dg = rbsr.datagrams,
        fr = rbsr.fragments,
        bytes = rbsr.refinement_bytes,
    );

    let rt = Runtime::new().unwrap();
    let mut port = 20_000u16;
    let mut rate_at = |fragments: usize, loss_percent: f64| -> f64 {
        port += 1;
        let link = Link::PERFECT.with_loss(Probability::percent(loss_percent));
        rt.block_on(exchange_completion_rate(link, fragments, TRIALS, port))
    };

    for &loss in &[0.1_f64, 1.0] {
        let rbsr_rate = rate_at(rbsr.fragments, loss);
        println!("[sketch-vs-rbsr] loss={loss}% RBSR P(complete)={rbsr_rate:.4} ({TRIALS} trials)");
        for &cap in SKETCH_CAPACITIES {
            let (_, _, fragments) = sketch_exchange_shape(cap);
            let sketch_rate = rate_at(fragments, loss);
            let verdict = if sketch_rate >= rbsr_rate {
                "sketch >= RBSR"
            } else {
                "sketch < RBSR -- worse"
            };
            println!(
                "[sketch-vs-rbsr] loss={loss}% sketch(cap={cap}, {fragments} frags) \
                 P(complete)={sketch_rate:.4} ({verdict})"
            );
        }
    }

    println!("[sketch-vs-rbsr] crossover sweep, sketch(cap=256) vs the real RBSR chain:");
    let (_, _, sketch_256_fragments) = sketch_exchange_shape(256);
    let mut crossover = None;
    let mut loss_percent = 0.1_f64;
    while loss_percent <= 3.0 {
        let rbsr_rate = rate_at(rbsr.fragments, loss_percent);
        let sketch_rate = rate_at(sketch_256_fragments, loss_percent);
        println!(
            "[sketch-vs-rbsr]   loss={loss_percent:.2}% RBSR={rbsr_rate:.4} \
             sketch256={sketch_rate:.4} delta={:+.4}",
            sketch_rate - rbsr_rate,
        );
        if crossover.is_none() && sketch_rate < rbsr_rate {
            crossover = Some(loss_percent);
        }
        loss_percent += 0.2;
    }
    match crossover {
        Some(p) => println!(
            "[sketch-vs-rbsr] crossover: sketch(256) first completes less often than RBSR at \
             loss~={p:.2}%"
        ),
        None => println!(
            "[sketch-vs-rbsr] crossover: not reached by loss=3.0% -- sketch(256) completed at \
             least as often as RBSR across the whole sweep"
        ),
    }
}
