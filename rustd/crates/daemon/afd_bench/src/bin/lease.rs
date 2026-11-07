//! `make bench-lease`.
//!
//! Every knob is an environment variable, because the make target already
//! speaks that language; the preamble every lane shares lives in
//! `afd_bench::cli`, and this file owns only what this lane asks for.

use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::lane::lease::drain::burst::{self, Burst};
use afd_bench::lane::lease::shape::Shape;
use afd_bench::lane::lease::{self, drain, saturation};
use afd_bench::lane::sweep;
use afd_bench::report::Lane;
use afd_core::env::ProcessEnv;
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

/// Requesters per pool connection under the saturation shape, when the
/// caller does not say how many.
const SATURATION_FACTOR: u64 = 2;

/// How long the contended window may run when the caller does not say.
const DEFAULT_WINDOW_SECONDS: u64 = 30;

/// Fleets a burst paces steers into when the caller does not say: the fifty
/// bursting at once the push-scheduler targets were set against (`M241_001` §5).
const BURST_FLEETS: u64 = 50;

/// A burst's window when the caller does not say: thirty thousand steers at
/// the default rate, under the deployment's backlog budget.
const BURST_WINDOW_SECONDS: u64 = 60;

/// Steers a second per fleet in a burst when the caller does not say.
const BURST_RATE: u64 = 10;

/// How long a burst's runner holds each lease: one short chat turn.
const BURST_TURN: Duration = Duration::from_secs(2);

/// One simulated turn when the caller does not say: two renewals and forty
/// activity posts per lease at today's timing (`M241_001` §4).
const DEFAULT_TURN: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-lease", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = ProcessEnv;
    let (profile, _target, provenance) = cli::admitted(&env)?;
    let shape = Shape::read(&env)?;
    shape.admit(profile)?;
    let mut parameters = lease::Parameters {
        fleets: number(&env, Parameter::Fleets.name(), DEFAULT_FLEETS)?,
        runners: number(&env, Parameter::Runners.name(), DEFAULT_RUNNERS)?,
        window: Duration::from_secs(number(&env, WINDOW_VARIABLE, DEFAULT_WINDOW_SECONDS)?),
        turn: Duration::from_millis(number(
            &env,
            Parameter::TurnMs.name(),
            u64::try_from(DEFAULT_TURN.as_millis()).unwrap_or(u64::MAX),
        )?),
    };
    parameters.admit(profile)?;

    if shape == Shape::Burst {
        parameters.fleets = number(&env, Parameter::Fleets.name(), BURST_FLEETS)?;
        parameters.window =
            Duration::from_secs(number(&env, WINDOW_VARIABLE, BURST_WINDOW_SECONDS)?);
        parameters.turn = Duration::from_millis(number(
            &env,
            Parameter::TurnMs.name(),
            u64::try_from(BURST_TURN.as_millis()).unwrap_or(u64::MAX),
        )?);
        parameters.admit(profile)?;
    }
    let burst = Burst {
        rate: number(&env, Parameter::Rate.name(), BURST_RATE)?,
    };

    let stores = cli::datastores(&env).await?;
    if shape == Shape::Saturation {
        // More requesters than connections is the shape's whole point, so its
        // default is read off the pool it was handed.
        let saturating = SATURATION_FACTOR * u64::from(stores.pool_size);
        parameters.runners = number(&env, Parameter::Runners.name(), saturating)?;
        parameters.admit(profile)?;
    }
    let prefix = RunPrefix::mint();
    // The sweep runs whether the lane succeeded or not; `cli::finish` reports
    // the lane's failure first when both failed.
    let (lane, measured) = match shape {
        Shape::Default => (
            Lane::Lease,
            lease::run_both(profile, provenance, parameters, &stores, &prefix).await,
        ),
        Shape::Saturation => (
            Lane::LeaseSaturation,
            saturation::run(profile, provenance, parameters, &stores, &prefix).await,
        ),
        Shape::Burst => (
            Lane::LeaseBurst,
            burst::run(profile, provenance, parameters, burst, &stores, &prefix).await,
        ),
    };
    if let Ok(report) = &measured {
        let summary = match shape {
            Shape::Default => drain::summary(report),
            Shape::Saturation => saturation::summary(report),
            Shape::Burst => burst::summary(report),
        };
        // the make target's output is what the acceptance rubric reads; no daemon runs here to carry an event.
        cli::show(&summary);
    }
    let swept = sweep::everything(&stores.database, &prefix).await;
    cli::finish(lane, profile, measured, swept)
}
