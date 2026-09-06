//! `make bench-steer` — what steer ingress accepts, and where it costs.
//!
//! Same knob discipline as every other lane: environment variables, nothing
//! positional, and no guessed datastore URL.

use core::time::Duration;
use std::process::ExitCode;

use afd_bench::datastores::{
    DATABASE_URL_VARIABLE, Datastores, REDIS_CA_CERT_VARIABLE, REDIS_URL_VARIABLE,
};
use afd_bench::error::Result;
use afd_bench::lane::{steer, sweep};
use afd_bench::profile::{Parameter, Profile};
use afd_bench::report::Lane;
use afd_bench::{Error, RunPrefix};

/// Which profile to run under.
const PROFILE_VARIABLE: &str = "BENCH_PROFILE";

/// How long the window may run.
const WINDOW_VARIABLE: &str = "BENCH_WINDOW_SECONDS";

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
    match measure().await {
        Ok(path) => {
            // logging: a make target answers the person who ran it on stdout; no daemon is running here to carry an event.
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            // logging: a refusal goes to stderr where the shell shows it; no subscriber is installed in this process.
            eprintln!("bench-steer refused: {refusal}");
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
    let profile: Profile = variable(PROFILE_VARIABLE)
        .unwrap_or_else(|| Profile::Rig.to_string())
        .parse()?;
    let _target = profile.admit(&|key: &str| std::env::var(key).ok())?;
    let parameters = steer::Parameters {
        fleets: number(Parameter::Fleets.name(), DEFAULT_FLEETS),
        concurrency: number(Parameter::Concurrency.name(), DEFAULT_CONCURRENCY),
        window: Duration::from_secs(number(WINDOW_VARIABLE, DEFAULT_WINDOW_SECONDS)),
    };
    parameters.admit(profile)?;

    let stores = Datastores::open(
        &required(DATABASE_URL_VARIABLE)?,
        &required(REDIS_URL_VARIABLE)?,
        variable(REDIS_CA_CERT_VARIABLE),
    )
    .await?;

    let prefix = RunPrefix::mint();
    // The sweep runs on both exit paths: a lane that swept only on success
    // would leave its whole population behind exactly when something failed.
    let measured = steer::run(profile, parameters, &stores, &prefix).await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await?;
    let mut report = measured?;
    report.fixture.swept = swept;

    let path = Lane::Steer.result_path(profile);
    report.write(&path)?;
    Ok(path.display().to_string())
}

/// An environment variable, or nothing.
fn variable(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

/// An environment variable that must be set.
fn required(key: &'static str) -> Result<String> {
    variable(key).ok_or(Error::VariableUnset { variable: key })
}

/// A numeric knob, or its default when unset or unreadable.
fn number(key: &str, fallback: u64) -> u64 {
    variable(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}
