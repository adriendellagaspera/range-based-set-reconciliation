use std::error::Error;
use std::fmt;
use std::io::{Read, Write};

use super::matrix::{ComparisonConfiguration, CASES, NETWORKS, PROBES, REPORT_SCHEMA_VERSION};
use super::model::{ComparisonReport, LaneMeasurement, NetworkProjection};

#[derive(Debug)]
pub struct InvalidReport {
    message: String,
}

impl InvalidReport {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InvalidReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for InvalidReport {}

#[derive(Debug)]
pub enum ArtifactError {
    Json(serde_json::Error),
    Invalid(InvalidReport),
    Io(std::io::Error),
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid comparison JSON: {error}"),
            Self::Invalid(error) => write!(formatter, "invalid comparison report: {error}"),
            Self::Io(error) => write!(formatter, "comparison artifact I/O failed: {error}"),
        }
    }
}

impl Error for ArtifactError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Invalid(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for ArtifactError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<InvalidReport> for ArtifactError {
    fn from(error: InvalidReport) -> Self {
        Self::Invalid(error)
    }
}

impl From<std::io::Error> for ArtifactError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn read_report(reader: impl Read) -> Result<ComparisonReport, ArtifactError> {
    let report = serde_json::from_reader(reader)?;
    validate(&report)?;
    Ok(report)
}

pub fn write_report(
    mut writer: impl Write,
    report: &ComparisonReport,
) -> Result<(), ArtifactError> {
    validate(report)?;
    serde_json::to_writer_pretty(&mut writer, report)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn validate(report: &ComparisonReport) -> Result<(), InvalidReport> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return invalid(format!(
            "unsupported schema version {}, expected {REPORT_SCHEMA_VERSION}",
            report.schema_version
        ));
    }
    validate_configuration(report)?;
    if report.cases.len() != CASES.len() {
        return invalid(format!(
            "expected {} scenarios, found {}",
            CASES.len(),
            report.cases.len()
        ));
    }
    for (case, expected) in report.cases.iter().zip(CASES) {
        if case.scenario != expected {
            return invalid(format!(
                "scenario {} does not match the preregistered matrix",
                case.scenario.id.as_str()
            ));
        }
        let expected_difference = match case.scenario.shape {
            super::DifferenceShape::UpdatedScattered => case.scenario.changed_keys * 2,
            _ => case.scenario.changed_keys,
        };
        if case.effective_difference != expected_difference {
            return invalid(format!(
                "scenario {} has inconsistent effective difference",
                case.scenario.id.as_str()
            ));
        }
        validate_lane(case.classic.lane, "classic")?;
        validate_lane(case.fixed, "fixed")?;
        validate_lane(case.bulk, "bulk")?;
        for (measurement, first_cells) in case.self_sizing.iter().zip(PROBES) {
            if measurement.first_cells != first_cells {
                return invalid(format!(
                    "scenario {} has unexpected self-sizing probe {}",
                    case.scenario.id.as_str(),
                    measurement.first_cells
                ));
            }
            validate_lane(measurement.lane, "self-sizing")?;
            non_negative_finite(measurement.mean_rounds, "mean rounds")?;
            non_negative_finite(measurement.mean_record_visits, "mean record visits")?;
        }
        non_negative_finite(
            case.riblt.build_local_elapsed_seconds,
            "RIBLT build elapsed",
        )?;
        non_negative_finite(
            case.riblt.stream_local_elapsed_seconds,
            "RIBLT stream elapsed",
        )?;
        if case.networks.len() != NETWORKS.len() {
            return invalid(format!(
                "scenario {} expected {} network rows, found {}",
                case.scenario.id.as_str(),
                NETWORKS.len(),
                case.networks.len()
            ));
        }
        for (projection, expected_profile) in case.networks.iter().zip(NETWORKS) {
            if projection.profile != expected_profile {
                return invalid(format!(
                    "scenario {} has a network row outside the preregistered matrix",
                    case.scenario.id.as_str()
                ));
            }
            validate_projection(projection)?;
        }
    }
    Ok(())
}

fn validate_configuration(report: &ComparisonReport) -> Result<(), InvalidReport> {
    if report.configuration != ComparisonConfiguration::current() {
        return invalid("configuration does not match the registered comparison contract");
    }
    Ok(())
}

fn validate_lane(lane: LaneMeasurement, label: &str) -> Result<(), InvalidReport> {
    non_negative_finite(lane.local_elapsed_seconds, label)?;
    probability(lane.failure_probability, label)?;
    if lane.wire.loss_exposed_fragments > lane.wire.fragments {
        return invalid(format!("{label} exposes more fragments than it transmits"));
    }
    Ok(())
}

fn validate_projection(projection: &NetworkProjection) -> Result<(), InvalidReport> {
    for (value, label) in [
        (projection.classic_seconds, "classic projection"),
        (projection.fixed_seconds, "fixed projection"),
        (
            projection.self_sizing_seconds[0],
            "self-sizing 128 projection",
        ),
        (
            projection.self_sizing_seconds[1],
            "self-sizing 184 projection",
        ),
        (projection.bulk_seconds, "bulk projection"),
        (projection.riblt.steady_seconds, "RIBLT projection"),
        (
            projection.riblt.expected_retry_rounds,
            "RIBLT expected retries",
        ),
    ] {
        non_negative_finite(value, label)?;
    }
    Ok(())
}

fn probability(value: f64, label: &str) -> Result<(), InvalidReport> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        invalid(format!("{label} has invalid probability {value}"))
    }
}

fn non_negative_finite(value: f64, label: &str) -> Result<(), InvalidReport> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        invalid(format!("{label} must be finite and non-negative"))
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, InvalidReport> {
    Err(InvalidReport::new(message))
}
