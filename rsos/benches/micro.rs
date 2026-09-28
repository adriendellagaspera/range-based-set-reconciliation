// Copyright 2026 Developers of the reconcile project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option.

//! Microbenchmarks for the shipped `FingerprintTreeMap` RSOS implementation.

use std::collections::BTreeMap;

use criterion::{
    criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration,
    SamplingMode, Throughput,
};
use rand::{Rng, SeedableRng};
use rsos::FingerprintTreeMap;

fn fingerprint_tree_map_new(c: &mut Criterion) {
    let mut group = c.benchmark_group("FingerprintTreeMap::new");
    group.bench_function("BTreeMap::new()", |b| b.iter(BTreeMap::<u32, u32>::new));
    group.bench_function("FingerprintTreeMap::new()", |b| {
        b.iter(FingerprintTreeMap::<u32, u32>::new)
    });
}

fn fingerprint_tree_map_fill(c: &mut Criterion) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let mut key_values = Vec::new();
    for _ in 0..1_000_000 {
        let key: u32 = rng.gen();
        let value: u32 = rng.gen();
        key_values.push((key, value));
    }
    let key_values = &key_values;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("FingerprintTreeMap::fill");
    group.plot_config(plot_config);
    let mut size = 10;
    while size <= key_values.len() {
        group.throughput(Throughput::Elements(size as u64));
        group.sample_size(10.max(1_000_000 / size).min(100));
        group.sampling_mode(SamplingMode::Linear);
        group.bench_with_input(
            BenchmarkId::new("BTreeMap::fill", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut tree = BTreeMap::<u32, u32>::new();
                    for (k, v) in key_values[..size].iter().copied() {
                        tree.insert(k, v);
                    }
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("FingerprintTreeMap::fill", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut tree = FingerprintTreeMap::<u32, u32>::new();
                    for (k, v) in key_values[..size].iter().copied() {
                        tree.insert(k, v);
                    }
                })
            },
        );
        size *= 10;
    }
}

fn fingerprint_tree_map_insert(c: &mut Criterion) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let mut key_values = Vec::new();
    for _ in 0..1_000_000 {
        let key: u32 = rng.gen();
        let value: u32 = rng.gen();
        key_values.push((key, value));
    }
    let key_values = &key_values;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("FingerprintTreeMap::insert");
    group.plot_config(plot_config);
    let mut size = 10;
    while size <= key_values.len() {
        group.throughput(Throughput::Elements(size as u64));
        group.sample_size(10.max(1_000_000 / size).min(100));
        group.sampling_mode(SamplingMode::Linear);
        group.bench_with_input(
            BenchmarkId::new("BTreeMap::insert", size),
            &size,
            |b, &size| {
                let mut tree = BTreeMap::<u32, u32>::new();
                for (k, v) in key_values[..size].iter().copied() {
                    tree.insert(k, v);
                }
                b.iter(|| {
                    // NOTE: do the insertion first because inserting a just-removed element is
                    // likely easier; do not reuse the same key, since it was just removed during
                    // the last iteration
                    let k = rng.gen();
                    let v = rng.gen();
                    tree.insert(k, v);
                    tree.remove(&k);
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("FingerprintTreeMap::insert", size),
            &size,
            |b, &size| {
                let mut tree = FingerprintTreeMap::<u32, u32>::new();
                for (k, v) in key_values[..size].iter().copied() {
                    tree.insert(k, v);
                }
                b.iter(|| {
                    // NOTE: do the insertion first because inserting a just-removed element is
                    // likely easier; do not reuse the same key, since it was just removed during
                    // the last iteration
                    let k = rng.gen();
                    let v = rng.gen();
                    tree.insert(k, v);
                    tree.remove(&k);
                })
            },
        );
        size *= 10;
    }
}

fn fingerprint_tree_map_remove(c: &mut Criterion) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let mut key_values = Vec::new();
    for _ in 0..1_000_000 {
        let key: u32 = rng.gen();
        let value: u32 = rng.gen();
        key_values.push((key, value));
    }
    let key_values = &key_values;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("FingerprintTreeMap::remove");
    group.plot_config(plot_config);
    let mut size = 10;
    while size <= key_values.len() {
        group.throughput(Throughput::Elements(size as u64));
        group.sample_size(10.max(1_000_000 / size).min(100));
        group.sampling_mode(SamplingMode::Linear);
        group.bench_with_input(
            BenchmarkId::new("BTreeMap::remove", size),
            &size,
            |b, &size| {
                let mut tree = BTreeMap::<u32, u32>::new();
                for (k, v) in key_values[..size].iter().copied() {
                    tree.insert(k, v);
                }
                b.iter(|| {
                    // NOTE: do the removal first because removing a just-inserted element is
                    // likely easier; do not reuse the same key, since it was just reinserted
                    // during the last iteration
                    let idx = rng.gen_range(0..size);
                    let (k, v) = &key_values[idx];
                    tree.remove(k);
                    tree.insert(*k, *v);
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("FingerprintTreeMap::remove", size),
            &size,
            |b, &size| {
                let mut tree = FingerprintTreeMap::<u32, u32>::new();
                for (k, v) in key_values[..size].iter().copied() {
                    tree.insert(k, v);
                }
                b.iter(|| {
                    // NOTE: do the removal first because removing a just-inserted element is
                    // likely easier; do not reuse the same key, since it was just reinserted
                    // during the last iteration
                    let idx = rng.gen_range(0..size);
                    let (k, v) = &key_values[idx];
                    tree.remove(k);
                    tree.insert(*k, *v);
                })
            },
        );
        size *= 10;
    }
}

fn fingerprint_tree_map_range_fingerprint(c: &mut Criterion) {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let mut key_values = Vec::new();
    for _ in 0..1_000_000 {
        let key: u32 = rng.gen();
        let value: u32 = rng.gen();
        key_values.push((key, value));
    }
    let key_values = &key_values;

    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
    let mut group = c.benchmark_group("FingerprintTreeMap::aggregate");
    group.plot_config(plot_config);
    let mut size = 10;
    while size <= key_values.len() {
        group.sample_size(10.max(1_000_000 / size).min(100));
        group.sampling_mode(SamplingMode::Linear);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut tree = FingerprintTreeMap::<u32, u32>::new();
            for (k, v) in key_values[..size].iter().copied() {
                tree.insert(k, v);
            }
            b.iter(|| {
                let k1: u32 = rng.gen();
                let k2: u32 = rng.gen();
                let range = if k1 < k2 { k1..k2 } else { k2..k1 };
                tree.aggregate(range);
            })
        });
        size *= 10;
    }
}


criterion_group!(
    benches,
    fingerprint_tree_map_new,
    fingerprint_tree_map_fill,
    fingerprint_tree_map_insert,
    fingerprint_tree_map_remove,
    fingerprint_tree_map_range_fingerprint,
);
criterion_main!(benches);
