//! The process owns booted pools until supervised shutdown has joined their borrowers.

use af_core::env::EnvSource;

use super::{BootFailure, Booted, boot};
use crate::daemon::{Daemon, Outcome};
use crate::supervisor::Supervisor;

/// Runs the daemon to completion, then drops what it borrowed.
///
/// The last line is the point: `booted` — the pools — is dropped only after
/// `Daemon::run` has cancelled and JOINED every task, which is invariant C2
/// expressed as the order of two statements the compiler will not let you swap.
///
/// `port` is already resolved — [`crate::cli`] took it from `--port`, the
/// `PORT` fallback, or [`super::DEFAULT_PORT`], and rejected anything unusable before
/// this was called. Nothing here re-reads the environment for it.
///
/// # Errors
/// Returns the boot stage that could not complete: an environment naming every
/// fault at once, a datastore that would not answer, or a port that would not
/// bind. Serving never starts in any of those cases.
pub async fn run<E: EnvSource + ?Sized + Sync, F>(
    env: &E,
    port: u16,
    signal: F,
) -> Result<Outcome, BootFailure>
where
    F: Future<Output = ()> + Send,
{
    let (supervisor, booted) = boot_supervised(env, port).await?;
    show_banner(&booted);

    // A failed native listener must stop the process rather than leave a healthy public API
    // advertising a runner service that no longer has a listener or scheduler.
    let outcome = Daemon::new(supervisor)
        .draining(booted.drain.clone())
        .run(booted.serving_failed.cancelled(), signal)
        .await;

    // C2, as two statements the compiler will not let you swap: the pools are
    // dropped only after a shutdown that joined every task reading through them.
    drop(booted);
    Ok(outcome)
}

/// Boots under a fresh supervisor, shutting it down if boot fails.
///
/// # Errors
/// Returns the boot failure, after the supervisor holding whatever boot had
/// already spawned has been shut down.
async fn boot_supervised<E: EnvSource + ?Sized + Sync>(
    env: &E,
    port: u16,
) -> Result<(Supervisor, Booted), BootFailure> {
    let mut supervisor = Supervisor::new();
    // Not `?`, for [`super::open`]'s reason one stage later. Once `super::open_telemetry` has
    // handed the exporters over, the thing that delivers them is a supervised
    // task waiting on the cancellation token — and a supervisor that is dropped
    // rather than shut down never cancels it. A bind that fails after that
    // handoff would take the whole boot's records with it, silently.
    //
    // `shutdown` consumes the supervisor, which is why this is a match: the
    // borrow `boot` held has to end before the move.
    let booted = match boot(env, port, &mut supervisor).await {
        Ok(booted) => booted,
        Err(failure) => {
            let report = supervisor.shutdown().await;
            if !report.is_clean() {
                tracing::warn!(
                    event = "boot_teardown_unclean",
                    "a task did not stop cleanly while tearing down a failed boot"
                );
            }
            return Err(failure);
        }
    };
    Ok((supervisor, booted))
}

/// Prints the startup banner: where this daemon listens and which datastore
/// roles it opened.
fn show_banner(booted: &Booted) {
    crate::banner::show(
        env!("CARGO_PKG_VERSION"),
        &[
            format!("listening {}", booted.address),
            format!("postgres:{}", booted.database.role().tag()),
            format!("redis:{}", booted.queue.role().tag()),
        ],
        std::process::id(),
    );
}
