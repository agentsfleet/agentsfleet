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

use std::net::SocketAddr;
use std::sync::Arc;

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_core::env::EnvSource;
use afd_db::Db;
use afd_observability::{Analytics, Telemetry};
use afd_redis::{Redis, RedisConfig, SubscriptionHub};
use afd_sse::{Ceiling, Live};
use tokio::net::TcpListener;

pub use self::accept::Acceptor;
#[cfg(feature = "test-util")]
pub use self::accept::serve_accepts;

use self::accept::accept_loop;
use crate::daemon::{Daemon, Outcome};
#[doc(inline)]
pub use crate::error::BootFailure;
use crate::identity::Capabilities;
use crate::plane::{ServingPlane, Shared};
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
    // 2. Postgres, then Redis — the Zig order, and the useful one: a daemon
    //    with no database has nothing to serve, so it is the first thing worth
    //    failing on.
    let database = Db::connect(config.api_pool()).await?;
    let queue = Redis::connect(config.redis()).await?;

    // 3. Everything the router is generic over, chosen here and nowhere else,
    //    then the router over it. The admission ceiling is passed alongside
    //    rather than held by the plane: it is a property of the PROCESS, not a
    //    service a verb acts through, and mixing the two would put a
    //    concurrency limit behind a trait about datastores.
    let (capabilities, sessions) = crate::identity::resolve(config.identity());
    announce_identity(&capabilities);
    // The one clone of the key material, at boot, into the handle every store
    // that opens a sealed row shares. `Kek` zeroes on drop, so the copy this
    // makes is not a copy that outlives the process.
    let kek = Arc::new(config.kek().clone());
    // The broker is built BEFORE the plane because it reads this deployment's
    // own platform credentials out of the vault, which is an asynchronous step
    // the plane's constructor is not. A deployment holding none still boots and
    // still serves every other verb.
    let broker = crate::credentials::resolve(
        &afd_credential::vault::Vault::new(database.clone(), Arc::clone(&kek)),
        config.platform_admin_workspace(),
    )
    .await;
    // The pub/sub hub, opened once for the process: every live stream rides
    // this ONE connection, so a wall of tiles costs map entries rather than
    // dials. An instance that cannot open it still serves — the stream routes
    // answer and stay silent, and every other verb is unaffected — because a
    // best-effort surface must not be able to refuse boot.
    let live = open_live(config.redis(), config.sse_max_streams()).await;
    let hub = live.hub().cloned();
    let plane: Shared = Arc::new(ServingPlane::new(crate::plane::PlaneParts {
        database: database.clone(),
        queue: queue.clone(),
        kek,
        capabilities,
        sessions,
        bundles: crate::bundles::resolve(config.bundles()),
        broker,
        live,
        analytics: analytics.clone(),
        login: crate::plane::LoginConfig {
            code_pepper: config.session_code_pepper().clone(),
            app_url: config.app_url().to_owned(),
            api_url: config.api_url().into(),
        },
    }));
    let router = afd_api::router::build(plane, &Admission::new(DEFAULT_MAX_IN_FLIGHT));

    // The pump itself lives inside the hub; what is supervised here is its
    // STOP. A shutdown closes the hub, which closes every channel a live stream
    // is waiting on — so the streams unwind and their tasks end, instead of
    // parking on a socket nobody is reading.
    if let Some(hub) = hub {
        supervisor.spawn(crate::HUB_PUMP, move |token| async move {
            token.cancelled().await;
            hub.shutdown();
        });
    }

    // 4. The background sweepers, before the listener: they read through pools
    //    that are open by now, and starting them after the socket would leave a
    //    window where the plane serves while nothing is noticing dead runners.
    crate::sweepers::spawn(&mut *supervisor, &database, &queue);
    // The worker set is up. `concurrency` is how many supervised tasks this
    // process carries — the closest true reading of the Zig's field, whose
    // runner daemon is not what this binary is.
    // Saturating rather than a cast: the field is 16-bit on the wire, and a
    // process with more than 65,535 supervised tasks reports the ceiling rather
    // than wrapping to a small number that would read as healthy.
    let supervised = u16::try_from(supervisor.inventory().len()).unwrap_or(u16::MAX);
    analytics.report(&Telemetry::WorkerStarted {
        concurrency: supervised,
    });

    // 5. Listen last. Until this line the process is not reachable, which is
    //    what makes every refusal above a refusal rather than an outage.
    let listener = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).await?;
    let address = listener.local_addr()?;

    supervisor.spawn(ACCEPT_LOOP, move |token| {
        accept_loop(listener, router, token)
    });

    // Reported only once the socket is bound, so `server_started` means an
    // instance that can be reached and not one that got as far as trying.
    analytics.report(&Telemetry::ServerStarted {
        port: address.port(),
    });
    // Delivering what is queued is part of stopping: an event captured by the
    // last request served is one this daemon still owes. Supervised so it runs
    // in shutdown order, before the pools the caller drops.
    supervisor.spawn(crate::ANALYTICS_FLUSH, move |token| async move {
        token.cancelled().await;
        analytics.flush().await;
    });

    Ok(Booted {
        address,
        database,
        queue,
    })
}

/// Says which surfaces this instance can actually serve.
///
/// Once, at boot, because the alternative is an operator discovering it from a
/// 503 on their first enrolment. Reads the RESOLVED seam rather than the config
/// it was built from: preflight has already refused a boot whose provider knobs
/// were missing, so the only way to reach the warning below is a provider that
/// was configured and could not be constructed — which is a reduced surface,
/// not a fault, because the runner plane consults neither seam.
fn announce_identity(capabilities: &Capabilities) {
    match capabilities {
        Capabilities::Provider(_built) => {
            tracing::info!(
                event = "identity_provider_configured",
                "identity provider configured — tenant and runner planes both serve"
            );
        }
        Capabilities::Unconfigured(_absent) => {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the dead copy.
            let code = afd_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::warn!(
                error_code = code,
                event = "identity_provider_unusable",
                "identity provider unusable — the runner plane serves normally \
                 and every tenant-plane capability read answers unavailable"
            );
        }
    }
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

/// The live-stream surface, or its silent form when the hub will not open.
///
/// The ceiling is built either way: an instance that carries no frames still
/// has to refuse the stream past its ceiling, so a client learns the same thing
/// about capacity whether or not this deployment's pub/sub connection came up.
async fn open_live(config: &RedisConfig, max_streams: usize) -> Live {
    let ceiling = Ceiling::new(max_streams);
    match SubscriptionHub::start(config.clone()).await {
        Ok(hub) => Live::new(hub, ceiling),
        Err(unopened) => {
            let code = afd_core::error_code::STARTUP_REDIS_CONNECT.as_str();
            let reason = unopened.to_string();
            tracing::warn!(
                error_code = code,
                reason,
                event = "hub_unavailable",
                "the live-stream surface will carry no frames; every other verb is unaffected"
            );
            Live::detached(ceiling)
        }
    }
}

/// The product-analytics reporter, or its silent form.
///
/// A deployment naming no project reports nothing — which is every developer's
/// and every test's, so it is a value rather than a refusal.
async fn open_analytics(config: Option<&crate::preflight::PostHogConfig>) -> Analytics {
    match config {
        Some(project) => Analytics::resolve(&project.project_key, project.host.as_deref()).await,
        None => Analytics::silent(),
    }
}
