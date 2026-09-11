//! `make bench-steer`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::lane::{steer, sweep};
use afd_bench::report::Lane;
use core::time::Duration;

use afd_bench::knobs::{WINDOW_VARIABLE, number};
use afd_bench::profile::Parameter;
use tokio_util::sync::CancellationToken;

/// Fleets to spread steers across when the caller does not say.
///
/// Several, because a single fleet would measure one Redis stream's key rather
/// than the ingress path: appends to one key serialise on the server.
const DEFAULT_FLEETS: u64 = 50;

/// Submitters appending at once when the caller does not say.
const DEFAULT_CONCURRENCY: u64 = 8;

/// How long the window runs when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 15;

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-steer", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = cli::process_env();
    let (profile, target) = cli::admitted(&env)?;
    let parameters = steer::Parameters {
        fleets: number(&env, Parameter::Fleets.name(), DEFAULT_FLEETS)?,
        concurrency: number(&env, Parameter::Concurrency.name(), DEFAULT_CONCURRENCY)?,
        window: Duration::from_secs(number(&env, WINDOW_VARIABLE, DEFAULT_WINDOW_SECONDS)?),
    };
    parameters.admit(profile)?;

    let stores = cli::datastores(profile, &target, &env).await?;
    let prefix = RunPrefix::mint();
    cli::announce_prefix(&prefix);
    let cancellation = CancellationToken::new();
    // The sweep runs whether the lane succeeded or not; `cli::finish` reports
    // the lane's failure first when both failed.
    let measured = cli::cancellable(
        cancellation.clone(),
        Box::pin(steer::run_cancelled(
            profile,
            parameters,
            &stores,
            &prefix,
            cancellation,
        )),
    )
    .await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await;
    cli::finish(Lane::Steer, profile, measured, swept)
}
