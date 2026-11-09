//! What boot opens, and what it starts beside it.
//!
//! Split from `serve` on the same line `exporting` was: boot's job is to open
//! things and hand them on, and these are the pools, the plane and the
//! background workers it opens. `serve` keeps the sequence; this keeps what
//! each step in it constructs.

use std::sync::Arc;
use std::time::Duration;

use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use af_observability::Analytics;
use afd_db::Db;

use super::clients::Clients;
use super::optional::announce_identity;
use crate::error::BootFailure;
use crate::plane::Shared;
use crate::preflight::BootConfig;
use crate::supervisor::Supervisor;

/// How long boot spends establishing the pool's floor before it serves.
///
/// `floor × establishment`, with room for an establishment far slower than the
/// 147-337 ms this lane measures — a cross-region TLS handshake against a
/// loaded server is seconds, not milliseconds, and a deadline that fits only
/// the fast case silently ships a cold pool. Fifteen seconds covers the floor
/// a production ceiling implies at a handshake cost several times the lane's.
///
/// It is a deadline and not a requirement: [`Db::warm`] cannot fail, and a pool
/// that did not fill is a slower pool rather than a broken one — the datastore
/// was already proven reachable by `Db::connect`. Boot proceeds either way, and
/// `pool_warm_incomplete` is what says it did.
const POOL_WARM_DEADLINE: Duration = Duration::from_secs(15);

pub(super) struct Runtime {
    pub(super) database: Db,
    /// The live-stream surface, kept beside the plane it was moved into.
    ///
    /// A `Live` is two handles, so this is the same ceiling the routes admit
    /// against — which is the point: a gauge reading a different value from
    /// the one that decides admission would report a number no shed agrees
    /// with.
    pub(super) live: afd_sse::Live,
    pub(super) plane: Shared,
    pub(super) bus: afd_bus::Handle,
    /// The same key the plane seals with. The outbound worker opens its own
    /// grant store over it, so boot hands one key to both rather than reading
    /// the knob twice.
    pub(super) kek: Arc<Kek>,
    /// The answer-delivery worker's own pool, built with the plane's before
    /// anything was spawned.
    pub(super) outbound_client: reqwest::Client,
}

mod plane;

pub(super) async fn open_runtime(
    config: &BootConfig,
    analytics: &Analytics,
) -> Result<Runtime, BootFailure> {
    // First, before a socket opens: a client that will not build is a defect
    // in the build, and nothing below should be opened only to be abandoned.
    let clients = Clients::open()?;
    let authoring_transport = clients.authoring;
    #[cfg(feature = "test-util")]
    let authoring_transport = config.authoring_transport().unwrap_or(authoring_transport);
    let database = open_database(config).await?;
    let (capabilities, sessions, signup_writeback) = crate::identity::resolve(config.identity());
    announce_identity(&capabilities);
    let kek = Arc::new(config.kek().clone());
    let broker = open_broker(config, &database, &kek, clients.broker).await;
    let bus = afd_bus::nats::connect(config.bus()).await?;
    let live = live_over(&bus, config.sse_max_streams());
    let observed = live.clone();
    let plane = plane::build(plane::Inputs {
        config,
        analytics,
        database: &database,
        bus: &bus,
        kek: &kek,
        identity: (capabilities, sessions, signup_writeback),
        broker,
        live,
        authoring_transport,
        vendor_client: clients.plane,
        schedule_client: clients.schedule,
    });
    #[cfg(feature = "test-util")]
    let plane = plane.with_connector_endpoint(config.connector_endpoint());
    Ok(Runtime {
        database,
        live: observed,
        plane: Arc::new(plane),
        bus,
        kek,
        outbound_client: clients.outbound,
    })
}

/// The live-stream surface over the bus boot connected.
///
/// It always carries a bus: an unreachable one refuses boot as
/// `UZ-STARTUP-004` before the router exists.
fn live_over(bus: &afd_bus::Handle, max_streams: usize) -> afd_sse::Live {
    afd_sse::Live::new(bus.clone(), afd_sse::StreamLimit::new(max_streams))
}

