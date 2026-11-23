//! A draft-owned run on a connected runner, through the production
//! assignment, renewal and progress dispatch over the live datastore.
//!
//! Each case owns a private database: draft placement takes the oldest queued
//! draft across every tenant, so on the shared lane a sibling suite's draft
//! could win this runner's pass.
#![expect(
    clippy::indexing_slicing,
    reason = "test: a step indexes the JSON frame it just received"
)]

use af_core::{clock, error_code, id::Uuid7, money::NANOS_PER_USD, test_util::trace::Capture};
use af_crypto::{aad::Aad, entropy::Entropy, envelope::Sealer, secret::Kek};
use af_wire::{
    protocol::Up,
    run::AcceptedKinds,
    runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier},
};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{
        ModelSeed, TenantSeed, TestDatabase, UserSeed, WalletSeed, WorkspaceSeed, mint_id,
    },
};
use afd_setup::{
    Access,
    types::{CreateDraft, Source},
};
use serde_json::{Value, json};

use crate::{Gateway, test_util::RunnerConnection};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The model key's name in the workspace vault.
const SECRET_REF: &str = "setup-model";
const PROVIDER: &str = "openai";
/// The gateway fixture's key, which the assignment opens the model key under.
const KEY: [u8; 32] = [0x11; 32];
/// The seated model routes through the Responses API.
const RESPONSES_INFERENCE: &str =
    r#"{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":8192}"#;
const ENTRY: &str = "\
INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,workspace_id,selected,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,false,now(),now())";
const SECRET: &str = "\
INSERT INTO vault.secrets(id,workspace_id,key_name,dek_ciphertext,dek_nonce,dek_tag,secret_nonce,
secret_ciphertext,secret_tag,kek_version,meta_provider,meta_has_key,created_at,updated_at)
VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,true,now(),now())";

/// A tenant with one queued setup turn, and a platform runner connected to a
/// gateway over the case's own database.
struct Seated {
    gateway: Gateway,
    connection: RunnerConnection,
    database: Db,
    owner: Db,
    lane: TestDatabase,
}

impl Seated {
    async fn create() -> TestResult<Self> {
        let lane = TestDatabase::create().await;
        let owner = lane.open(DbRole::Migrator, &[]).await;
        afd_db::Migrator::new().run(&owner).await?;
        let database = lane.open(DbRole::Api, &[]).await;
        let (tenant, workspace, person, entry) = (mint_id(), mint_id(), mint_id(), mint_id());
        seat(&owner, &tenant, &workspace, &person, &entry).await?;
        let drafts = afd_setup::Drafts::new(database.clone(), Entropy::new());
        let request = CreateDraft {
            save: false,
            source: Source::Blank,
            setup_model_ref: entry.to_string(),
            message: Some("Watch the checkout queue".to_owned()),
            operation_id: mint_id(),
        };
        let access = Access {
            workspace: &workspace,
            subject: person.as_str(),
        };
        drafts.create(access, request, clock::now()).await?;
        let runners = afd_runner::Runners::new(database.clone(), Entropy::new());
        let enrolled = runners
            .register(&registration(), "fixture:platform-operator", clock::now())
            .await?;
        let (bus, _server) = afd_bus::test_util::detached()?;
        let gateway =
            crate::fixture::gateway(database.clone(), bus, afd_bus::wake::Wake::default())?;
        let connection = RunnerConnection::open(
            &gateway,
            enrolled.runner_id,
            enrolled.token.digest().clone(),
            AcceptedKinds::ALL,
            1,
        )
        .await?;
        Ok(Self {
            gateway,
            connection,
            database,
            owner,
            lane,
        })
    }

