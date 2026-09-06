//! `make bench-lease` — what lease issuance sustains, and what it costs.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language and a second surface would be a second thing to keep
//! in step with it. Nothing is positional and nothing is guessed: an unset
//! datastore URL is a refusal naming the variable, not a fallback to whatever
//! the developer's shell happens to export.

use core::time::Duration;
use std::process::ExitCode;

use afd_bench::datastores::{
    DATABASE_URL_VARIABLE, Datastores, REDIS_CA_CERT_VARIABLE, REDIS_URL_VARIABLE,
};
use afd_bench::error::Result;
use afd_bench::lane::{lease, sweep};
use afd_bench::profile::{Parameter, Profile};
use afd_bench::report::Lane;
use afd_bench::{Error, RunPrefix};

/// Which profile to run under.
const PROFILE_VARIABLE: &str = "BENCH_PROFILE";

/// How long the contended window may run.
const WINDOW_VARIABLE: &str = "BENCH_WINDOW_SECONDS";

/// Ready fleets to seed when the caller does not say.
const DEFAULT_FLEETS: u64 = 200;

/// Runners to enrol when the caller does not say.
///
/// Above the fleet count would make every poll contend and measure nothing but
/// contention; well below it would never contend at all. A quarter of the
/// population is enough of both to see each.
const DEFAULT_RUNNERS: u64 = 8;

/// How long the contended window runs when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 30;

#[tokio::main]
async fn main() -> ExitCode {
    match measure().await {
        Ok(path) => {
            // logging: a make target answers the person who ran it on stdout; no daemon is running here to carry an event.
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            // The whole chain, not just our sentence. Every variant composes
            // its cause with `#[from]` precisely so the statement Postgres
            // rejected or the knob Redis wanted is still reachable here.
            // logging: a refusal goes to stderr where the shell shows it; no subscriber is installed in this process.
            eprintln!("bench-lease refused: {refusal}");
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
    // Before a connection opens: the acknowledgement, the target, the caps.
    let _target = profile.admit(&|key: &str| std::env::var(key).ok())?;
    let parameters = lease::Parameters {
        fleets: number(Parameter::Fleets.name(), DEFAULT_FLEETS),
        runners: number(Parameter::Runners.name(), DEFAULT_RUNNERS),
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
    // The sweep runs on BOTH exit paths. A lane that swept only on success
    // would leave its whole population behind exactly when something went
    // wrong, which is when the mess is largest.
    let measured = lease::run(profile, parameters, &stores, &prefix).await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await?;
    let mut report = measured?;
    report.fixture.swept = swept;

    let path = Lane::Lease.result_path(profile);
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
