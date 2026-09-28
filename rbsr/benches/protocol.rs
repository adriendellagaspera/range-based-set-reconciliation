// Copyright 2026 Developers of the reconcile project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option.

//! Intrinsic protocol benchmark for the shipped RBSR implementation.
//!
//! This target deliberately measures algorithmic work only: protocol messages/rounds, advertised
//! ranges, enumeration outcomes, local RSOS queries, and CPU time. Serialization, MTU,
//! fragmentation, RTT, loss, and transport framing belong to comparative research, not to this
//! crate's protocol benchmark.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rbsr::{FanOut, FixedFanOut, RefinementPolicy};
use rsos::FingerprintTreeMap;

mod support;

use support::protocol::{reconcile, Cost, Counting};

const SIZES: &[usize] = &[1_000, 10_000, 100_000, 1_000_000];
const DIFFERENCES: &[(usize, Clustering)] = &[
    (1, Clustering::Scattered),
    (10, Clustering::Scattered),
    (10, Clustering::Clustered),
    (100, Clustering::Scattered),
    (100, Clustering::Clustered),
];
const SESSION_SEED: u64 = 42;

#[derive(Clone, Copy, Debug)]
enum Clustering {
    Scattered,
    Clustered,
}

impl Clustering {
    fn label(self) -> &'static str {
        match self {
            Self::Scattered => "scattered",
            Self::Clustered => "clustered",
        }
    }
}

fn value(key: u64) -> u64 {
    key.wrapping_mul(2_654_435_761)
}

fn differing_keys(n: usize, d: usize, clustering: Clustering) -> Vec<u64> {
    assert!(d < n, "difference size must be smaller than store size");
    match clustering {
        Clustering::Scattered => (1..=d as u64)
            .map(|i| (n as u64 / (d as u64 + 1)) * i)
            .collect(),
        Clustering::Clustered => {
            let start = n / 2 - d / 2;
            (start..start + d).map(|key| key as u64).collect()
        }
    }
}

fn stores(
    n: usize,
    d: usize,
    clustering: Clustering,
) -> (FingerprintTreeMap<u64, u64>, FingerprintTreeMap<u64, u64>) {
    let missing = differing_keys(n, d, clustering);
    let mut left = FingerprintTreeMap::new();
    let mut right = FingerprintTreeMap::new();

    for key in 0..n as u64 {
        let current = value(key);
        left.insert(key, current);
        if !missing.contains(&key) {
            right.insert(key, current);
        }
    }

    (left, right)
}

fn counted_reconcile(
    left: &FingerprintTreeMap<u64, u64>,
    right: &FingerprintTreeMap<u64, u64>,
    policy: &dyn RefinementPolicy,
) -> Cost {
    let (counted_left, counted_right) = (Counting::new(left), Counting::new(right));
    let mut rng = StdRng::seed_from_u64(SESSION_SEED);
    let mut cost = reconcile(&counted_left, &counted_right, policy, &mut rng);
    cost.queries = counted_left.queries() + counted_right.queries();
    cost
}

fn protocol(c: &mut Criterion) {
    let policy = FixedFanOut::new(FanOut::NEGENTROPY);
    let mut group = c.benchmark_group("rbsr_protocol");

    for &n in SIZES {
        for &(d, clustering) in DIFFERENCES {
            if d >= n {
                continue;
            }

            let (left, right) = stores(n, d, clustering);
            let cost = counted_reconcile(&left, &right, &policy);
            println!(
                "[rbsr-protocol] n={n} d={d} layout={} | messages={} ranges={} idlist_ranges={} idlist_elements={} | aggregate={} rank={} select={}",
                clustering.label(),
                cost.messages,
                cost.ranges,
                cost.enumerations,
                cost.enumerated_elements,
                cost.queries.aggregate,
                cost.queries.rank,
                cost.queries.select,
            );

            let id = format!("n={n}/d={d}/{}", clustering.label());
            group.bench_function(BenchmarkId::from_parameter(id), |bencher| {
                bencher.iter(|| {
                    let mut rng = StdRng::seed_from_u64(SESSION_SEED);
                    black_box(reconcile(
                        black_box(&left),
                        black_box(&right),
                        &policy,
                        &mut rng,
                    ))
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, protocol);
criterion_main!(benches);
