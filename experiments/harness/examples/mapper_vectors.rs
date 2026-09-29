// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Line-oriented mapper-v2 interoperability adapter.
//!
//! Input is `<fingerprint-hex> <capacity-decimal> <seed-hex>`; output is the three decimal cell
//! positions. It exists so the pinned Go artifact can be compared without giving this research
//! crate a JSON or command-line dependency.

use std::io::{self, BufRead};

use set_reconciliation_experiments::iblt::positions;

fn main() {
    for (line_number, line) in io::stdin().lock().lines().enumerate() {
        let line = line.unwrap_or_else(|error| panic!("line {}: {error}", line_number + 1));
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(
            fields.len(),
            3,
            "line {}: expected three fields",
            line_number + 1
        );
        let fingerprint = u64::from_str_radix(fields[0].trim_start_matches("0x"), 16)
            .unwrap_or_else(|error| panic!("line {} fingerprint: {error}", line_number + 1));
        let capacity = fields[1]
            .parse::<usize>()
            .unwrap_or_else(|error| panic!("line {} capacity: {error}", line_number + 1));
        let seed = u64::from_str_radix(fields[2].trim_start_matches("0x"), 16)
            .unwrap_or_else(|error| panic!("line {} seed: {error}", line_number + 1));
        let [a, b, c] = positions(fingerprint, capacity, seed)
            .unwrap_or_else(|error| panic!("line {}: {error:?}", line_number + 1));
        println!("{a} {b} {c}");
    }
}
