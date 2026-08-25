//! Why the process is stopping, and the teardown that runs whichever answer it is.
//!
//! # The shape, and where it comes from
//!
//! `ApiManager::run` in exonum (`components/api/src/manager.rs`) splits into an
//! outer `run` and an inner `run_inner`, and the outer one stops the servers
//! "in any case" — every early return from the loop, error included, still
//! tears down. That split is the whole idea, and it is worth more here than the
//! actix specifics around it: [`Supervisor::shutdown`] consuming `self` makes
//! the ordering unforgeable at the type level, but nothing made it unforgeable
//! at the CALL site, where a `?` on an error path is all it takes to skip.
//!
//! So [`Daemon::run`] performs the whole sequence and there is no other way to
//! stop: decide why, then always tear down, then report both.
//!
//! # Three ways to stop, not one
//!
//! `serve.zig` models one — a signal arrives. exonum's loop selects over the
//! signal AND the server's own termination, and habitat's supervisor does the
//! same thing with a `shutdown_mode` the loop RETURNS. Both are right, and a
//! daemon that only waits for a signal hangs when its listener dies of
//! something else: a bind lost, an accept loop that returned an error, a
//! runtime that shut its I/O driver. That process is unkillable except by
//! SIGKILL and reports nothing on its way out.
//!
//! # Why there are no shutdown flags here
//!
//! `serve_shutdown.zig` keeps `shutdown_requested` and `background_stop` apart
//! so a signal arriving DURING boot cannot kill the background stack while the
//! server may still come up and briefly serve. It needs two flags because its
//! watcher is a separate thread polling every 100ms, so "the server stopped"
//! and "the signal arrived" are events that race.
//!
//! Here they cannot race, because they are statements in order. A signal during
//! boot resolves the shutdown future; the server comes up, sees an already
//! resolved future, and stops immediately; the supervisor is cancelled after.
//! That is precisely the property the two flags protected — the half-dead-node
//! window — with one less piece of shared mutable state to keep consistent.

use crate::supervisor::{ShutdownReport, Supervisor};

/// Why the daemon stopped serving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    /// An operator or an init system asked it to stop.
    ///
    /// The ordinary path, and the only one that is not a fault.
    Signalled,
    /// The server returned on its own, without being asked.
    ///
    /// A lost bind, an accept loop that gave up, an I/O driver that went away.
    /// Modelled because a daemon that only waits for a signal HANGS here, and
    /// a hung process reports nothing about why.
    ServerStopped,
}

/// What one run of the daemon amounted to.
#[derive(Debug)]
pub struct Outcome {
    /// Why serving ended.
    pub cause: StopCause,
    /// What the teardown of the background fleet did.
    pub shutdown: ShutdownReport,
}

impl Outcome {
    /// Whether this run ended the way it was supposed to.
    ///
    /// Both halves matter: a clean join after a server that fell over is not a
    /// clean run, and neither is a signalled stop that left a task behind.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.cause == StopCause::Signalled && self.shutdown.is_clean()
    }
}

/// Owns the server and the background fleet, and the order they stop in.
#[derive(Debug)]
pub struct Daemon {
    supervisor: Supervisor,
}

impl Daemon {
    /// A daemon supervising `supervisor`'s tasks.
    #[must_use]
    pub const fn new(supervisor: Supervisor) -> Self {
        Self { supervisor }
    }

    /// The tasks this daemon supervises, before it runs.
    #[must_use]
    pub fn inventory(&self) -> Vec<&'static str> {
        self.supervisor.inventory()
    }

    /// Serves until something stops it, then tears down — on every path.
    ///
    /// `server` is the future that is serving; `signal` is the one that
    /// resolves when the process is asked to stop. Neither is spawned here,
    /// because whichever finishes first is the answer and a spawned future
    /// cannot be raced against.
    ///
    /// The ordering is the contract, and it is stated once, here:
    ///
    /// 1. Await whichever of server or signal finishes first.
    /// 2. Cancel every supervised task and JOIN it.
    /// 3. Only then may the caller drop the pools those tasks borrowed.
    ///
    /// Step 2 runs whatever step 1 decided, which is the property this function
    /// exists to hold. Step 3 belongs to the caller and is enforced by
    /// [`Supervisor::shutdown`] consuming the supervisor.
    pub async fn run<S, F>(self, server: S, signal: F) -> Outcome
    where
        S: Future<Output = ()>,
        F: Future<Output = ()>,
    {
        let cause = Self::serve_until_stopped(server, signal).await;
        tracing::info!(cause = ?cause, event = "serving_ended",
        "serving ended; stopping background tasks");

        // Unconditional. Not in a branch, not after a `?`.
        let shutdown = self.supervisor.shutdown().await;

        if !shutdown.is_clean() {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the copy that never runs.
            let abandoned = shutdown.abandoned.len();
            let panicked = shutdown.panicked.len();
            let code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::error!(
                error_code = code,
                abandoned,
                panicked,
                event = "background_fleet_unclean",
                "background fleet did not stop cleanly"
            );
        }

        Outcome { cause, shutdown }
    }

    /// Awaits whichever of the two finishes first, and names which.
    async fn serve_until_stopped<S, F>(server: S, signal: F) -> StopCause
    where
        S: Future<Output = ()>,
        F: Future<Output = ()>,
    {
        tokio::select! {
            // Biased so the outcome is a fact about the futures rather than
            // about which arm tokio's random branch order happened to poll.
            // A signal that arrives during boot has already resolved by the
            // time this runs, and MUST be reported as a signal rather than as
            // the server having fallen over.
            biased;
            () = signal => StopCause::Signalled,
            () = server => StopCause::ServerStopped,
        }
    }
}
