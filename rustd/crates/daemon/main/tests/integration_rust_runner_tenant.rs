//! A tenant's own runner, end to end: the Rust runner, holding a token its
//! tenant registered, takes a chat on the tenant's own model and settles it.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::envelope::Sealer;
use af_crypto::secret::Kek;
use af_wire::runner::NetworkPolicy;
use afd_db::test_util::mint_id;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;
use std::time::Duration;

use super::{assert_settled, rust_runner, scripted};
use crate::e2e::{GOOD_KEK, MODEL, PROVIDER, Scenario, scenario_with_provider};
use crate::e2e_seed::{FIXTURE_INSTRUCTIONS, FLEET_CONFIG_JSON, scenario_enrolment};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::{mint_tenant_token, provider_granting};

/// Where the tenant's registered model finds its credential.
const TENANT_SECRET: &str = "tenant-runner-model";

const TENANT_MODEL_ENTRY: &str = "INSERT INTO model.tenant_models \
    (id, tenant_id, model_name, secret_ref, workspace_id, created_at, updated_at) \
    VALUES ($1, $2, $3, $4, $5, now(), now())";

const TENANT_SECRET_ROW: &str = "INSERT INTO vault.secrets \
    (id, workspace_id, key_name, dek_ciphertext, dek_nonce, dek_tag, secret_nonce, \
     secret_ciphertext, secret_tag, kek_version, meta_provider, meta_has_key, created_at, updated_at) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, true, now(), now())";

const LEASE_OF_EVENT: &str = "SELECT runner_id, posture FROM runner.leases WHERE event_id = $1";

/// Registers the tenant's own model with a sealed key and moves the scenario's
/// fleet onto it.
async fn seat_tenant_model(run: &Scenario) {
    let entry = mint_id();
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let body = format!(r#"{{"provider":"{PROVIDER}","api_key":"sk-tenant-fixture-key"}}"#);
    let sealed = Sealer::new()
        .seal(
            &kek,
            &Aad::new(run.workspace.as_str(), TENANT_SECRET),
            body.as_bytes(),
        )
        .expect("the tenant key seals");
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    sqlx::query(TENANT_SECRET_ROW)
        .bind(mint_id())
        .bind(&run.workspace)
        .bind(TENANT_SECRET)
        .bind(sealed.wrapped_dek())
        .bind(sealed.dek_nonce().as_slice())
        .bind(sealed.dek_tag().as_slice())
        .bind(sealed.payload_nonce().as_slice())
        .bind(sealed.payload_ciphertext())
        .bind(sealed.payload_tag().as_slice())
        .bind(sealed.kek_version())
        .bind(PROVIDER)
        .execute(&mut *connection)
        .await
        .expect("the tenant key is stored");
    sqlx::query(TENANT_MODEL_ENTRY)
        .bind(&entry)
        .bind(&run.tenant)
        .bind(MODEL)
        .bind(TENANT_SECRET)
        .bind(&run.workspace)
        .execute(&mut *connection)
        .await
        .expect("the tenant's model is registered");
    let definition = crate::e2e_native::definition(FLEET_CONFIG_JSON, FIXTURE_INSTRUCTIONS);
    crate::e2e_native::reselect_on(&mut connection, &run.fleet, &definition, entry.as_str()).await;
}

/// A runner the scenario's tenant registers for its fleet through
/// `POST /v1/tenants/me/runners`, with no egress control the scripted host
/// would have to enforce. Answers its id and token.
async fn tenant_runner(run: &Scenario, credential: &str) -> (Uuid7, String) {
    let mut request = scenario_enrolment(&run.fleet);
    request.host_id = Cow::Borrowed("tenant-host-e2e.fixture.test");
    request.assigned_policy.network_policy = NetworkPolicy::AllowAll;
    request.assigned_policy.registry_allowlist = Vec::new();
    let response = afd_identity::direct(Duration::from_secs(5))
        .build()
        .expect("an HTTP client")
        .post(format!("{}/v1/tenants/me/runners", run.base))
        .bearer_auth(credential)
        .header("Idempotency-Key", mint_id().as_str())
        .json(&request)
        .send()
        .await
        .expect("the registration answers");
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    let body: Value = response.json().await.expect("a JSON registration");
    let field = |name: &str| {
        body.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .expect("the registration names the runner and its token")
    };
    let runner = Uuid7::parse(&field("runner_id")).expect("a runner id");
    (runner, field("runner_token"))
}

// Value: protects=a tenant's own runner completes a chat on the tenant's model,
// billed self-managed; fails_when=any of register, connect, claim, delivery or
// report refuses a tenant runner; why_new=tenant runners; seam=the Rust runner
// against the shipping daemon.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn tenant_runner_completes_a_tenant_model_chat() {
    let mut supervisor = Supervisor::new();
    let provider = provider_granting("runner:tenant").await;
    let run = scenario_with_provider(&mut supervisor, Some(&provider)).await;
    let credential = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &credential, run.seeded_at).await;
    seat_tenant_model(&run).await;
    let (runner_id, token) = tenant_runner(&run, &credential).await;
    let home = tempfile::tempdir().expect("a storage home");
    let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
    let shutdown = tokio_util::sync::CancellationToken::new();
    let engine = afr_sandbox::UnsandboxedEngine::new(sandboxes.path().to_owned())
        .expect("a debug build permits the unsandboxed engine");
    let runner = tokio::spawn(rust_runner(
        &run,
        &token,
        home.path(),
        (Box::new(engine), Box::new(scripted())),
        shutdown.clone(),
    ));

    assert_settled(&run).await;
    let (leased_by, posture): (Uuid7, String) = sqlx::query_as(LEASE_OF_EVENT)
        .bind(&run.event_id)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("a connection"))
        .await
        .expect("the chat's lease");
    assert_eq!(leased_by, runner_id, "the tenant's runner ran the chat");
    assert_eq!(posture, afd_billing::sql::posture::SELF_MANAGED);

    shutdown.cancel();
    runner
        .await
        .expect("the runner task joins")
        .expect("the runner stops cleanly on shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
}
