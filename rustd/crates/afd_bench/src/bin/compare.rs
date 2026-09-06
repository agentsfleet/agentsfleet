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

/// How the two positional arguments are spelled, for the refusal.
const USAGE: &str = "usage: compare <steer|lease|outbound|cardinality> <rig|dev|prod>";

fn main() -> ExitCode {
    match rendered() {
        Ok(delta) => {
            // logging: the comparison IS this command's output, printed for a reader rather than emitted as a signal.
            print!("{delta}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            // logging: a refusal goes to stderr where the shell shows it; no subscriber is installed in this process.
            eprintln!("bench-compare refused: {refusal}");
            let mut cause: Option<&dyn core::error::Error> = core::error::Error::source(&refusal);
            while let Some(reason) = cause {
                // logging: the cause chain belongs on the same stream as the refusal it explains.
                eprintln!("  caused by: {reason}");
                cause = reason.source();
            }
            ExitCode::FAILURE
        }
    }
}

/// Parse the two arguments and render the comparison.
fn rendered() -> Result<String> {
    let mut arguments = std::env::args().skip(1);
    let lane = lane(arguments.next().unwrap_or_default().as_str())?;
    let profile: Profile = arguments.next().unwrap_or_default().parse()?;
    compare::against_baseline(&lane.result_path(profile), &lane.baseline_path(profile))
}

/// A lane name, or a refusal that says what the names are.
fn lane(name: &str) -> Result<Lane> {
    [Lane::Steer, Lane::Lease, Lane::Outbound, Lane::Cardinality]
        .into_iter()
        .find(|candidate| candidate.name() == name)
        .ok_or(afd_bench::Error::UnknownLane { usage: USAGE })
}
