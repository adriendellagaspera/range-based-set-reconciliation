use serde::{Deserialize, Serialize};

use super::matrix::{
    ComparisonConfiguration, NetworkProfile, Scenario, BANDWIDTH_BYTES_PER_SECOND,
};
use crate::controller::{predicted_cost, ActionEstimate, Strategy, TransportState};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WireUse {
    pub bytes: u64,
    pub fragments: u64,
    pub loss_exposed_fragments: u64,
    pub rounds: u32,
}

impl WireUse {
    pub(super) fn plus(self, other: Self) -> Self {
        Self {
            bytes: self.bytes + other.bytes,
            fragments: self.fragments + other.fragments,
            loss_exposed_fragments: self
                .loss_exposed_fragments
                .max(other.loss_exposed_fragments),
            rounds: self.rounds + other.rounds,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneMeasurement {
    pub local_elapsed_seconds: f64,
    pub wire: WireUse,
    pub failure_probability: f64,
    pub max_temp_bytes: u64,
}

impl LaneMeasurement {
    pub(super) fn projected_seconds(
        self,
        strategy: Strategy,
        profile: NetworkProfile,
        fallback_seconds: f64,
    ) -> f64 {
        predicted_cost(
            ActionEstimate {
                strategy,
                cpu_seconds: self.local_elapsed_seconds,
                wire_bytes: self.wire.bytes,
                fragments: self.wire.fragments,
                loss_exposed_fragments: self.wire.loss_exposed_fragments,
                dependent_rounds: self.wire.rounds,
                failure_probability: self.failure_probability,
                fallback_seconds,
                temp_bytes: self.max_temp_bytes,
            },
            TransportState {
                rtt_seconds: profile.rtt_seconds,
                bandwidth_bytes_per_second: BANDWIDTH_BYTES_PER_SECOND,
                loss_probability: profile.loss_probability,
            },
        )
        .total_seconds()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassicMeasurement {
    pub lane: LaneMeasurement,
    pub messages: u64,
    pub enumerated_elements: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelfSizingMeasurement {
    pub first_cells: usize,
    pub lane: LaneMeasurement,
    pub mean_rounds: f64,
    pub mean_record_visits: f64,
    pub max_second_cells: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RibltMeasurement {
    pub prefix: usize,
    pub success: bool,
    pub build_local_elapsed_seconds: f64,
    pub stream_local_elapsed_seconds: f64,
    pub persistent_bytes_per_peer: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RibltProjection {
    pub steady_seconds: f64,
    pub ideal_stop_bytes: u64,
    pub line_rate_stop_bytes: u64,
    pub datagrams: u64,
    pub stop_overshoot_bytes: u64,
    pub expected_retry_rounds: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkProjection {
    pub profile: NetworkProfile,
    pub classic_seconds: f64,
    pub fixed_seconds: f64,
    pub self_sizing_seconds: [f64; 2],
    pub bulk_seconds: f64,
    pub expected_fixed_bytes: u64,
    pub expected_self_sizing_bytes: [u64; 2],
    pub riblt: RibltProjection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaseReport {
    pub scenario: Scenario,
    pub effective_difference: usize,
    pub classic: ClassicMeasurement,
    pub fixed: LaneMeasurement,
    pub self_sizing: [SelfSizingMeasurement; 2],
    pub bulk: LaneMeasurement,
    pub riblt: RibltMeasurement,
    pub networks: Vec<NetworkProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonReport {
    pub schema_version: u32,
    pub configuration: ComparisonConfiguration,
    pub cases: Vec<CaseReport>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProbePolicy {
    #[serde(rename = "m1-128")]
    Cells128,
    #[serde(rename = "m1-184")]
    Cells184,
}

impl ProbePolicy {
    pub fn first_cells(self) -> usize {
        match self {
            Self::Cells128 => 128,
            Self::Cells184 => 184,
        }
    }

    pub fn seconds(self, projection: &NetworkProjection) -> f64 {
        match self {
            Self::Cells128 => projection.self_sizing_seconds[0],
            Self::Cells184 => projection.self_sizing_seconds[1],
        }
    }
}