    /// The draft run the production assignment hands this runner.
    async fn take(&mut self) -> TestResult<Uuid7> {
        let frame = self
            .connection
            .assign()
            .await?
            .ok_or("the queued setup turn is assigned")?;
        let assigned: Value = frame.decode()?;
        assert_eq!(assigned["run"]["owner"]["kind"], "draft", "{assigned}");
        let run = assigned["run"]["run_id"]
            .as_str()
            .ok_or("the assignment names its run")?;
        Ok(Uuid7::parse(run)?)
    }

    async fn ask(&mut self, frame: &Value) -> crate::Result<Value> {
        let text = frame.to_string();
        let message: Up<'_> = serde_json::from_str(&text)?;
        Ok(self.connection.request(&message).await?.decode()?)
    }

    fn holds_draft(&self, run: &Uuid7) -> bool {
        self.connection.services().cache.draft(run).is_some()
    }

    async fn close(self) -> TestResult {
        self.connection.close().await;
        drop(self.gateway);
        self.database.close().await;
        self.owner.close().await;
        self.lane.cleanup().await;
        Ok(())
    }
}

/// The tenant's owner, a priced model behind a key sealed under [`KEY`], and
/// a funded wallet.
async fn seat(
    owner: &Db,
    tenant: &Uuid7,
    workspace: &Uuid7,
    person: &Uuid7,
    entry: &Uuid7,
) -> TestResult {
    let mut connection = owner.acquire().await?;
    TenantSeed::new(tenant).insert(&mut *connection).await?;
    WorkspaceSeed::new(workspace, tenant)
        .insert(&mut *connection)
        .await?;
    UserSeed::new(person, tenant, person.as_str(), person.as_str())
        .insert(&mut *connection)
        .await?;
    let catalogue = mint_id();
    let model = format!("gpt-4o-{catalogue}");
    ModelSeed::new(&catalogue, PROVIDER, &model, 128_000)
        .rates(NANOS_PER_USD, NANOS_PER_USD, NANOS_PER_USD)
        .inference(RESPONSES_INFERENCE)
        .insert(&mut *connection)
        .await?;
    sqlx::query(ENTRY)
        .bind(entry)
        .bind(tenant)
        .bind(&model)
        .bind(SECRET_REF)
        .bind(workspace)
        .execute(&mut *connection)
        .await?;
    let sealed = Sealer::new().seal(
        &Kek::from_bytes(KEY),
        &Aad::new(workspace.as_str(), SECRET_REF),
        br#"{"provider":"openai","api_key":"fixture-setup-key"}"#,
    )?;
    sqlx::query(SECRET)
        .bind(mint_id())
        .bind(workspace)
        .bind(SECRET_REF)
        .bind(sealed.wrapped_dek())
        .bind(sealed.dek_nonce().as_slice())
        .bind(sealed.dek_tag().as_slice())
        .bind(sealed.payload_nonce().as_slice())
        .bind(sealed.payload_ciphertext())
        .bind(sealed.payload_tag().as_slice())
        .bind(sealed.kek_version())
        .bind(PROVIDER)
        .execute(&mut *connection)
        .await?;
    WalletSeed::new(tenant, 1_000 * NANOS_PER_USD)
        .insert(&mut *connection)
        .await?;
    Ok(())
}

fn registration() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: format!("draft-{}.test", mint_id()).into(),
        labels: Vec::new(),
        assigned_policy: AssignedPolicy {
            durable_tasks_enabled: false,
            decision_model_id: None,
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
    }
}

