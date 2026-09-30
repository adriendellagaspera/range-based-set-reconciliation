// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! One-jump self-sizing transition over the pinned plain IBLT reproduction.
//!
//! A failed first decode may size one fresh sketch. It can never establish equality: decoded
//! records remain a proposal for the caller's authoritative root recheck.

use super::{DecodeResult, Record, Sketch};

/// Reconcile-rs-facing cell price established by #7/#8.
pub const CELL_BYTES: u64 = 48;

/// Requested lower-tail protection for sizing the fresh second sketch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuccessTarget {
    /// 95% estimator coverage target.
    P95,
    /// 99% estimator coverage target.
    P99,
    /// 99.9% estimator coverage target.
    P999,
}

/// Hard limits checked before allocating or rebuilding the second sketch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionLimits {
    /// Largest permitted second-sketch cell count.
    pub max_second_cells: usize,
    /// Largest cumulative serialized cell payload across both attempts.
    pub max_payload_bytes: u64,
    /// Largest cumulative number of records visited across both peers and attempts.
    pub max_record_visits: u64,
}

/// Cumulative work already paid by a transition outcome.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceUse {
    /// Number of sketches sent or proposed for transmission.
    pub attempts: u8,
    /// Sum of their cell counts.
    pub cells: u64,
    /// `cells * CELL_BYTES`, excluding topology-specific envelopes.
    pub payload_bytes: u64,
    /// Records visited while rebuilding both peers' sketches.
    pub record_visits: u64,
}

/// Why the transition returned control to a safe fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FallbackReason {
    /// The supplied second seed was zero or equal to the first seed.
    NonIndependentSeed,
    /// No measured calibration exists for this first-probe capacity.
    UncalibratedFirstCapacity,
    /// The first estimate was non-finite or non-positive.
    InvalidEstimate,
    /// The calibrated second sketch exceeds its cell cap.
    SecondCellCap,
    /// The next attempt would exceed the cumulative payload cap.
    PayloadCap,
    /// Rebuilding both inputs would exceed the cumulative scan-work cap.
    RecordVisitCap,
    /// The fresh second sketch also failed to peel.
    SecondDecodeFailed,
}

/// Outcome of the bounded one-jump experiment.
#[derive(Clone, Debug, PartialEq)]
pub enum ProbeOutcome {
    /// The first sketch peeled; its records still require a root recheck.
    FirstDecoded {
        /// Signed difference proposal.
        decoded: DecodeResult,
        /// Work paid by the first attempt.
        resources: ResourceUse,
    },
    /// A fresh, independently seeded second sketch peeled.
    SecondDecoded {
        /// Signed difference proposal from M2 alone.
        decoded: DecodeResult,
        /// Estimate read from M1 before its failed peel.
        first_estimate: f64,
        /// Calibrated M2 cell count.
        second_cells: usize,
        /// Work paid by M1 and M2.
        resources: ResourceUse,
    },
    /// No result was applied; the caller must continue with RBSR or bulk.
    Fallback {
        /// Limit or decode condition that stopped the transition.
        reason: FallbackReason,
        /// M1 estimate when one was available.
        first_estimate: Option<f64>,
        /// Work actually paid before fallback.
        resources: ResourceUse,
    },
}

/// Fully explicit input to one self-sizing transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionPlan {
    /// First-probe cell count; calibrated values are exactly 128 and 184.
    pub first_cells: usize,
    /// Mapper seed for M1.
    pub first_seed: u64,
    /// Fresh mapper seed for M2.
    pub second_seed: u64,
    /// Conservative sizing target.
    pub target: SuccessTarget,
    /// Limits enforced before each allocation/rebuild.
    pub limits: TransitionLimits,
}

/// Execute M1 and, only after failure, at most one independently seeded M2.
pub fn run_transition(left: &[Record], right: &[Record], plan: TransitionPlan) -> ProbeOutcome {
    run_with_multiplier(left, right, plan, calibrated_multiplier(plan))
}

