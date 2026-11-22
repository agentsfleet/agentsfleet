//! An opening that cannot read one valid hello, or one exact confirmation,
//! takes no ownership.

use super::{control::ControlOpening, request::ControlRequest};
use crate::{ConnectionContext, Gateway, error::ErrorKind};
use af_core::{error_code, id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_stream::{Budget, ConnectionInfo, Transport};
use af_wire::protocol::StreamKind;
use afd_auth::{credential::Presented, directory::Digest, principal::Runner};
use axum::body::Body;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn gateway() -> TestResult<Gateway> {
    crate::fixture::gateway(
        afd_db::test_util::unreachable_db(),
        afd_bus::test_util::unreachable(),
        afd_bus::wake::Wake::default(),
    )
}

/// A runner that is not enrolled; only refusals before any datastore read reach it.
fn stranger() -> TestResult<(Runner, Digest)> {
    Ok((
        Runner::new(Uuid7::parse("019f0000-0000-7000-8000-000000000001")?, false),
        Digest::of(&Presented::new("agt_r_fixture")?),
    ))
}

/// The opening of `runner`'s control lane, whose body carries `frames` then
/// ends, and the response body its answers are written to.
async fn opening(
    gateway: &Gateway,
    (runner, digest): (Runner, Digest),
    frames: &[Value],
) -> TestResult<crate::Result<(ControlOpening, Body)>> {
    let stop = CancellationToken::new();
    let (mut upload, body) = af_stream::Outgoing::pipe();
    let budget = Budget::default();
    for frame in frames {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let encoded =
            af_stream::encode(frame, &budget, StreamKind::Control, &stop, deadline).await?;
        af_stream::write(&mut upload, &encoded, &stop, deadline).await?;
    }
    drop(upload);
    let request = ControlRequest {
        context: Arc::new(ConnectionContext::new(ConnectionInfo {
            transport: Transport::H2,
            peer: ([127, 0, 0, 1], 0).into(),
            stop,
            native_stats: None,
        })),
        previous: None,
        digest,
        body: Body::new(body),
    };
    Ok(ControlOpening::new(gateway, runner, request))
}

/// Why preparing a stranger's opening over `frames` failed.
async fn refused(gateway: &Gateway, frames: &[Value]) -> TestResult<crate::Error> {
    let (opening, _response) = opening(gateway, stranger()?, frames).await??;
    Ok(opening
        .prepare()
        .await
        .err()
        .ok_or("an opening without a valid hello was prepared")?)
}

fn hello() -> Value {
    json!({"type":"hello","request_id":1,"accepted_kinds":["fleet"],"running":[],"credits":1})
}

#[tokio::test]
async fn a_control_lane_that_ends_before_its_hello_is_closed() -> TestResult {
    let closed = refused(&gateway()?, &[]).await?;
    assert!(matches!(closed.kind(), ErrorKind::Closed), "{closed:?}");
    Ok(())
}

#[tokio::test]
async fn a_control_lane_that_opens_with_anything_but_hello_is_refused() -> TestResult {
    let ping = refused(&gateway()?, &[json!({"type":"ping","request_id":1})]).await?;
    assert!(matches!(ping.kind(), ErrorKind::Protocol), "{ping:?}");
    assert_eq!(ping.code(), error_code::INVALID_REQUEST);
    Ok(())
}

#[tokio::test]
async fn a_hello_naming_a_malformed_running_run_is_refused() -> TestResult {
    let hello = json!({"type":"hello","request_id":1,"accepted_kinds":["fleet"],
        "running":["zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz"],"credits":1});
    let malformed = refused(&gateway()?, &[hello]).await?;
    assert!(
        matches!(malformed.kind(), ErrorKind::Identifier { .. }),
        "{malformed:?}"
    );
    assert_eq!(malformed.code(), error_code::UUIDV7_INVALID_ID_SHAPE);
    Ok(())
}

#[tokio::test]
async fn a_draining_replica_takes_no_new_control_lane() -> TestResult {
    let gateway = gateway()?;
    gateway.inner.registry.begin_drain();
    let refused = opening(&gateway, stranger()?, &[]).await?.err();
    assert!(
        refused
            .as_ref()
            .is_some_and(|error| matches!(error.kind(), ErrorKind::Closed)),
        "{refused:?}"
    );
    assert_eq!(
        gateway.inner.dependencies.stream_limit.live(),
        0,
        "the refusal holds no stream permit"
    );
    Ok(())
}

/// Whether confirming `runner`'s opening, whose body carries a hello and then
/// `confirmation` when one is given, was refused as `expected`.
async fn confirmation_refused(
    gateway: &Gateway,
    runner: (Runner, Digest),
    confirmation: Option<Value>,
    expected: fn(&ErrorKind) -> bool,
) -> TestResult {
    let frames: Vec<Value> = std::iter::once(hello()).chain(confirmation).collect();
    let (opening, _response) = opening(gateway, runner, &frames).await??;
    let refused = opening.prepare().await?.confirm().await.err();
    assert!(
        refused.as_ref().is_some_and(|error| expected(error.kind())),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn only_the_exact_confirmation_of_the_proposal_commits_ownership() -> TestResult {
    use afd_db::{config::DbRole, test_util::TestDatabase};
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let owner = lane.open(DbRole::Migrator, &[]).await;
    let runners = afd_runner::Runners::new(database.clone(), af_crypto::entropy::Entropy::new());
    let enrolled = runners
        .register(
            &registration(),
            "fixture:platform-operator",
            af_core::clock::now(),
        )
        .await?;
    let runner = || {
        (
            Runner::new(enrolled.runner_id.clone(), false),
            enrolled.token.digest().clone(),
        )
    };
    let (bus, _server) = afd_bus::test_util::detached()?;
    let gateway = crate::fixture::gateway(database.clone(), bus, afd_bus::wake::Wake::default())?;
    let foreign = json!({"type":"confirm","request_id":1,
        "connection_id":afd_db::test_util::mint_id()});
    confirmation_refused(&gateway, runner(), Some(foreign), |kind| {
        matches!(kind, ErrorKind::Protocol)
    })
    .await?;
    confirmation_refused(&gateway, runner(), None, |kind| {
        matches!(kind, ErrorKind::Closed)
    })
    .await?;
    let claimed: Option<Uuid7> =
        sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id = $1")
            .bind(&enrolled.runner_id)
            .fetch_one(&mut *owner.acquire().await?)
            .await?;
    assert_eq!(claimed, None, "no refused confirmation took ownership");
    sqlx::query("DELETE FROM runner.runners WHERE id = $1")
        .bind(&enrolled.runner_id)
        .execute(&mut *owner.acquire().await?)
        .await?;
    database.close().await;
    owner.close().await;
    lane.cleanup().await;
    Ok(())
}

fn registration() -> af_wire::runner::RegisterRequest<'static> {
    use af_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
    af_wire::runner::RegisterRequest {
        host_id: format!("opening-{}.test", afd_db::test_util::mint_id()).into(),
        labels: Vec::new(),
        assigned_policy: AssignedPolicy {
            durable_tasks_enabled: true,
            decision_model_id: None,
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
    }
}
