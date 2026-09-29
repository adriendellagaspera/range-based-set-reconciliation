// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Formula-pinning unit tests for [`super::FingerprintDerivedSplit`] and
//! [`super::SpanRelativeFingerprintSplit`], adapted once `local_for_testing`
//! (Akvize/reconcile-rs#530) made the two policies buildable outside `rbsr`, plus the
//! property tests for [`super::CountDeltaFanOut`] (#12). Constructs `Comparison`s directly via
//! the public `Comparison::new`, exercising no driver.

use super::*;

use rbsr::{ConstantStrideSplit, FanOut, FixedFanOut, SpanHashedStrideSplit, STRIDE_SPREAD};
use rsos::{Aggregate, Fingerprint};

/// Two aggregates of the given sizes guaranteed not to agree, the local one carrying `limb`.
fn probe_mismatch(local: usize, remote: usize, limb: u64) -> Comparison {
    Comparison::new(
        Aggregate::new(local, Fingerprint([limb, 0, 0, 0])),
        Aggregate::new(remote, Fingerprint([u64::MAX, 9, 9, 9])),
        0,
    )
}

fn stride_of<P: RefinementPolicy + ?Sized>(policy: &P, comparison: Comparison) -> usize {
    let Decision::Split(stride) = policy.decide(comparison) else {
        panic!("span={} must split", comparison.span());
    };
    stride.get()
}

/// Pins [`FingerprintDerivedSplit`]'s exact stride formula (`1 + limb % 32`) against known
/// fingerprint limbs, not just "differs from a rank-cut policy somewhere" — the mutation gate
/// (`AGENTS.md` `.claude/rules/tests.md`) needs a witness for `+`, not `*` or another operator
/// combining the `1`, and for `%`, not `/`, dividing by `32`.
#[test]
fn fingerprint_derived_stride_is_one_plus_limb_mod_32() {
    // 0 and 32 both reduce to remainder 0 (stride 1, pinning `%` over `/`, which would give 0
    // and 1); 31 and 63 both reduce to 31 (stride 32, pinning `+` over `*`, which would give
    // 0 and 0).
    for (limb, expected) in [(0u64, 1usize), (31, 32), (32, 1), (63, 32)] {
        assert_eq!(
            stride_of(&FingerprintDerivedSplit, probe_mismatch(1_000, 2_000, limb)),
            expected,
            "limb {limb}"
        );
    }
}

/// The defect [`ConstantStrideSplit`] is the control for, stated as a property rather than
/// asserted of one span: a span-independent stride stops refining once the span reaches it.
#[test]
fn span_independent_strides_stop_refining_below_their_own_spread() {
    for span in 2..=STRIDE_SPREAD as usize {
        // A constant at the top of the spread never refines anywhere in this window.
        assert!(
            stride_of(
                &ConstantStrideSplit::per_child(STRIDE_SPREAD as usize),
                probe_mismatch(span, span + 1, 7)
            ) >= span,
            "span={span}: a constant stride of {STRIDE_SPREAD} must not refine it"
        );
    }
    // Both span-independent probes put *some* limb/span in the no-progress region, which is
    // what a span-relative stride makes impossible.
    assert!(
        (0..64u64).any(|limb| stride_of(&FingerprintDerivedSplit, probe_mismatch(4, 5, limb)) >= 4)
    );
    assert!((2..=STRIDE_SPREAD as usize)
        .any(|span| stride_of(&SpanHashedStrideSplit, probe_mismatch(span, span + 1, 7)) >= span));
}

/// Pins [`SpanRelativeFingerprintSplit`]'s exact formula, `1 + limb % (span − 1)`.
///
/// `span_relative_fingerprint_stride_always_refines` below cannot do this on its own: dropping
/// the `+ 1` leaves the stride inside `1..span` too, because `SplitStride::per_child` raises a
/// zero stride to one. Distinguishing the two needs a witness where the remainder is non-zero,
/// so the `+ 1` is observable rather than absorbed by that clamp.
#[test]
fn span_relative_fingerprint_stride_is_one_plus_limb_mod_span_minus_one() {
    // (span, limb, expected): remainder 0 pins that the clamp is not what produces the 1;
    // remainders 5 and 98 pin `+` over `*` and over `%`, each of which would drop the offset.
    for (span, limb, expected) in [
        (100usize, 99u64, 1usize),
        (100, 5, 6),
        (100, 98, 99),
        (3, 1, 2),
    ] {
        assert_eq!(
            stride_of(
                &SpanRelativeFingerprintSplit,
                probe_mismatch(span, span + 1, limb)
            ),
            expected,
            "span={span}, limb={limb}"
        );
    }
}

