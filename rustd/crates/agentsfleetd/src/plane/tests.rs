//! Composition-root ownership proofs.

#![expect(
    clippy::expect_used,
    reason = "test construction failures should stop the mapping proof"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the plane is built only to compare store addresses; its clients never send a request"
)]

use std::sync::Arc;

use afd_api::{Services as _, TenantSurface as _};
use afd_auth::{NoCapabilitySource, NoVerifier};
use afd_core::env::MapEnv;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::secrets::Registry;
use afd_crypto::secret::{Kek, SecretBytes, SecretString};
use afd_db::config::{DbRole, PoolConfig};
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_observability::Analytics;
use afd_sse::{Ceiling, Live};

use super::{Capabilities, LoginConfig, PlaneParts, ServingPlane, Sessions, SignupWriteback};

/// Where a person is sent, which the plane must never confuse with the API.
const DASHBOARD: &str = "https://app.fixture.test";

/// Where API calls and machine callbacks go.
const DEPLOYMENT: &str = "https://api.fixture.test";

/// Where the external scheduler delivers a fire.
const SCHEDULE_DESTINATION: &str = "https://api.fixture.test/v1/schedules/fire";

fn same<T: ?Sized>(left: &T, right: &T) -> bool {
    std::ptr::eq(left, right)
}

fn plane() -> ServingPlane {
    let database_config = PoolConfig::resolve(
        &MapEnv::from_pairs([("DATABASE_URL_API", "postgres://127.0.0.1:1/fixture")]),
        DbRole::Api,
    )
    .expect("the unreachable database URL is valid");
    let database = afd_db::Db::unreachable(&database_config);
    let queue = afd_dragonfly::Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Api,
        "redis://127.0.0.1:1".to_owned(),
    ))
    .expect("a lazy Dragonfly handle opens no socket");
    let vendors = Vendors::new(Platform::empty(), reqwest::Client::new());
    ServingPlane::new(PlaneParts {
        authoring_transport: afd_authoring::Network::new()
            .expect("fixed transport builds without a network request"),
        database,
        queue,
        kek: Arc::new(Kek::from_bytes([0x2a; 32])),
        capabilities: Capabilities::Unconfigured(NoCapabilitySource),
        signup_writeback: SignupWriteback::Unconfigured(crate::identity::NoWriteback),
        sessions: Sessions::Unconfigured(NoVerifier),
        broker: Arc::new(Broker::new(
            Arc::new(Registry::default()),
            Arc::new(vendors),
        )),
        live: Live::detached(Ceiling::new(4)),
        // No admin workspace and no scheduler credentials: the fail-closed
        // deployment state, which is what this fixture is for — the plane must
        // map every service to its own store whether or not either is present.
        platform_admin_workspace: None,
        identity_webhook_secret: None,
        schedule: crate::plane::ScheduleConfig {
            client: reqwest::Client::new(),
            token: SecretString::new(String::new()),
            destination: SCHEDULE_DESTINATION.to_owned(),
            api_base: String::new(),
            keys: None,
        },
        analytics: Analytics::silent(),
        slack_api_base: String::new(),
        vendor_client: reqwest::Client::new(),
        login: LoginConfig {
            code_pepper: SecretBytes::new(b"plane-test-pepper".to_vec()),
            app_url: afd_connector::Dashboard::parse(DASHBOARD)
                .expect("the fixture dashboard is a URL"),
            api_url: DEPLOYMENT.into(),
        },
    })
}

