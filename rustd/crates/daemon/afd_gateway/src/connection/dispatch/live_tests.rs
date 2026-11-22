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
    run::AcceptedKinds,
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
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, pool).await;
        let owner = lane.open(DbRole::Migrator, &[]).await;
        let runners = afd_runner::Runners::new(database.clone(), Entropy::new());
        let enrolled = runners
            .register(&registration(), "fixture:platform-operator", clock::now())
            .await?;
        let (bus, _server) = afd_bus::test_util::detached()?;
        let gateway =
            crate::fixture::gateway(database.clone(), bus, afd_bus::wake::Wake::default())?;
        let connection = RunnerConnection::open(
            &gateway,
            enrolled.runner_id.clone(),
            enrolled.token.digest().clone(),
            AcceptedKinds::ALL,
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
    let hello = json!({"type":"hello","request_id":2,"accepted_kinds":["fleet","setup"],
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