/// The joint-progress property, as a property over the whole reachable input space rather
/// than a literal: a span-relative stride always cuts at least two children.
#[test]
fn span_relative_fingerprint_stride_always_refines() {
    for span in 2..512usize {
        for limb in [0u64, 1, 7, 31, 32, 1_000_003, u64::MAX / 3, u64::MAX] {
            let stride = stride_of(
                &SpanRelativeFingerprintSplit,
                probe_mismatch(span, span + 1, limb),
            );
            assert!(
                (1..span).contains(&stride),
                "span={span}, limb={limb}: stride {stride} is outside 1..{span}"
            );
            assert!(span.div_ceil(stride) >= 2, "span={span}, limb={limb}");
        }
    }
}

/// The oracle-coupled column must actually read the oracle, or it is not testing what it
/// claims; the oracle-independent column must actually ignore it, same reason.
#[test]
fn only_the_oracle_coupled_column_reacts_to_the_fingerprint() {
    let quiet = probe_mismatch(100, 200, 0);
    let loud = probe_mismatch(100, 200, 17);
    assert_ne!(
        stride_of(&FingerprintDerivedSplit, quiet),
        stride_of(&FingerprintDerivedSplit, loud)
    );
    assert_ne!(
        stride_of(&SpanRelativeFingerprintSplit, quiet),
        stride_of(&SpanRelativeFingerprintSplit, loud)
    );
    assert_eq!(
        stride_of(&SpanHashedStrideSplit, quiet),
        stride_of(&SpanHashedStrideSplit, loud)
    );
    let constant = ConstantStrideSplit::per_child(7);
    assert_eq!(stride_of(&constant, quiet), stride_of(&constant, loud));
    assert_eq!(constant.stride().get(), 7);
}

/// Agreeing aggregates skip before either policy reads the fingerprint at all.
#[test]
fn agreeing_aggregates_are_skipped_without_reading_the_oracle() {
    let aggregate = Aggregate::new(1_000, Fingerprint([9, 9, 9, 9]));
    let agreed = Comparison::new(aggregate, aggregate, 0);
    assert_eq!(FingerprintDerivedSplit.decide(agreed), Decision::Skip);
    assert_eq!(SpanRelativeFingerprintSplit.decide(agreed), Decision::Skip);
}

/// The floor/cap pair every [`CountDeltaFanOut`] test below is built from: the shipped default as
/// the floor, and `benches/protocol.rs`'s widest swept fan-out as the cap.
const PROBE_FLOOR: FanOut = FanOut::NEGENTROPY;
const PROBE_CAP: usize = 256;

fn probe_count_delta() -> CountDeltaFanOut {
    CountDeltaFanOut::new(PROBE_FLOOR, FanOut::new(PROBE_CAP))
}

/// The degeneracy #12 requires, stated as an identity against the policy it degrades *to* rather
/// than as a stride literal: on a range whose peers advertise equal counts — the blind spot, and
/// the shape an LWW update to an existing key produces — every decision must be the default's.
///
/// The equality is over `Decision`, not over the stride alone, so a variant divergence (an
/// `Enumerate` where the default splits) fails it too.
#[test]
fn a_zero_count_delta_decides_exactly_as_the_default_it_degrades_to() {
    let adaptive = probe_count_delta();
    let default = FixedFanOut::new(PROBE_FLOOR);
    for span in [2usize, 3, 15, 16, 17, 100, 1_000, 65_536, 1_000_000] {
        for limb in [0u64, 1, 17, u64::MAX] {
            let balanced = probe_mismatch(span, span, limb);
            assert_eq!(
                adaptive.decide(balanced),
                default.decide(balanced),
                "span={span}, limb={limb}"
            );
        }
    }
}

/// The cutoffs are the controls', not this policy's own: on every input `shared_cutoffs` answers
/// for, the adaptive policy and the default must agree whatever the delta is. Without this a
/// widened fan-out could move *when* a range is enumerated, which is the one variable #12 puts out
/// of scope.
#[test]
fn the_shared_cutoffs_are_reached_identically_whatever_the_delta() {
    let adaptive = probe_count_delta();
    let default = FixedFanOut::new(PROBE_FLOOR);
    // (local, remote): peer holds nothing, we hold nothing, both hold one, we hold one.
    for (local, remote) in [(500usize, 0usize), (0, 500), (1, 1), (1, 500)] {
        let comparison = probe_mismatch(local, remote, 3);
        assert_eq!(
            adaptive.decide(comparison),
            default.decide(comparison),
            "local={local}, remote={remote}"
        );
    }
    let aggregate = Aggregate::new(1_000, Fingerprint([9, 9, 9, 9]));
    assert_eq!(
        adaptive.decide(Comparison::new(aggregate, aggregate, 0)),
        Decision::Skip
    );
}

