//! Which provider key a delivered run carries: the platform's only on a
//! platform runner, and a tenant's own key on its tenant's runner.

use af_crypto::aad::Aad;
use af_crypto::envelope::Sealer;
use af_crypto::secret::Kek;
use af_fleet_runtime::definition::{POLICY_SEMANTICS, canonical_digest};
use af_wire::runner::{NetworkPolicy, SandboxTier};
use afd_db::test_util::mint_id;
use afd_runner::Registered;

use super::seed::seed_provider_resolution;
use super::*;
use crate::report_seed::FIXTURE_KEK_HEX;
use crate::requests::{enrolment_tagged, placement_tag};
use crate::seed::{MODEL, PROVIDER, reports_capable};

/// The platform default's key, as `seed_provider_resolution` seals it.
const PLATFORM_KEY: &str = "sk-fixture-not-a-credential";

/// The tenant's own key, sealed for its registered model.
const TENANT_KEY: &str = "sk-tenant-fixture-key";

/// Where the tenant's registered model finds its credential.
const TENANT_SECRET: &str = "tenant-runner-model";

/// A native setup the fleet is moved onto, naming the tenant's model.
const CONFIG: &str = r#"{"name":"tenant-model","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

const TENANT_MODEL_ENTRY: &str = "INSERT INTO model.tenant_models \
    (id, tenant_id, model_name, secret_ref, workspace_id, created_at, updated_at) \
    VALUES ($1, $2, $3, $4, $5, now(), now())";

const TENANT_SECRET_ROW: &str = "INSERT INTO vault.secrets \
    (id, workspace_id, key_name, dek_ciphertext, dek_nonce, dek_tag, secret_nonce, \
     secret_ciphertext, secret_tag, kek_version, meta_provider, meta_has_key, created_at, updated_at) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, true, now(), now())";

/// A runner `tenant` registered, tagged for `fleet`, that has reported capable.
async fn tenant_runner(fixtures: &Fixtures, tenant: &Uuid7, fleet: &Uuid7) -> Uuid7 {
    let request = enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        2,
        &placement_tag(fleet),
    );
    let registered = fixtures
        .runners()
        .register_tenant(
            tenant,
            &mint_id(),
            &request,
            "fixture:tenant-owner",
            fixtures.enrolled_at(),
        )
        .await
        .expect("the tenant registers its runner");
    assert!(
        matches!(registered, Registered::Created(_)),
        "a fresh key registers"
    );
    let runner = match registered {
        Registered::Created(enrolled) => enrolled.runner_id,
        Registered::Replayed { runner_id, .. } => runner_id,
    };
    reports_capable(fixtures, &runner).await;
    runner
}

/// Registers `tenant`'s own model with a sealed key, and moves `fleet`'s head
/// revision onto it.
async fn seat_tenant_model(fixtures: &Fixtures, tenant: &Uuid7, fleet: &Uuid7) {
    let workspace = seed::workspace_of(fixtures, fleet).await;
    let entry = mint_id();
    let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the lane key is well formed");
    let body = format!(r#"{{"provider":"{PROVIDER}","api_key":"{TENANT_KEY}"}}"#);
    let sealed = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace.as_str(), TENANT_SECRET),
            body.as_bytes(),
        )
        .expect("the tenant key seals");
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query(TENANT_SECRET_ROW)
        .bind(mint_id())
        .bind(&workspace)
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
        .bind(tenant)
        .bind(MODEL)
        .bind(TENANT_SECRET)
        .bind(&workspace)
        .execute(&mut *connection)
        .await
        .expect("the tenant's model is registered");
    let definition = crate::setup::definition(CONFIG);
    let stored = serde_json::to_string(&definition).expect("definition JSON");
    let digest = canonical_digest(&definition).expect("canonical digest");
    fixtures
        .append_head(
            fleet,
            &stored,
            &digest,
            POLICY_SEMANTICS,
            Some(entry.as_str()),
        )
        .await;
}

/// The delivered policy's provider key.
fn api_key(payload: &serde_json::Value) -> Option<&str> {
    payload
        .pointer("/policy/api_key")
        .and_then(serde_json::Value::as_str)
}

const SLOTS_OF_RUNNER: &str = "SELECT count(*) FROM thread.threads WHERE last_runner_id = $1";

async fn runner_runs(fixtures: &Fixtures, runner: &Uuid7) -> Vec<String> {
    sqlx::query_scalar("SELECT posture FROM thread.runs WHERE runner_id = $1")
        .bind(runner)
        .fetch_all(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the runs read")
}

// Value: protects=the platform default key leaves the daemon only for a
// platform runner; fails_when=the model filter lets a tenant runner claim, or
// the delivery builds no key; why_new=the model filter; seam=none.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn platform_key_never_reaches_a_tenant_runner() {
    let fixtures = Fixtures::create_isolated().await;
    let ready = ready(&fixtures).await;
    let _default = seed_provider_resolution(&fixtures, &ready.fleet).await;
    let tenant = tenant_runner(&fixtures, &ready.tenant, &ready.fleet).await;

    for poll in 1..=2 {
        let picked = crate::seed::select_fleet(&fixtures.runs(), &tenant, &ready.fleet).await;
        assert!(
            picked.is_none(),
            "poll {poll}: no candidate for the tenant runner"
        );
    }
    assert!(
        runner_runs(&fixtures, &tenant).await.is_empty(),
        "and no run"
    );
    let slots: i64 = sqlx::query_scalar(SLOTS_OF_RUNNER)
        .bind(&tenant)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
        .await
        .expect("the slots read");
    assert_eq!(slots, 0, "and no slot write");

    let claimed = claim(&fixtures, &ready).await;
    let payload = drive(&fixtures, &ready, claimed)
        .await
        .expect("a platform runner runs the fleet");
    assert_eq!(
        api_key(&payload),
        Some(PLATFORM_KEY),
        "it carries the platform key"
    );
    fixtures.cleanup().await;
}

// Value: protects=a tenant runner runs its tenant's model on its tenant's key,
// billed self-managed; fails_when=the claim refuses tenant-model work or the
// delivery resolves the platform key; why_new=tenant runners; seam=none.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn tenant_runner_carries_the_tenant_key() {
    let fixtures = Fixtures::create_isolated().await;
    let ready = ready(&fixtures).await;
    let _default = seed_provider_resolution(&fixtures, &ready.fleet).await;
    seat_tenant_model(&fixtures, &ready.tenant, &ready.fleet).await;
    fixtures
        .seed_wallet(&ready.tenant, 1_000 * afd_billing::nanos::NANOS_PER_USD)
        .await;
    let tenant = tenant_runner(&fixtures, &ready.tenant, &ready.fleet).await;

    let claimed = crate::seed::select_fleet(&fixtures.runs(), &tenant, &ready.fleet)
        .await
        .expect("the tenant runner takes its tenant's model work");
    let payload = fixtures
        .plane()
        .claim_selected(claimed, &tenant, fixtures.enrolled_at())
        .await
        .expect("every gate verdict is a decision, not a fault")
        .expect("the run is delivered");
    assert_eq!(
        api_key(&payload),
        Some(TENANT_KEY),
        "it carries the tenant's own key"
    );
    assert_eq!(
        runner_runs(&fixtures, &tenant).await,
        vec![afd_billing::sql::posture::SELF_MANAGED.to_owned()],
        "and is billed self-managed"
    );
    fixtures.cleanup().await;
}
