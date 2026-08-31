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

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT};
use afd_core::env::EnvSource;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
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
    let router = afd_api::router::build(runtime.plane, &Admission::new(DEFAULT_MAX_IN_FLIGHT));
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

async fn listen(
    port: u16,
    router: axum::Router,
    supervisor: &mut Supervisor,
) -> Result<SocketAddr, BootFailure> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).await?;
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
