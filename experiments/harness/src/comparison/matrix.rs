use devkit::protocol_cost::{MAX_DATAGRAM_PAYLOAD, MTU_FRAGMENT_PAYLOAD};
use serde::{Deserialize, Serialize};

use super::transport::CODED_SYMBOL_BYTES;
use set_reconciliation_comparators::iblt::transition::CELL_BYTES;

pub const REPORT_SCHEMA_VERSION: u32 = 2;
pub const FIXED_CAPACITY: usize = 184;
pub const PROBES: [usize; 2] = [128, 184];
pub const TRIALS: u64 = 16;
pub const BANDWIDTH_BYTES_PER_SECOND: f64 = 125_000_000.0;
const SELF_SIZING_REFERENCE: &str = "whitewum/self-sizing@4ba615ca76978564d5d7d4b75424a680cae6f21d";
const RIBLT_REFERENCE: &str = "yangl1996/riblt@4afa6bc06cb2237d9ea273a51d97a7e05b3f573b";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DifferenceShape {
    DeletionScattered,
    DeletionClustered,
    UpdatedScattered,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioId {
    Needle,
    BroadScattered,
    BroadClustered,
    UpdatedEqualCount,
    #[serde(rename = "probe-boundary-128")]
    ProbeBoundary128,
    #[serde(rename = "safe-cap-boundary-184")]
    SafeCapBoundary184,
    BulkRegime,
}

impl ScenarioId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Needle => "needle",
            Self::BroadScattered => "broad-scattered",
            Self::BroadClustered => "broad-clustered",
            Self::UpdatedEqualCount => "updated-equal-count",
            Self::ProbeBoundary128 => "probe-boundary-128",
            Self::SafeCapBoundary184 => "safe-cap-boundary-184",
            Self::BulkRegime => "bulk-regime",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: ScenarioId,
    pub records: usize,
    pub changed_keys: usize,
    pub shape: DifferenceShape,
}

const fn scenario(
    id: ScenarioId,
    records: usize,
    changed_keys: usize,
    shape: DifferenceShape,
) -> Scenario {
    Scenario {
        id,
        records,
        changed_keys,
        shape,
    }
}

pub const CASES: [Scenario; 7] = [
    scenario(
        ScenarioId::Needle,
        1_000_000,
        1,
        DifferenceShape::DeletionScattered,
    ),
    scenario(
        ScenarioId::BroadScattered,
        100_000,
        100,
        DifferenceShape::DeletionScattered,
    ),
    scenario(
        ScenarioId::BroadClustered,
        100_000,
        100,
        DifferenceShape::DeletionClustered,
    ),
    scenario(
        ScenarioId::UpdatedEqualCount,
        100_000,
        100,
        DifferenceShape::UpdatedScattered,
    ),
    scenario(
        ScenarioId::ProbeBoundary128,
        100_000,
        128,
        DifferenceShape::DeletionScattered,
    ),
    scenario(
        ScenarioId::SafeCapBoundary184,
        100_000,
        184,
        DifferenceShape::DeletionScattered,
    ),
    scenario(
        ScenarioId::BulkRegime,
        10_000,
        5_000,
        DifferenceShape::DeletionScattered,
    ),
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkId {
    Loopback,
    Lan,
    CleanWan,
    LossyWan,
}

impl NetworkId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Lan => "lan",
            Self::CleanWan => "clean-wan",
            Self::LossyWan => "lossy-wan",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkProfile {
    pub id: NetworkId,
    pub rtt_seconds: f64,
    pub loss_probability: f64,
}

pub const NETWORKS: [NetworkProfile; 4] = [
    NetworkProfile {
        id: NetworkId::Loopback,
        rtt_seconds: 0.0,
        loss_probability: 0.0,
    },
    NetworkProfile {
        id: NetworkId::Lan,
        rtt_seconds: 0.001,
        loss_probability: 0.0,
    },
    NetworkProfile {
        id: NetworkId::CleanWan,
        rtt_seconds: 0.050,
        loss_probability: 0.0,
    },
    NetworkProfile {
        id: NetworkId::LossyWan,
        rtt_seconds: 0.050,
        loss_probability: 0.01,
    },
];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonConfiguration {
    pub trials: u64,
    pub fixed_capacity: usize,
    pub probes: [usize; 2],
    pub bandwidth_bytes_per_second: f64,
    pub max_datagram_payload: usize,
    pub mtu_fragment_payload: usize,
    pub iblt_cell_bytes: u64,
    pub riblt_coded_symbol_bytes: u64,
    pub self_sizing_reference: String,
    pub riblt_reference: String,
}

impl ComparisonConfiguration {
    pub(super) fn current() -> Self {
        Self {
            trials: TRIALS,
            fixed_capacity: FIXED_CAPACITY,
            probes: PROBES,
            bandwidth_bytes_per_second: BANDWIDTH_BYTES_PER_SECOND,
            max_datagram_payload: MAX_DATAGRAM_PAYLOAD,
            mtu_fragment_payload: MTU_FRAGMENT_PAYLOAD,
            iblt_cell_bytes: CELL_BYTES,
            riblt_coded_symbol_bytes: CODED_SYMBOL_BYTES,
            self_sizing_reference: SELF_SIZING_REFERENCE.to_owned(),
            riblt_reference: RIBLT_REFERENCE.to_owned(),
        }
    }
}
