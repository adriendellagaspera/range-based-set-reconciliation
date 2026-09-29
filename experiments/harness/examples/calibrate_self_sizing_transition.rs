// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Deterministic, non-CI calibration sweep for #36's exact M1=128/184 policy points.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use set_reconciliation_experiments::iblt::transition::{
    run_transition, FallbackReason, ProbeOutcome, SuccessTarget, TransitionLimits, TransitionPlan,
};
use set_reconciliation_experiments::iblt::Record;
use set_reconciliation_experiments::reporting::wilson_99_ci;

const BASE_SEED: u64 = 0x36_128_184;

#[derive(Clone, Copy, Debug)]
enum Signs {
    Positive,
    Balanced,
    Random,
}

fn main() {
    let trials = std::env::args()
        .nth(1)
        .map(|value| value.parse().expect("trials must be a positive integer"))
        .unwrap_or(100_000_u64);
    assert!(trials > 0);
    println!(
        "first_cells,load,signs,target,trials,first_failures,second_failures,second_failure_rate,wilson99_upper"
    );
    for first_cells in [128, 184] {
        for load in [0.5, 0.8, 1.0, 1.6, 3.0, 8.0] {
            for signs in [Signs::Positive, Signs::Balanced, Signs::Random] {
                for target in [SuccessTarget::P95, SuccessTarget::P99, SuccessTarget::P999] {
                    run_cell(first_cells, load, signs, target, trials);
                }
            }
        }
    }
}

fn run_cell(first_cells: usize, load: f64, signs: Signs, target: SuccessTarget, trials: u64) {
    let difference = (load * first_cells as f64).round() as usize;
    let mut rng = StdRng::seed_from_u64(
        BASE_SEED ^ first_cells as u64 ^ difference as u64 ^ sign_tag(signs) ^ target_tag(target),
    );
    let mut first_failures = 0;
    let mut second_failures = 0;
    for _ in 0..trials {
        let (left, right) = corpus(&mut rng, difference, signs);
        let first_seed = nonzero(rng.gen());
        let mut second_seed = nonzero(rng.gen());
        if second_seed == first_seed {
            second_seed = second_seed.wrapping_add(1).max(1);
        }
        let outcome = run_transition(
            &left,
            &right,
            TransitionPlan {
                first_cells,
                first_seed,
                second_seed,
                target,
                limits: TransitionLimits {
                    max_second_cells: usize::MAX,
                    max_payload_bytes: u64::MAX,
                    max_record_visits: u64::MAX,
                },
            },
        );
        match outcome {
            ProbeOutcome::FirstDecoded { .. } => {}
            ProbeOutcome::SecondDecoded { .. } => first_failures += 1,
            ProbeOutcome::Fallback {
                reason: FallbackReason::SecondDecodeFailed,
                ..
            } => {
                first_failures += 1;
                second_failures += 1;
            }
            other => panic!("unexpected uncapped calibration outcome: {other:?}"),
        }
    }
    let rate = if first_failures == 0 {
        0.0
    } else {
        second_failures as f64 / first_failures as f64
    };
    let upper = if first_failures == 0 {
        0.0
    } else {
        wilson_99_ci(second_failures, first_failures).1
    };
    println!(
        "{first_cells},{load:.1},{signs:?},{target:?},{trials},{first_failures},{second_failures},{rate:.8},{upper:.8}"
    );
}

fn corpus(rng: &mut StdRng, difference: usize, signs: Signs) -> (Vec<Record>, Vec<Record>) {
    let mut left = Vec::with_capacity(difference);
    let mut right = Vec::with_capacity(difference);
    for index in 0..difference {
        let record = Record {
            fingerprint: rng.gen(),
            id: index as u64,
        };
        let negative = match signs {
            Signs::Positive => false,
            Signs::Balanced => index % 2 == 1,
            Signs::Random => rng.gen(),
        };
        if negative {
            right.push(record);
        } else {
            left.push(record);
        }
    }
    (left, right)
}

fn nonzero(value: u64) -> u64 {
    value.max(1)
}

fn sign_tag(signs: Signs) -> u64 {
    match signs {
        Signs::Positive => 1 << 32,
        Signs::Balanced => 2 << 32,
        Signs::Random => 3 << 32,
    }
}

fn target_tag(target: SuccessTarget) -> u64 {
    match target {
        SuccessTarget::P95 => 1 << 48,
        SuccessTarget::P99 => 2 << 48,
        SuccessTarget::P999 => 3 << 48,
    }
}
