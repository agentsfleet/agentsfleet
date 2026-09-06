//! `make bench-outbound` — what one delivery worker sustains, and what a slow
//! destination costs the jobs behind it.
//!
//! Same knob discipline as every other lane: environment variables, nothing
//! positional, and no guessed datastore URL.

use core::time::Duration;
use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::datastores::{
    DATABASE_URL_VARIABLE, Datastores, REDIS_CA_CERT_VARIABLE, REDIS_URL_VARIABLE,
};
use afd_bench::error::Result;
use afd_bench::knobs::{fraction, number, required, variable};
use afd_bench::lane::{outbound, sweep};
use afd_bench::profile::{Parameter, Profile};
use afd_bench::report::Lane;

/// Which profile to run under.
const PROFILE_VARIABLE: &str = "BENCH_PROFILE";

/// How long the window may run.
const WINDOW_VARIABLE: &str = "BENCH_WINDOW_SECONDS";

/// Jobs to queue when the caller does not say.
const DEFAULT_JOBS: u64 = 200;

/// Fraction of destinations scripted slow when the caller does not say.
const SLOW_FRACTION_VARIABLE: &str = "BENCH_SLOW_FRACTION";

/// Fraction of destinations scripted retryable when the caller does not say.
const RETRYABLE_FRACTION_VARIABLE: &str = "BENCH_RETRYABLE_FRACTION";

/// One slow destination in sixteen, so head-of-line cost is visible without
/// dominating the drain.
const DEFAULT_SLOW_FRACTION: f64 = 0.0625;

/// No retryable destinations unless asked: the ladder costs seconds per job.
const DEFAULT_RETRYABLE_FRACTION: f64 = 0.0;

/// How long the window runs when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 60;

#[tokio::main]
async fn main() -> ExitCode {
    match measure().await {
        Ok(path) => {
            // logging: a make target answers the person who ran it on stdout; no daemon is running here to carry an event.
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            // logging: a refusal goes to stderr where the shell shows it; no subscriber is installed in this process.
            eprintln!("bench-outbound refused: {refusal}");
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

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = |key: &str| std::env::var(key).ok();
    let profile: Profile = variable(&env, PROFILE_VARIABLE)
        .unwrap_or_else(|| Profile::Rig.to_string())
        .parse()?;
    let _target = profile.admit(&env)?;
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

    let redis_url = required(&env, REDIS_URL_VARIABLE)?;
    let ca_cert = variable(&env, REDIS_CA_CERT_VARIABLE);
    let stores = Datastores::open(
        &required(&env, DATABASE_URL_VARIABLE)?,
        &redis_url,
        ca_cert.clone(),
    )
    .await?;

    let prefix = RunPrefix::mint();
    // The sweep runs on both exit paths: a lane that swept only on success
    // would leave its whole population behind exactly when something failed.
    let measured = outbound::run(profile, parameters, &stores, &redis_url, ca_cert, &prefix).await;
    let swept = sweep::outbound_stream(&stores.queue, &prefix).await?;
    let mut report = measured?;
    report.fixture.swept = swept;

    let path = Lane::Outbound.result_path(profile);
    report.write(&path)?;
    Ok(path.display().to_string())
}
