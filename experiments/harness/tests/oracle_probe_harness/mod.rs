// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! One import surface for the two #356 follow-up measurement modules
//! (`joint_progress_and_the_oracle_coupling_confound.rs`,
//! `the_union_bounds_effective_multiplier.rs`), assembled from the two crates that own its halves
//! rather than restated here.
//!
//! The store/driver primitives are `rbsr`'s (`probe_harness`, `rbsr_internal_testing`-gated):
//! [`rbsr::NarrowStore`], [`rbsr::balanced_swap`], [`rbsr::drive`], [`rbsr::drive_pair`],
//! [`rbsr::Termination`]. That crate's own re-export comment names this repository as the "fuller
//! measurement harness outside this workspace" they exist to found, and its [`rbsr::drive`] already
//! carries the visited-state table that **proves** a stall instead of inferring one from a round
//! cap — the load-bearing part of both modules' method.
//!
//! The measurement/reporting half is [`set_reconciliation_experiments::reporting`], this crate's own library layer.
//!
//! **Nothing is defined here.** A definition in this file would be a second copy of one of those
//! two, and the copies are exactly what drifts: `mask`/`MAX_ROUNDS`/`state_hash` used to live here
//! unused by either consumer, beside re-implementations of all seven `probe_harness` items.

// Each integration test binary compiles this module separately and uses a different subset of it.
#![allow(unused_imports)]

pub use rbsr::{
    balanced_swap, drive, drive_pair, Drive, NarrowStore, Termination, DRIVE_STORE_SIZE,
};
pub use set_reconciliation_experiments::reporting::*;
