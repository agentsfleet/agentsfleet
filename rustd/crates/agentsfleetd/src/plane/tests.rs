//! Composition-root ownership proofs.

#![expect(
    clippy::expect_used,
    reason = "test construction failures should stop the mapping proof"
)]

use std::sync::Arc;

use afd_api::{Services as _, TenantSurface as _};
use afd_auth::{NoCapabilitySource, NoVerifier};
use afd_core::env::MapEnv;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::secrets::Registry;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::config::{DbRole, PoolConfig};
use afd_observability::Analytics;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_sse::{Ceiling, Live};

use super::{Capabilities, LoginConfig, PlaneParts, ServingPlane, Sessions};

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
    let queue = afd_redis::Redis::unreachable(&RedisConfig::from_url(
        RedisRole::Api,
        "redis://127.0.0.1:1".to_owned(),
    ))
    .expect("a lazy Redis handle opens no socket");
    let vendors = Vendors::new(Platform::empty(), reqwest::Client::new());
    ServingPlane::new(PlaneParts {
        database,
        queue,
        kek: Arc::new(Kek::from_bytes([0x2a; 32])),
        capabilities: Capabilities::Unconfigured(NoCapabilitySource),
        sessions: Sessions::Unconfigured(NoVerifier),
        stores: crate::bundles::resolve(None),
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
            token: String::new(),
            destination: String::new(),
            api_base: String::new(),
            keys: None,
        },
        analytics: Analytics::silent(),
        login: LoginConfig {
            code_pepper: SecretBytes::new(b"plane-test-pepper".to_vec()),
            app_url: "https://app.fixture.test".to_owned(),
            api_url: "https://api.fixture.test".into(),
        },
    })
}

#[tokio::test]
async fn production_plane_maps_every_service_to_its_owned_store() {
    let plane = plane();

    assert!(same(plane.authenticator(), &plane.authenticator));
    assert!(same(plane.runners(), &plane.runners));
    assert!(same(plane.leases(), &plane.leases));
    assert!(same(plane.bundles(), &plane.bundles));
    assert!(same(plane.sessions(), &plane.logins));
    assert!(same(plane.workspaces(), &plane.workspaces));
    assert!(same(plane.workspace_directory(), &plane.workspaces));
    assert!(same(plane.api_keys(), &plane.api_keys));
    assert!(same(plane.cli_credentials(), &plane.cli_credentials));
    assert!(same(plane.fleets(), &plane.fleets));
    assert!(same(plane.preferences(), &plane.preferences));
    assert!(same(plane.approvals(), &plane.approvals));
    assert!(same(plane.grants(), &plane.grants));
    assert!(same(plane.events(), &plane.events));
    assert!(same(plane.live(), &plane.live));
    assert!(same(plane.analytics(), &plane.analytics));
    assert!(same(plane.steering(), &plane.steering));
    assert!(same(plane.memories(), &plane.leases.memories));
    assert!(same(plane.secrets(), &plane.secrets));
    assert!(same(plane.billing(), &plane.billing));
    assert!(same(plane.catalogue(), &plane.models));
    assert!(same(
        plane.runner_lease_history(),
        &plane.runner_lease_history
    ));
    assert!(same(plane.models(), &plane.admin_models));
    assert!(same(plane.platform_keys(), &plane.platform_keys));
    assert!(same(plane.libraries(), &plane.libraries));
    assert!(same(plane.library_imports(), &plane.library_imports));
    assert_eq!(plane.deployment(), "https://api.fixture.test");
    assert!(plane.now().as_millis() > 0);
}
