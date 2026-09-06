//! `make bench-cardinality` — what an idle fleet costs, to a million on the rig.
//!
//! Same knob discipline as every other lane: environment variables, nothing
//! positional, and no guessed datastore URL.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::datastores::{
    DATABASE_URL_VARIABLE, Datastores, REDIS_CA_CERT_VARIABLE, REDIS_URL_VARIABLE,
};
use afd_bench::error::Result;
use afd_bench::knobs::{number, required, variable};
use afd_bench::lane::{cardinality, sweep};
use afd_bench::profile::{Parameter, Profile};
use afd_bench::report::Lane;

/// Which profile to run under.
const PROFILE_VARIABLE: &str = "BENCH_PROFILE";

/// The ladder's top rung when the caller does not say.
///
/// Ten thousand: enough to see whether memory per fleet holds flat across
/// three rungs, small enough that seeding is minutes and not an afternoon.
/// The rig profile's cap is a million; ask for it with `BENCH_FLEETS`.
const DEFAULT_FLEETS: u64 = 10_000;

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
            eprintln!("bench-cardinality refused: {refusal}");
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
    let target = profile.admit(&env)?;
    let parameters = cardinality::Parameters {
        fleets: number(&env, Parameter::Fleets.name(), DEFAULT_FLEETS)?,
    };
    parameters.admit(profile)?;

    let stores = Datastores::open(
        &required(&env, DATABASE_URL_VARIABLE)?,
        &required(&env, REDIS_URL_VARIABLE)?,
        variable(&env, REDIS_CA_CERT_VARIABLE),
    )
    .await?;

    let prefix = RunPrefix::mint();
    // The sweep runs on both exit paths: a lane that swept only on success
    // would leave its whole population behind exactly when something failed.
    let measured = cardinality::run(profile, &target, parameters, &stores, &prefix).await;
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await?;
    let mut report = measured?;
    report.fixture.swept = swept;

    let path = Lane::Cardinality.result_path(profile);
    report.write(&path)?;
    Ok(path.display().to_string())
}