fn run_with_multiplier(
    left: &[Record],
    right: &[Record],
    plan: TransitionPlan,
    multiplier_hundredths: Option<u64>,
) -> ProbeOutcome {
    if plan.first_cells <= super::HASH_COUNT {
        return fallback(
            FallbackReason::UncalibratedFirstCapacity,
            None,
            ResourceUse::default(),
        );
    }
    let Some(total_records) = left.len().checked_add(right.len()) else {
        return fallback(FallbackReason::RecordVisitCap, None, ResourceUse::default());
    };
    let record_count = match u64::try_from(total_records) {
        Ok(count) => count,
        Err(_) => return fallback(FallbackReason::RecordVisitCap, None, ResourceUse::default()),
    };
    let first = match attempt_use(plan.first_cells, record_count) {
        Some(resources) => resources,
        None => return fallback(FallbackReason::PayloadCap, None, ResourceUse::default()),
    };
    if first.payload_bytes > plan.limits.max_payload_bytes {
        return fallback(FallbackReason::PayloadCap, None, ResourceUse::default());
    }
    if first.record_visits > plan.limits.max_record_visits {
        return fallback(FallbackReason::RecordVisitCap, None, ResourceUse::default());
    }

    let first_decoded = decode_pair(left, right, plan.first_cells, plan.first_seed);
    if first_decoded.success {
        return ProbeOutcome::FirstDecoded {
            decoded: first_decoded,
            resources: first,
        };
    }
    let estimate = first_decoded.estimated_difference;
    if plan.second_seed == 0 || plan.second_seed == plan.first_seed {
        return fallback(FallbackReason::NonIndependentSeed, Some(estimate), first);
    }
    let Some(multiplier) = multiplier_hundredths else {
        return fallback(
            FallbackReason::UncalibratedFirstCapacity,
            Some(estimate),
            first,
        );
    };
    let Some(second_cells) = sized_cells(estimate, multiplier) else {
        return fallback(FallbackReason::InvalidEstimate, Some(estimate), first);
    };
    if second_cells > plan.limits.max_second_cells {
        return fallback(FallbackReason::SecondCellCap, Some(estimate), first);
    }
    let Some(second) = attempt_use(second_cells, record_count) else {
        return fallback(FallbackReason::PayloadCap, Some(estimate), first);
    };
    let Some(total) = combine(first, second) else {
        return fallback(FallbackReason::PayloadCap, Some(estimate), first);
    };
    if total.payload_bytes > plan.limits.max_payload_bytes {
        return fallback(FallbackReason::PayloadCap, Some(estimate), first);
    }
    if total.record_visits > plan.limits.max_record_visits {
        return fallback(FallbackReason::RecordVisitCap, Some(estimate), first);
    }

    let decoded = decode_pair(left, right, second_cells, plan.second_seed);
    if decoded.success {
        ProbeOutcome::SecondDecoded {
            decoded,
            first_estimate: estimate,
            second_cells,
            resources: total,
        }
    } else {
        fallback(FallbackReason::SecondDecodeFailed, Some(estimate), total)
    }
}

fn calibrated_multiplier(plan: TransitionPlan) -> Option<u64> {
    // ceil(100 * 1.56/q), where q is the lower chi-square ratio at this exact M1 and target;
    // 1.56 is the pinned artifact's 1.3 decoder multiplier times its 1.2 engineering margin.
    match (plan.first_cells, plan.target) {
        (128, SuccessTarget::P95) => Some(195),
        (128, SuccessTarget::P99) => Some(214),
        (128, SuccessTarget::P999) => Some(238),
        (184, SuccessTarget::P95) => Some(187),
        (184, SuccessTarget::P99) => Some(202),
        (184, SuccessTarget::P999) => Some(221),
        _ => None,
    }
}

fn sized_cells(estimate: f64, multiplier_hundredths: u64) -> Option<usize> {
    let raw = (estimate * multiplier_hundredths as f64 / 100.0).ceil();
    if !raw.is_finite() || raw <= super::HASH_COUNT as f64 || raw > usize::MAX as f64 {
        return None;
    }
    Some(raw as usize)
}

fn decode_pair(left: &[Record], right: &[Record], cells: usize, seed: u64) -> DecodeResult {
    let mut left_sketch = Sketch::new(cells, seed).expect("transition validates capacity");
    let mut right_sketch = Sketch::new(cells, seed).expect("transition validates capacity");
    for &record in left {
        left_sketch.insert(record);
    }
    for &record in right {
        right_sketch.insert(record);
    }
    left_sketch
        .subtract(&right_sketch)
        .expect("transition constructs identical sketch shapes")
        .decode()
}

fn attempt_use(cells: usize, record_visits: u64) -> Option<ResourceUse> {
    let cells = u64::try_from(cells).ok()?;
    Some(ResourceUse {
        attempts: 1,
        cells,
        payload_bytes: cells.checked_mul(CELL_BYTES)?,
        record_visits,
    })
}

fn combine(left: ResourceUse, right: ResourceUse) -> Option<ResourceUse> {
    Some(ResourceUse {
        attempts: left.attempts.checked_add(right.attempts)?,
        cells: left.cells.checked_add(right.cells)?,
        payload_bytes: left.payload_bytes.checked_add(right.payload_bytes)?,
        record_visits: left.record_visits.checked_add(right.record_visits)?,
    })
}

fn fallback(
    reason: FallbackReason,
    first_estimate: Option<f64>,
    resources: ResourceUse,
) -> ProbeOutcome {
    ProbeOutcome::Fallback {
        reason,
        first_estimate,
        resources,
    }
}

#[cfg(test)]
mod tests;
