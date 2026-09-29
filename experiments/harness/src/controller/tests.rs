use super::*;

fn transport(rtt_seconds: f64) -> TransportState {
    TransportState {
        rtt_seconds,
        bandwidth_bytes_per_second: 1_000_000.0,
        loss_probability: 0.0,
    }
}

fn caps() -> ResourceCaps {
    ResourceCaps {
        max_wire_bytes: 1_000_000,
        max_fragments: 1_000,
        max_temp_bytes: 1_000_000,
        max_dependent_rounds: 32,
        max_expected_retries: 8.0,
    }
}

fn estimate(strategy: Strategy, wire_bytes: u64, rounds: u32) -> ActionEstimate {
    ActionEstimate {
        wire_bytes,
        dependent_rounds: rounds,
        ..ActionEstimate::new(strategy)
    }
}

#[test]
fn only_root_equality_can_conclude_in_sync() {
    assert_eq!(
        choose_next(true, &[], transport(0.1), caps()),
        Decision::InSync
    );
    assert_eq!(
        choose_next(false, &[], transport(0.1), caps()),
        Decision::NoFeasibleAction
    );
}

#[test]
fn low_rtt_favours_low_byte_rbsr_but_high_rtt_can_flip_to_one_shot_iblt() {
    let candidates = [
        estimate(Strategy::ClassicRbsr, 1_000, 6),
        estimate(Strategy::SelfSizingIblt, 30_000, 1),
    ];

    assert!(matches!(
        choose_next(false, &candidates, transport(0.001), caps()),
        Decision::Act {
            strategy: Strategy::ClassicRbsr,
            ..
        }
    ));
    assert!(matches!(
        choose_next(false, &candidates, transport(0.100), caps()),
        Decision::Act {
            strategy: Strategy::SelfSizingIblt,
            ..
        }
    ));
}

#[test]
fn increasing_rtt_never_reduces_a_round_dependent_prediction() {
    let candidate = estimate(Strategy::ClassicRbsr, 10_000, 5);
    let low = predicted_cost(candidate, transport(0.001)).total_seconds();
    let high = predicted_cost(candidate, transport(0.100)).total_seconds();
    assert!(high > low);
}

#[test]
fn slower_bandwidth_never_reduces_predicted_cost() {
    let candidate = estimate(Strategy::Bulk, 100_000, 1);
    let fast = predicted_cost(
        candidate,
        TransportState {
            bandwidth_bytes_per_second: 10_000_000.0,
            ..transport(0.01)
        },
    )
    .total_seconds();
    let slow = predicted_cost(
        candidate,
        TransportState {
            bandwidth_bytes_per_second: 100_000.0,
            ..transport(0.01)
        },
    )
    .total_seconds();
    assert!(slow > fast);
}

#[test]
fn fallback_risk_is_part_of_expected_wall_clock() {
    let safe = estimate(Strategy::ClassicRbsr, 5_000, 2);
    let risky = ActionEstimate {
        failure_probability: 0.5,
        fallback_seconds: 1.0,
        ..estimate(Strategy::SelfSizingIblt, 1_000, 1)
    };

    assert!(matches!(
        choose_next(false, &[safe, risky], transport(0.01), caps()),
        Decision::Act {
            strategy: Strategy::ClassicRbsr,
            ..
        }
    ));
}

#[test]
fn hard_caps_reject_an_oversized_sketch_even_when_its_raw_cost_is_lower() {
    let rbsr = estimate(Strategy::ClassicRbsr, 10_000, 5);
    let iblt = ActionEstimate {
        temp_bytes: 2_000_000,
        ..estimate(Strategy::SelfSizingIblt, 1_000, 1)
    };

    assert!(matches!(
        choose_next(false, &[rbsr, iblt], transport(0.1), caps()),
        Decision::Act {
            strategy: Strategy::ClassicRbsr,
            ..
        }
    ));
}

#[test]
fn fragment_loss_exposure_can_reject_an_otherwise_fast_sketch() {
    let lossy = TransportState {
        loss_probability: 0.05,
        ..transport(0.1)
    };
    let sketch = ActionEstimate {
        fragments: 200,
        loss_exposed_fragments: 200,
        ..estimate(Strategy::SelfSizingIblt, 1_000, 1)
    };
    let bulk = ActionEstimate {
        fragments: 20,
        loss_exposed_fragments: 0,
        ..estimate(Strategy::Bulk, 20_000, 1)
    };

    assert!(matches!(
        choose_next(false, &[sketch, bulk], lossy, caps()),
        Decision::Act {
            strategy: Strategy::Bulk,
            ..
        }
    ));
}

#[test]
fn oracle_and_regret_use_the_best_static_strategy_for_each_cell() {
    let costs = [
        ObservedCost {
            strategy: Strategy::ClassicRbsr,
            wall_clock_seconds: 4.0,
        },
        ObservedCost {
            strategy: Strategy::StreamingRbsr,
            wall_clock_seconds: 2.0,
        },
        ObservedCost {
            strategy: Strategy::Bulk,
            wall_clock_seconds: 3.0,
        },
    ];

    assert_eq!(oracle_winner(&costs), Some(costs[1]));
    assert_eq!(regret(2.2, &costs), Some(1.1));
}

#[test]
fn heterogeneous_cells_show_the_adaptation_headroom_oracle_is_meant_to_measure() {
    let rows = [
        CostTableRow {
            classic_rbsr_seconds: 1.0,
            streaming_rbsr_seconds: 2.0,
            self_sizing_iblt_seconds: 5.0,
            bulk_seconds: 8.0,
        },
        CostTableRow {
            classic_rbsr_seconds: 8.0,
            streaming_rbsr_seconds: 3.0,
            self_sizing_iblt_seconds: 1.0,
            bulk_seconds: 5.0,
        },
        CostTableRow {
            classic_rbsr_seconds: 8.0,
            streaming_rbsr_seconds: 6.0,
            self_sizing_iblt_seconds: 4.0,
            bulk_seconds: 1.0,
        },
    ];

    let summary = oracle_summary(&rows).unwrap();
    assert_eq!(summary.adaptive_oracle_seconds, 3.0);
    assert_eq!(summary.best_static_strategy, Strategy::SelfSizingIblt);
    assert_eq!(summary.best_static_seconds, 10.0);
    assert!((summary.adaptation_headroom - 10.0 / 3.0).abs() < f64::EPSILON);
}
