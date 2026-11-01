//! Boot: what happens between a resolved environment and a served port.
//!
//! The order carries meaning — pools before Dragonfly before the router, and
//! nothing listening until all of them answered. Teardown ordering lives in
//! types rather than in declaration order: [`Supervisor::shutdown`] consumes
//! itself, so nothing borrowed can be dropped early, and [`Daemon::run`] tears
//! down on every path out.
//!
//! # Why hyper directly rather than `axum::serve`
//!
//! `axum::serve` exposes no way to bound the header buffer, and Dimension 5.3
//! is a bound on the header buffer. So the accept loop is ours and
//! [`afd_api::connection_builder`] carries the limit — the same builder §5's
//! oversize-head test drives.
//!
//! # Every connection is a supervised child of the accept loop
//!
//! A connection task is spawned per accepted socket and given the same
//! cancellation token. The alternative would be `tokio::spawn`, which detaches
//! — and a detached connection can outlive the pools it reads through. That is
//! the unsupervised spawn path Dimension 7.5 says does not exist.

mod accept;
mod clients;
pub mod drain;
mod exporting;
mod listen;
mod optional;
mod runtime;
#[cfg(feature = "test-util")]
mod test_util;
#[cfg(feature = "test-util")]
pub use test_util::boot_with_connector_boundary;

use std::net::SocketAddr;

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_core::env::EnvSource;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
use afd_observability::{Analytics, Telemetry};

pub use self::accept::Acceptor;
#[cfg(feature = "test-util")]
pub use self::accept::serve_accepts;
pub use self::drain::{DRAIN_TIMEOUT, Drain, Settled};

use self::exporting::gauge_sources;
pub(crate) use self::exporting::{attach_exports, flush_unsupervised, open_telemetry};
use self::listen::listen;
pub use self::listen::{ACCEPT_LOOP, dual_stack_listener};
use self::optional::open_analytics;
use self::runtime::{Runtime, open_runtime, spawn_background};
use crate::daemon::{Daemon, Outcome};
#[doc(inline)]
pub use crate::error::BootFailure;
use crate::preflight::{BootConfig, preflight};
use crate::supervisor::Supervisor;

/// The environment fallback for `--port`, named here and read by [`crate::cli`].
///
/// It is a `clap` `env` fallback rather than something this module reads, so
/// the generated `--help` documents it. Before that it was a second, silent
/// input path — the only one, in fact, since `--port` did not exist.
pub const PORT_KNOB: &str = "PORT";

/// What the daemon binds when neither `--port` nor `PORT` says otherwise.
///
/// Port 3000, the one the tunnel dials — see [`dual_stack_listener`].
pub const DEFAULT_PORT: u16 = 3000;

/// Everything boot opened, in the order it opened it.
///
/// Returned rather than kept, so the caller holds the pools and drops them
/// AFTER the shutdown that joined every task reading through them.
#[derive(Debug)]
pub struct Booted {
    /// The address actually bound — resolved, so port 0 reports what it got.
    pub address: SocketAddr,
    /// The Postgres pool, to be dropped only after shutdown returns.
    pub database: Db,
    /// The Dragonfly client, likewise.
    pub queue: Dragonfly,
    /// The accept loop's stop signal and its in-flight count, so the shutdown
    /// path can stop accepting and wait for live requests before the supervisor
    /// cancels anything. Cloned from the one the accept loop holds.
    pub drain: Drain,
}

/// Opens everything the daemon serves through, in dependency order.
///
/// # Errors
/// Returns the first stage that could not complete. Environment faults are
/// reported together; a datastore that will not answer is reported alone,
/// because there is nothing to gather once the process cannot proceed.
pub async fn boot<E: EnvSource + ?Sized>(
    env: &E,
    port: u16,
    supervisor: &mut Supervisor,
) -> Result<Booted, BootFailure> {
    // 1. Every knob, validated before a single socket opens. A key that cannot
    //    work must refuse boot BEFORE anything serves, which is only a promise
    //    if nothing has been opened yet.
    //
    //    This one refusal is NOT reported: where the events go is itself a knob,
    //    and a preflight that failed is a preflight that did not read it. The
    //    fault is on stderr, which is where an operator reading a container that
    //    will not start is already looking.
    let config: BootConfig = preflight(env)?;
    let analytics = open_analytics(config.posthog()).await;

    match open(config, analytics.clone(), port, supervisor).await {
        Ok(booted) => Ok(booted),
        Err(failure) => {
            analytics.report(&Telemetry::StartupFailed {
                command: COMMAND_SERVE.to_owned(),
                phase: failure.phase().to_owned(),
                reason: failure.to_string(),
                error_code: failure.code().as_str().to_owned(),
            });
            // Delivered before returning, because the caller's next move is to
            // render the fault and exit — and a queued event on a client that
            // is about to be dropped is an event nobody sees.
            analytics.flush().await;
            Err(failure)
        }
    }
}

/// [`boot`], with every credential minted through `exchanger` in place of the
/// vendors' token endpoints: the lane's way to run a mint end to end without
/// a GitHub App.
///
/// # Errors
/// As [`boot`], without the failure report a production boot sends.
#[cfg(feature = "test-util")]
pub async fn boot_with_exchanger<E: EnvSource + ?Sized>(
    env: &E,
    port: u16,
    supervisor: &mut Supervisor,
    exchanger: std::sync::Arc<dyn afd_credential::credential::Exchanger>,
) -> Result<Booted, BootFailure> {
    let config = preflight(env)?.with_exchanger(exchanger);
    let analytics = open_analytics(config.posthog()).await;
    open(config, analytics, port, supervisor).await
}

