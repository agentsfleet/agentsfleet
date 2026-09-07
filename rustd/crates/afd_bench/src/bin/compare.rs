//! `make bench-compare` — how a result moved against its committed baseline.
//!
//! # This never fails on a number
//!
//! Whichever direction a measurement went, this exits zero. The lanes run on
//! shared runners, and a gate on throughput would fire on a noisy neighbour as
//! readily as on a regression — after which it gets muted, and the real
//! regression sails through the muted gate. The one non-zero exit is a result
//! file that will not parse, which is an absent measurement rather than a
//! disappointing one.

use std::process::ExitCode;

use afd_bench::error::Result;
use afd_bench::profile::Profile;
use afd_bench::report::{Lane, compare};

fn main() -> ExitCode {
    match rendered() {
        Ok(delta) => {
            // logging: the comparison IS this command's output, printed for a reader rather than emitted as a signal.
            print!("{delta}");
            ExitCode::SUCCESS
        }
        Err(refusal) => afd_bench::cli::exit("bench-compare", Err(refusal)),
    }
}

/// Parse the two arguments and render the comparison.
fn rendered() -> Result<String> {
    let mut arguments = std::env::args().skip(1);
    let lane: Lane = arguments.next().unwrap_or_default().parse()?;
    let profile: Profile = arguments.next().unwrap_or_default().parse()?;
    compare::against_baseline(&lane.result_path(profile), &lane.baseline_path(profile))
}
