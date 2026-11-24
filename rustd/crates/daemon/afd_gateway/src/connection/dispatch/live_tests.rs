//! A connected runner's frames are answered through the production dispatch,
//! over the live datastore and an in-process broker.
#![expect(
    clippy::indexing_slicing,
    reason = "test: a step indexes the JSON frame it just received"
)]

use af_core::{clock, error_code, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{
    protocol::Up,
    runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier},
};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{TestDatabase, mint_id},
};
use serde_json::{Value, json};

use crate::{Gateway, error::ErrorKind, test_util::RunnerConnection};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// One enrolled runner connected to a gateway over the shared lane.
struct Live {
    gateway: Gateway,
    connection: RunnerConnection,
    runner: Uuid7,
    database: Db,
    owner: Db,
    lane: TestDatabase,
}

impl Live {
    async fn connect() -> TestResult<Self> {
        Self::over(&[]).await
    }

    /// A connection whose gateway pool is opened with the `pool` knobs.
    async fn over(pool: &[(&str, &str)]) -> TestResult<Self> {
        let (bus, _server) = afd_bus::test_util::detached()?;
        Self::on(pool, bus).await
    }

    /// A connection whose gateway pool is opened with the `pool` knobs, over `bus`.
    async fn on(pool: &[(&str, &str)], bus: afd_bus::Handle) -> TestResult<Self> {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, pool).await;
        let owner = lane.open(DbRole::Migrator, &[]).await;
        let runners = afd_runner::Runners::new(database.clone(), Entropy::new());
        let enrolled = runners
            .register(&registration(), "fixture:platform-operator", clock::now())
            .await?;
        let gateway =
            crate::fixture::gateway(database.clone(), bus, afd_bus::wake::Wake::default())?;
        let connection = RunnerConnection::open(
            &gateway,
            enrolled.runner_id.clone(),
            enrolled.token.digest().clone(),
            1,
        )
        .await?;
        Ok(Self {
            gateway,
            connection,
            runner: enrolled.runner_id,
            database,
            owner,
            lane,
        })
    }

    /// The decoded answer to `frame`, sent as the runner would send it.
    async fn ask(&mut self, frame: &Value) -> crate::Result<Value> {
        let text = frame.to_string();
        let message: Up<'_> = serde_json::from_str(&text)?;
        Ok(self.connection.request(&message).await?.decode()?)
    }

    async fn close(self) -> TestResult {
        self.connection.close().await;
        sqlx::query("DELETE FROM runner.runners WHERE id = $1")
            .bind(&self.runner)
            .execute(&mut *self.owner.acquire().await?)
            .await?;
        self.database.close().await;
        self.owner.close().await;
        self.lane.cleanup().await;
        Ok(())
    }
}

