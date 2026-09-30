// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The comparison-map width `τ`, priced on one axis against both threat models (#9).
//!
//! Three series, one `τ` axis:
//!
//! | series | where it comes from |
//! |---|---|
//! | total wire bytes | a real drive, re-priced per `τ` — see *Why one drive prices every `τ`* |
//! | `k`-tree attack cost | `2^(2√τ − 1)`, the formula `wagner_false_convergence.rs`'s `wagner_cost_matches_the_k_tree_formula` pins against measured work |
//! | honest-model margin | `τ − log₂(2C)`, `C` the comparison count the same drive performed |
//!
//! **The narrower map is `(count, Σ mod 2^τ)`, not a truncated digest.** Reduction mod `2^τ` is a
//! group homomorphism, so the summary can be *stored* narrow and still compose; and the count stays
//! exact, which is what folding the count into a truncated digest gives away. [`TruncatedSumStore`]
//! is that map, over the real unreduced 256-bit [`rsos::digest`] sum.
//!
//! **Why one drive prices every `τ`.** Splits are cut by rank, so the ranges an execution compares
//! are a deterministic function of the data — not of the lift. The only way `τ` could move the
//! trace is by turning a disagreement into a false SKIP, whose rate is `2^-τ` and which no run at
//! these widths will see. `the_trace_is_identical_at_every_width` checks that rather than assuming
//! it. So `ranges` and `C` are measured once, and only the per-range aggregate width moves.
//!
//! **And the substitution is exact, not modelled.** Since #382 `Fingerprint` serializes through a
//! hand-written `[u8; 32]`, so it costs exactly 32 wire bytes whatever it holds — a narrowed one
//! would cost exactly `τ/8`. `the_fingerprint_encodes_to_a_fixed_width` pins that; without it the
//! byte series would be arithmetic over an assumption.
//!
//! The report is `#[ignore]`d — driving a 10⁶-key store folds a digest per element per level, which
//! is a release-mode job:
//!
//! ```sh
//! cargo test --release --test comparison_map_width_in_both_threat_models -- --ignored --nocapture
//! ```

use rand::rngs::StdRng;
use rand::SeedableRng;
use std::ops::{Bound, RangeBounds};

use rbsr::{initial_ranges, protocol_round, EnumerationRange, RangeAggregate, RsosView};
use rsos::{digest, Aggregate, Fingerprint};

/// The widths #9 puts on the axis. 256 is what ships; 64 is the floor below which the honest-model
/// margin stops being comfortable at any realistic comparison count.
const TAUS: [u32; 6] = [64, 96, 128, 160, 192, 256];

/// What a `Fingerprint` costs on the wire today, from its hand-written `Serialize`
/// (`rsos/src/fingerprint.rs`, #382) — fixed width, so this is a constant rather than an average.
const FINGERPRINT_WIRE_BYTES: usize = 32;

/// `(n, d)` cells the three series are reported over. The first is the operating point #9 argues
/// from: post-#257 it is the case whose bandwidth argument evaporated.
const CELLS: [(usize, usize); 2] = [(1_000_000, 1), (100_000, 100)];

/// `Σ`'s low `tau` bits, zero above them — the narrower additive group, limb by limb.
fn truncate(sum: Fingerprint, tau: u32) -> Fingerprint {
    let mut limbs = [0u64; 4];
    for (index, limb) in limbs.iter_mut().enumerate() {
        let low = 64 * index as u32;
        *limb = match tau.saturating_sub(low) {
            0 => 0,
            bits if bits >= 64 => sum.0[index],
            bits => sum.0[index] & ((1u64 << bits) - 1),
        };
    }
    Fingerprint(limbs)
}

/// A store advertising `(count, Σ mod 2^τ)`: the exact count, and the real 256-bit digest sum
/// reduced into the narrower group. Never a truncated digest — see the module docs.
struct TruncatedSumStore {
    tau: u32,
    keys: Vec<u64>,
}

impl TruncatedSumStore {
    fn new(tau: u32, mut keys: Vec<u64>) -> TruncatedSumStore {
        keys.sort_unstable();
        keys.dedup();
        TruncatedSumStore { tau, keys }
    }

    fn span<R: RangeBounds<u64>>(&self, range: &R) -> (usize, usize) {
        let start = match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(k) => self.keys.partition_point(|x| x < k),
            Bound::Excluded(k) => self.keys.partition_point(|x| x <= k),
        };
        let end = match range.end_bound() {
            Bound::Unbounded => self.keys.len(),
            Bound::Included(k) => self.keys.partition_point(|x| x <= k),
            Bound::Excluded(k) => self.keys.partition_point(|x| x < k),
        };
        (start, end.max(start))
    }

    fn keys_in(&self, range: &EnumerationRange<u64>) -> usize {
        let (start, end) = self.span(range);
        end - start
    }
}

impl RsosView<u64> for TruncatedSumStore {
    fn size(&self) -> usize {
        self.keys.len()
    }

