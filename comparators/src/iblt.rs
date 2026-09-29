// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Reproduction core for Wu et al.'s self-sizing IBLT estimator (#36).
//!
//! The mapper and estimator follow `whitewum/self-sizing` at commit
//! `4ba615ca76978564d5d7d4b75424a680cae6f21d`, mapper version 2. This is a
//! synthetic experiment surface, not a protocol or wire-format commitment.

pub mod transition;

use std::collections::VecDeque;

/// Mapper identifier used by the pinned reference artifact.
pub const MAPPER_VERSION: u8 = 2;
/// Number of distinct cells touched by every record.
pub const HASH_COUNT: usize = 3;

const SEED_A: u64 = 0xA1B2_C3D4_E5F6_0718;
const SEED_B: u64 = 0x1234_5678_90AB_CDEF;
const SEED_C: u64 = 0xFEDC_BA98_7654_3210;
const CHECKSUM_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const RETRY_STEP: u64 = CHECKSUM_SEED;

/// A synthetic IBLT entry: a mapper fingerprint and an independently recoverable identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Record {
    /// Value routed through mapper v2 and checked during peeling.
    pub fingerprint: u64,
    /// Payload recovered by a successful signed decode.
    pub id: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Cell {
    count: i64,
    fingerprint_xor: u64,
    id_xor: u64,
    checksum_xor: u64,
}

/// An invalid sketch construction or subtraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SketchError {
    /// Mapper v2 needs more cells than hash locations so rejection sampling terminates.
    CapacityTooSmall,
    /// Subtraction requires identical cell counts and mapper seeds.
    ShapeMismatch,
}

/// A deterministic, plain IBLT used to reproduce the paper artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sketch {
    seed: u64,
    cells: Vec<Cell>,
}

/// Result of peeling one subtracted sketch.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodeResult {
    /// Whether peeling emptied every cell.
    pub success: bool,
    /// Records present only in the left-hand sketch.
    pub plus: Vec<Record>,
    /// Records present only in the right-hand sketch.
    pub minus: Vec<Record>,
    /// Number of non-empty cells after peeling stops.
    pub residual_cells: usize,
    /// Difference estimate computed from the untouched counters, before peeling.
    pub estimated_difference: f64,
}

impl Sketch {
    /// Construct an empty sketch with a mapper seed independent of record fingerprints.
    pub fn new(capacity: usize, seed: u64) -> Result<Self, SketchError> {
        if capacity <= HASH_COUNT {
            return Err(SketchError::CapacityTooSmall);
        }
        Ok(Self {
            seed,
            cells: vec![Cell::default(); capacity],
        })
    }

    /// Insert one record.
    pub fn insert(&mut self, record: Record) {
        self.apply(record, 1);
    }

    /// Remove one record.
    pub fn remove(&mut self, record: Record) {
        self.apply(record, -1);
    }

    /// Subtract `other` from this sketch, preserving signed difference orientation.
    pub fn subtract(&self, other: &Self) -> Result<Self, SketchError> {
        if self.seed != other.seed || self.cells.len() != other.cells.len() {
            return Err(SketchError::ShapeMismatch);
        }
        let cells = self
            .cells
            .iter()
            .zip(&other.cells)
            .map(|(left, right)| Cell {
                count: left.count - right.count,
                fingerprint_xor: left.fingerprint_xor ^ right.fingerprint_xor,
                id_xor: left.id_xor ^ right.id_xor,
                checksum_xor: left.checksum_xor ^ right.checksum_xor,
            })
            .collect();
        Ok(Self {
            seed: self.seed,
            cells,
        })
    }

    /// Estimate the symmetric-difference size from the current counter vector.
    pub fn estimate_difference(&self) -> f64 {
        estimate_counts(self.cells.iter().map(|cell| cell.count), self.cells.len())
    }

    /// Peel a clone of the sketch and recover signed records where decoding succeeds.
    pub fn decode(&self) -> DecodeResult {
        let estimated_difference = self.estimate_difference();
        let mut cells = self.cells.clone();
        let mut queue: VecDeque<_> = cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| is_pure(*cell).then_some(index))
            .collect();
        let mut plus = Vec::new();
        let mut minus = Vec::new();

        while let Some(index) = queue.pop_front() {
            let cell = cells[index];
            if !is_pure(cell) {
                continue;
            }
            let side = cell.count;
            let record = Record {
                fingerprint: cell.fingerprint_xor,
                id: cell.id_xor,
            };
            if side == 1 {
                plus.push(record);
            } else {
                minus.push(record);
            }
            for position in positions_unchecked(record.fingerprint, cells.len(), self.seed) {
                let target = &mut cells[position];
                target.count -= side;
                target.fingerprint_xor ^= record.fingerprint;
                target.id_xor ^= record.id;
                target.checksum_xor ^= checksum(record.fingerprint);
                if is_pure(*target) {
                    queue.push_back(position);
                }
            }
        }

        plus.sort_unstable();
        minus.sort_unstable();
        let residual_cells = cells
            .iter()
            .filter(|cell| **cell != Cell::default())
            .count();
        DecodeResult {
            success: residual_cells == 0,
            plus,
            minus,
            residual_cells,
            estimated_difference,
        }
    }

    fn apply(&mut self, record: Record, direction: i64) {
        for position in positions_unchecked(record.fingerprint, self.cells.len(), self.seed) {
            let cell = &mut self.cells[position];
            cell.count += direction;
            cell.fingerprint_xor ^= record.fingerprint;
            cell.id_xor ^= record.id;
            cell.checksum_xor ^= checksum(record.fingerprint);
        }
    }
}

/// Return mapper-v2's three distinct locations, including its collision retry rule.
pub fn positions(
    fingerprint: u64,
    capacity: usize,
    seed: u64,
) -> Result<[usize; HASH_COUNT], SketchError> {
    if capacity <= HASH_COUNT {
        return Err(SketchError::CapacityTooSmall);
    }
    Ok(positions_unchecked(fingerprint, capacity, seed))
}

fn positions_unchecked(fingerprint: u64, capacity: usize, seed: u64) -> [usize; HASH_COUNT] {
    let mut bases = [SEED_A, SEED_B, SEED_C];
    if seed != 0 {
        for base in &mut bases {
            *base = mix64(*base ^ seed);
        }
    }
    let mut result = [0; HASH_COUNT];
    for (stream, base) in bases.into_iter().enumerate() {
        let mut retry = 0_u64;
        loop {
            let candidate = (mix64(
                base.wrapping_add(fingerprint)
                    .wrapping_add(retry.wrapping_mul(RETRY_STEP)),
            ) % capacity as u64) as usize;
            if !result[..stream].contains(&candidate) {
                result[stream] = candidate;
                break;
            }
            retry = retry.wrapping_add(1);
        }
    }
    result
}

fn estimate_counts(counts: impl Iterator<Item = i64>, capacity: usize) -> f64 {
    let m = capacity as f64;
    let k = HASH_COUNT as f64;
    let (sum, sum_squares) = counts.fold((0.0, 0.0), |(sum, squares), count| {
        let count = count as f64;
        (sum + count, squares + count * count)
    });
    (sum_squares - sum * sum / m) / (k * (1.0 - k / m))
}

fn checksum(fingerprint: u64) -> u64 {
    mix64(CHECKSUM_SEED.wrapping_add(fingerprint))
}

fn is_pure(cell: Cell) -> bool {
    cell.count.unsigned_abs() == 1 && cell.checksum_xor == checksum(cell.fingerprint_xor)
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests;
