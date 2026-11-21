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

use af_core::test_util::trace::Capture;
use af_crypto::secret::{Kek, SecretBytes};
use af_observability::Analytics;
use afd_api_ingress::services::IngressPlane;
use afd_api_operator::OperatorPlane;
use afd_api_runner::services::RunnerPlane;
use afd_api_tenant::services::TenantPlane;
use afd_auth::{NoCapabilitySource, NoVerifier};
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::secrets::Registry;
use afd_http::services::Shared;
use afd_sse::{Live, StreamLimit};

use super::{Capabilities, LoginConfig, PlaneParts, ServingPlane, Sessions, SignupWriteback};

/// Where a person is sent, which the plane must never confuse with the API.
const DASHBOARD: &str = "https://app.fixture.test";

/// Where API calls and machine callbacks go.
const DEPLOYMENT: &str = "https://api.fixture.test";

/// A fire token's shape, signed by nobody: it passes the shape check and so
/// reaches the read of the keys, which is what these cases are about.
const FIRE: &str = "aaa.bbb.ccc";

fn same<T: ?Sized>(left: &T, right: &T) -> bool {
    std::ptr::eq(left, right)
}

fn plane() -> ServingPlane {
    plane_with(None)
}

fn plane_with(admin: Option<af_core::id::Uuid7>) -> ServingPlane {
    let database = afd_db::test_util::unreachable_db();
    let vendors = Vendors::new(Platform::empty(), reqwest::Client::new());
    ServingPlane::new(PlaneParts {
        runtime: crate::preflight::runtime::read(&af_core::env::MapEnv::default())
            .expect("default runtime settings"),
        storage_client: reqwest::Client::new(),
        database,
        bus: afd_bus::test_util::unreachable(),
        kek: Arc::new(Kek::from_bytes([0x2a; 32])),
        capabilities: Capabilities::Unconfigured(NoCapabilitySource),
        signup_writeback: SignupWriteback::Unconfigured(crate::identity::NoWriteback),
        sessions: Sessions::Unconfigured(NoVerifier),
        broker: Arc::new(Broker::new(
            Arc::new(Registry::default()),
            Arc::new(vendors),
        )),
        runner_wake: afd_bus::wake::Wake::default(),
        frame_memory: af_stream::MemoryLimits::default(),
        live: Live::detached(StreamLimit::new(4)),
        // No admin workspace by default: the fail-closed deployment state,
        // which is what this fixture is for — the plane must map every service
        // to its own store whether or not one is present.
        platform_admin_workspace: admin,
        checkpoint_storage: None,
        identity_webhook_secret: None,
        schedule: schedule(),
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

fn schedule() -> crate::plane::ScheduleConfig {
    crate::plane::ScheduleConfig {
        client: reqwest::Client::new(),
        api_url: DEPLOYMENT.into(),
    }
}

#[tokio::test]
async fn production_plane_maps_every_service_to_its_owned_store() {
    let plane = plane();

    assert!(same(Shared::authenticator(&plane), &plane.authenticator));
    assert!(same(
        IngressPlane::signup_metadata(&plane),
        &plane.signup_writeback
    ));
    assert!(same(RunnerPlane::runners(&plane), &plane.runners));
    assert!(same(OperatorPlane::runners(&plane), &plane.runners));
    assert!(same(RunnerPlane::runs(&plane), &plane.runs));
    assert!(same(TenantPlane::sessions(&plane), &plane.login.logins));
    assert!(same(Shared::workspaces(&plane), &plane.tenant.workspaces));
    assert!(same(
        TenantPlane::workspace_directory(&plane),
        &plane.tenant.workspaces
    ));
    assert!(same(TenantPlane::api_keys(&plane), &plane.tenant.api_keys));
    assert!(same(
        TenantPlane::cli_credentials(&plane),
        &plane.tenant.cli_credentials
    ));
    assert!(same(TenantPlane::fleets(&plane), &plane.tenant.fleets));
    assert!(same(RunnerPlane::fleets(&plane), &plane.tenant.fleets));
}

/// The second half of the service map: the stores the tenant, runner and
/// operator planes each reach for activity, money and administration.
#[tokio::test]
async fn production_plane_maps_every_activity_and_admin_service_to_its_owned_store() {
    let plane = plane();

    assert!(same(
        TenantPlane::preferences(&plane),
        &plane.tenant.preferences
    ));
    assert!(same(
        TenantPlane::approvals(&plane),
        &plane.activity.approvals
    ));
    assert!(same(
        IngressPlane::approvals(&plane),
        &plane.activity.approvals
    ));
    assert!(same(TenantPlane::grants(&plane), &plane.activity.grants));
    assert!(same(TenantPlane::events(&plane), &plane.activity.events));
    assert!(same(RunnerPlane::events(&plane), &plane.activity.events));
    assert!(same(TenantPlane::live(&plane), &plane.live));
    assert!(same(TenantPlane::analytics(&plane), &plane.analytics));
    assert!(same(
        TenantPlane::steering(&plane),
        &plane.activity.steering
    ));
    assert!(same(
        RunnerPlane::steering(&plane),
        &plane.activity.steering
    ));
    assert!(same(TenantPlane::memories(&plane), &plane.runs.memories));
    assert!(same(TenantPlane::secrets(&plane), &plane.tenant.secrets));
    assert!(same(TenantPlane::billing(&plane), &plane.tenant.billing));
    assert!(same(TenantPlane::catalogue(&plane), &plane.tenant.models));
    assert!(same(
        OperatorPlane::runner_run_history(&plane),
        &plane.admin.runner_run_history
    ));
    assert!(same(OperatorPlane::models(&plane), &plane.admin.models));
    assert!(same(
        OperatorPlane::platform_keys(&plane),
        &plane.admin.platform_keys
    ));
    assert_eq!(TenantPlane::deployment(&plane), DEPLOYMENT);
    assert!(Shared::now(&plane) > jiff::Timestamp::UNIX_EPOCH);
}

/// The setup, outbound and tenant surfaces each hand a route the store the
/// plane built for it, so no two routes share one by accident.
#[tokio::test]
async fn production_plane_maps_every_surface_to_its_owned_store() {
    let plane = plane();

    assert!(same(TenantPlane::drafts(&plane), &plane.setup.drafts));
    assert!(same(TenantPlane::setup(&plane), &plane.setup.setup));
    assert!(same(TenantPlane::creations(&plane), &plane.setup.creations));
    assert!(same(TenantPlane::ingress(&plane), &plane.activity.ingress));
    assert!(same(IngressPlane::ingress(&plane), &plane.activity.ingress));
    assert!(same(
        TenantPlane::schedules(&plane),
        &plane.outbound.schedules
    ));
    assert!(same(
        RunnerPlane::schedules(&plane),
        &plane.outbound.schedules
    ));
    assert!(same(
        IngressPlane::schedules(&plane),
        &plane.outbound.schedules
    ));
    assert!(same(
        TenantPlane::connectors(&plane),
        &plane.outbound.connectors
    ));
    assert!(same(
        IngressPlane::connectors(&plane),
        &plane.outbound.connectors
    ));
    assert!(same(
        RunnerPlane::interjector(&plane),
        &plane.outbound.interjector
    ));
    assert!(same(
        TenantPlane::invite_mail(&plane),
        &plane.tenant.invite_mail
    ));
    assert!(same(IngressPlane::signups(&plane), &plane.tenant.signups));
    assert!(same(TenantPlane::team(&plane), &plane.tenant.team));
}

/// Each deployment fact reaches the route that needs it as it was booted: the
/// dashboard a person is sent to is not the API, and a deployment with no admin
/// workspace, no identity secret and so no `qstash` bag answers `None` or
/// `Absent` for each, which is what the routes behind them fail closed on.
#[tokio::test]
async fn production_plane_hands_each_route_its_deployment_facts() {
    let plane = plane();

    assert_eq!(
        TenantPlane::dashboard(&plane).as_str(),
        format!("{DASHBOARD}/")
    );
    assert_ne!(TenantPlane::dashboard(&plane).as_str(), plane.deployment());
    let fire = IngressPlane::scheduler(&plane).verify(FIRE, b"{}").await;
    assert_eq!(
        fire.as_ref().err().and_then(afd_cron::Error::bag_fault),
        Some(afd_vault::bag::BagFault::Absent),
        "no admin workspace means no `qstash` bag, refused before the vault is read"
    );
    assert!(IngressPlane::identity_webhook_secret(&plane).is_none());
    assert!(TenantPlane::platform_admin_workspace(&plane).is_none());
    assert!(IngressPlane::platform_admin_workspace(&plane).is_none());
}

/// With an admin workspace the scheduler reads its bag from the vault, so a
/// vault that will not answer is a retryable outage and not an absent bag.
#[tokio::test]
async fn an_admin_workspace_makes_the_scheduler_read_its_bag_from_the_vault() {
    let admin = af_core::id::Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011")
        .expect("a fixture identifier");
    let plane = plane_with(Some(admin));

    let refused = plane
        .scheduler()
        .verify(FIRE, b"{}")
        .await
        .expect_err("the fixture's database is unreachable");

    assert_eq!(
        refused.bag_fault(),
        None,
        "the vault was asked, so no field was read"
    );
    assert!(
        refused.is_retryable(),
        "an outage heals; an absent bag does not"
    );
}

/// Preflight refuses an API URL that cannot form a callback, so this arm is the
/// belt to its braces: should one arrive, the scheduler is built to match no
/// fire and the log says why, rather than registering a truncated callback.
#[tokio::test]
async fn an_api_url_that_cannot_form_a_callback_leaves_a_line_saying_so() {
    let capture = Capture::install();

    let _scheduler = super::platform::qstash(
        &afd_db::test_util::unreachable_db(),
        &Arc::new(Kek::from_bytes([0x2a; 32])),
        None,
        crate::plane::ScheduleConfig {
            client: reqwest::Client::new(),
            api_url: "not a url".into(),
        },
    );

    let logged = capture.only("scheduler_callback_unusable");
    assert_eq!(
        logged.field("error_code"),
        Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
}

/// The wake listener `ServingPlane::new` builds hears the NATS wake ring: a
/// ring published on a thread's subject (`af.thread.<id>.wake`) reaches it. This proves
/// the helper the plane calls, not that the plane hands its result to the
/// run store; that hand-off is one line beside it in `plane/mod.rs`.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_the_wake_queue_hears_the_shared_bus() {
    use af_core::id::Uuid7;
    use afd_bus::{Subject, wake::Wake};

    /// Long enough for the listener task to hear a frame the server pushed.
    const HEARD_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

    let (bus, server) = afd_bus::test_util::detached().expect("fixture bus");
    let wake = Wake::default();
    let stop = tokio_util::sync::CancellationToken::new();
    let _listeners = wake.listen(&bus, &stop).await.expect("wake subscriptions");
    let rung = Uuid7::parse("0195b4ba-8d3a-7000-8abc-000000000001").expect("fleet");
    server.publish(&Subject::ThreadWake(rung.clone()), rung.as_str());

    let deadline = tokio::time::Instant::now() + HEARD_WITHIN;
    let mut taken = None;
    while taken.is_none() && tokio::time::Instant::now() < deadline {
        taken = wake.take();
        tokio::task::yield_now().await;
    }
    assert_eq!(
        taken,
        Some(rung),
        "the daemon's wake listener hears the wake ring"
    );
}
