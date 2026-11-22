//! What boot opens, and what it starts beside it.
//!
//! Split from `serve` on the same line `exporting` was: boot's job is to open
//! things and hand them on, and these are the pools, the plane and the
//! background workers it opens. `serve` keeps the sequence; this keeps what
//! each step in it constructs.

use std::sync::Arc;

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
    let database = open_database(config).await?;
    let (capabilities, sessions, signup_writeback) = crate::identity::resolve(
        config.identity(),
        config.runtime.operations.tuning.identity(),
    );
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
        storage_client: clients.storage,
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
    database.warm().await;
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
        &afd_vault::Vault::new(database.clone(), Arc::clone(kek), Entropy::new()),
        config.platform_admin_workspace(),
        client,
        config.runtime.operations.tuning.minting(),
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
        .map(|secret| af_crypto::secret::SecretBytes::new(secret.expose().as_bytes().to_vec()))
}

/// What the login flow signs codes with and redirects between.
fn login_config(config: &BootConfig) -> crate::plane::LoginConfig {
    crate::plane::LoginConfig {
        code_pepper: config.session_code_pepper().clone(),
        app_url: config.app_url().clone(),
        api_url: config.api_url().into(),
    }
}

/// What the plane needs to register schedules and receive their fires.
///
/// Neither the scheduler's credentials nor its callback are decided here: the
/// credentials are read from the `qstash` bag on use, and the callback is the
/// adapter's to derive from the api url (`plane::platform::qstash`).
fn schedule_config(config: &BootConfig, client: reqwest::Client) -> crate::plane::ScheduleConfig {
    crate::plane::ScheduleConfig {
        client,
        api_url: config.api_url().into(),
        retries: config.runtime.operations.scheduler_retries,
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
    // The connector answer-delivery worker. It claims from
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
        &config.runtime.operations,
    );
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::sync::Arc;

    use af_core::env::MapEnv;
    use af_core::test_util::trace::Capture;
    use afd_bus::{Bus, Handle, Subject, Subscription};

    use super::{identity_webhook_secret, live_over, spawn_background};
    use crate::preflight::runner_listener::{TLS_CERT_FILE, TLS_KEY_FILE};
    use crate::supervisor::Supervisor;

    /// The secret an identity provider signs signup deliveries with.
    const WEBHOOK_SECRET: &str = "whsec_fixture-not-a-credential";

    /// A configuration preflight resolves, carrying `extra` beside what it
    /// requires before it will answer at all.
    #[expect(
        clippy::expect_used,
        reason = "a fixture environment preflight refuses should stop the test"
    )]
    fn configured(extra: &[(&'static str, &'static str)]) -> crate::preflight::BootConfig {
        let mut pairs = vec![
            (
                "DATABASE_URL_API",
                "postgres://afd:afd@127.0.0.1:5432/agentsfleet",
            ),
            ("NATS_URL", "nats://127.0.0.1:4222"),
            (
                "ENCRYPTION_MASTER_KEY",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
            (
                "AUTH_SESSION_CODE_PEPPER",
                "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
            ),
            ("OIDC_ISSUER", "https://identity.fixture.test"),
            ("OIDC_AUDIENCE", "agentsfleetd-lane"),
            ("CLERK_API_BASE", "https://api.identity.fixture.test"),
            (
                "CLERK_SECRET_KEY",
                "fixture-provider-secret-not-a-credential",
            ),
            (TLS_CERT_FILE, "/run/runner/chain.pem"),
            (TLS_KEY_FILE, "/run/runner/leaf.pem"),
        ];
        pairs.extend(extra.iter().copied());
        crate::preflight::preflight(&MapEnv::from_pairs(pairs))
            .expect("the fixture environment resolves")
    }

    /// A configured secret reaches the signup wall byte for byte; none leaves
    /// the wall refusing every delivery.
    #[test]
    fn the_signup_wall_verifies_against_the_configured_secret_only() {
        let secret = identity_webhook_secret(&configured(&[(
            crate::preflight::IDENTITY_WEBHOOK_SECRET_KNOB,
            WEBHOOK_SECRET,
        )]));

        assert_eq!(
            secret.as_ref().map(af_crypto::secret::SecretBytes::expose),
            Some(WEBHOOK_SECRET.as_bytes())
        );
        assert!(identity_webhook_secret(&configured(&[])).is_none());
    }

    /// The futures `async_trait` hands back, spelled out so this test needs no
    /// macro of its own.
    type Answer<'a, T> = Pin<Box<dyn Future<Output = afd_bus::Result<T>> + Send + 'a>>;

    /// A broker that will not drain when the daemon stops; every other call
    /// fails as a disconnected one does.
    #[derive(Debug)]
    struct Undrainable(Handle);

    impl Bus for Undrainable {
        fn publish<'life0, 'life1, 'life2, 'async_trait>(
            &'life0 self,
            subject: &'life1 Subject,
            payload: &'life2 str,
        ) -> Answer<'async_trait, ()>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            'life2: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(self.0.publish(subject, payload))
        }

        fn subscribe<'life0, 'life1, 'async_trait>(
            &'life0 self,
            subject: &'life1 Subject,
        ) -> Answer<'async_trait, Subscription>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(self.0.subscribe(subject))
        }

        fn subscribe_thread_wakes<'life0, 'async_trait>(
            &'life0 self,
        ) -> Answer<'async_trait, Subscription>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(self.0.subscribe_thread_wakes())
        }

        fn shutdown<'life0, 'async_trait>(&'life0 self) -> Answer<'async_trait, ()>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            // Text is no number, so this is always the refusal.
            let refused = serde_json::from_str::<u8>("not a frame")
                .map(drop)
                .map_err(afd_bus::Error::from);
            Box::pin(std::future::ready(refused))
        }

        fn connected<'life0, 'async_trait>(
            &'life0 self,
        ) -> Pin<Box<dyn Future<Output = bool> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(std::future::ready(false))
        }
    }

    /// A bus that fails to drain at shutdown does not stop the daemon from
    /// stopping; it leaves one line naming the failure.
    #[tokio::test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the outbound client is built only to be handed over; it never sends"
    )]
    async fn a_bus_that_will_not_drain_at_shutdown_leaves_a_line() {
        let config = configured(&[]);
        let mut supervisor = Supervisor::new();
        let capture = Capture::install();

        spawn_background(
            &mut supervisor,
            &config,
            &afd_db::test_util::unreachable_db(),
            &Arc::new(af_crypto::secret::Kek::from_bytes([0x2a; 32])),
            Handle::new(Undrainable(afd_bus::test_util::unreachable())),
            reqwest::Client::new(),
        )
        .await;
        let _report = supervisor.shutdown().await;

        let logged = capture.only("notification_bus_shutdown_failed");
        assert!(
            logged
                .field("error_code")
                .is_some_and(|code| !code.is_empty()),
            "the line names its code"
        );
        logged.assert_uncoded_reason();
    }

    #[tokio::test]
    async fn the_live_surface_rides_the_booted_bus_at_its_capacity() -> afd_bus::Result<()> {
        let (bus, _server) = afd_bus::test_util::detached()?;
        let live = live_over(&bus, 3);
        assert!(live.bus().is_some());
        assert_eq!(live.capacity(), 3);
        Ok(())
    }
}
