// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under either Apache-2.0 or MIT.

#![forbid(unsafe_code)]

//! Experimental set-reconciliation harness.
//!
//! Reproduction cores for algorithms from the literature live in the sibling
//! `set-reconciliation-comparators` crate. This crate owns measurement, policy probes,
//! transport projections, reporting, and experiment orchestration.

pub mod comparison;
pub mod controller;
pub mod policy;
pub mod reporting;
