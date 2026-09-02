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
mod optional;

use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_core::env::EnvSource;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_observability::producers::GaugeSources;
use afd_observability::{Analytics, Telemetry};
use afd_redis::Redis;
use tokio::net::TcpListener;

pub use self::accept::Acceptor;
#[cfg(feature = "test-util")]
pub use self::accept::serve_accepts;

use self::accept::accept_loop;
use self::optional::{announce_identity, open_analytics, open_live};
use crate::daemon::{Daemon, Outcome};
#[doc(inline)]
pub use crate::error::BootFailure;
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

/// How long boot spends establishing the pool's floor before it serves.
///
/// The floor is a quarter of the ceiling and each connection costs the 147-337
/// ms this lane measures, so the whole warm-up fits here several times over.
/// It is a deadline and not a requirement: [`Db::warm`] cannot fail, and a pool
/// that did not fill is a slower pool rather than a broken one — the datastore
/// was already proven reachable by `Db::connect`. Boot proceeds either way.
const POOL_WARM_DEADLINE: Duration = Duration::from_secs(5);

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

struct Runtime {
    database: Db,
    queue: Redis,
    /// The live-stream surface, kept beside the plane it was moved into.
    ///
    /// A `Live` is two handles, so this is the same ceiling the routes admit
    /// against — which is the point: a gauge reading a different value from
    /// the one that decides admission would report a number no shed agrees
    /// with.
    live: afd_sse::Live,
    plane: Shared,
    hub: Option<afd_redis::SubscriptionHub>,
    /// The same key the plane seals with. The outbound worker opens its own
    /// grant store over it, so boot hands one key to both rather than reading
    /// the knob twice.
    kek: Arc<Kek>,
}

async fn open_runtime(config: &BootConfig, analytics: &Analytics) -> Result<Runtime, BootFailure> {
    let database = Db::connect(config.api_pool()).await?;
    // Before the router exists, so the first request finds live connections
    // instead of paying a handshake inside an acquire budget sized for a wait.
    // sqlx does not do this itself: it bootstraps `min_connections` only when
    // `idle_timeout` and `max_lifetime` are both unset, and its own defaults
    // set both. `warm` reports its shortfall through `pool_warm_incomplete`.
    database.warm(POOL_WARM_DEADLINE).await;
    let queue = Redis::connect(config.redis()).await?;
    let (capabilities, sessions) = crate::identity::resolve(config.identity());
    announce_identity(&capabilities);
    let kek = Arc::new(config.kek().clone());
    let broker = crate::credentials::resolve(
        &afd_credential::vault::Vault::new(database.clone(), Arc::clone(&kek)),
        config.platform_admin_workspace(),
    )
    .await;
    let live = open_live(config.redis(), config.sse_max_streams()).await;
    let hub = live.hub().cloned();
    let observed = live.clone();
    let plane = Arc::new(ServingPlane::new(crate::plane::PlaneParts {
        database: database.clone(),
        queue: queue.clone(),
        // Cloned rather than moved: the outbound worker below opens its own
        // grant store over the same key, for the reason `crate::outbound`
        // gives — it runs beside the plane, not through it.
        kek: Arc::clone(&kek),
        capabilities,
        sessions,
        stores: crate::bundles::resolve(config.bundles()),
        platform_admin_workspace: config.platform_admin_workspace().cloned(),
        // Fail-closed: a deployment that named no secret refuses every signup
        // delivery rather than trusting an unverified one to open an account.
        identity_webhook_secret: config
            .identity_webhook_secret()
            .map(|secret| afd_crypto::secret::SecretBytes::new(secret.as_bytes().to_vec())),
        broker,
        live,
        analytics: analytics.clone(),
        // A destination that will not build is a deployment that cannot
        // register schedules, and it fails CLOSED rather than registering a
        // truncated callback: the empty string matches no token's subject, so
        // every fire is refused until the api url is corrected.
        schedule: crate::plane::ScheduleConfig {
            client: reqwest::Client::new(),
            token: config.qstash_token().unwrap_or_default().to_owned(),
            destination: afd_cron::qstash::destination_url(config.api_url()).unwrap_or_default(),
            // The one place the vendor's US region is chosen, and only when this
            // deployment named no scheduler of its own.
            api_base: config
                .qstash_url()
                .unwrap_or(afd_cron::qstash::API_BASE)
                .to_owned(),
            keys: config.qstash_signing_keys().cloned(),
        },
        login: crate::plane::LoginConfig {
            code_pepper: config.session_code_pepper().clone(),
            app_url: config.app_url().to_owned(),
            api_url: config.api_url().into(),
        },
    }));
    Ok(Runtime {
        database,
        queue,
        live: observed,
        plane,
        hub,
        kek,
    })
}

