use super::model::{ComparisonReport, ProbePolicy};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Totals {
    pub classic_seconds: f64,
    pub fixed_seconds: f64,
    pub self_sizing_seconds: [f64; 2],
    pub selected_self_sizing_seconds: f64,
    pub riblt_steady_seconds: f64,
    pub bulk_seconds: f64,
}

pub fn best_static_probe(report: &ComparisonReport) -> ProbePolicy {
    let totals = report.cases.iter().flat_map(|case| &case.networks).fold(
        [0.0, 0.0],
        |mut totals, projection| {
            totals[0] += projection.self_sizing_seconds[0];
            totals[1] += projection.self_sizing_seconds[1];
            totals
        },
    );
    if totals[0] <= totals[1] {
        ProbePolicy::Cells128
    } else {
        ProbePolicy::Cells184
    }
}

pub fn totals(report: &ComparisonReport, probe: ProbePolicy) -> Totals {
    report.cases.iter().flat_map(|case| &case.networks).fold(
        Totals {
            classic_seconds: 0.0,
            fixed_seconds: 0.0,
            self_sizing_seconds: [0.0, 0.0],
            selected_self_sizing_seconds: 0.0,
            riblt_steady_seconds: 0.0,
            bulk_seconds: 0.0,
        },
        |mut totals, projection| {
            totals.classic_seconds += projection.classic_seconds;
            totals.fixed_seconds += projection.fixed_seconds;
            totals.self_sizing_seconds[0] += projection.self_sizing_seconds[0];
            totals.self_sizing_seconds[1] += projection.self_sizing_seconds[1];
            totals.selected_self_sizing_seconds += probe.seconds(projection);
            totals.riblt_steady_seconds += projection.riblt.steady_seconds;
            totals.bulk_seconds += projection.bulk_seconds;
            totals
        },
    )
}