    fn aggregate<R: RangeBounds<u64>>(&self, range: R) -> Aggregate {
        let (start, end) = self.span(&range);
        let slice = &self.keys[start..end];
        let sum = slice
            .iter()
            .fold(Fingerprint::ZERO, |acc, key| acc.combine(digest(key)));
        Aggregate::new(slice.len(), truncate(sum, self.tau))
    }

    fn rank(&self, z: &u64) -> usize {
        self.keys.partition_point(|x| x < z)
    }

    fn select(&self, r: usize) -> &u64 {
        &self.keys[r]
    }
}

/// What one drive cost, at the width it was driven at.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Trace {
    /// One-way messages: how many times a batch of active ranges crossed the wire.
    messages: usize,
    /// Advertised `RangeAggregate`s, which is also the comparison count `C` the union bound is
    /// stated over — every advertised range is classified exactly once by the responder.
    ranges: usize,
    /// Bincode-encoded bytes of those aggregates, through `gossip::bincode`'s own encoder.
    refinement_bytes: usize,
    enumerated_elements: usize,
}

/// Drive a pair to a fixed point, pricing every advertised batch with the real encoder.
///
/// The peers alternate from `b`, since `initial_ranges` came from `a` — `devkit::protocol_cost`'s
/// own convention, mirrored rather than reused because that driver needs an `rsos::Rsos` and this
/// store is key-only.
fn drive(a: &TruncatedSumStore, b: &TruncatedSumStore) -> Trace {
    let mut trace = Trace::default();
    let mut rng = StdRng::seed_from_u64(0);
    let mut active: Vec<RangeAggregate<u64>> = initial_ranges(a);
    let mut responder_is_b = true;
    let mut scratch = Vec::new();

    while !active.is_empty() {
        for segment in &active {
            scratch.clear();
            gossip::bincode::encode(segment, &mut scratch)
                .expect("encoding a RangeAggregate into an in-memory buffer cannot fail");
            trace.refinement_bytes += scratch.len();
        }
        trace.messages += 1;
        trace.ranges += active.len();

        let mut children = Vec::new();
        let mut enumerations: Vec<EnumerationRange<u64>> = Vec::new();
        let responder = if responder_is_b { b } else { a };
        protocol_round(
            responder,
            active,
            &mut children,
            &mut enumerations,
            &mut rng,
        );
        for range in &enumerations {
            trace.enumerated_elements += responder.keys_in(range);
        }

        active = children;
        responder_is_b = !responder_is_b;
        assert!(
            trace.messages < 100_000,
            "reconciliation failed to converge — the refinement is not shrinking"
        );
    }
    trace
}

/// `n` sequential keys, and the same minus `d` evenly scattered ones — `benches/protocol.rs`'s
/// `store`/`missing_keys` layout, so the byte figures here are comparable with its tables.
fn cell(n: usize, d: usize) -> (Vec<u64>, Vec<u64>) {
    let full: Vec<u64> = (0..n as u64).collect();
    let missing: Vec<u64> = (1..=d as u64)
        .map(|i| (n as u64 / (d as u64 + 1)) * i)
        .collect();
    let holed = full
        .iter()
        .copied()
        .filter(|key| !missing.contains(key))
        .collect();
    (full, holed)
}

/// `log₂` of the `k`-tree work at width `τ`: `2^(2√τ − 1)`, from the formula
/// `wagner_cost_matches_the_k_tree_formula` pins. Reported as an exponent because the value itself
/// overflows every integer type on this axis.
fn k_tree_log2_cost(tau: u32) -> f64 {
    2.0 * f64::from(tau).sqrt() - 1.0
}

/// The honest model's margin at width `τ` over `comparisons` ranges: a union bound over the
/// comparisons an execution makes leaves `τ − log₂(2C)` bits, not `τ/2`.
fn honest_margin_bits(tau: u32, comparisons: usize) -> f64 {
    f64::from(tau) - (2.0 * comparisons as f64).log2()
}

/// The property the whole byte series rests on: `Fingerprint`'s wire size does not depend on what it
/// holds, so a narrowed one would cost exactly `τ/8` and the per-`τ` substitution is arithmetic over
/// a constant rather than over an average.
///
/// This is the assertion that would catch #382 being reverted to a derived, varint-compressed
/// encoding — under which a random limb and a mostly-zero one cost different numbers of bytes and
/// every figure in the report below would silently become a mean.
#[test]
fn the_fingerprint_encodes_to_a_fixed_width_whatever_it_holds() {
    let bounded = |fingerprint| {
        let mut out = Vec::new();
        let aggregate: RangeAggregate<u64> =
            RangeAggregate::new(Some(7), Some(9_999), Aggregate::new(123, fingerprint));
        gossip::bincode::encode(&aggregate, &mut out).expect("in-memory encoding cannot fail");
        out.len()
    };
    let zero = bounded(Fingerprint::ZERO);
    for tau in TAUS {
        assert_eq!(
            bounded(truncate(Fingerprint([u64::MAX; 4]), tau)),
            zero,
            "τ={tau}: a narrowed fingerprint changed the encoded length, so the width is not fixed"
        );
    }
    assert_eq!(bounded(Fingerprint([u64::MAX; 4])), zero);

    // And the constant the report subtracts is the one the encoder actually charges.
    let mut with = Vec::new();
    gossip::bincode::encode(&Aggregate::new(123, Fingerprint::ZERO), &mut with)
        .expect("in-memory encoding cannot fail");
    let mut without = Vec::new();
    gossip::bincode::encode(&123usize, &mut without).expect("in-memory encoding cannot fail");
    assert_eq!(with.len() - without.len(), FINGERPRINT_WIRE_BYTES);
}