async fn spawn_background(
    supervisor: &mut Supervisor,
    config: &BootConfig,
    database: &Db,
    queue: &Redis,
    kek: &Arc<Kek>,
    hub: Option<afd_redis::SubscriptionHub>,
) {
    if let Some(hub) = hub {
        supervisor.spawn(crate::HUB_PUMP, move |token| async move {
            token.cancelled().await;
            hub.shutdown();
        });
    }
    // The sweepers read through pools that are open by now. Starting them
    // after the socket would leave a window where the plane serves while
    // nothing is noticing dead runners.
    crate::sweepers::spawn(supervisor, database, queue);
    // The connector answer-delivery worker, beside them and for the same
    // reason. Its own Redis connection, because it blocks on the stream — see
    // `crate::outbound`. It opens its own grant store over the SAME key the
    // plane seals with, which is why the KEK arrives shared rather than
    // rebuilt here.
    crate::outbound::spawn(
        supervisor,
        config.redis(),
        database,
        queue,
        afd_connector::Grants::new(
            afd_vault::Vault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            database.clone(),
            Entropy::new(),
        ),
        crate::credentials::vendor_exchange_client(),
    )
    .await;
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

/// What the three boot-owned gauges read.
///
/// Every one is a lock-free load on a value boot already holds, which is what
/// makes them safe inside a collection callback: the SDK runs those under its
/// own pipeline lock, with no timeout, so a reading that could block would
/// take every family silent at once rather than slow one down.
fn gauge_sources(admission: &Admission, live: &afd_sse::Live) -> GaugeSources {
    let requests = admission.clone();
    let streams = live.clone();
    GaugeSources {
        requests_in_flight: Arc::new(move || u64::try_from(requests.in_flight()).ok()),
        streams_in_flight: Arc::new(move || u64::try_from(streams.carrying()).ok()),
        resident_memory: Arc::new(crate::telemetry::resident_bytes),
    }
}

/// Builds the export pipelines and supervises their flush, where a collector
/// is configured.
///
/// A deployment that named none boots, serves, and exports nothing — which is
/// every developer's environment and most tests. The task is spawned only in
/// the configured case, which is why the daemon's inventory carries it
/// conditionally and `integration_serve.rs` says so.
pub(crate) fn open_telemetry(
    config: &BootConfig,
    supervisor: &mut Supervisor,
    sources: &GaugeSources,
) -> Result<(), BootFailure> {
    let Some(otlp) = config.otlp() else {
        // The Zig daemon's own event name and reason field, kept: a dashboard
        // or an alert matching on this line matches it from either binary.
        tracing::info!(
            reason = "no endpoint configured",
            event = "startup_otel_disabled",
            "telemetry is not exporting"
        );
        return Ok(());
    };

    let exports = crate::telemetry::install(otlp, sources)?;
    if let Some(signals) = crate::logs::signals() {
        let attached = signals.attach(&exports);
        tracing::debug!(attached, event = "telemetry_layers_attached");
    }
    supervisor.spawn(crate::OTLP_EXPORT, move |token| async move {
        token.cancelled().await;
        // The last thing that happens to telemetry. Every signal the process
        // still holds is delivered here, before the pools it described are
        // dropped — which is the same ordering the analytics flush has, and
        // for the same reason.
        exports.flush();
    });
    Ok(())
}
