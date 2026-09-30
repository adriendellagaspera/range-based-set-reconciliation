// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::env;
use std::fs::OpenOptions;
use std::io::{BufWriter, Error, ErrorKind};
use std::path::PathBuf;

use set_reconciliation_experiments::comparison::{run, write_report};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = output_path()?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    write_report(BufWriter::new(file), &run())?;
    eprintln!("wrote {}", output.display());
    Ok(())
}

fn output_path() -> Result<PathBuf, Error> {
    let mut args = env::args_os().skip(1);
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(path), None) if flag == "--output" => Ok(path.into()),
        _ => Err(Error::new(
            ErrorKind::InvalidInput,
            "usage: cargo bench --bench reconciliation -- --output REPORT.json",
        )),
    }
}