/// The subcommand a boot failure names.
const COMMAND_SERVE: &str = "serve";

/// Everything after the knobs are known, so its failures can be reported.
async fn open(
    config: BootConfig,
    analytics: Analytics,
    port: u16,
    supervisor: &mut Supervisor,
) -> Result<Booted, BootFailure> {
    // The exporter first, and its position here is load-bearing. `open_runtime`
    // opens the database, and the records it emits while doing so —
    // `pool_initialized`, `pool_warmed`, `pool_warm_incomplete` — are exactly
    // the ones an operator wants when asking whether an instance warmed its
    // pool. The reload slot `main` installed is empty until something fills
    // it, so filling it after this line means those records reach stderr and
    // nothing else. They did, for as long as this ran the other way round.
    //
    // Only the gauge PRODUCERS need what boot has not opened yet, and they
    // wait below.
    let prepared = attach_exports(&config)?;
    // Not `?`, and that is the cost of attaching early. From the line above
    // until `open_telemetry` below, the exporters are filled but unsupervised:
    // a `?` here would return holding the very pool records the attach exists
    // to preserve, and drop them. This is the only fallible call in that
    // window; after the handoff the supervisor owns the flush.
    let runtime = match open_runtime(&config, &analytics).await {
        Ok(runtime) => runtime,
        Err(failure) => {
            flush_unsupervised(prepared).await;
            return Err(failure);
        }
    };
    let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
    // Before the router takes them: both are the state a gauge reads, and the
    // clones are handles onto the same semaphore and the same ceiling rather
    // than second copies that could disagree with what admission decides.
    let sources = gauge_sources(&admission, &runtime.live);
    open_telemetry(prepared, supervisor, &sources)?;
    serve_plane(runtime, &admission, &config, analytics, port, supervisor).await
}

/// Routes the plane, starts every background worker, then binds the port.
///
/// The second half of [`open`], from the point the exporters are supervised.
/// The order is the original one: router, background workers, the worker
/// count, and the bind last, so nothing answers on the port before the
/// workers are spawned.
async fn serve_plane(
    runtime: Runtime,
    admission: &Admission,
    config: &BootConfig,
    analytics: Analytics,
    port: u16,
    supervisor: &mut Supervisor,
) -> Result<Booted, BootFailure> {
    let router = afd_api::router::build(runtime.plane, admission);
    // The sweepers read through pools that are open by now. Starting them
    // after the socket would leave a window where the plane serves while
    // nothing is noticing dead runners. They take the analytics because the
    // authoring sweeper finishes interrupted chat messages, a funnel step.
    crate::sweepers::spawn(supervisor, &runtime.database, &analytics);
    spawn_background(
        supervisor,
        config,
        &runtime.database,
        &runtime.queue,
        &runtime.kek,
        runtime.hub,
        runtime.outbound_client,
    )
    .await;
    report_workers(supervisor, &analytics);
    let drain = Drain::new();
    let address = listen(port, router, supervisor, drain.clone()).await?;
    announce_started(supervisor, analytics, address);
    Ok(Booted {
        address,
        database: runtime.database,
        queue: runtime.queue,
        drain,
    })
}

/// Reports the serving port, then hands `analytics` to a supervised flush that
/// delivers queued product events when the daemon is told to stop.
fn announce_started(supervisor: &mut Supervisor, analytics: Analytics, address: SocketAddr) {
    analytics.report(&Telemetry::ServerStarted {
        port: address.port(),
    });
    supervisor.spawn(crate::ANALYTICS_FLUSH, move |token| async move {
        token.cancelled().await;
        analytics.flush().await;
    });
}

/// Reports how many workers boot put under the supervisor.
fn report_workers(supervisor: &Supervisor, analytics: &Analytics) {
    let supervised = u16::try_from(supervisor.inventory().len()).unwrap_or(u16::MAX);
    analytics.report(&Telemetry::WorkerStarted {
        concurrency: supervised,
    });
}

/// Runs the daemon to completion, then drops what it borrowed.
///
/// The last line is the point: `booted` — the pools — is dropped only after
/// `Daemon::run` has cancelled and JOINED every task, which is invariant C2
/// expressed as the order of two statements the compiler will not let you swap.
///
/// `port` is already resolved — [`crate::cli`] took it from `--port`, the
/// `PORT` fallback, or [`DEFAULT_PORT`], and rejected anything unusable before
/// this was called. Nothing here re-reads the environment for it.
///
/// # Errors
/// Returns the boot stage that could not complete: an environment naming every
/// fault at once, a datastore that would not answer, or a port that would not
/// bind. Serving never starts in any of those cases.
pub async fn run<E: EnvSource + ?Sized, F>(
    env: &E,
    port: u16,
    signal: F,
) -> Result<Outcome, BootFailure>
where
    F: Future<Output = ()>,
{
    let (supervisor, booted) = boot_supervised(env, port).await?;
    show_banner(&booted);

    // The server future is `pending`: serving happens in the supervised accept
    // loop, so what ends this run is the signal — or the accept loop itself
    // dying, which `Daemon` would then have to report. Modelled as pending
    // because a loop that returned would leave the supervisor with a finished
    // task rather than a stopped server, and `Daemon::run` reads the outcome
    // from the report either way.
    let outcome = Daemon::new(supervisor)
        .draining(booted.drain.clone())
        .run(std::future::pending(), signal)
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
async fn boot_supervised<E: EnvSource + ?Sized>(
    env: &E,
    port: u16,
) -> Result<(Supervisor, Booted), BootFailure> {
    let mut supervisor = Supervisor::new();
    // Not `?`, for [`open`]'s reason one stage later. Once `open_telemetry` has
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
