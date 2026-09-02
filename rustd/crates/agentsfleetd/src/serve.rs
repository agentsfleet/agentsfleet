//! Boot: what happens between a resolved environment and a served port.
//!
//! The order is `cmd/serve.zig`'s, because the order is the part that carries
//! meaning — pools before Redis before the router, and nothing listening until
//! all of them answered. What is NOT ported is the shape: `serve.zig` is one
//! function holding a defer chain that teardown has to unwind in exactly the
//! right sequence, and the sequence is only correct because the declarations
//! happen to be in the right order. Here the ordering lives in types:
//! [`Supervisor::shutdown`] consumes itself, so nothing borrowed can be dropped
//! early, and [`Daemon::run`] tears down on every path out.
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
//! cancellation token. `serve.zig` has no equivalent because httpz owns its own
//! worker pool; here the alternative would be `tokio::spawn`, which detaches —
//! and a detached connection can outlive the pools it reads through. That is
//! the unsupervised spawn path Dimension 7.5 says does not exist.

mod accept;
mod exporting;
mod optional;
mod runtime;

use std::net::{Ipv6Addr, SocketAddr};

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_core::env::EnvSource;
use afd_db::Db;
use afd_observability::{Analytics, Telemetry};
use afd_redis::Redis;
use tokio::net::TcpListener;

pub use self::accept::Acceptor;
#[cfg(feature = "test-util")]
pub use self::accept::serve_accepts;

use self::accept::accept_loop;
use self::exporting::gauge_sources;
pub(crate) use self::exporting::open_telemetry;
use self::optional::open_analytics;
use self::runtime::{open_runtime, spawn_background};
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
/// `http/server.zig`'s default, kept.
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
    /// The Redis client, likewise.
    pub queue: Redis,
}

/// Opens everything the daemon serves through, in `cmd/serve.zig`'s order.
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

/// The subcommand a boot failure names.
const COMMAND_SERVE: &str = "serve";

/// Everything after the knobs are known, so its failures can be reported.
async fn open(
    config: BootConfig,
    analytics: Analytics,
    port: u16,
    supervisor: &mut Supervisor,
) -> Result<Booted, BootFailure> {
    let runtime = open_runtime(&config, &analytics).await?;
    let admission = Admission::new(DEFAULT_MAX_IN_FLIGHT);
    // Before the router takes them: both are the state a gauge reads, and the
    // clones are handles onto the same semaphore and the same ceiling rather
    // than second copies that could disagree with what admission decides.
    let sources = gauge_sources(&admission, &runtime.live);
    open_telemetry(&config, supervisor, &sources)?;
    let router = afd_api::router::build(runtime.plane, &admission);
    spawn_background(
        supervisor,
        &config,
        &runtime.database,
        &runtime.queue,
        &runtime.kek,
        runtime.hub,
    )
    .await;
    report_workers(supervisor, &analytics);
    let address = listen(port, router, supervisor).await?;
    analytics.report(&Telemetry::ServerStarted {
        port: address.port(),
    });
    supervisor.spawn(crate::ANALYTICS_FLUSH, move |token| async move {
        token.cancelled().await;
        analytics.flush().await;
    });

    Ok(Booted {
        address,
        database: runtime.database,
        queue: runtime.queue,
    })
}

fn report_workers(supervisor: &Supervisor, analytics: &Analytics) {
    let supervised = u16::try_from(supervisor.inventory().len()).unwrap_or(u16::MAX);
    analytics.report(&Telemetry::WorkerStarted {
        concurrency: supervised,
    });
}

/// Binds `[::]:port` — the address the Cloudflare Tunnel can actually reach.
///
/// # Why the address is a deployment contract, not a preference
///
/// The daemon publishes no public Fly service in any environment:
/// `deploy/fly/agentsfleetd-{dev,prod}/fly.toml` says so, and the only route in
/// is Cloudflare Tunnel dialling `agentsfleetd-<env>.internal:3000`. Fly's
/// private network resolves a `.internal` name to a 6PN address, which is
/// **IPv6 only** — so a listener bound to `0.0.0.0` refuses the tunnel's
/// connection and the edge answers 502 while the machine still reports healthy,
/// because Fly's readiness probe reaches port 3000 over IPv4. That asymmetry is
/// why the bug shipped twice: the Zig daemon defaulted its interface to `"::"`
/// after the same incident (`src/http/server.zig`, since removed) and the port
/// to Rust dropped the default.
///
/// # Why one bind serves both stacks
///
/// An `AF_INET6` socket accepts IPv4 through v4-mapped addresses unless
/// `IPV6_V6ONLY` is set, and Linux leaves that option off by default
/// (`net.ipv6.bindv6only` is 0). That is what the Zig daemon relied on — it
/// reasoned about the option explicitly and concluded it had none to set — and
/// it is what keeps Fly's IPv4 readiness probe answered by the same listener
/// the tunnel reaches over IPv6. `std` offers no way to set the option anyway:
/// it must be changed between `socket()` and `bind()`, and `bind` does both.
/// The assumption is not left to inspection — `tests/serve.rs` connects over
/// each stack in turn, on Linux in Continuous Integration (CI).
///
/// # Errors
/// Returns the `io::Error` from the bind: a port already held, or one this
/// process may not have.
pub async fn dual_stack_listener(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv6Addr::UNSPECIFIED, port)).await
}

async fn listen(
    port: u16,
    router: axum::Router,
    supervisor: &mut Supervisor,
) -> Result<SocketAddr, BootFailure> {
    let listener = dual_stack_listener(port).await?;
    let address = listener.local_addr()?;
    supervisor.spawn(ACCEPT_LOOP, move |token| {
        accept_loop(listener, router, token)
    });
    Ok(address)
}

/// The supervised name of the accept loop.
pub const ACCEPT_LOOP: &str = "accept_loop";

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
    let mut supervisor = Supervisor::new();
    let booted = boot(env, port, &mut supervisor).await?;
    crate::banner::show(
        env!("CARGO_PKG_VERSION"),
        &[
            format!("listening {}", booted.address),
            format!("postgres:{}", booted.database.role().tag()),
            format!("redis:{}", booted.queue.role().tag()),
        ],
        std::process::id(),
    );

    // The server future is `pending`: serving happens in the supervised accept
    // loop, so what ends this run is the signal — or the accept loop itself
    // dying, which `Daemon` would then have to report. Modelled as pending
    // because a loop that returned would leave the supervisor with a finished
    // task rather than a stopped server, and `Daemon::run` reads the outcome
    // from the report either way.
    let outcome = Daemon::new(supervisor)
        .run(std::future::pending(), signal)
        .await;

    // C2, as two statements the compiler will not let you swap: the pools are
    // dropped only after a shutdown that joined every task reading through them.
    drop(booted);
    Ok(outcome)
}
