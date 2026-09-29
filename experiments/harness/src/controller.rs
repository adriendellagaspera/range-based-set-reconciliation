// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Deterministic selector for #41's progressive reconciliation controller.
//!
//! This module does not predict `d`, divergence shape, or strategy-specific work. Those estimates
//! must come from measured probes such as #36/#37. It prices each available next action in seconds,
//! rejects actions crossing hard resource caps, and chooses the minimum remaining expected cost.
//!
//! The v0 objective is `cpu + bytes/bandwidth + rounds*RTT + retries + P(fail)*fallback`.
//! `loss_exposed_fragments` is the group that must jointly survive one retry-sensitive attempt;
//! reliable-stream actions may set it to zero. Total fragment count remains a separate hard cap.

mod oracle;

pub use oracle::{
    oracle_summary, oracle_winner, regret, CostTableRow, ObservedCost, OracleSummary,
};

/// The four continuation families in #41's v0 comparator.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Strategy {
    /// Classic stop-and-wait `FixedFanOut(16)` RBSR.
    ClassicRbsr,
    /// Bounded speculative/streaming RBSR (#37).
    StreamingRbsr,
    /// Small/self-sizing IBLT, including its priced fallback (#36).
    SelfSizingIblt,
    /// Direct enumeration / bulk transfer.
    Bulk,
}

impl Strategy {
    const ALL: [Self; 4] = [
        Self::ClassicRbsr,
        Self::StreamingRbsr,
        Self::SelfSizingIblt,
        Self::Bulk,
    ];
}

/// Transport telemetry, separate from algorithmic estimates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportState {
    /// Feedback latency, in seconds.
    pub rtt_seconds: f64,
    /// Usable payload bandwidth, in bytes per second.
    pub bandwidth_bytes_per_second: f64,
    /// Independent fragment loss probability in `[0, 1]`.
    pub loss_probability: f64,
}

impl TransportState {
    fn validate(self) {
        assert!(self.rtt_seconds.is_finite() && self.rtt_seconds >= 0.0);
        assert!(
            self.bandwidth_bytes_per_second.is_finite() && self.bandwidth_bytes_per_second > 0.0
        );
        assert!(self.loss_probability.is_finite() && (0.0..=1.0).contains(&self.loss_probability));
    }
}

/// Hard constraints, deliberately not hidden inside weighted penalties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceCaps {
    /// Maximum bytes for one next action.
    pub max_wire_bytes: u64,
    /// Maximum total IP fragments for one next action.
    pub max_fragments: u64,
    /// Maximum temporary memory for one next action.
    pub max_temp_bytes: u64,
    /// Maximum dependent feedback barriers for one next action.
    pub max_dependent_rounds: u32,
    /// Maximum expected retries caused by one loss-sensitive group.
    pub max_expected_retries: f64,
}

impl ResourceCaps {
    fn permits(self, estimate: ActionEstimate, transport: TransportState) -> bool {
        assert!(self.max_expected_retries.is_finite() && self.max_expected_retries >= 0.0);
        estimate.wire_bytes <= self.max_wire_bytes
            && estimate.fragments <= self.max_fragments
            && estimate.temp_bytes <= self.max_temp_bytes
            && estimate.dependent_rounds <= self.max_dependent_rounds
            && expected_retries(estimate.loss_exposed_fragments, transport.loss_probability)
                <= self.max_expected_retries
    }
}

/// One strategy's measured/predicted cost-to-go to the next re-evaluation point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionEstimate {
    /// Candidate continuation family.
    pub strategy: Strategy,
    /// Local work, in seconds.
    pub cpu_seconds: f64,
    /// Total wire bytes.
    pub wire_bytes: u64,
    /// Total IP fragments.
    pub fragments: u64,
    /// Fragments that must jointly survive one retry-sensitive attempt.
    pub loss_exposed_fragments: u64,
    /// Sequential feedback barriers.
    pub dependent_rounds: u32,
    /// Probability the action fails to finish its intended transition.
    pub failure_probability: f64,
    /// Remaining safe-fallback wall clock after that failure, in seconds.
    pub fallback_seconds: f64,
    /// Peak temporary memory.
    pub temp_bytes: u64,
}

