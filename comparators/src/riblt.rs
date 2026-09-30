// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Minimal reproduction of `yangl1996/riblt@4afa6bc06cb2` for issue #36.
//!
//! The source-symbol shape and hash match the pinned self-sizing artifact's RIBLT arm. This is a
//! research comparator, not a production wire implementation; transport pricing remains in the
//! benchmark that consumes the coded-symbol count.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::mem::size_of;

use crate::iblt::Record;

const MAPPING_MULTIPLIER: u64 = 0xda94_2042_e4dd_58b5;
const SYMBOL_HASH_SEED: u64 = 0xd6e8_feb8_6659_fd93;
const TWO_POW_32: f64 = 4_294_967_296.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HashedRecord {
    record: Record,
    hash: u64,
}

impl HashedRecord {
    fn new(record: Record) -> Self {
        Self {
            record,
            hash: symbol_hash(record),
        }
    }
}

/// One rateless coded symbol before transport serialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodedSymbol {
    /// XOR of recoverable source records.
    pub record: Record,
    /// XOR of non-homomorphic source hashes.
    pub hash: u64,
    /// Signed source-symbol degree.
    pub count: i64,
}

impl Default for CodedSymbol {
    fn default() -> Self {
        Self {
            record: Record {
                fingerprint: 0,
                id: 0,
            },
            hash: 0,
            count: 0,
        }
    }
}

impl CodedSymbol {
    fn apply(mut self, record: HashedRecord, direction: i64) -> Self {
        self.record.fingerprint ^= record.record.fingerprint;
        self.record.id ^= record.record.id;
        self.hash ^= record.hash;
        self.count += direction;
        self
    }

    /// Subtract another coded symbol with the same sequence index.
    pub fn subtract(self, other: Self) -> Self {
        Self {
            record: Record {
                fingerprint: self.record.fingerprint ^ other.record.fingerprint,
                id: self.record.id ^ other.record.id,
            },
            hash: self.hash ^ other.hash,
            count: self.count - other.count,
        }
    }