/// The width rule as a property over the whole delta axis rather than at sampled points: never
/// below the floor, never above the cap, and never narrowing as the delta grows.
///
/// Monotonicity is the load-bearing half. A rule that widened and then narrowed again would still
/// satisfy the two bounds while contradicting the signal it claims to read.
#[test]
fn the_fan_out_is_monotone_in_the_delta_and_stays_inside_floor_cap() {
    let adaptive = probe_count_delta();
    let mut previous = adaptive.fan_out_for_delta(0);
    assert_eq!(previous, PROBE_FLOOR);
    for delta in 1..2 * PROBE_CAP {
        let fan_out = adaptive.fan_out_for_delta(delta);
        assert!(fan_out >= previous, "delta={delta} narrowed the fan-out");
        assert!(fan_out.get() >= PROBE_FLOOR.get(), "delta={delta}");
        assert!(fan_out.get() <= PROBE_CAP, "delta={delta}");
        previous = fan_out;
    }
    // Saturated well past the cap, including at the `saturating_add` boundary.
    assert_eq!(adaptive.fan_out_for_delta(usize::MAX).get(), PROBE_CAP);
}

/// That the policy actually *widens* — the realized child count, not the nominal fan-out.
///
/// Stated on children rather than on the stride because the stride is the implementation detail,
/// and stated as monotone-plus-strictly-greater rather than as "at least `delta + 1` children"
/// because that stronger claim is false: `SplitStride::for_fan_out` is `⌈span / b⌉`, and
/// `⌈span / ⌈span / b⌉⌉` falls short of `b` whenever the stride does not divide the span
/// (`⌈100/16⌉ = 7`, so 15 children, not 16 — the loss the primitive's own docs name). A test
/// asserting `delta + 1` children would be asserting a property of a different primitive.
#[test]
fn a_larger_delta_never_narrows_the_split_and_eventually_widens_it() {
    let adaptive = probe_count_delta();
    let default = FixedFanOut::new(PROBE_FLOOR);
    for span in [1_000usize, 100_000, 1_000_000] {
        let children = |policy: &dyn RefinementPolicy, delta: usize| {
            span.div_ceil(stride_of(policy, probe_mismatch(span, span - delta, 3)))
        };
        let baseline = children(&default, 1);
        let mut previous = 0;
        // A delta reaching the span empties the peer's side, which `shared_cutoffs` enumerates
        // before any width is chosen — outside what this property is about.
        for delta in [1usize, 2, 15, 16, 17, 63, 255, 256, 1_000]
            .into_iter()
            .filter(|&d| d < span)
        {
            let emitted = children(&adaptive, delta);
            assert!(
                emitted >= previous,
                "span={span}, delta={delta}: {emitted} children after {previous}"
            );
            assert!(
                emitted >= baseline,
                "span={span}, delta={delta}: {emitted} children, below the default's {baseline}"
            );
            previous = emitted;
        }
        // Below the floor the delta buys nothing; past it, it must actually buy width.
        assert_eq!(children(&adaptive, 1), baseline, "span={span}");
        assert!(children(&adaptive, span / 2) > baseline, "span={span}");
    }
}

/// Width-only, per #12's scope: a disagreeing range is never turned into a SKIP, at any delta and
/// in either direction. A policy that could skip would be deciding "in sync", which is the one
/// authority `(root fingerprint, root size)` keeps.
#[test]
fn a_disagreeing_range_is_never_skipped_at_any_delta() {
    let adaptive = probe_count_delta();
    for span in [2usize, 16, 1_000, 1_000_000] {
        for remote in [0usize, 1, 2, span / 2, span, span * 2, span + 1_000] {
            assert_ne!(
                adaptive.decide(probe_mismatch(span, remote, 11)),
                Decision::Skip,
                "span={span}, remote={remote}"
            );
        }
    }
}

/// The cap is normalized rather than trusted: a cap below the floor would describe a policy that
/// must both fall back to the default and refuse to reach it.
#[test]
fn a_cap_below_the_floor_is_raised_to_it() {
    let degenerate = CountDeltaFanOut::new(PROBE_FLOOR, FanOut::BINARY);
    assert_eq!(degenerate.cap(), PROBE_FLOOR);
    assert_eq!(degenerate.fan_out_for_delta(0), PROBE_FLOOR);
    assert_eq!(degenerate.fan_out_for_delta(1_000), PROBE_FLOOR);
}
