use std::time::Instant;

use devkit::protocol_cost::{reconcile, Cost};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rbsr::{FanOut, FixedFanOut};

use super::corpus::{build, element_bytes, Corpus};
use super::matrix::{
    ComparisonConfiguration, DifferenceShape, Scenario, BANDWIDTH_BYTES_PER_SECOND, CASES,
    FIXED_CAPACITY, NETWORKS, PROBES, REPORT_SCHEMA_VERSION, TRIALS,
};
use super::model::{
    CaseReport, ClassicMeasurement, ComparisonReport, LaneMeasurement, NetworkProjection,
    RibltMeasurement, WireUse,
};
use super::self_sizing::measure_self_sizing;
use super::transport::{bulk_shape, iblt_attempt, riblt_projection};
use super::verification::ExactDifference;
use crate::controller::Strategy;
use set_reconciliation_comparators::iblt::transition::CELL_BYTES;
use set_reconciliation_comparators::iblt::Sketch;
use set_reconciliation_comparators::riblt::{DifferenceDecoder, Encoder};

const RIBLT_HARD_CAP: usize = 65_536;

pub fn run() -> ComparisonReport {
    ComparisonReport {
        schema_version: REPORT_SCHEMA_VERSION,
        configuration: ComparisonConfiguration::current(),
        cases: CASES.into_iter().map(run_case).collect(),
    }
}

pub(super) fn run_case(scenario: Scenario) -> CaseReport {
    let data = build(scenario);
    let classic = measure_classic(&data);
    let bulk = measure_bulk(scenario);
    let fixed = measure_fixed(&data);
    let self_sizing = PROBES.map(|first| measure_self_sizing(&data, first, classic.lane.wire));
    let riblt = measure_riblt(&data);
    let networks = NETWORKS
        .into_iter()
        .map(|profile| {
            let classic_seconds =
                classic
                    .lane
                    .projected_seconds(Strategy::ClassicRbsr, profile, 0.0);
            let bulk_seconds = bulk.projected_seconds(Strategy::Bulk, profile, 0.0);
            let (fallback_seconds, fallback_wire) = if classic_seconds <= bulk_seconds {
                (classic_seconds, classic.lane.wire)
            } else {
                (bulk_seconds, bulk.wire)
            };
            let fixed_seconds =
                fixed.projected_seconds(Strategy::SelfSizingIblt, profile, classic_seconds);
            let self_sizing_seconds = self_sizing.map(|measured| {
                measured
                    .lane
                    .projected_seconds(Strategy::SelfSizingIblt, profile, fallback_seconds)
            });
            NetworkProjection {
                profile,
                classic_seconds,
                fixed_seconds,
                self_sizing_seconds,
                bulk_seconds,
                expected_fixed_bytes: expected_total(
                    fixed.wire.bytes,
                    fixed.failure_probability,
                    classic.lane.wire.bytes,
                ),
                expected_self_sizing_bytes: self_sizing.map(|measured| {
                    expected_total(
                        measured.lane.wire.bytes,
                        measured.lane.failure_probability,
                        fallback_wire.bytes,
                    )
                }),
                riblt: riblt_projection(
                    riblt,
                    profile.rtt_seconds,
                    BANDWIDTH_BYTES_PER_SECOND,
                    profile.loss_probability,
                ),
            }
        })
        .collect();

    CaseReport {
        scenario,
        effective_difference: match scenario.shape {
            DifferenceShape::UpdatedScattered => scenario.changed_keys * 2,
            _ => scenario.changed_keys,
        },
        classic,
        fixed,
        self_sizing,
        bulk,
        riblt,
        networks,
    }
}

fn measure_classic(data: &Corpus) -> ClassicMeasurement {
    let mut scratch = Vec::new();
    let mut price = |key| vec![element_bytes(key, &mut scratch)];
    let mut rng = StdRng::seed_from_u64(42);
    let started = Instant::now();
    let cost = reconcile(
        &data.left_store,
        &data.right_store,
        &FixedFanOut::new(FanOut::NEGENTROPY),
        Some(&mut price),
        &mut rng,
    );
    let local_elapsed_seconds = started.elapsed().as_secs_f64();
    ClassicMeasurement {
        lane: classic_lane(local_elapsed_seconds, &cost),
        messages: cost.messages as u64,
        enumerated_elements: cost.enumerated_elements as u64,
    }
}