#[tokio::test]
async fn production_plane_maps_every_service_to_its_owned_store() {
    let plane = plane();

    assert!(same(plane.authenticator(), &plane.authenticator));
    assert!(same(plane.signup_metadata(), &plane.signup_writeback));
    assert!(same(plane.runners(), &plane.runners));
    assert!(same(plane.leases(), &plane.leases));
    assert!(same(plane.sessions(), &plane.login.logins));
    assert!(same(plane.workspaces(), &plane.tenant.workspaces));
    assert!(same(plane.workspace_directory(), &plane.tenant.workspaces));
    assert!(same(plane.api_keys(), &plane.tenant.api_keys));
    assert!(same(plane.cli_credentials(), &plane.tenant.cli_credentials));
    assert!(same(plane.fleets(), &plane.tenant.fleets));
    assert!(same(plane.preferences(), &plane.tenant.preferences));
    assert!(same(plane.approvals(), &plane.activity.approvals));
    assert!(same(plane.grants(), &plane.activity.grants));
    assert!(same(plane.events(), &plane.activity.events));
    assert!(same(plane.live(), &plane.live));
    assert!(same(plane.analytics(), &plane.analytics));
    assert!(same(plane.steering(), &plane.activity.steering));
    assert!(same(plane.memories(), &plane.leases.memories));
    assert!(same(plane.secrets(), &plane.tenant.secrets));
    assert!(same(plane.billing(), &plane.tenant.billing));
    assert!(same(plane.catalogue(), &plane.tenant.models));
    assert!(same(
        plane.runner_lease_history(),
        &plane.admin.runner_lease_history
    ));
    assert!(same(plane.models(), &plane.admin.models));
    assert!(same(plane.platform_keys(), &plane.admin.platform_keys));
    assert_eq!(plane.deployment(), DEPLOYMENT);
    assert!(plane.now() > jiff::Timestamp::UNIX_EPOCH);
}

/// The authoring, outbound and tenant surfaces each hand a route the store the
/// plane built for it, so no two routes share one by accident.
#[tokio::test]
async fn production_plane_maps_every_surface_to_its_owned_store() {
    let plane = plane();

    assert!(same(plane.authoring(), &plane.authoring.drafts));
    assert!(same(plane.authoring_setup(), &plane.authoring.setup));
    assert!(same(plane.creations(), &plane.authoring.creations));
    assert!(same(plane.authoring_models(), &plane.authoring.models));
    assert!(same(plane.ingress(), &plane.activity.ingress));
    assert!(same(plane.schedules(), &plane.outbound.schedules));
    assert!(same(plane.connectors(), &plane.outbound.connectors));
    assert!(same(plane.interjector(), &plane.outbound.interjector));
    assert!(same(plane.invite_mail(), &plane.tenant.invite_mail));
    assert!(same(plane.signups(), &plane.tenant.signups));
    assert!(same(plane.team(), &plane.tenant.team));
}

/// Each deployment fact reaches the route that needs it as it was booted: the
/// dashboard a person is sent to is not the API, the scheduler's destination
/// is the configured one, and a deployment with no admin workspace, no
/// identity secret and no signing keys answers `None` for each, which is what
/// the routes behind them fail closed on.
#[tokio::test]
async fn production_plane_hands_each_route_its_deployment_facts() {
    let plane = plane();

    assert_eq!(plane.dashboard().as_str(), format!("{DASHBOARD}/"));
    assert_ne!(plane.dashboard().as_str(), plane.deployment());
    assert_eq!(plane.schedule_destination(), SCHEDULE_DESTINATION);
    assert!(plane.schedule_signing_keys().is_none());
    assert!(plane.identity_webhook_secret().is_none());
    assert!(plane.platform_admin_workspace().is_none());
}

/// The doorbell `ServingPlane::new` builds listens on the live-stream hub: a
/// ring published on a partition's production channel reaches it. This proves
/// the helper the plane calls, not that the plane hands its result to the
/// lease store; that hand-off is one line beside it in `plane/mod.rs`.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_the_daemon_doorbell_hears_its_hub() {
    use afd_dragonfly::{Partition, ReadyPrefix, SubscriptionHub};

    /// Long enough for the listener task to hear a frame the server pushed.
    const HEARD_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

    let (hub, server) = SubscriptionHub::detached();
    let live = Live::new(hub, Ceiling::new(1));
    let doorbell = super::listening_doorbell(&live);
    let rung = Partition::new(6).expect("a partition below sixteen");
    let cursor = Partition::new(0).expect("a partition below sixteen");

    server.publish(
        &rung.wake_channel_under(&ReadyPrefix::production()),
        "token",
    );

    let deadline = tokio::time::Instant::now() + HEARD_WITHIN;
    let mut taken = None;
    while taken.is_none() && tokio::time::Instant::now() < deadline {
        taken = doorbell.take(cursor);
        tokio::task::yield_now().await;
    }
    assert_eq!(
        taken,
        Some(rung),
        "the daemon's doorbell listens on its hub"
    );
}
