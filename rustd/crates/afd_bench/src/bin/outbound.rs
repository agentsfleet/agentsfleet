//! `make bench-outbound`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::lane::{outbound, sweep};
use afd_bench::report::Lane;
use core::time::Duration;

use afd_bench::knobs::{
    RETRYABLE_FRACTION_VARIABLE, SLOW_FRACTION_VARIABLE, WINDOW_VARIABLE, fraction, number,
};
use afd_bench::profile::Parameter;

/// Jobs to queue when the caller does not say.
const DEFAULT_JOBS: u64 = 200;

/// One slow destination in sixteen, so head-of-line cost is visible without
/// dominating the drain.
const DEFAULT_SLOW_FRACTION: f64 = 0.0625;

/// No refusing destinations unless asked: the ladder costs seconds per job.
const DEFAULT_RETRYABLE_FRACTION: f64 = 0.0;

/// How long the drain may run when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 60;

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-outbound", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = cli::process_env();
    let (profile, _target) = cli::admitted(&env)?;
    let parameters = outbound::Parameters {
        jobs: number(&env, Parameter::Jobs.name(), DEFAULT_JOBS)?,
        slow_fraction: fraction(&env, SLOW_FRACTION_VARIABLE, DEFAULT_SLOW_FRACTION)?,
        retryable_fraction: fraction(
            &env,
            RETRYABLE_FRACTION_VARIABLE,
            DEFAULT_RETRYABLE_FRACTION,
        )?,
        window: Duration::from_secs(number(&env, WINDOW_VARIABLE, DEFAULT_WINDOW_SECONDS)?),
    };
    parameters.admit(profile)?;

    let stores = cli::datastores(&env).await?;
    let prefix = RunPrefix::mint();
    // The sweep runs whether the lane succeeded or not; `cli::finish` reports
    // the lane's failure first when both failed.
    let measured = outbound::run(profile, parameters, &stores, &prefix).await;
    let swept = sweep::outbound_stream(&stores.queue, &prefix).await;
    cli::finish(Lane::Outbound, profile, measured, swept)
}