fn classic_lane(local_elapsed_seconds: f64, cost: &Cost) -> LaneMeasurement {
    let enumerated = cost.enumerated_bytes.first().copied().unwrap_or(0) as u64;
    let enum_shape = bulk_shape(enumerated);
    let refinement_exposed =
        super::transport::message_shape(cost.largest_message_bytes as u64).loss_exposed_fragments;
    LaneMeasurement {
        local_elapsed_seconds,
        wire: WireUse {
            bytes: cost.refinement_bytes as u64 + enumerated,
            fragments: cost.fragments as u64 + enum_shape.fragments,
            loss_exposed_fragments: refinement_exposed.max(enum_shape.loss_exposed_fragments),
            rounds: cost.messages.div_ceil(2) as u32,
        },
        failure_probability: 0.0,
        max_temp_bytes: 0,
    }
}

fn measure_bulk(scenario: Scenario) -> LaneMeasurement {
    let started = Instant::now();
    let mut scratch = Vec::new();
    let bytes = (0..scenario.records as u64)
        .map(|key| element_bytes(key, &mut scratch) as u64)
        .sum();
    LaneMeasurement {
        local_elapsed_seconds: started.elapsed().as_secs_f64(),
        wire: bulk_shape(bytes),
        failure_probability: 0.0,
        max_temp_bytes: 0,
    }
}

fn measure_fixed(data: &Corpus) -> LaneMeasurement {
    let mut local_elapsed_seconds = 0.0;
    let mut fallbacks = 0_u64;
    let expected = ExactDifference::new(&data.left_records, &data.right_records);
    for trial in 0..TRIALS {
        let seed = 0x8000 + trial;
        let started = Instant::now();
        let mut left = Sketch::new(FIXED_CAPACITY, seed).expect("fixed capacity is valid");
        let mut right = Sketch::new(FIXED_CAPACITY, seed).expect("fixed capacity is valid");
        for &record in &data.left_records {
            left.insert(record);
        }
        for &record in &data.right_records {
            right.insert(record);
        }
        let decoded = left
            .subtract(&right)
            .expect("fixed sketches have identical shapes")
            .decode();
        local_elapsed_seconds += started.elapsed().as_secs_f64();
        let success = decoded.success;
        if success {
            expected.verify(&decoded.plus, &decoded.minus);
        }
        fallbacks += (!success) as u64;
    }
    let failure_probability = fallbacks as f64 / TRIALS as f64;
    let success = iblt_attempt(FIXED_CAPACITY, true);
    let failed = iblt_attempt(FIXED_CAPACITY, false);
    LaneMeasurement {
        local_elapsed_seconds: local_elapsed_seconds / TRIALS as f64,
        wire: WireUse {
            bytes: weighted(success.bytes, failed.bytes, failure_probability),
            fragments: weighted(success.fragments, failed.fragments, failure_probability),
            loss_exposed_fragments: success
                .loss_exposed_fragments
                .max(failed.loss_exposed_fragments),
            rounds: 1,
        },
        failure_probability,
        max_temp_bytes: FIXED_CAPACITY as u64 * CELL_BYTES,
    }
}

fn measure_riblt(data: &Corpus) -> RibltMeasurement {
    let expected = ExactDifference::new(&data.left_records, &data.right_records);
    let build_started = Instant::now();
    let mut source = Encoder::default();
    let mut target = Encoder::default();
    for &record in &data.left_records {
        source.add_record(record);
    }
    for &record in &data.right_records {
        target.add_record(record);
    }
    let build_local_elapsed_seconds = build_started.elapsed().as_secs_f64();
    let persistent_bytes_per_peer = source.raw_state_bytes().max(target.raw_state_bytes());

    let stream_started = Instant::now();
    let mut decoder = DifferenceDecoder::default();
    let mut prefix = 0;
    for next in 1..=RIBLT_HARD_CAP {
        let difference = source
            .next_coded_symbol()
            .subtract(target.next_coded_symbol());
        decoder.add_coded_symbol(difference);
        decoder.try_decode();
        prefix = next;
        if decoder.decoded() {
            break;
        }
    }
    let stream_local_elapsed_seconds = stream_started.elapsed().as_secs_f64();
    if decoder.decoded() {
        expected.verify(&decoder.remote_records(), &decoder.local_records());
    }
    RibltMeasurement {
        prefix,
        success: decoder.decoded(),
        build_local_elapsed_seconds,
        stream_local_elapsed_seconds,
        persistent_bytes_per_peer,
    }
}

fn weighted(success: u64, failure: u64, failure_probability: f64) -> u64 {
    ((1.0 - failure_probability) * success as f64 + failure_probability * failure as f64).ceil()
        as u64
}

fn expected_total(base: u64, probability: f64, fallback: u64) -> u64 {
    (base as f64 + probability * fallback as f64).ceil() as u64
}
