use std::collections::HashSet;

use bincode::{DefaultOptions, Serializer};
use lww_register::clock::{Hlc, LogicalCounter, NodeId, PhysicalTime, Timestamp};
use lww_register::Entry;
use rsos::FingerprintTreeMap;

use super::matrix::{DifferenceShape, Scenario};
use set_reconciliation_comparators::iblt::Record;

const VALUE_BYTES: usize = 8;
const WRITE_INSTANT_MS: u64 = 1_786_752_000_000;
const NODE_ID: u64 = 0xfeed_face_dead_beef;

pub(super) struct Corpus {
    pub left_store: FingerprintTreeMap<u64, u64>,
    pub right_store: FingerprintTreeMap<u64, u64>,
    pub left_records: Vec<Record>,
    pub right_records: Vec<Record>,
}

pub(super) fn build(scenario: Scenario) -> Corpus {
    let differing = differing_keys(scenario);
    let mut left_store = FingerprintTreeMap::new();
    let mut right_store = FingerprintTreeMap::new();
    let mut left_records = Vec::with_capacity(scenario.records);
    let mut right_records = Vec::with_capacity(scenario.records);

    for key in 0..scenario.records as u64 {
        let value = key.wrapping_mul(2_654_435_761);
        left_store.insert(key, value);
        left_records.push(record(key, 0));
        match scenario.shape {
            DifferenceShape::DeletionScattered | DifferenceShape::DeletionClustered
                if differing.contains(&key) => {}
            DifferenceShape::UpdatedScattered if differing.contains(&key) => {
                right_store.insert(key, !value);
                right_records.push(record(key, 1));
            }
            _ => {
                right_store.insert(key, value);
                right_records.push(record(key, 0));
            }
        }
    }

    Corpus {
        left_store,
        right_store,
        left_records,
        right_records,
    }
}

pub(super) fn record(key: u64, version: u64) -> Record {
    let id = key.wrapping_mul(2).wrapping_add(version);
    Record {
        fingerprint: mix64(id ^ 0xd6e8_feb8_6659_fd93),
        id,
    }
}

pub(super) fn element_bytes(key: u64, scratch: &mut Vec<u8>) -> usize {
    scratch.clear();
    let entry = Entry::present(stamp(key), vec![key as u8; VALUE_BYTES]);
    use serde::Serialize;

    (key, entry)
        .serialize(&mut Serializer::new(&mut *scratch, DefaultOptions::new()))
        .expect("encoding an in-memory entry cannot fail");
    scratch.len()
}

fn differing_keys(scenario: Scenario) -> HashSet<u64> {
    match scenario.shape {
        DifferenceShape::DeletionClustered => {
            let start = scenario.records / 2 - scenario.changed_keys / 2;
            (start..start + scenario.changed_keys)
                .map(|key| key as u64)
                .collect()
        }
        DifferenceShape::DeletionScattered | DifferenceShape::UpdatedScattered => {
            (1..=scenario.changed_keys as u64)
                .map(|i| (scenario.records as u64 / (scenario.changed_keys as u64 + 1)) * i)
                .collect()
        }
    }
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn stamp(key: u64) -> Timestamp {
    Timestamp::new(
        Hlc::new(
            PhysicalTime::from_millis(WRITE_INSTANT_MS + key),
            LogicalCounter::ZERO,
        ),
        NodeId::new(NODE_ID),
    )
}
