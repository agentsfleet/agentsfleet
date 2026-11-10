//! Boot: what happens between a resolved environment and a served port.
//!
//! The order carries meaning — pools before Dragonfly before the router, and
//! nothing listening until all of them answered. Teardown ordering lives in
//! types rather than in declaration order: [`Supervisor::shutdown`] consumes
//! itself, so nothing borrowed can be dropped early, and [`crate::daemon::Daemon::run`] tears
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
mod observations;
mod optional;
mod process;
pub use process::run;
mod runner_stream;
mod runtime;
#[cfg(feature = "test-util")]
mod test_util;
#[cfg(feature = "test-util")]
pub use test_util::{boot_with_authoring_boundary, boot_with_connector_boundary};

use std::net::SocketAddr;

use af_core::env::EnvSource;
use af_observability::{Analytics, Telemetry};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_db::Db;
use afd_dragonfly::Dragonfly;

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
    /// Native TCP and UDP address serving only runner streams.
    pub runner_stream_address: SocketAddr,
    /// Wakes the process owner if native serving or scheduling ends unexpectedly.
    serving_failed: tokio_util::sync::CancellationToken,
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
pub async fn boot<E: EnvSource + ?Sized + Sync>(
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
pub async fn boot_with_exchanger<E: EnvSource + ?Sized + Sync>(
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
    let sources = gauge_sources(&admission, &runtime.live, &runtime.database);
    open_telemetry(prepared, supervisor, &sources, &runtime.database)?;
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
    let listener = runner_stream::bind(config.runner_listener()).await?;
    let runner_stream_address = listener.local_addr()?;
    let gateway = runtime.plane.runner_streams().clone();
    let native = runner_stream::Signals::default();
    let serving_failed = tokio_util::sync::CancellationToken::new();
    let router = afd_api::router::build(std::sync::Arc::clone(&runtime.plane), admission);
    start_workers(supervisor, config, &analytics, &runtime).await;
    let drain = runner_drain(&gateway, &native);
    serve_runner(
        listener,
        &router,
        gateway,
        &drain,
        native,
        &serving_failed,
        supervisor,
    );
    let stream_port = runner_stream_address.port();
    tracing::info!(event = "runner_stream_listener_started", port = stream_port);
    let address = listen(port, router, supervisor, drain.clone()).await?;
    announce_started(supervisor, analytics, address);
    Ok(Booted {
        address,
        runner_stream_address,
        serving_failed,
        database: runtime.database,
        queue: runtime.queue,
        drain,
    })
}

fn runner_drain(gateway: &afd_gateway::Gateway, native: &runner_stream::Signals) -> Drain {
    Drain::new().with_runner_streams(gateway.clone(), native.stop.clone(), native.closed.clone())
}

fn serve_runner(
    listener: af_stream::Listener,
    router: &axum::Router,
    gateway: afd_gateway::Gateway,
    drain: &Drain,
    native: runner_stream::Signals,
    serving_failed: &tokio_util::sync::CancellationToken,
    supervisor: &mut Supervisor,
) {
    runner_stream::spawn(
        listener,
        router.clone(),
        gateway,
        drain.clone(),
        native,
        serving_failed.clone(),
        supervisor,
    );
}

/// Start maintenance before either listener can answer through the assembled router.
async fn start_workers(
    supervisor: &mut Supervisor,
    config: &BootConfig,
    analytics: &Analytics,
    runtime: &Runtime,
) {
    crate::sweepers::spawn(
        supervisor,
        &runtime.database,
        analytics,
        runtime.plane.task_owner(),
    );
    spawn_background(
        supervisor,
        config,
        &runtime.database,
        &runtime.queue,
        &runtime.kek,
        runtime.bus.clone(),
        runtime.outbound_client.clone(),
    )
    .await;
    report_workers(supervisor, analytics);
}

/// Reports the serving port, then hands `analytics` to a supervised flush that
/// delivers queued product events when the daemon is told to stop.
fn announce_started(supervisor: &mut Supervisor, analytics: Analytics, address: SocketAddr) {
    analytics.report(&Telemetry::ServerStarted {
        port: address.port(),
    });
    supervisor.spawn(crate::ANALYTICS_FLUSH, async move |token| {
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
