// Copyright 2026 Developers of the reconcile project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.

//! #37 structural probe: price bounded producer-side lookahead over the real RSOS and shipped
//! fixed-b rank partition. This is not a two-sided classic-RBSR execution model.

use rand::rngs::StdRng;
use rand::SeedableRng;
use std::ops::Bound;

use devkit::protocol_cost::reconcile;
use rbsr::{
    initial_ranges, protocol_round_with_policy, Comparison, Decision, FanOut, FixedFanOut,
    RangeAggregate, RefinementPolicy, SplitStride,
};
use rsos::{FingerprintTreeMap, Rsos};

#[derive(Clone, Copy, Debug)]
enum Layout {
    Scattered,
    Clustered,
}

impl Layout {
    fn label(self) -> &'static str {
        match self {
            Self::Scattered => "scattered",
            Self::Clustered => "clustered",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Verdict {
    Skip,
    Terminal,
    Split,
}

#[derive(Default, Debug)]
struct Stats {
    feedback_barriers: usize,
    sent_ranges: usize,
    useful_ranges: usize,
    skip_waste_ranges: usize,
    terminal_waste_ranges: usize,
    range_payload_bytes: usize,
    useful_bytes: usize,
    skip_waste_bytes: usize,
    terminal_waste_bytes: usize,
    max_flight_payload_bytes: usize,
}

impl Stats {
    fn wasted_ranges(&self) -> usize {
        self.skip_waste_ranges + self.terminal_waste_ranges
    }
    fn wasted_bytes(&self) -> usize {
        self.skip_waste_bytes + self.terminal_waste_bytes
    }
    fn assert_accounting(&self) {
        assert_eq!(self.sent_ranges, self.useful_ranges + self.wasted_ranges());
        assert_eq!(
            self.range_payload_bytes,
            self.useful_bytes + self.wasted_bytes()
        );
    }
}

struct ForceFixedFanOut;

impl RefinementPolicy for ForceFixedFanOut {
    fn decide(&self, comparison: Comparison) -> Decision {
        if comparison.span() <= 1 {
            Decision::Enumerate
        } else {
            Decision::Split(SplitStride::for_fan_out(
                comparison.span(),
                FanOut::NEGENTROPY,
            ))
        }
    }
}

fn store(n: usize, missing: &[u64]) -> FingerprintTreeMap<u64, u64> {
    let mut map = FingerprintTreeMap::new();
    for key in 0..n as u64 {
        if !missing.contains(&key) {
            map.insert(key, key.wrapping_mul(2_654_435_761));
        }
    }
    map
}

fn store_updated(n: usize, changed: &[u64]) -> FingerprintTreeMap<u64, u64> {
    let mut map = FingerprintTreeMap::new();
    for key in 0..n as u64 {
        let value = key.wrapping_mul(2_654_435_761);
        map.insert(
            key,
            if changed.contains(&key) {
                !value
            } else {
                value
            },
        );
    }
    map
}

fn differing_keys(n: usize, d: usize, layout: Layout) -> Vec<u64> {
    match layout {
        Layout::Scattered => (1..=d as u64)
            .map(|i| (n as u64 / (d as u64 + 1)) * i)
            .collect(),
        Layout::Clustered => {
            let start = (n / 2 - d / 2) as u64;
            (start..start + d as u64).collect()
        }
    }
}

fn owned_bound(bound: Bound<&u64>) -> Bound<u64> {
    match bound {
        Bound::Included(key) => Bound::Included(*key),
        Bound::Excluded(key) => Bound::Excluded(*key),
        Bound::Unbounded => Bound::Unbounded,
    }
}

fn bounds(node: &RangeAggregate<u64>) -> (Bound<u64>, Bound<u64>) {
    (
        owned_bound(node.start_bound()),
        owned_bound(node.end_bound()),
    )
}

fn range_bytes(node: &RangeAggregate<u64>, scratch: &mut Vec<u8>) -> usize {
    scratch.clear();
    gossip::bincode::encode(node, scratch)
        .expect("encoding a RangeAggregate into memory cannot fail");
    scratch.len()
}

fn classify<S: Rsos<u64>>(node: &RangeAggregate<u64>, receiver: &S) -> Verdict {
    let (start, end) = bounds(node);
    let remote = receiver.aggregate((start, end));
    if node.aggregate() == &remote {
        return Verdict::Skip;
    }

    let local_size = node.aggregate().size();
    let remote_size = remote.size();
    assert!(
        remote_size <= local_size,
        "probe assumes a full producer and holed/equal-key receiver"
    );
    if remote_size == 0 || local_size <= 1 {
        Verdict::Terminal
    } else {
        Verdict::Split
    }
}

fn children<S: Rsos<u64>>(producer: &S, node: RangeAggregate<u64>) -> Vec<RangeAggregate<u64>> {
    let mut children = Vec::new();
    let mut enumerations = Vec::new();
    let mut rng = StdRng::seed_from_u64(0);
    protocol_round_with_policy(
        producer,
        &ForceFixedFanOut,
        vec![node],
        &mut children,
        &mut enumerations,
        &mut rng,
    );
    children
}

#[allow(clippy::too_many_arguments)]
fn emit_block<S: Rsos<u64>>(
    producer: &S,
    receiver: &S,
    node: RangeAggregate<u64>,
    remaining_lookahead: usize,
    ancestor_verdict: Option<Verdict>,
    stats: &mut Stats,
    next_frontier: &mut Vec<RangeAggregate<u64>>,
    scratch: &mut Vec<u8>,
) {
    let bytes = range_bytes(&node, scratch);
    stats.sent_ranges += 1;
    stats.range_payload_bytes += bytes;

    let verdict = if ancestor_verdict.is_none() {
        Some(classify(&node, receiver))
    } else {
        None
    };
    match ancestor_verdict {
        None => {
            stats.useful_ranges += 1;
            stats.useful_bytes += bytes;
        }
        Some(Verdict::Skip) => {
            stats.skip_waste_ranges += 1;
            stats.skip_waste_bytes += bytes;
        }
        Some(Verdict::Terminal) => {
            stats.terminal_waste_ranges += 1;
            stats.terminal_waste_bytes += bytes;
        }
        Some(Verdict::Split) => unreachable!("split descendants remain useful"),
    }

    if remaining_lookahead == 0 {
        if verdict == Some(Verdict::Split) {
            next_frontier.extend(children(producer, node));
        }
        return;
    }
    if node.aggregate().size() <= 1 {
        return;
    }

    let child_ancestor = match ancestor_verdict {
        Some(v) => Some(v),
        None => match verdict {
            Some(Verdict::Split) => None,
            Some(v) => Some(v),
            None => unreachable!(),
        },
    };
    for child in children(producer, node) {
        emit_block(
            producer,
            receiver,
            child,
            remaining_lookahead - 1,
            child_ancestor,
            stats,
            next_frontier,
            scratch,
        );
    }
}

fn simulate<S: Rsos<u64>>(producer: &S, receiver: &S, lookahead: usize) -> Stats {
    let mut frontier = initial_ranges(producer);
    let mut stats = Stats::default();
    let mut scratch = Vec::new();

    while !frontier.is_empty() {
        stats.feedback_barriers += 1;
        let before = stats.range_payload_bytes;
        let mut next = Vec::new();
        for node in frontier {
            emit_block(
                producer,
                receiver,
                node,
                lookahead,
                None,
                &mut stats,
                &mut next,
                &mut scratch,
            );
        }
        stats.max_flight_payload_bytes = stats
            .max_flight_payload_bytes
            .max(stats.range_payload_bytes - before);
        frontier = next;
    }
    stats.assert_accounting();
    stats
}

fn print_case(
    n: usize,
    d: usize,
    layout: Layout,
    updated: bool,
    producer: &FingerprintTreeMap<u64, u64>,
    receiver: &FingerprintTreeMap<u64, u64>,
) {
    let classic = reconcile(
        producer,
        receiver,
        &FixedFanOut::default(),
        None::<&mut dyn FnMut(u64) -> Vec<usize>>,
        &mut StdRng::seed_from_u64(42),
    );
    let shape = if updated { "updated" } else { layout.label() };
    println!(
        "[streaming-rbsr] case n={n} d={d} {shape}: contextual-real-classic refinement={}B ranges={} msgs={} dgrams={} frags={}",
        classic.refinement_bytes, classic.ranges, classic.messages, classic.datagrams, classic.fragments,
    );

    let baseline = simulate(producer, receiver, 0);
    for lookahead in 0..=2 {
        let stats = simulate(producer, receiver, lookahead);
        assert_eq!(
            stats.useful_bytes, baseline.useful_bytes,
            "lookahead must not change the retrospectively useful producer-side path"
        );
        let wasted_pct = if stats.range_payload_bytes == 0 {
            0.0
        } else {
            100.0 * stats.wasted_bytes() as f64 / stats.range_payload_bytes as f64
        };
        println!(
            "[streaming-rbsr]   producer-lookahead L={lookahead}: feedback-barriers={} ranges={} useful={} skip-waste={} terminal-waste={} range-payload={}B useful={}B wasted={}B ({wasted_pct:.1}%) max-flight-payload={}B",
            stats.feedback_barriers, stats.sent_ranges, stats.useful_ranges,
            stats.skip_waste_ranges, stats.terminal_waste_ranges, stats.range_payload_bytes,
            stats.useful_bytes, stats.wasted_bytes(), stats.max_flight_payload_bytes,
        );
    }
}

#[test]
#[ignore = "research probe: million-entry case and report output; run explicitly in release mode"]
fn streaming_rbsr_lookahead_report() {
    let cases = [
        (1_000_000, 1, Layout::Scattered, false),
        (100_000, 100, Layout::Scattered, false),
        (100_000, 100, Layout::Clustered, false),
        (100_000, 100, Layout::Scattered, true),
    ];

    for (n, d, layout, updated) in cases {
        let changed = differing_keys(n, d, layout);
        let producer = store(n, &[]);
        let receiver = if updated {
            store_updated(n, &changed)
        } else {
            store(n, &changed)
        };
        print_case(n, d, layout, updated, &producer, &receiver);
    }
}