fn registration() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: format!("dispatch-{}.test", mint_id()).into(),
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

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_opening_frame_on_an_open_connection_is_a_protocol_error() -> TestResult {
    let mut live = Live::connect().await?;
    let hello = json!({"type":"hello","request_id":2,
        "running":[],"credits":1});
    let confirm = json!({"type":"confirm","request_id":3,"connection_id":mint_id()});
    for frame in [hello, confirm] {
        let refused = live.ask(&frame).await.err();
        assert!(
            refused
                .as_ref()
                .is_some_and(|error| matches!(error.kind(), ErrorKind::Protocol)),
            "{frame}: {refused:?}"
        );
    }
    live.close().await
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_answers_each_run_it_names_on_its_own() -> TestResult {
    let mut live = Live::connect().await?;
    let unknown = mint_id();
    let renewed = live
        .ask(&json!({"type":"renew_batch","request_id":4,"runs":[
            {"run_id":"zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz","request":{"requests":[]}},
            {"run_id":unknown,"request":{"requests":[]}}
        ]}))
        .await?;
    assert_eq!(renewed["type"], "renewed");
    assert_eq!(
        renewed["runs"][0]["run_id"],
        "zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz"
    );
    assert_eq!(renewed["runs"][0]["answer"]["status"], "refused");
    assert_eq!(
        renewed["runs"][0]["answer"]["error_code"],
        error_code::UUIDV7_INVALID_ID_SHAPE.as_str()
    );
    assert_eq!(renewed["runs"][0]["answer"]["retryable"], false);
    assert_eq!(renewed["runs"][1]["run_id"], unknown.as_str());
    assert_eq!(
        renewed["runs"][1]["answer"]["error_code"],
        error_code::RUN_NOT_FOUND.as_str()
    );
    let malformed_only = live
        .ask(&json!({"type":"renew_batch","request_id":5,"runs":[
            {"run_id":"yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy","request":{"requests":[]}}
        ]}))
        .await?;
    assert_eq!(
        malformed_only["runs"],
        json!([{"run_id":"yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy","answer":renewed["runs"][0]["answer"]}])
    );
    live.close().await
}

/// An upload verification for `run`, the bulk action that holds a permit.
fn verification(request_id: u64, run: &Uuid7) -> Value {
    json!({"type":"turn_checkpoint","request_id":request_id,"run_id":run,"request":{
        "fencing_token":1,"operation_id":mint_id(),"action":{"kind":"authorize",
        "checkpoint":{"checkpoint_id":mint_id(),"pin_id":mint_id()},
        "purpose":"upload","object_ids":[mint_id()]}}})
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_upload_verification_with_no_replica_permit_left_is_refused_unavailable() -> TestResult {
    let mut live = Live::connect().await?;
    let run = mint_id();
    let free = live.ask(&verification(6, &run)).await?;
    assert_eq!(free["answer"]["status"], "refused");
    assert_ne!(
        free["answer"]["error_code"],
        error_code::RUN_CHECKPOINT_UNAVAILABLE.as_str(),
        "with permits free the verification reached the run"
    );
    let gateway = live.gateway.clone();
    let held = gateway.inner.checkpoint_verifications.try_acquire_many(8)?;
    let busy = live.ask(&verification(7, &run)).await?;
    assert_eq!(busy["type"], "turn_checkpoint");
    assert_eq!(busy["answer"]["status"], "refused");
    assert_eq!(
        busy["answer"]["error_code"],
        error_code::RUN_CHECKPOINT_UNAVAILABLE.as_str()
    );
    drop(held);
    let again = live.ask(&verification(8, &run)).await?;
    assert_eq!(again["answer"]["error_code"], free["answer"]["error_code"]);
    live.close().await
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_that_misses_its_deadline_refuses_every_run_retryably() -> TestResult {
    // One pooled connection, and an acquire that outwaits the frame's deadline.
    let mut live = Live::over(&[
        ("DATABASE_POOL_SIZE", "1"),
        ("DATABASE_MIN_POOL_SIZE", "1"),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", "20000"),
    ])
    .await?;
    let held = live.database.acquire().await?;
    let (first, second) = (mint_id(), mint_id());
    let renewed = live
        .ask(&json!({"type":"renew_batch","request_id":9,"runs":[
            {"run_id":first,"request":{"requests":[]}},
            {"run_id":second,"request":{"requests":[]}}
        ]}))
        .await?;
    drop(held);
    let timed_out = json!({"status":"refused",
        "error_code":error_code::INTERNAL_OPERATION_FAILED.as_str(),
        "detail":"The stream timed out while renewing work. Retain the last confirmed run deadline.",
        "retryable":true});
    for (index, run) in [first, second].iter().enumerate() {
        assert_eq!(renewed["runs"][index]["run_id"], run.as_str());
        assert_eq!(renewed["runs"][index]["answer"], timed_out, "run {index}");
    }
    live.close().await
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_the_datastore_cannot_take_now_keeps_each_run_for_a_retry() -> TestResult {
    // One pooled connection, held, and an acquire that gives up well inside the frame.
    let mut live = Live::over(&[
        ("DATABASE_POOL_SIZE", "1"),
        ("DATABASE_MIN_POOL_SIZE", "1"),
        ("DATABASE_ACQUIRE_TIMEOUT_MS", "200"),
    ])
    .await?;
    let held = live.database.acquire().await?;
    let run = mint_id();
    let renewed = live
        .ask(&json!({"type":"renew_batch","request_id":10,"runs":[
            {"run_id":run,"request":{"requests":[]}}
        ]}))
        .await?;
    drop(held);
    assert_eq!(renewed["runs"][0]["run_id"], run.as_str());
    assert_eq!(renewed["runs"][0]["answer"]["status"], "refused");
    assert_eq!(
        renewed["runs"][0]["answer"]["error_code"],
        error_code::INTERNAL_DB_UNAVAILABLE.as_str()
    );
    assert_eq!(renewed["runs"][0]["answer"]["retryable"], true);
    live.close().await
}

/// A connection that is already closing starts no command listener: it
/// returns before it asks the broker for anything.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_closed_connection_listens_for_no_commands() -> TestResult {
    let mut live = Live::on(&[], afd_bus::Handle::new(crate::fixture::Silent)).await?;
    live.connection.shutdown().await;
    let services = std::sync::Arc::clone(live.connection.services());
    // The silent broker would hold a subscription forever; the bound proves none was asked.
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        super::super::commands::listen(services),
    )
    .await??;
    live.close().await
}

/// A broker that refuses the command subscription is asked again on the
/// backoff while the connection lives, each refusal observed, and the
/// listener ends cleanly when the connection stops.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_refused_command_subscription_is_retried_until_the_connection_stops() -> TestResult {
    let live = Live::on(&[], afd_bus::test_util::unreachable()).await?;
    let services = std::sync::Arc::clone(live.connection.services());
    let capture = af_core::test_util::trace::Capture::install();
    let listening = tokio::spawn(super::super::commands::listen(std::sync::Arc::clone(
        &services,
    )));
    // The first retry waits 100 ms plus at most a 250 ms stagger; the second twice that.
    tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
    services.handle.stop.cancel();
    listening.await??;
    let refused = capture
        .events()
        .into_iter()
        .filter(|event| {
            event.field("event") == Some("runner_stream_operation_failed")
                && event.field("operation") == Some("recheck")
        })
        .count();
    assert!(
        refused >= 3,
        "each refused subscription is observed: {refused}"
    );
    live.close().await
}

/// A fleet run's durable-task progress the datastore cannot retain is
/// observed and dropped: cosmetic progress never fails the runner's frame.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn fleet_progress_the_datastore_cannot_retain_is_observed_not_returned() -> TestResult {
    let mut live = Live::connect().await?;
    let run = mint_id();
    // What an assignment leaves for a live fleet run: its activity target.
    live.connection.services().cache.insert(
        afd_fleet::run::AssignedRun {
            run_id: run.clone(),
            expires_at: jiff::Timestamp::MAX,
            target: Some(afd_fleet::run::activity::Target {
                fleet_id: mint_id(),
                turn_id: mint_id(),
                run_created_at: clock::now(),
                turn_created_at: clock::now(),
                timing_eligible: true,
                fence: 1,
            }),
            reported: None,
        },
        clock::now(),
    )?;
    live.database.close().await;
    let capture = af_core::test_util::trace::Capture::install();
    let progress = json!({"activity_id":mint_id(),"durable_task_id":null,
        "source_turn_id":null,"operation":"capture","phase":"uploading","provider":null,
        "model":null,"completed":2,"total":3,"sequence":7});
    let frame = json!({"type":"progress","run_id":run,"request":{"frames":[
        {"durable_task_transfer_progress":progress}]}});
    let text = frame.to_string();
    let message: Up<'_> = serde_json::from_str(&text)?;
    live.connection.send(&message).await?;
    let observed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| {
            event.field("event") == Some("runner_stream_operation_failed")
                && event.field("operation") == Some("progress")
        })
        .collect();
    // The operation wrapper observes the failure once; the handler adds none.
    assert_eq!(observed.len(), 1, "{observed:?}");
    assert!(
        observed
            .iter()
            .all(|event| event.level == tracing::Level::WARN
                && event.field("error_code") == Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
                && event.field("retryable") == Some("true"))
    );
    live.close().await
}
