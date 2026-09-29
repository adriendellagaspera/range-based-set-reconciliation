// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Shared reconciliation experiment used to compare RBSR, IBLT, RIBLT, and bulk transfer.

mod analysis;
mod artifact;
mod corpus;
pub use devkit::experiment;
mod matrix;
mod measure;
mod model;
mod self_sizing;
mod transport;
mod verification;

pub use analysis::{best_static_probe, totals, Totals};
pub use artifact::{read_report, write_report, ArtifactError, InvalidReport};
pub use matrix::{
    ComparisonConfiguration, DifferenceShape, NetworkId, NetworkProfile, Scenario, ScenarioId,
    BANDWIDTH_BYTES_PER_SECOND, CASES, FIXED_CAPACITY, NETWORKS, PROBES, REPORT_SCHEMA_VERSION,
    TRIALS,
};
pub use measure::run;
pub use model::{
    CaseReport, ClassicMeasurement, ComparisonReport, LaneMeasurement, NetworkProjection,
    ProbePolicy, RibltMeasurement, RibltProjection, SelfSizingMeasurement, WireUse,
};

#[cfg(test)]
mod tests;
