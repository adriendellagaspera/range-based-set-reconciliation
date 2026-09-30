// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::*;
use rbsr::{FixedFanOut, SqrtFanOut};
use rsos::{Aggregate, Fingerprint};

/// A Wilson interval always contains the point estimate and stays inside `[0, 1]`.
#[test]
fn wilson_interval_contains_the_point_estimate_and_is_bounded() {
    for (successes, trials) in [(0u64, 10u64), (1, 10), (5, 10), (10, 10), (3, 1_000)] {
        let (lo, hi) = wilson_99_ci(successes, trials);
        let p = successes as f64 / trials as f64;
        assert!(
            lo <= p && p <= hi,
            "successes={successes} trials={trials}: p={p} outside [{lo}, {hi}]"
        );
        assert!((0.0..=1.0).contains(&lo) && (0.0..=1.0).contains(&hi));
        assert!(lo <= hi);
    }
}

/// A wider trial count never widens the interval for the same observed rate.
#[test]
fn a_larger_trial_count_never_widens_the_interval() {
    let (lo_small, hi_small) = wilson_99_ci(50, 100);
    let (lo_large, hi_large) = wilson_99_ci(5_000, 10_000);
    assert!(hi_large - lo_large <= hi_small - lo_small);
}

fn mismatch(local: usize, remote: usize) -> Comparison {
    Comparison::new(
        Aggregate::new(local, Fingerprint([1, 0, 0, 0])),
        Aggregate::new(remote, Fingerprint([2, 0, 0, 0])),
        0,
    )
}

/// Below the threshold, `EnumerateBelow` always enumerates a disagreeing comparison, regardless
/// of what the wrapped policy would otherwise do.
#[test]
fn enumerate_below_always_enumerates_under_its_threshold() {
    let policy = EnumerateBelow {
        threshold: 32,
        inner: SqrtFanOut,
    };
    for span in 1..=32usize {
        assert_eq!(
            policy.decide(mismatch(span, span + 1)),
            Decision::Enumerate,
            "span={span}"
        );
    }
}

/// Above the threshold, `EnumerateBelow` defers to the wrapped policy exactly.
#[test]
fn enumerate_below_defers_to_the_inner_policy_above_its_threshold() {
    let policy = EnumerateBelow {
        threshold: 4,
        inner: FixedFanOut::default(),
    };
    let comparison = mismatch(1_000, 1_000);
    assert_eq!(
        policy.decide(comparison),
        FixedFanOut::default().decide(comparison)
    );
}

/// An agreeing comparison is always skipped, even below the threshold — the driver never needs
/// to enumerate a range both sides already agree on.
#[test]
fn enumerate_below_skips_agreement_even_under_the_threshold() {
    let aggregate = Aggregate::new(1, Fingerprint([9, 9, 9, 9]));
    let agreed = Comparison::new(aggregate, aggregate, 0);
    let policy = EnumerateBelow {
        threshold: 32,
        inner: SqrtFanOut,
    };
    assert_eq!(policy.decide(agreed), Decision::Skip);
}

/// `Observing` counts every comparison exactly once, agreeing or not.
#[test]
fn observing_counts_every_comparison_it_sees() {
    let observing = Observing::new(SqrtFanOut);
    for span in [10usize, 20, 30] {
        observing.decide(mismatch(span, span));
    }
    assert_eq!(observing.comparisons.get(), 3);
}

/// `collision_capable` counts only same-size disagreements, never a size mismatch (which
/// `agrees()` already rejects unconditionally) and never true agreement.
#[test]
fn observing_marks_collision_capable_only_on_same_size_disagreement() {
    let observing = Observing::new(SqrtFanOut);
    observing.decide(mismatch(100, 100)); // same size, disagreeing: capable
    observing.decide(mismatch(100, 200)); // different size: never capable
    let aggregate = Aggregate::new(50, Fingerprint([7, 7, 7, 7]));
    observing.decide(Comparison::new(aggregate, aggregate, 0)); // agreeing: never capable
    assert_eq!(observing.collision_capable.get(), 1);
}

/// The symmetric difference is symmetric, and empty exactly when the two sets are equal.
#[test]
fn symmetric_difference_is_symmetric_and_empty_iff_equal() {
    let a = [1u64, 2, 3];
    let b = [2u64, 3, 4];
    assert_eq!(symmetric_difference(&a, &b), symmetric_difference(&b, &a));
    assert!(symmetric_difference(&a, &a).is_empty());
    assert_eq!(
        symmetric_difference(&a, &b),
        [1u64, 4].into_iter().collect()
    );
}