/// Reducing mod `2^τ` is a group homomorphism, which is what lets the summary be *stored* narrow
/// rather than merely compared narrow. Asserted over the real combiner: truncating the parts and
/// combining must equal combining and truncating.
///
/// Without this the narrow map would only be a comparison map, and #9's "summaries can also be
/// stored narrow" — the whole reason it is a cheaper option than a truncated digest — would not
/// hold.
#[test]
fn truncation_commutes_with_the_combiner() {
    for tau in TAUS {
        for seed in 0..64u64 {
            let (left, right) = (digest(&(seed * 2)), digest(&(seed * 2 + 1)));
            assert_eq!(
                truncate(left.combine(right), tau),
                truncate(truncate(left, tau).combine(truncate(right, tau)), tau),
                "τ={tau}, seed={seed}"
            );
        }
    }
}

/// Count exactness survives narrowing, which is the guarantee a truncated digest gives away: a
/// range whose peers hold different cardinalities is refused at every width, so no `τ` can turn it
/// into a SKIP.
#[test]
fn an_unbalanced_range_is_never_skipped_at_any_width() {
    let full: Vec<u64> = (0..512u64).collect();
    let holed: Vec<u64> = full.iter().copied().filter(|&k| k != 301).collect();
    for tau in TAUS {
        let a = TruncatedSumStore::new(tau, full.clone());
        let b = TruncatedSumStore::new(tau, holed.clone());
        let active: Vec<RangeAggregate<u64>> = initial_ranges(&a);
        let mut children = Vec::new();
        let mut enumerations = Vec::new();
        let outcome = protocol_round(
            &b,
            active,
            &mut children,
            &mut enumerations,
            &mut StdRng::seed_from_u64(0),
        );
        assert_eq!(
            outcome.skipped(),
            0,
            "τ={tau}: an unbalanced range was skipped"
        );
    }
}

/// One drive prices every `τ`, checked rather than assumed: a rank-cut policy compares the same
/// ranges at every width, so the trace is width-independent up to a collision the report's widths
/// will never see.
#[test]
fn the_trace_is_identical_at_every_width() {
    let (full, holed) = cell(4_096, 3);
    let reference = drive(
        &TruncatedSumStore::new(TAUS[0], full.clone()),
        &TruncatedSumStore::new(TAUS[0], holed.clone()),
    );
    assert!(reference.ranges > 0 && reference.enumerated_elements >= 3);
    for tau in TAUS {
        let trace = drive(
            &TruncatedSumStore::new(tau, full.clone()),
            &TruncatedSumStore::new(tau, holed.clone()),
        );
        assert_eq!(trace, reference, "τ={tau}: the width moved the trace");
    }
}

/// The three series on one `τ` axis (#9). Printed, not asserted: the numbers are the finding.
#[test]
#[ignore = "drives 10⁶-key stores; release-mode job, see the module docs"]
fn the_three_series_on_one_tau_axis() {
    println!(
        "[tau] comparison-map width priced in both threat models. Bytes are a real drive re-priced \
         per τ ({FINGERPRINT_WIRE_BYTES} B/range of fingerprint replaced by τ/8); k-tree cost is \
         2^(2√τ − 1); honest margin is τ − log₂(2C)."
    );
    for (n, d) in CELLS {
        let (full, holed) = cell(n, d);
        let trace = drive(
            &TruncatedSumStore::new(256, full),
            &TruncatedSumStore::new(256, holed),
        );
        println!(
            "[tau] n={n} d={d} scattered — {} ranges (C), {} msgs, {} elements, \
             {} B refinement at τ=256",
            trace.ranges, trace.messages, trace.enumerated_elements, trace.refinement_bytes
        );
        let fingerprint_free = trace.refinement_bytes - trace.ranges * FINGERPRINT_WIRE_BYTES;
        let baseline = trace.refinement_bytes as f64;
        for tau in TAUS {
            let bytes = fingerprint_free + trace.ranges * (tau as usize / 8);
            println!(
                "[tau]   τ={tau:<4} refinement {bytes:>9} B  {:>5.2}x of τ=256   \
                 k-tree 2^{:<5.1}   honest margin {:>6.1} bits",
                bytes as f64 / baseline,
                k_tree_log2_cost(tau),
                honest_margin_bits(tau, trace.ranges),
            );
        }
    }
}
