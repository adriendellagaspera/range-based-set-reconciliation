// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Offline oracle used to falsify #41 before investing in probe-selection machinery.

use super::Strategy;

/// Measured wall clock of one static strategy in one corpus/network cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservedCost {
    /// Static strategy measured in the cell.
    pub strategy: Strategy,
    /// Complete measured completion time, in seconds.
    pub wall_clock_seconds: f64,
}

/// True static costs for all four v0 strategies in one corpus/network cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CostTableRow {
    /// Classic RBSR total cost.
    pub classic_rbsr_seconds: f64,
    /// Streaming RBSR total cost.
    pub streaming_rbsr_seconds: f64,
    /// Self-sizing IBLT total cost.
    pub self_sizing_iblt_seconds: f64,
    /// Bulk/enumeration total cost.
    pub bulk_seconds: f64,
}

impl CostTableRow {
    fn costs(self) -> [ObservedCost; 4] {
        [
            observed(Strategy::ClassicRbsr, self.classic_rbsr_seconds),
            observed(Strategy::StreamingRbsr, self.streaming_rbsr_seconds),
            observed(Strategy::SelfSizingIblt, self.self_sizing_iblt_seconds),
            observed(Strategy::Bulk, self.bulk_seconds),
        ]
    }
}

fn observed(strategy: Strategy, wall_clock_seconds: f64) -> ObservedCost {
    assert!(wall_clock_seconds.is_finite() && wall_clock_seconds >= 0.0);
    ObservedCost {
        strategy,
        wall_clock_seconds,
    }
}

/// Oracle upper-bound summary across heterogeneous benchmark cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OracleSummary {
    /// Sum of per-cell winners: the perfect-selection lower bound.
    pub adaptive_oracle_seconds: f64,
    /// Best one strategy can do when fixed across every cell.
    pub best_static_seconds: f64,
    /// Strategy attaining `best_static_seconds`.
    pub best_static_strategy: Strategy,
    /// `best_static/adaptive_oracle`; above one is adaptation headroom.
    pub adaptation_headroom: f64,
}

/// Cheapest measured static strategy in one cell.
pub fn oracle_winner(costs: &[ObservedCost]) -> Option<ObservedCost> {
    costs
        .iter()
        .copied()
        .filter(|cost| cost.wall_clock_seconds.is_finite() && cost.wall_clock_seconds >= 0.0)
        .min_by(|a, b| a.wall_clock_seconds.total_cmp(&b.wall_clock_seconds))
}

/// First falsifier for #41: does perfect cross-family selection have meaningful headroom?
pub fn oracle_summary(rows: &[CostTableRow]) -> Option<OracleSummary> {
    if rows.is_empty() {
        return None;
    }
    let mut static_totals = [0.0; 4];
    let mut adaptive_oracle_seconds = 0.0;

    for row in rows {
        let costs = row.costs();
        for (total, cost) in static_totals.iter_mut().zip(costs) {
            *total += cost.wall_clock_seconds;
        }
        adaptive_oracle_seconds += oracle_winner(&costs)?.wall_clock_seconds;
    }

    let (best_index, &best_static_seconds) = static_totals
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.total_cmp(b))?;
    let adaptation_headroom = if adaptive_oracle_seconds == 0.0 {
        1.0
    } else {
        best_static_seconds / adaptive_oracle_seconds
    };
    Some(OracleSummary {
        adaptive_oracle_seconds,
        best_static_seconds,
        best_static_strategy: Strategy::ALL[best_index],
        adaptation_headroom,
    })
}

/// `chosen/per-cell oracle`; one is perfect selection, above one is regret.
pub fn regret(chosen_cost_seconds: f64, static_costs: &[ObservedCost]) -> Option<f64> {
    assert!(chosen_cost_seconds.is_finite() && chosen_cost_seconds >= 0.0);
    let oracle = oracle_winner(static_costs)?;
    if oracle.wall_clock_seconds == 0.0 {
        return Some(if chosen_cost_seconds == 0.0 {
            1.0
        } else {
            f64::INFINITY
        });
    }
    Some(chosen_cost_seconds / oracle.wall_clock_seconds)
}