    fn is_pure(self) -> bool {
        matches!(self.count, -1 | 1) && self.hash == symbol_hash(self.record)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RandomMapping {
    prng: u64,
    last_index: usize,
}

impl RandomMapping {
    fn new(hash: u64) -> Self {
        Self {
            prng: hash,
            last_index: 0,
        }
    }

    fn next_index(&mut self) -> usize {
        self.prng = self.prng.wrapping_mul(MAPPING_MULTIPLIER);
        let random = self.prng.wrapping_add(1) as f64;
        let step =
            ((self.last_index as f64 + 1.5) * (TWO_POW_32 / random.sqrt() - 1.0)).ceil() as usize;
        self.last_index = self.last_index.saturating_add(step);
        self.last_index
    }
}

#[derive(Debug, Default)]
struct CodingWindow {
    symbols: Vec<HashedRecord>,
    mappings: Vec<RandomMapping>,
    queue: BinaryHeap<Reverse<(usize, usize)>>,
    next_index: usize,
}

impl CodingWindow {
    fn add_record(&mut self, record: Record) {
        self.add_hashed_with_mapping(
            HashedRecord::new(record),
            RandomMapping::new(symbol_hash(record)),
        );
    }

    fn add_hashed_with_mapping(&mut self, record: HashedRecord, mapping: RandomMapping) {
        let source_index = self.symbols.len();
        self.symbols.push(record);
        self.mappings.push(mapping);
        self.queue.push(Reverse((mapping.last_index, source_index)));
    }

    fn apply(&mut self, mut coded: CodedSymbol, direction: i64) -> CodedSymbol {
        while self
            .queue
            .peek()
            .is_some_and(|Reverse((coded_index, _))| *coded_index == self.next_index)
        {
            let Reverse((_, source_index)) = self.queue.pop().expect("peeked queue entry exists");
            coded = coded.apply(self.symbols[source_index], direction);
            let next = self.mappings[source_index].next_index();
            self.queue.push(Reverse((next, source_index)));
        }
        self.next_index += 1;
        coded
    }

    fn records(&self) -> Vec<Record> {
        self.symbols.iter().map(|record| record.record).collect()
    }

    fn raw_state_bytes(&self) -> usize {
        self.symbols.len() * size_of::<HashedRecord>()
            + self.mappings.len() * size_of::<RandomMapping>()
            + self.queue.len() * size_of::<Reverse<(usize, usize)>>()
    }
}

/// Incremental encoder for the pinned Rateless IBLT sequence.
#[derive(Debug, Default)]
pub struct Encoder {
    window: CodingWindow,
}

impl Encoder {
    /// Add one source record before producing any coded symbol.
    pub fn add_record(&mut self, record: Record) {
        assert_eq!(
            self.window.next_index, 0,
            "RIBLT source set cannot change after streaming starts"
        );
        self.window.add_record(record);
    }

    /// Produce the next coded symbol in the infinite sequence.
    pub fn next_coded_symbol(&mut self) -> CodedSymbol {
        self.window.apply(CodedSymbol::default(), 1)
    }

    /// Raw payload bytes retained by the three per-source arrays, excluding allocator overhead.
    pub fn raw_state_bytes(&self) -> usize {
        self.window.raw_state_bytes()
    }
}

/// Decoder for a stream of source-minus-target coded symbols, as used by the pinned artifact.
#[derive(Debug, Default)]
pub struct DifferenceDecoder {
    coded: Vec<CodedSymbol>,
    local: CodingWindow,
    remote: CodingWindow,
    decodable: Vec<usize>,
    decoded: usize,
}

impl DifferenceDecoder {
    /// Feed the next coded-symbol difference in sequence order.
    pub fn add_coded_symbol(&mut self, coded: CodedSymbol) {
        let coded = self.remote.apply(coded, -1);
        let coded = self.local.apply(coded, 1);
        self.coded.push(coded);
        if coded.is_pure() || (coded.count == 0 && coded.hash == 0) {
            self.decodable.push(self.coded.len() - 1);
        }
    }

    fn apply_new_record(&mut self, record: HashedRecord, direction: i64) -> RandomMapping {
        let mut mapping = RandomMapping::new(record.hash);
        while mapping.last_index < self.coded.len() {
            let index = mapping.last_index;
            self.coded[index] = self.coded[index].apply(record, direction);
            if self.coded[index].is_pure() {
                self.decodable.push(index);
            }
            mapping.next_index();
        }
        mapping
    }

    /// Peel every coded symbol that became decodable since the previous call.
    pub fn try_decode(&mut self) {
        let mut cursor = 0;
        while cursor < self.decodable.len() {
            let index = self.decodable[cursor];
            cursor += 1;
            let coded = self.coded[index];
            match coded.count {
                1 => {
                    let record = HashedRecord {
                        record: coded.record,
                        hash: coded.hash,
                    };
                    let mapping = self.apply_new_record(record, -1);
                    self.remote.add_hashed_with_mapping(record, mapping);
                    self.decoded += 1;
                }
                -1 => {
                    let record = HashedRecord {
                        record: coded.record,
                        hash: coded.hash,
                    };
                    let mapping = self.apply_new_record(record, 1);
                    self.local.add_hashed_with_mapping(record, mapping);
                    self.decoded += 1;
                }
                0 => self.decoded += 1,
                _ => unreachable!("a queued RIBLT symbol stays decodable while peeling"),
            }
        }
        self.decodable.clear();
    }

    /// Whether every coded symbol received so far has been peeled.
    pub fn decoded(&self) -> bool {
        self.decoded == self.coded.len()
    }

    /// Records present only at the stream source.
    pub fn remote_records(&self) -> Vec<Record> {
        self.remote.records()
    }

    /// Records present only at the stream target.
    pub fn local_records(&self) -> Vec<Record> {
        self.local.records()
    }
}

/// Reconcile two record sets until decode succeeds or `hard_cap` coded symbols were consumed.
#[cfg(test)]
pub fn reconcile_prefix(source: &[Record], target: &[Record], hard_cap: usize) -> RibltResult {
    let mut source_encoder = Encoder::default();
    let mut target_encoder = Encoder::default();
    for &record in source {
        source_encoder.add_record(record);
    }
    for &record in target {
        target_encoder.add_record(record);
    }
    let persistent_bytes = source_encoder.raw_state_bytes() + target_encoder.raw_state_bytes();
    let mut decoder = DifferenceDecoder::default();
    for prefix in 1..=hard_cap {
        let difference = source_encoder
            .next_coded_symbol()
            .subtract(target_encoder.next_coded_symbol());
        decoder.add_coded_symbol(difference);
        decoder.try_decode();
        if decoder.decoded() {
            return RibltResult {
                success: true,
                prefix,
                remote: decoder.remote_records(),
                local: decoder.local_records(),
                persistent_bytes,
            };
        }
    }
    RibltResult {
        success: false,
        prefix: hard_cap,
        remote: decoder.remote_records(),
        local: decoder.local_records(),
        persistent_bytes,
    }
}

/// Result of one bounded pinned-RIBLT reproduction run.
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RibltResult {
    /// Whether the received prefix fully peeled.
    pub success: bool,
    /// Number of coded symbols consumed.
    pub prefix: usize,
    /// Records present only at the source.
    pub remote: Vec<Record>,
    /// Records present only at the target.
    pub local: Vec<Record>,
    /// Raw source+target encoder state, excluding allocator overhead.
    pub persistent_bytes: usize,
}

fn symbol_hash(record: Record) -> u64 {
    mix64(record.fingerprint ^ record.id.rotate_left(23) ^ SYMBOL_HASH_SEED)
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
