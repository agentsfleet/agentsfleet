//! A tenant's own runner leaves the stream when its tenant revokes it, and when
//! its tenant is erased.

use std::borrow::Cow;

use af_core::error_code::{RUN_ADMIN_STATE_BLOCKED, RUN_INVALID_RUNNER_TOKEN};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_stream::Transport;
use af_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use afd_db::test_util::{TenantSeed, mint_id};
use afd_runner::Runners;

use super::{PUSH_BOUND, TestResult, fixture::Fixture, signals};
use crate::wire;

/// A registration for a host the tenant runs itself.
fn request() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("tenant-box.fixture.test"),
        assigned_policy: AssignedPolicy {
            decision_model_id: None,
            durable_tasks_enabled: false,
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::DenyAllEgress,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels: Vec::new(),
    }
}

/// Registers a runner for `tenant` and answers its id and token.
async fn tenant_runner(fixture: &Fixture, tenant: &Uuid7) -> TestResult<(Uuid7, String)> {
    let runners = Runners::new(fixture.scene.booted.database.clone(), Entropy::new());
    let enrolled = runners
        .register_tenant(
            tenant,
            &request(),
            "fixture:tenant-owner",
            af_core::clock::now(),
        )
        .await?;
    Ok((enrolled.runner_id, enrolled.token.expose().to_owned()))
}

/// The production runner downlink, holding `token`.
async fn connected(
    fixture: &Fixture,
    token: &str,
) -> TestResult<afr_supervisor::test_util::Session> {
    use afr_supervisor::config::{
        Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL,
    };
    let origin = format!(
        "https://localhost:{}",
        fixture.peer.runner_stream_address.port()
    );
    let root = agentsfleetd::test_util::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    let config = Config::from_env(&af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, fixture.scene.base.as_str()),
        (ENV_RUNNER_TOKEN, token),
        (ENV_STREAM_CA, root.as_str()),
        (ENV_STREAM_URL, origin.as_str()),
    ]))?;
    let session = afr_supervisor::test_util::Session::connect(&config).await?;
    let bytes = serde_json::to_vec(&wire::capable_beat())?;
    let beat = serde_json::from_slice(&bytes)?;
    session.heartbeat(&beat).await?;
    Ok(session)
}

// Value: protects=a tenant's revoke ends its host's stream at once and for good;
// fails_when=the tenant route writes the row but publishes no recheck, or the
// stream keeps a revoked tenant runner; why_new=tenant runners; seam=the real
// downlink and the tenant HTTP route over two daemons.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn tenant_revoke_closes_its_runner_stream() -> TestResult {
    let fixture = Fixture::start().await?;
    let (runner, token) = tenant_runner(&fixture, &fixture.scene.tenant).await?;
    let mut session = connected(&fixture, &token).await?;

    fixture
        .operator
        .tenant_runner(&fixture.scene, &runner, "revoke")
        .await?;
    let stopped = tokio::time::timeout(PUSH_BOUND, session.assign()).await?;
    assert!(
        stopped.is_err(),
        "a revoked tenant runner awaits no more assignments"
    );
    let failure = session
        .close()
        .await
        .err()
        .ok_or("the tenant's revoke did not halt the downlink")?;
    assert_eq!(failure.refusal_code(), Some(RUN_ADMIN_STATE_BLOCKED));
    signals::opening_refused(
        &fixture.scene,
        &token,
        RUN_ADMIN_STATE_BLOCKED,
        Transport::H2,
    )
    .await?;
    fixture.finish().await
}

// Value: protects=erasing a tenant takes its connected runner off the stream;
// fails_when=the cascade deletes the row but the live session keeps serving it;
// why_new=tenant runners cascade with their tenant; seam=the real downlink.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn tenant_erasure_closes_its_runner_stream() -> TestResult {
    let fixture = Fixture::start().await?;
    let erased = mint_id();
    TenantSeed::new(&erased)
        .insert(&mut *fixture.scene.booted.database.acquire().await?)
        .await?;
    let (_, token) = tenant_runner(&fixture, &erased).await?;
    let session = connected(&fixture, &token).await?;

    sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
        .bind(&erased)
        .execute(&mut *fixture.scene.booted.database.acquire().await?)
        .await?;
    let bytes = serde_json::to_vec(&wire::capable_beat())?;
    let beat = serde_json::from_slice(&bytes)?;
    let answer = session.heartbeat(&beat).await;
    let refused = answer.as_ref().map_or(true, |value| {
        [value.pointer("/status"), value.pointer("/answer/status")]
            .into_iter()
            .flatten()
            .any(|status| status == "refused")
    });
    assert!(
        refused,
        "the erased runner's next beat is refused: {answer:?}"
    );
    signals::opening_refused(
        &fixture.scene,
        &token,
        RUN_INVALID_RUNNER_TOKEN,
        Transport::H2,
    )
    .await?;
    fixture.finish().await
}
