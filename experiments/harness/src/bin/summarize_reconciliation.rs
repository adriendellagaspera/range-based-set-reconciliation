// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind};

use set_reconciliation_experiments::comparison::{
    best_static_probe, read_report, totals, ComparisonReport, ProbePolicy,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = input_path()?;
    let report = read_report(BufReader::new(File::open(path)?))?;
    render(&report);
    Ok(())
}

fn input_path() -> Result<std::ffi::OsString, Error> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next()) {
        (Some(path), None) => Ok(path),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            "usage: summarize_reconciliation REPORT.json",
        )),
    }
}

fn render(report: &ComparisonReport) {
    let probe = best_static_probe(report);
    println!("# Reconciliation microbenchmark");
    println!();
    println!("Times below are network projections in seconds, not measured transport latency.");
    println!(
        "Local timings use elapsed time, not process CPU. Preparation is unequal across arms."
    );
    println!("The selected probe is an in-sample oracle, not a validated deployment policy.");
    println!();
    println!(
        "Post-hoc self-sizing probe selected by aggregate projected time: M1={}.",
        probe.first_cells()
    );
    println!();
    println!(
        "| case | network | classic | fixed→RBSR | ss128 | ss184 | chosen SS | RIBLT steady | bulk | RIBLT ideal/line-rate bytes |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for case in &report.cases {
        for row in &case.networks {
            let riblt = if case.riblt.success {
                format!("{:.6}", row.riblt.steady_seconds)
            } else {
                "unavailable: decode failed".to_owned()
            };
            println!(
                "| {} | {} | {:.6} | {:.6} | {:.6} | {:.6} | {:.6} | {} | {:.6} | {}/{} |",
                case.scenario.id.as_str(),
                row.profile.id.as_str(),
                row.classic_seconds,
                row.fixed_seconds,
                row.self_sizing_seconds[0],
                row.self_sizing_seconds[1],
                probe.seconds(row),
                riblt,
                row.bulk_seconds,
                row.riblt.ideal_stop_bytes,
                row.riblt.line_rate_stop_bytes,
            );
        }
    }
    render_totals(report, probe);
    render_second_sketch_caps(report, probe);
}

fn render_totals(report: &ComparisonReport, probe: ProbePolicy) {
    let totals = totals(report, probe);
    let riblt = if report.cases.iter().all(|case| case.riblt.success) {
        format!("{:.6}s", totals.riblt_steady_seconds)
    } else {
        "unavailable: decode failed".to_owned()
    };
    println!();
    println!("## Aggregate across the preregistered cells");
    println!();
    println!(
        "classic={:.6}s fixed={:.6}s ss128={:.6}s ss184={:.6}s chosen_ss={:.6}s \
         riblt_steady={} bulk={:.6}s",
        totals.classic_seconds,
        totals.fixed_seconds,
        totals.self_sizing_seconds[0],
        totals.self_sizing_seconds[1],
        totals.selected_self_sizing_seconds,
        riblt,
        totals.bulk_seconds,
    );
}

fn render_second_sketch_caps(report: &ComparisonReport, probe: ProbePolicy) {
    println!();
    println!("## Fresh-M2 rejection ceiling vs classic RBSR");
    println!();
    println!("| case | after M1=128 | after M1=184 | selected policy |");
    println!("|---|---:|---:|---:|");
    for case in &report.cases {
        let selected = match probe {
            ProbePolicy::Cells128 => case.self_sizing[0].max_second_cells,
            ProbePolicy::Cells184 => case.self_sizing[1].max_second_cells,
        };
        println!(
            "| {} | {} | {} | {} |",
            case.scenario.id.as_str(),
            case.self_sizing[0].max_second_cells,
            case.self_sizing[1].max_second_cells,
            selected,
        );
    }
}
