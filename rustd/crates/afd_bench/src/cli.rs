//! What every lane binary does the same way, once.
//!
//! Five binaries used to carry the same `main`, the same refusal printer and
//! the same preamble — profile, acknowledgement, datastores, prefix, run,
//! sweep, write. A knob rename was five edits and a refusal-format change was
//! five more. Each binary now owns only its `Parameters` and its `run` call;
//! the rest lives here, with its tests.
//!
//! # A lane failure and a sweep failure are reported in that order
//!
//! The sweep runs whether the lane succeeded or not, but when BOTH fail the
//! lane's error is the one a reader needs: a sweep that could not reach the
//! datastore is usually a symptom of the outage that failed the lane. So the
//! sweep's result is bound, never `?`-propagated ahead of the measurement's.

use std::process::ExitCode;

use tokio_util::sync::CancellationToken;

use crate::RunPrefix;
use crate::datastores::{
    DATABASE_URL_VARIABLE, Datastores, REDIS_CA_CERT_VARIABLE, REDIS_URL_VARIABLE,
};
use crate::error::Result;
use crate::knobs::{required, variable};
use crate::profile::{PROFILE_VARIABLE, Profile, Target};
use crate::report::{Lane, Report};

/// The environment as a lookup over the real process, for every reader.
pub fn process_env() -> impl Fn(&str) -> Option<String> {
    |key: &str| std::env::var(key).ok()
}

/// The profile a lane runs under, with its target admitted.
///
/// # Errors
///
/// An unknown profile name, a production run without its acknowledgement,
/// or a deployed profile with nowhere to point.
pub fn admitted(env: &dyn Fn(&str) -> Option<String>) -> Result<(Profile, Target)> {
    let profile: Profile = variable(env, PROFILE_VARIABLE)
        .unwrap_or_else(|| Profile::Rig.to_string())
        .parse()?;
    let target = profile.admit(env)?;
    Ok((profile, target))
}

/// Both datastores, from the lane's three variables.
///
/// # Errors
///
/// A variable that is unset, or a datastore that would not answer.
pub async fn datastores(
    profile: Profile,
    target: &Target,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Datastores> {
    let database_url = required(env, DATABASE_URL_VARIABLE)?;
    let redis_url = required(env, REDIS_URL_VARIABLE)?;
    profile.check_endpoints(target, &database_url, &redis_url)?;
    target.verify_owned_rig(&database_url, &redis_url)?;
    Datastores::open_checked(
        target,
        &database_url,
        &redis_url,
        variable(env, REDIS_CA_CERT_VARIABLE),
    )
    .await
}

/// Reconcile a lane's measurement with the caller's sweep, then write the result.
///
/// # Errors
///
/// The lane's own error first; the sweep's only when the lane succeeded; the
/// write's when both did.
pub fn finish(
    lane: Lane,
    profile: Profile,
    measured: Result<Report>,
    swept: Result<u64>,
) -> Result<String> {
    let mut report = measured?;
    // ADDED to what the lane already swept itself, never written over it: the
    // outbound lane removes its own entries by id before it returns, and the
    // caller's prefix sweep is the fallback that finds whatever that missed.
    report.fixture.swept += swept?;
    let path = lane.result_path(profile);
    report.write(&path)?;
    Ok(path.display().to_string())
}

/// Print the identifier an interrupted run needs for orphan recovery.
pub fn announce_prefix(prefix: &RunPrefix) {
    // logging: developer CLI recovery input; no telemetry subscriber or sensitive data.
    println!("run_prefix={prefix}");
}

/// Await a lane until it finishes or the operator requests cancellation.
///
/// The caller still owns the prefix and always runs its sweep after this
/// returns, including when cancellation wins the race.
///
/// # Errors
///
/// Returns the lane's error, [`crate::Error::Cancelled`], or the operating
/// system failure that prevented installing the cancellation listener.
pub async fn cancellable<T>(
    cancellation: CancellationToken,
    lane: impl Future<Output = Result<T>>,
) -> Result<T> {
    cancellable_on(cancellation, lane, tokio::signal::ctrl_c()).await
}

async fn cancellable_on<T>(
    cancellation: CancellationToken,
    lane: impl Future<Output = Result<T>>,
    interrupt: impl Future<Output = std::io::Result<()>>,
) -> Result<T> {
    tokio::pin!(lane);
    tokio::select! {
        result = &mut lane => result,
        interrupted = interrupt => {
            let refusal = match interrupted {
                Ok(()) => crate::Error::Cancelled,
                Err(source) => crate::Error::InterruptUnavailable { source },
            };
            cancellation.cancel();
            let _finished = lane.await;
            Err(refusal)
        }
    }
}

/// Turn a lane's outcome into the process's, printing what a person needs.
#[must_use]
pub fn exit(name: &str, outcome: Result<String>) -> ExitCode {
    match outcome {
        Ok(path) => {
            // logging: a make target answers the person who ran it on stdout; no daemon is running here to carry an event.
            println!("wrote {path}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            // logging: a refusal goes to stderr where the shell shows it; no subscriber is installed in this process.
            eprintln!("{name} refused: {refusal}");
            let mut cause: Option<&dyn core::error::Error> = core::error::Error::source(&refusal);
            while let Some(reason) = cause {
                // logging: the cause chain belongs on the same stream as the refusal it explains.
                eprintln!("  caused by: {reason}");
                cause = reason.source();
            }
            if refusal.is_pre_flight() {
                // logging: tells the reader there is nothing to sweep, on the stream the refusal went to.
                eprintln!("  nothing was created by the benchmark lane");
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
