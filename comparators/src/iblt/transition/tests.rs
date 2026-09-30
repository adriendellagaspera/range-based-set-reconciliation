use super::*;

fn record(fingerprint: u64) -> Record {
    Record {
        fingerprint: fingerprint.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        id: fingerprint,
    }
}

fn records(count: u64) -> Vec<Record> {
    let mut records: Vec<_> = (1..=count).map(record).collect();
    records.sort_unstable();
    records
}

fn limits() -> TransitionLimits {
    TransitionLimits {
        max_second_cells: 100_000,
        max_payload_bytes: u64::MAX,
        max_record_visits: u64::MAX,
    }
}

fn plan() -> TransitionPlan {
    TransitionPlan {
        first_cells: 128,
        first_seed: 11,
        second_seed: 29,
        target: SuccessTarget::P99,
        limits: limits(),
    }
}

#[test]
fn a_successful_first_probe_never_builds_m2() {
    let outcome = run_transition(&records(8), &[], plan());
    let ProbeOutcome::FirstDecoded { decoded, resources } = outcome else {
        panic!("small difference should decode in M1");
    };
    assert_eq!(decoded.plus, records(8));
    assert_eq!(resources.attempts, 1);
    assert_eq!(resources.cells, 128);
    assert_eq!(resources.payload_bytes, 128 * CELL_BYTES);
    assert_eq!(resources.record_visits, 8);
}

#[test]
fn a_failed_probe_builds_one_fresh_m2_and_decodes_it_alone() {
    let input = records(220);
    let outcome = run_transition(&input, &[], plan());
    let ProbeOutcome::SecondDecoded {
        decoded,
        first_estimate,
        second_cells,
        resources,
    } = outcome
    else {
        panic!("calibrated M2 should decode this fixed instance");
    };
    assert_eq!(decoded.plus, input);
    assert!(first_estimate > 0.0);
    assert_eq!(second_cells, (first_estimate * 2.14).ceil() as usize);
    assert_eq!(resources.attempts, 2);
    assert_eq!(resources.cells, 128 + second_cells as u64);
    assert_eq!(resources.payload_bytes, resources.cells * CELL_BYTES);
    assert_eq!(resources.record_visits, 440);
}

#[test]
fn the_second_seed_must_be_fresh_and_nonzero() {
    let input = records(220);
    for second_seed in [0, plan().first_seed] {
        let outcome = run_transition(
            &input,
            &[],
            TransitionPlan {
                second_seed,
                ..plan()
            },
        );
        assert!(matches!(
            outcome,
            ProbeOutcome::Fallback {
                reason: FallbackReason::NonIndependentSeed,
                resources: ResourceUse { attempts: 1, .. },
                ..
            }
        ));
    }
}

#[test]
fn uncalibrated_first_capacity_is_not_interpolated() {
    let outcome = run_transition(
        &records(220),
        &[],
        TransitionPlan {
            first_cells: 129,
            ..plan()
        },
    );
    assert!(matches!(
        outcome,
        ProbeOutcome::Fallback {
            reason: FallbackReason::UncalibratedFirstCapacity,
            resources: ResourceUse { attempts: 1, .. },
            ..
        }
    ));
}

#[test]
fn every_cap_is_checked_before_m2_work_is_paid() {
    let input = records(220);
    let cases = [
        (
            TransitionLimits {
                max_second_cells: 4,
                ..limits()
            },
            FallbackReason::SecondCellCap,
        ),
        (
            TransitionLimits {
                max_payload_bytes: 128 * CELL_BYTES,
                ..limits()
            },
            FallbackReason::PayloadCap,
        ),
        (
            TransitionLimits {
                max_record_visits: 220,
                ..limits()
            },
            FallbackReason::RecordVisitCap,
        ),
    ];
    for (limits, reason) in cases {
        let outcome = run_transition(&input, &[], TransitionPlan { limits, ..plan() });
        assert!(matches!(
            outcome,
            ProbeOutcome::Fallback {
                reason: actual,
                resources: ResourceUse { attempts: 1, .. },
                ..
            } if actual == reason
        ));
    }
}

#[test]
fn a_second_decode_failure_returns_fallback_without_partial_results() {
    let outcome = run_with_multiplier(&records(220), &[], plan(), Some(10));
    assert!(matches!(
        outcome,
        ProbeOutcome::Fallback {
            reason: FallbackReason::SecondDecodeFailed,
            resources: ResourceUse { attempts: 2, .. },
            ..
        }
    ));
}

#[test]
fn capacities_128_and_184_have_distinct_non_interpolated_calibrations() {
    let targets = [SuccessTarget::P95, SuccessTarget::P99, SuccessTarget::P999];
    let at_128: Vec<_> = targets
        .into_iter()
        .map(|target| calibrated_multiplier(TransitionPlan { target, ..plan() }))
        .collect();
    let at_184: Vec<_> = targets
        .into_iter()
        .map(|target| {
            calibrated_multiplier(TransitionPlan {
                first_cells: 184,
                target,
                ..plan()
            })
        })
        .collect();
    assert_eq!(at_128, [Some(195), Some(214), Some(238)]);
    assert_eq!(at_184, [Some(187), Some(202), Some(221)]);
}