impl ActionEstimate {
    /// Zero-cost skeleton for `strategy`, for measured rows built field by field.
    pub fn new(strategy: Strategy) -> Self {
        Self {
            strategy,
            cpu_seconds: 0.0,
            wire_bytes: 0,
            fragments: 0,
            loss_exposed_fragments: 0,
            dependent_rounds: 0,
            failure_probability: 0.0,
            fallback_seconds: 0.0,
            temp_bytes: 0,
        }
    }

    fn validate(self) {
        assert!(self.cpu_seconds.is_finite() && self.cpu_seconds >= 0.0);
        assert!(self.failure_probability.is_finite());
        assert!((0.0..=1.0).contains(&self.failure_probability));
        assert!(self.fallback_seconds.is_finite() && self.fallback_seconds >= 0.0);
        assert!(self.loss_exposed_fragments <= self.fragments);
    }
}

/// Explainable terms of one analytical prediction, all in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostBreakdown {
    /// Local work.
    pub cpu_seconds: f64,
    /// Serialization/transmission time.
    pub transfer_seconds: f64,
    /// Sequential-feedback time.
    pub rtt_seconds: f64,
    /// Conservative retry exposure.
    pub retry_seconds: f64,
    /// Expected safe-fallback cost.
    pub fallback_seconds: f64,
}

impl CostBreakdown {
    /// Sum the explicit v0 objective terms.
    pub fn total_seconds(self) -> f64 {
        self.cpu_seconds
            + self.transfer_seconds
            + self.rtt_seconds
            + self.retry_seconds
            + self.fallback_seconds
    }
}

/// Price one next action under current transport telemetry.
pub fn predicted_cost(estimate: ActionEstimate, transport: TransportState) -> CostBreakdown {
    estimate.validate();
    transport.validate();
    CostBreakdown {
        cpu_seconds: estimate.cpu_seconds,
        transfer_seconds: estimate.wire_bytes as f64 / transport.bandwidth_bytes_per_second,
        rtt_seconds: estimate.dependent_rounds as f64 * transport.rtt_seconds,
        retry_seconds: expected_retries(
            estimate.loss_exposed_fragments,
            transport.loss_probability,
        ) * transport.rtt_seconds,
        fallback_seconds: estimate.failure_probability * estimate.fallback_seconds,
    }
}

fn expected_retries(fragments: u64, loss_probability: f64) -> f64 {
    if fragments == 0 || loss_probability == 0.0 {
        return 0.0;
    }
    if loss_probability == 1.0 {
        return f64::INFINITY;
    }
    let success = (1.0 - loss_probability).powf(fragments as f64);
    (1.0 / success) - 1.0
}

/// Result of one controller re-evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    /// Root aggregate equality is the sole authority for this result.
    InSync,
    /// Execute this action, then feed its observations back into the controller.
    Act {
        /// Chosen family.
        strategy: Strategy,
        /// Predicted wall clock to the next useful state.
        predicted_seconds: f64,
    },
    /// No supplied action stayed within every hard cap.
    NoFeasibleAction,
}

/// Choose the feasible action with the lowest predicted cost-to-go.
///
/// No sketch, prior, estimate, or controller prediction can return [`Decision::InSync`]: callers
/// must supply authoritative root equality explicitly.
pub fn choose_next(
    root_agrees: bool,
    estimates: &[ActionEstimate],
    transport: TransportState,
    caps: ResourceCaps,
) -> Decision {
    if root_agrees {
        return Decision::InSync;
    }
    transport.validate();

    estimates
        .iter()
        .copied()
        .filter(|estimate| {
            estimate.validate();
            caps.permits(*estimate, transport)
        })
        .map(|estimate| {
            (
                estimate.strategy,
                predicted_cost(estimate, transport).total_seconds(),
            )
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map_or(
            Decision::NoFeasibleAction,
            |(strategy, predicted_seconds)| Decision::Act {
                strategy,
                predicted_seconds,
            },
        )
}

#[cfg(test)]
mod tests;