/// The database, warmed.
async fn open_database(config: &BootConfig) -> Result<Db, BootFailure> {
    let database = Db::connect(config.api_pool()).await?;
    // Before the router exists, so the first request finds live connections
    // instead of paying a handshake inside an acquire budget sized for a wait.
    // sqlx does not do this itself: it bootstraps `min_connections` only when
    // `idle_timeout` and `max_lifetime` are both unset, and the pool pins
    // both. `warm` reports its shortfall through `pool_warm_incomplete`.
    database.warm(POOL_WARM_DEADLINE).await;
    Ok(database)
}

/// The credential broker, minting through the platform admin workspace's
/// vault when one is set.
async fn open_broker(
    config: &BootConfig,
    database: &Db,
    kek: &Arc<Kek>,
    client: reqwest::Client,
) -> Arc<afd_credential::credential::Broker> {
    let broker = crate::credentials::resolve(
        &afd_credential::vault::Vault::new(database.clone(), Arc::clone(kek)),
        config.platform_admin_workspace(),
        client,
    )
    .await;
    #[cfg(feature = "test-util")]
    let broker = config
        .exchanger()
        .map_or(broker, crate::credentials::with_exchanger);
    broker
}

/// The secret signup deliveries are verified against.
///
/// Fail-closed: a deployment that named no secret refuses every signup
/// delivery rather than trusting an unverified one to open an account.
fn identity_webhook_secret(config: &BootConfig) -> Option<af_crypto::secret::SecretBytes> {
    config
        .identity_webhook_secret()
        .map(|secret| af_crypto::secret::SecretBytes::new(secret.as_bytes().to_vec()))
}

/// What the login flow signs codes with and redirects between.
fn login_config(config: &BootConfig) -> crate::plane::LoginConfig {
    crate::plane::LoginConfig {
        code_pepper: config.session_code_pepper().clone(),
        app_url: config.app_url().clone(),
        api_url: config.api_url().into(),
    }
}

/// The scheduler the plane registers schedules with.
///
/// A destination that will not build is a deployment that cannot register
/// schedules, and it fails CLOSED rather than registering a truncated
/// callback: the empty string matches no token's subject, so every fire is
/// refused until the api url is corrected.
fn schedule_config(config: &BootConfig, client: reqwest::Client) -> crate::plane::ScheduleConfig {
    crate::plane::ScheduleConfig {
        client,
        token: af_crypto::secret::SecretString::new(
            config.qstash_token().unwrap_or_default().to_owned(),
        ),
        destination: afd_cron::qstash::destination_url(config.api_url()).unwrap_or_default(),
        // The one place the vendor's US region is chosen, and only when this
        // deployment named no scheduler of its own.
        api_base: config
            .qstash_url()
            .unwrap_or(afd_cron::qstash::API_BASE)
            .to_owned(),
        keys: config.qstash_signing_keys().cloned(),
    }
}

pub(super) async fn spawn_background(
    supervisor: &mut Supervisor,
    config: &BootConfig,
    database: &Db,
    kek: &Arc<Kek>,
    bus: afd_bus::Handle,
    outbound_client: reqwest::Client,
) {
    let outbound_bus = bus.clone();
    supervisor.spawn(crate::BUS_SHUTDOWN, async move |token| {
        token.cancelled().await;
        if let Err(error) = bus.shutdown().await {
            let error_code = error.code().as_str();
            let reason = error.reason();
            tracing::warn!(
                event = "notification_bus_shutdown_failed",
                error_code,
                reason
            );
        }
    });
    // The connector answer-delivery worker, beside the sweepers `serve`
    // spawned just before this, and for the same reason. It claims from
    // Postgres and is pinged over the bus — see `crate::outbound`. It opens its
    // own grant store over the SAME key the plane seals with, which is why the
    // KEK arrives shared rather than rebuilt here.
    crate::outbound::spawn(
        supervisor,
        database,
        outbound_bus,
        afd_connector::Grants::new(
            afd_vault::Vault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            database.clone(),
            Entropy::new(),
        ),
        outbound_client,
        config.slack_api_base().to_owned(),
    );
}

#[cfg(test)]
mod tests {
    use super::live_over;

    #[tokio::test]
    async fn the_live_surface_rides_the_booted_bus_at_its_capacity() -> afd_bus::Result<()> {
        let (bus, _server) = afd_bus::test_util::detached()?;
        let live = live_over(&bus, 3);
        assert!(live.bus().is_some());
        assert_eq!(live.capacity(), 3);
        Ok(())
    }
}
