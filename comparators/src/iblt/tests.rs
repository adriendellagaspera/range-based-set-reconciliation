use super::*;

fn record(fingerprint: u64) -> Record {
    Record {
        fingerprint,
        id: fingerprint.rotate_left(17) ^ 0xD1FF_E2E0_CAFE_BABE,
    }
}

#[test]
fn mapper_v2_matches_pinned_reference_vectors() {
    assert_eq!(mix64(1), 0x5692_161D_100B_05E5);
    assert_eq!(checksum(0x0020_6D7A_4C58_CD2C), 0xF5AA_3C0C_47B6_95A8);
    assert_eq!(
        positions(0x0020_6D7A_4C58_CD2C, 6_000, 0),
        Ok([3_301, 359, 5_223])
    );
    assert_eq!(
        positions(0x0020_6D7A_4C58_CD2C, 6_000, 0x1234),
        Ok([1_021, 161, 2_602])
    );
    assert_eq!(positions(0x20D, 6_000, 0), Ok([5_167, 4_921, 4_024]));
}

#[test]
fn mapper_v2_always_returns_three_distinct_positions() {
    for capacity in [4, 5, 7, 16, 64, 256, 6_000, 65_537] {
        for fingerprint in 0..2_048 {
            let [a, b, c] = positions(mix64(fingerprint), capacity, 0x49_424C_5432).unwrap();
            assert!(a < capacity && b < capacity && c < capacity);
            assert!(a != b && a != c && b != c);
        }
    }
}

#[test]
fn subtraction_and_peeling_recover_the_signed_difference() {
    let common = record(2);
    let left_only = [record(1), record(3)];
    let right_only = [record(4), record(5)];
    let mut left = Sketch::new(64, 0xCAFE).unwrap();
    let mut right = Sketch::new(64, 0xCAFE).unwrap();
    for item in left_only.into_iter().chain([common]) {
        left.insert(item);
    }
    for item in right_only.into_iter().chain([common]) {
        right.insert(item);
    }

    let decoded = left.subtract(&right).unwrap().decode();

    assert!(decoded.success);
    assert_eq!(decoded.plus, left_only);
    assert_eq!(decoded.minus, right_only);
    assert_eq!(decoded.residual_cells, 0);
}

#[test]
fn estimate_is_captured_before_successful_peeling() {
    let mut left = Sketch::new(128, 7).unwrap();
    let right = Sketch::new(128, 7).unwrap();
    for fingerprint in 1..=20 {
        left.insert(record(mix64(fingerprint)));
    }
    let difference = left.subtract(&right).unwrap();
    let expected = difference.estimate_difference();

    let decoded = difference.decode();

    assert!(decoded.success);
    assert_eq!(decoded.residual_cells, 0);
    assert_eq!(decoded.estimated_difference.to_bits(), expected.to_bits());
    assert!(decoded.estimated_difference > 0.0);
}

#[test]
fn estimator_is_unbiased_over_fixed_seed_trials() {
    const CAPACITY: usize = 128;
    const DIFFERENCE: usize = 40;
    const TRIALS: usize = 2_000;
    let mut estimate_sum = 0.0;
    for trial in 0..TRIALS {
        let mut sketch = Sketch::new(CAPACITY, mix64(trial as u64)).unwrap();
        for item in 0..DIFFERENCE {
            let fingerprint = mix64(((trial * DIFFERENCE + item) as u64).wrapping_add(1));
            let value = record(fingerprint);
            if item % 2 == 0 {
                sketch.insert(value);
            } else {
                sketch.remove(value);
            }
        }
        estimate_sum += sketch.estimate_difference();
    }
    let mean = estimate_sum / TRIALS as f64;
    assert!((mean / DIFFERENCE as f64 - 1.0).abs() < 0.02, "mean={mean}");
}

#[test]
fn identical_inputs_are_byte_deterministic() {
    let build = || {
        let mut sketch = Sketch::new(64, 99).unwrap();
        for fingerprint in 0..20 {
            sketch.insert(record(mix64(fingerprint)));
        }
        sketch
    };
    assert_eq!(build(), build());
}

#[test]
fn invalid_shapes_are_rejected() {
    assert_eq!(
        Sketch::new(HASH_COUNT, 0),
        Err(SketchError::CapacityTooSmall)
    );
    let seeded = Sketch::new(16, 1).unwrap();
    assert_eq!(
        seeded.subtract(&Sketch::new(16, 2).unwrap()),
        Err(SketchError::ShapeMismatch)
    );
    assert_eq!(
        seeded.subtract(&Sketch::new(17, 1).unwrap()),
        Err(SketchError::ShapeMismatch)
    );
}
