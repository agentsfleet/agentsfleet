//! `make bench-lease`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::datastores::Datastores;
use afd_bench::error::Result;
use afd_bench::lane::lease::{self, drain};
use afd_bench::lane::sweep;
use afd_bench::profile::Profile;
use afd_bench::report::{Lane, Provenance, Report};
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
    let (profile, _target, provenance) = cli::admitted(&env)?;
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
    let measured = measure_both(profile, provenance, parameters, &stores, &prefix).await;
    if let Ok(report) = &measured {
        // logging: the make target's output is what the acceptance rubric reads; no daemon runs here to carry an event.
        println!("{}", drain::summary(report));
    }
    let swept = sweep::everything(&stores.database, &stores.queue, &prefix).await;
    cli::finish(Lane::Lease, profile, measured, swept)
}

/// The drain, then the contended window, as one report.
///
/// On a rig the run owns, the readiness index is emptied before anything is
/// seeded, so the idle cost measured is this run's. The drain goes FIRST. The contended window leases and never reports, so
/// its fleets end it claimed and marked for a claim's whole lifetime; a drain
/// after it would poll those marks, and its idle cost would be theirs. Each
/// population is its own, so the contended numbers stay comparable with
/// every earlier baseline either way.
async fn measure_both(
    profile: Profile,
    provenance: Provenance,
    parameters: lease::Parameters,
    stores: &Datastores,
    prefix: &RunPrefix,
) -> Result<Report> {
    if provenance.owned {
        drain::reset_readiness(stores).await?;
    }
    let mut drained = Report::new(Lane::Lease, profile, provenance.clone());
    drain::run(profile, parameters, stores, prefix, &mut drained).await?;
    let mut report = lease::run(profile, provenance, parameters, stores, prefix).await?;
    report.measurements.append(&mut drained.measurements);
    report.fixture.created += drained.fixture.created;
    Ok(report)
}
