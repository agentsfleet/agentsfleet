//! `make bench-cardinality`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::knobs::number;
use afd_bench::lane::{cardinality, sweep};
use afd_bench::profile::Parameter;
use afd_bench::report::Lane;
use tokio_util::sync::CancellationToken;

/// The ladder's top rung when the caller does not say.
///
/// Ten thousand: enough to see whether memory per fleet holds flat across
/// three rungs, small enough that seeding is minutes and not an afternoon.
/// The rig profile's cap is a million; ask for it with `BENCH_FLEETS`.
const DEFAULT_FLEETS: u64 = 10_000;

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-cardinality", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = cli::process_env();
    let (profile, target) = cli::admitted(&env)?;
    let parameters = cardinality::Parameters {
        fleets: number(&env, Parameter::Fleets.name(), DEFAULT_FLEETS)?,
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
        Box::pin(cardinality::run_cancelled(
            profile,
            &target,
            parameters,
            &stores,
            &prefix,
            cancellation,
        )),
    )
    .await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await;
    cli::finish(Lane::Cardinality, profile, measured, swept)
}
