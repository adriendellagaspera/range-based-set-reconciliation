use std::env;
use std::fs::File;
use std::io::{self, BufReader, Error, ErrorKind};

use set_reconciliation_experiments::comparison::experiment::{read_experiment_report, write_experiment_summary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let path = match (args.next(), args.next()) {
        (Some(path), None) => path,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "usage: summarize_experiment REPORT.json",
            )
            .into())
        }
    };
    let report = read_experiment_report(BufReader::new(File::open(path)?))?;
    write_experiment_summary(io::stdout().lock(), &report)?;
    Ok(())
}