/// A frame naming a draft's run and a fleet run nobody holds renews the
/// draft's on the draft path and refuses the other on the fleet batch, each
/// answered in the place the frame named it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_frame_renews_its_draft_run_on_the_draft_path() -> TestResult {
    let mut seated = Seated::create().await?;
    let run = seated.take().await?;
    let unknown = mint_id();
    let renewed = seated
        .ask(&json!({"type":"renew_batch","request_id":4,"runs":[
            {"run_id":unknown,"request":{"requests":[]}},
            {"run_id":run,"request":{"requests":[]}}
        ]}))
        .await?;
    assert_eq!(renewed["type"], "renewed");
    assert_eq!(renewed["runs"][0]["run_id"], unknown.as_str());
    assert_eq!(
        renewed["runs"][0]["answer"]["error_code"],
        error_code::RUN_NOT_FOUND.as_str()
    );
    assert_eq!(renewed["runs"][1]["run_id"], run.as_str());
    assert_eq!(renewed["runs"][1]["answer"]["status"], "ok", "{renewed}");
    let granted = renewed["runs"][1]["answer"]["response"]["run_expires_at"]
        .as_str()
        .ok_or("the draft's grant names its deadline")?
        .parse::<jiff::Timestamp>()?;
    assert!(granted > clock::now(), "the grant is ahead: {granted}");
    assert!(seated.holds_draft(&run), "a renewed draft run stays held");
    seated.close().await
}

/// A draft run whose stop committed is refused cancelled on renewal, not
/// retryably, and the connection stops holding it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cancelled_draft_run_is_refused_and_forgotten() -> TestResult {
    let mut seated = Seated::create().await?;
    let run = seated.take().await?;
    sqlx::query("UPDATE thread.runs SET cancel_requested_at = now() WHERE id = $1")
        .bind(&run)
        .execute(&mut *seated.owner.acquire().await?)
        .await?;
    let capture = Capture::install();
    let renewed = seated
        .ask(&json!({"type":"renew_batch","request_id":5,"runs":[
            {"run_id":run,"request":{"requests":[]}}
        ]}))
        .await?;
    assert_eq!(renewed["runs"][0]["run_id"], run.as_str());
    assert_eq!(renewed["runs"][0]["answer"]["status"], "refused");
    assert_eq!(
        renewed["runs"][0]["answer"]["error_code"],
        error_code::RUN_CANCELLED.as_str()
    );
    assert_eq!(renewed["runs"][0]["answer"]["retryable"], false);
    assert!(
        !seated.holds_draft(&run),
        "a refused draft run is forgotten"
    );
    let observed = capture
        .events()
        .iter()
        .filter(|event| {
            event.field("event") == Some("runner_stream_operation_failed")
                && event.field("operation") == Some("renewal")
                && event.field("error_code") == Some(error_code::RUN_CANCELLED.as_str())
        })
        .count();
    assert_eq!(observed, 1, "the refusal is observed once");
    seated.close().await
}

/// A draft run's live answer whose publication cannot read its authority is
/// observed and dropped: the activity frame itself is still accepted.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_preview_the_datastore_cannot_take_is_observed_not_returned() -> TestResult {
    let mut seated = Seated::create().await?;
    let run = seated.take().await?;
    seated.database.close().await;
    let capture = Capture::install();
    let frame = json!({"type":"progress","run_id":run,"request":{"frames":[
        {"fleet_response_chunk":{"stream_seq":0,"stream_start":true,
         "text_kind":"answer","text":"A daily check at nine."}}]}});
    let text = frame.to_string();
    let message: Up<'_> = serde_json::from_str(&text)?;
    seated.connection.send(&message).await?;
    let observed = capture
        .events()
        .iter()
        .filter(|event| {
            event.field("event") == Some("runner_stream_operation_failed")
                && event.field("operation") == Some("progress")
        })
        .count();
    assert_eq!(observed, 1, "the failed preview is observed once");
    seated.close().await
}

