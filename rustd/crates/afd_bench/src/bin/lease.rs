//! `make bench-lease`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::lane::{lease, sweep};
use afd_bench::report::Lane;
use core::time::Duration;

use afd_bench::knobs::{WINDOW_VARIABLE, number};
use afd_bench::profile::Parameter;

/// Ready fleets to seed when the caller does not say.
const DEFAULT_FLEETS: u64 = 200;

/// Runners to enrol when the caller does not say.
///
/// Above the fleet count would make every poll contend and measure nothing but
/// contention; well below it would never contend at all. A twenty-fifth of the
/// population is enough of both to see each.
const DEFAULT_RUNNERS: u64 = 8;

/// How long the contended window may run when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 30;

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-lease", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = cli::process_env();
    let (profile, _target) = cli::admitted(&env)?;
    let parameters = lease::Parameters {
        fleets: number(&env, Parameter::Fleets.name(), DEFAULT_FLEETS)?,
        runners: number(&env, Parameter::Runners.name(), DEFAULT_RUNNERS)?,
        window: Duration::from_secs(number(&env, WINDOW_VARIABLE, DEFAULT_WINDOW_SECONDS)?),
    };
    parameters.admit(profile)?;

    let stores = cli::datastores(&env).await?;
    let prefix = RunPrefix::mint();
    // The sweep runs whether the lane succeeded or not; `cli::finish` reports
    // the lane's failure first when both failed.
    let measured = lease::run(profile, parameters, &stores, &prefix).await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await;
    cli::finish(Lane::Lease, profile, measured, swept)
}
