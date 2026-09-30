use super::{reconcile_prefix, CodedSymbol, Encoder};
use crate::iblt::Record;

fn record(fingerprint: u64, id: u64) -> Record {
    Record { fingerprint, id }
}

#[test]
fn encoder_matches_pinned_go_vector() {
    let mut encoder = Encoder::default();
    for record in [record(1, 10), record(2, 20), record(3, 30)] {
        encoder.add_record(record);
    }

    let expected = [
        (0, 0, 0x2c07_3d08_3e0d_23e9, 3),
        (3, 30, 0xc731_5cd9_11ac_f033, 1),
        (1, 10, 0x95b2_75cf_04ea_b502, 1),
        (2, 20, 0x5283_2916_1546_4531, 2),
        (0, 0, 0, 0),
        (2, 20, 0x7e84_141e_2b4b_66d8, 1),
        (2, 20, 0x5283_2916_1546_4531, 2),
        (0, 0, 0x2c07_3d08_3e0d_23e9, 3),
        (1, 10, 0x95b2_75cf_04ea_b502, 1),
        (0, 0, 0, 0),
        (1, 10, 0x95b2_75cf_04ea_b502, 1),
        (2, 20, 0x5283_2916_1546_4531, 2),
    ];

    for (fingerprint, id, hash, count) in expected {
        assert_eq!(
            encoder.next_coded_symbol(),
            CodedSymbol {
                record: record(fingerprint, id),
                hash,
                count,
            }
        );
    }
}

#[test]
fn decoder_matches_pinned_go_prefix_and_orientation() {
    let common = [record(11, 101), record(12, 102), record(13, 103)];
    let remote = [record(21, 201), record(22, 202)];
    let local = [record(31, 301)];
    let source = common.into_iter().chain(remote).collect::<Vec<_>>();
    let target = common.into_iter().chain(local).collect::<Vec<_>>();

    let mut decoded = reconcile_prefix(&source, &target, 100);
    decoded.remote.sort_unstable();
    decoded.local.sort_unstable();

    assert!(decoded.success);
    assert_eq!(decoded.prefix, 5);
    assert_eq!(decoded.remote, remote);
    assert_eq!(decoded.local, local);
    assert_eq!(decoded.persistent_bytes, 504);
}

#[test]
fn insufficient_prefix_never_claims_success() {
    let source = [record(21, 201), record(22, 202)];
    let target = [record(31, 301)];
    let decoded = reconcile_prefix(&source, &target, 1);

    assert!(!decoded.success);
    assert_eq!(decoded.prefix, 1);
}

#[test]
fn recovered_difference_matches_multiple_deterministic_sets() {
    for d in [1_u64, 2, 8, 32] {
        let common = (0..64).map(|id| record(id + 1, id + 10_000));
        let remote = (0..d).map(|id| record(1_000 + id, 20_000 + id));
        let local = (0..d).map(|id| record(2_000 + id, 30_000 + id));
        let source = common.clone().chain(remote.clone()).collect::<Vec<_>>();
        let target = common.chain(local.clone()).collect::<Vec<_>>();
        let mut decoded = reconcile_prefix(&source, &target, d as usize * 16 + 64);
        let mut want_remote = remote.collect::<Vec<_>>();
        let mut want_local = local.collect::<Vec<_>>();
        decoded.remote.sort_unstable();
        decoded.local.sort_unstable();
        want_remote.sort_unstable();
        want_local.sort_unstable();

        assert!(
            decoded.success,
            "d={d} did not decode within the generous cap"
        );
        assert_eq!(decoded.remote, want_remote);
        assert_eq!(decoded.local, want_local);
    }
}