/// A committed draft assignment this connection cannot hold, its run table
/// full, never reaches the runner: the run is released back to its queue, the
/// release is logged, and the next pass assigns it again under a new fence.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_assignment_the_connection_cannot_hold_releases_its_run() -> TestResult {
    let mut seated = Seated::create().await?;
    let cache = &seated.connection.services().cache;
    let held: Vec<Uuid7> = (0..af_wire::protocol::RUNS_MAX)
        .map(|_| mint_id())
        .collect();
    for run in &held {
        cache.insert(
            afd_fleet::run::AssignedRun {
                run_id: run.clone(),
                expires_at: jiff::Timestamp::MAX,
                target: None,
                reported: None,
                draft: None,
            },
            clock::now(),
        )?;
    }
    let capture = Capture::install();
    let refused = seated.connection.assign().await.err();
    assert!(
        refused
            .as_ref()
            .is_some_and(|error| matches!(error.kind(), crate::error::ErrorKind::Protocol)),
        "{refused:?}"
    );
    let released: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("run_frame_undelivered_released"))
        .collect();
    assert_eq!(released.len(), 1, "{released:?}");
    assert_eq!(released[0].field("released"), Some("true"));
    assert_eq!(released[0].level, tracing::Level::WARN);
    let fenced = released[0]
        .field("fencing_token")
        .ok_or("the release names its fence")?
        .parse::<u64>()?;
    for run in &held {
        seated.connection.services().cache.forget(run);
    }
    let again = seated
        .connection
        .assign()
        .await?
        .ok_or("the released turn is assigned again")?;
    let again: Value = again.decode()?;
    assert_eq!(again["run"]["owner"]["kind"], "draft");
    assert!(
        again["run"]["fencing_token"].as_u64() > Some(fenced),
        "the second assignment is fenced past the first: {again}"
    );
    seated.close().await
}

/// A recovery pass that finds nothing left to recover clears the recovery
/// it was asked for, so ordinary passes stop deferring to it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_recovery_pass_with_nothing_to_recover_clears_its_recovery() -> TestResult {
    let mut seated = Seated::create().await?;
    seated.take().await?;
    seated.connection.ready(1).await?;
    let wake = &seated.gateway.inner.dependencies.wake;
    wake.recheck();
    let recovery = wake.recovery().ok_or("a recheck asks for recovery")?;
    let services = std::sync::Arc::clone(seated.connection.services());
    services.handle.snapshot.lock().recovery_pending = Some(recovery);
    let outcome = services
        .assign(crate::connection::AssignMode::Recovery)
        .await?;
    assert_eq!(outcome, crate::connection::AssignOutcome::Empty);
    assert_eq!(
        services.handle.snapshot.lock().recovery_pending,
        None,
        "the pass cleared the recovery it ran for"
    );
    seated.close().await
}

/// A draft turn whose assignment no frame can carry is never sent: its turn
/// fails as too large for a frame through the draft's own settlement, the
/// frame code is logged, and the pass assigns nothing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_turn_past_one_frame_fails_instead_of_being_sent() -> TestResult {
    let mut seated = Seated::create().await?;
    // Seeded past the route's bound, as only a stored row can be: one frame's worth of text.
    let past = af_wire::protocol::MESSAGE_BYTES_MAX + 1;
    let turn: Uuid7 = sqlx::query_scalar(
        "UPDATE thread.turns SET request_json = jsonb_build_object('message', repeat('x', $1)) \
         WHERE producer IS NULL RETURNING id",
    )
    .bind(i32::try_from(past)?)
    .fetch_one(&mut *seated.owner.acquire().await?)
    .await?;
    let capture = Capture::install();
    let assigned = seated.connection.assign().await?;
    assert!(assigned.is_none(), "an unframed run is never sent");
    let unframed: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("run_frame_too_large"))
        .collect();
    assert_eq!(unframed.len(), 1, "{unframed:?}");
    assert_eq!(unframed[0].field("turn_id"), Some(turn.as_str()));
    assert_eq!(
        unframed[0].field("error_code"),
        Some(error_code::RUN_FRAME_TOO_LARGE.as_str())
    );
    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, failure_reason FROM thread.turns WHERE id = $1")
            .bind(&turn)
            .fetch_one(&mut *seated.owner.acquire().await?)
            .await?;
    assert_eq!(status, "failed");
    assert_eq!(reason.as_deref(), Some("frame_too_large"));
    seated.close().await
}
