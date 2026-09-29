use super::*;
use std::collections::HashSet;

#[test]
fn matrix_keys_are_unique() {
    for (index, scenario) in CASES.iter().enumerate() {
        assert!(
            CASES[..index].iter().all(|other| other.id != scenario.id),
            "duplicate scenario id {}",
            scenario.id.as_str()
        );
    }
    for (index, network) in NETWORKS.iter().enumerate() {
        assert!(
            NETWORKS[..index].iter().all(|other| other.id != network.id),
            "duplicate network id {}",
            network.id.as_str()
        );
    }
}

#[test]
fn updated_records_count_twice_in_the_symmetric_difference() {
    let scenario = CASES
        .iter()
        .find(|scenario| scenario.shape == DifferenceShape::UpdatedScattered)
        .copied()
        .unwrap();
    let data = corpus::build(scenario);
    assert_eq!(
        data.left_records.len().abs_diff(data.right_records.len()),
        0
    );
    let right_ids = data
        .right_records
        .iter()
        .map(|record| record.id)
        .collect::<HashSet<_>>();
    assert_eq!(
        data.left_records
            .iter()
            .filter(|record| !right_ids.contains(&record.id))
            .count(),
        scenario.changed_keys
    );
}

#[test]
fn machine_artifact_round_trips_without_a_text_report_parser() {
    let report = fixture_report();
    let mut json = Vec::new();
    write_report(&mut json, &report).unwrap();
    assert_eq!(read_report(json.as_slice()).unwrap(), report);

    let human_output = b"benchmark report: case=needle network=lan classic=0.1s";
    assert!(matches!(
        read_report(human_output.as_slice()),
        Err(ArtifactError::Json(_))
    ));
}

#[test]
fn machine_artifact_rejects_unknown_fields_and_versions() {
    let report = fixture_report();
    let mut json = serde_json::to_value(&report).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("human_stdout".to_owned(), serde_json::Value::Bool(true));
    let bytes = serde_json::to_vec(&json).unwrap();
    assert!(matches!(
        read_report(bytes.as_slice()),
        Err(ArtifactError::Json(_))
    ));

    let mut wrong_version = report;
    wrong_version.schema_version += 1;
    assert!(matches!(
        write_report(Vec::new(), &wrong_version),
        Err(ArtifactError::Invalid(_))
    ));
}

#[test]
fn report_validation_rejects_an_incomplete_matrix() {
    let mut report = fixture_report();
    report.cases[0].networks.pop();
    assert!(matches!(
        write_report(Vec::new(), &report),
        Err(ArtifactError::Invalid(_))
    ));
}

#[test]
fn probe_selection_uses_one_policy_for_the_whole_matrix() {
    let mut report = fixture_report();
    assert_eq!(best_static_probe(&report), ProbePolicy::Cells128);
    report.cases[0].networks[0].self_sizing_seconds = [100.0, 0.0];
    assert_eq!(best_static_probe(&report), ProbePolicy::Cells184);
}

fn fixture_report() -> ComparisonReport {
    ComparisonReport {
        schema_version: REPORT_SCHEMA_VERSION,
        configuration: ComparisonConfiguration::current(),
        cases: CASES
            .map(|scenario| CaseReport {
                scenario,
                effective_difference: match scenario.shape {
                    DifferenceShape::UpdatedScattered => scenario.changed_keys * 2,
                    _ => scenario.changed_keys,
                },
                classic: ClassicMeasurement {
                    lane: lane(),
                    messages: 1,
                    enumerated_elements: 1,
                },
                fixed: lane(),
                self_sizing: PROBES.map(|first_cells| SelfSizingMeasurement {
                    first_cells,
                    lane: lane(),
                    mean_rounds: 1.0,
                    mean_record_visits: 1.0,
                    max_second_cells: 1,
                }),
                bulk: lane(),
                riblt: RibltMeasurement {
                    prefix: 1,
                    success: true,
                    build_local_elapsed_seconds: 1.0,
                    stream_local_elapsed_seconds: 1.0,
                    persistent_bytes_per_peer: 1,
                },
                networks: NETWORKS
                    .map(|profile| NetworkProjection {
                        profile,
                        classic_seconds: 1.0,
                        fixed_seconds: 1.0,
                        self_sizing_seconds: [1.0, 2.0],
                        bulk_seconds: 1.0,
                        expected_fixed_bytes: 1,
                        expected_self_sizing_bytes: [1, 1],
                        riblt: RibltProjection {
                            steady_seconds: 1.0,
                            ideal_stop_bytes: 1,
                            line_rate_stop_bytes: 1,
                            datagrams: 1,
                            stop_overshoot_bytes: 0,
                            expected_retry_rounds: 0.0,
                        },
                    })
                    .to_vec(),
            })
            .to_vec(),
    }
}

fn lane() -> LaneMeasurement {
    LaneMeasurement {
        local_elapsed_seconds: 1.0,
        wire: WireUse {
            bytes: 1,
            fragments: 1,
            loss_exposed_fragments: 1,
            rounds: 1,
        },
        failure_probability: 0.0,
        max_temp_bytes: 1,
    }
}

#[test]
fn legacy_cpu_named_artifacts_are_not_silently_reinterpreted() {
    let mut legacy = fixture_report();
    legacy.schema_version = 1;
    let bytes = serde_json::to_vec(&legacy).unwrap();
    assert!(matches!(
        read_report(bytes.as_slice()),
        Err(ArtifactError::Invalid(_))
    ));
}

#[test]
fn small_comparison_runs_verify_signed_sketch_outputs() {
    for shape in [
        DifferenceShape::DeletionScattered,
        DifferenceShape::DeletionClustered,
        DifferenceShape::UpdatedScattered,
    ] {
        let case = measure::run_case(Scenario {
            id: ScenarioId::BroadScattered,
            records: 64,
            changed_keys: 4,
            shape,
        });
        assert!(case.riblt.success);
        assert_eq!(case.fixed.failure_probability, 0.0);
        assert!(case
            .self_sizing
            .iter()
            .all(|lane| lane.lane.failure_probability == 0.0));
    }
}
