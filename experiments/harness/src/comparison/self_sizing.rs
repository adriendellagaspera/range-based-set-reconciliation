use std::time::Instant;

use super::corpus::Corpus;
use super::matrix::TRIALS;
use super::model::{LaneMeasurement, SelfSizingMeasurement, WireUse};
use super::transport::iblt_attempt;
use super::verification::ExactDifference;
use set_reconciliation_comparators::iblt::transition::{
    run_transition, ProbeOutcome, SuccessTarget, TransitionLimits, TransitionPlan, CELL_BYTES,
};
use set_reconciliation_comparators::iblt::HASH_COUNT;

pub(super) fn measure_self_sizing(
    data: &Corpus,
    first_cells: usize,
    rbsr: WireUse,
) -> SelfSizingMeasurement {
    let max_second_cells = max_second_cells(first_cells, rbsr);
    let records_per_attempt = (data.left_records.len() + data.right_records.len()) as u64;
    let expected = ExactDifference::new(&data.left_records, &data.right_records);
    let mut aggregate = TrialAggregate::default();
    let mut max_temp_bytes = first_cells as u64 * CELL_BYTES;

    for trial in 0..TRIALS {
        let plan = TransitionPlan {
            first_cells,
            first_seed: 0x1000 + trial * 2 + 1,
            second_seed: 0x1000 + trial * 2 + 2,
            target: SuccessTarget::P99,
            limits: TransitionLimits {
                max_second_cells,
                max_payload_bytes: rbsr.bytes.max(first_cells as u64 * CELL_BYTES),
                max_record_visits: records_per_attempt.saturating_mul(2),
            },
        };
        let started = Instant::now();
        let outcome = run_transition(&data.left_records, &data.right_records, plan);
        aggregate.local_elapsed_seconds += started.elapsed().as_secs_f64();
        match &outcome {
            ProbeOutcome::FirstDecoded { decoded, .. }
            | ProbeOutcome::SecondDecoded { decoded, .. } => {
                expected.verify(&decoded.plus, &decoded.minus);
            }
            ProbeOutcome::Fallback { .. } => {}
        }
        let (wire, visits, fallback) = transition_resources(outcome, first_cells);
        max_temp_bytes = max_temp_bytes.max(wire.max_temp_bytes);
        aggregate.add(wire.use_, visits, fallback);
    }

    let trials = TRIALS as f64;
    SelfSizingMeasurement {
        first_cells,
        lane: LaneMeasurement {
            local_elapsed_seconds: aggregate.local_elapsed_seconds / trials,
            wire: WireUse {
                bytes: (aggregate.bytes as f64 / trials).ceil() as u64,
                fragments: (aggregate.fragments as f64 / trials).ceil() as u64,
                loss_exposed_fragments: aggregate.loss_exposed_fragments,
                rounds: (aggregate.rounds as f64 / trials).ceil() as u32,
            },
            failure_probability: aggregate.fallbacks as f64 / trials,
            max_temp_bytes,
        },
        mean_rounds: aggregate.rounds as f64 / trials,
        mean_record_visits: aggregate.record_visits as f64 / trials,
        max_second_cells,
    }
}

fn max_second_cells(first_cells: usize, rbsr: WireUse) -> usize {
    let first = iblt_attempt(first_cells, false);
    if first.bytes >= rbsr.bytes || first.fragments >= rbsr.fragments {
        return 0;
    }
    let byte_ceiling = ((rbsr.bytes - first.bytes) / CELL_BYTES) as usize;
    (HASH_COUNT + 1..=byte_ceiling)
        .take_while(|&cells| {
            let total = first.plus(iblt_attempt(cells, true));
            total.bytes <= rbsr.bytes && total.fragments <= rbsr.fragments
        })
        .last()
        .unwrap_or(0)
}

fn transition_resources(outcome: ProbeOutcome, first_cells: usize) -> (TrialWire, u64, bool) {
    match outcome {
        ProbeOutcome::FirstDecoded { resources, .. } => (
            TrialWire::new(iblt_attempt(first_cells, true), first_cells),
            resources.record_visits,
            false,
        ),
        ProbeOutcome::SecondDecoded {
            second_cells,
            resources,
            ..
        } => (
            TrialWire::new(
                iblt_attempt(first_cells, false).plus(iblt_attempt(second_cells, true)),
                second_cells,
            ),
            resources.record_visits,
            false,
        ),
        ProbeOutcome::Fallback { resources, .. } => {
            let (wire, largest) = match resources.attempts {
                0 => (WireUse::default(), first_cells),
                1 => (iblt_attempt(first_cells, false), first_cells),
                _ => {
                    let second = resources.cells.saturating_sub(first_cells as u64) as usize;
                    (
                        iblt_attempt(first_cells, false).plus(iblt_attempt(second, false)),
                        second,
                    )
                }
            };
            (TrialWire::new(wire, largest), resources.record_visits, true)
        }
    }
}

#[derive(Clone, Copy)]
struct TrialWire {
    use_: WireUse,
    max_temp_bytes: u64,
}

impl TrialWire {
    fn new(use_: WireUse, largest_sketch: usize) -> Self {
        Self {
            use_,
            max_temp_bytes: largest_sketch as u64 * CELL_BYTES,
        }
    }
}

#[derive(Default)]
struct TrialAggregate {
    local_elapsed_seconds: f64,
    bytes: u64,
    fragments: u64,
    loss_exposed_fragments: u64,
    rounds: u64,
    record_visits: u64,
    fallbacks: u64,
}

impl TrialAggregate {
    fn add(&mut self, wire: WireUse, record_visits: u64, fallback: bool) {
        self.bytes += wire.bytes;
        self.fragments += wire.fragments;
        self.loss_exposed_fragments = self.loss_exposed_fragments.max(wire.loss_exposed_fragments);
        self.rounds += wire.rounds as u64;
        self.record_visits += record_visits;
        self.fallbacks += fallback as u64;
    }
}
