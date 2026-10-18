//! An isolated platform default exercises actual admission pricing and saved routing.

use super::{Fixture, claim, fixture, mint, now};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use std::sync::Arc;

async fn configured() -> Fixture {
    let f = fixture::fund(Fixture::isolated().await, 1).await;
    let raw = serde_json::value::RawValue::from_string(
        r#"{"provider":"openai","api_key":"platform-fixture-key"}"#.into(),
    )
    .expect("fixture credential JSON");
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key");
    Vault::new(f.db.clone(), Arc::new(key), Entropy::new())
        .create(
            &f.workspace,
            &SecretName::parse("openai").expect("provider name"),
            &SecretBody::parse(&raw).expect("credential shape"),
            now(),
        )
        .await
        .expect("sealed platform credential");
    sqlx::query("INSERT INTO model.platform_defaults(provider,source_workspace_id,active,platform_model_id,created_at,updated_at) VALUES('openai',$1::uuid,true,$2::uuid,now(),now())")
        .bind(f.workspace.as_str()).bind(f.catalogue.first().expect("owned catalogue model").as_str())
        .execute(&mut *f.connection().await).await.expect("isolated platform default");
    f
}

async fn queued(f: &Fixture) -> afd_authoring::types::Draft {
    let mut request = f.request(mint(), Some("Use the configured platform model"));
    request.authoring_model_ref = afd_credential::provider::PLATFORM_MODEL_REF.into();
    f.drafts
        .create(f.access(), request, now())
        .await
        .expect("platform choice queued")
        .draft
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn saved_platform_reference_resolves_the_platform_credential_and_billing_posture() {
    let f = configured().await;
    let draft = queued(&f).await;
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key");
    let providers =
        afd_credential::provider::Providers::new(f.db.clone(), Arc::new(key), Entropy::new());
    let resolved = providers
        .resolve_reference(&f.tenant, &draft.authoring_model_ref)
        .await
        .expect("platform model and credential resolve together");
    assert_eq!(resolved.posture, afd_billing::Posture::Platform);
    assert_eq!(resolved.provider.as_ref(), "openai");
    assert_eq!(resolved.api_key().expose(), "platform-fixture-key");
    assert_eq!(
        resolved
            .inference
            .expect("persisted route")
            .max_output_tokens,
        8192
    );
    f.cleanup().await;
}

// Value: protects=platform choices reserve actual provider costs and saved output settings;
// fails_when=admission treats platform inference as bring-your-own credentials or hardcodes a cap;
// why_new=the existing admitted leases use only workspace-owned models;
// seam=isolated real Postgres and encrypted vault; no_claim=live inference.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn platform_admission_reserves_priced_input_output_and_runtime() {
    let f = configured().await;
    let draft = queued(&f).await;
    let lease = claim(&f, &draft)
        .await
        .expect("configured platform model admitted");
    assert_eq!(lease.model.reference, "platform-default");
    assert_eq!(lease.limits.output_per_turn, 8192);
    let (reserved, managed): (i64, bool) = sqlx::query_as(
        "SELECT reserved_nanos,(model_snapshot->>'platform_managed')::boolean FROM authoring.attempts WHERE id=$1::uuid",
    ).bind(&lease.attempt_id).fetch_one(&mut *f.connection().await).await.expect("durable priced bound");
    // Eight 32,768-token inputs at $1/million, 8,192 output at $2/million,
    // seven rounding units, plus the separately priced five-minute runtime.
    assert_eq!(reserved, 30_000_000 + 8 * 32_768_000 + 16_384_000 + 7);
    assert!(managed);
    f.cleanup().await;
}

// Value: protects=unsafe stored prices cannot overflow or create an uncovered obligation;
// fails_when=negative/overflowing provider costs create an attempt instead of owner billing recovery;
// why_new=workspace admission never prices provider tokens;
// seam=isolated real Postgres; no_claim=production catalogue corruption.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn invalid_platform_prices_fail_without_creating_an_attempt() {
    let f = configured().await;
    let draft = queued(&f).await;
    let model = f.catalogue.first().expect("owned catalogue model").as_str();
    let negative = sqlx::query(SET_INPUT_PRICE)
        .bind(model)
        .bind(-1_i64)
        .execute(&mut *f.connection().await)
        .await;
    assert!(
        negative.is_err(),
        "the price table refuses a negative rate, so no claim can read one"
    );
    sqlx::query(SET_INPUT_PRICE)
        .bind(model)
        .bind(i64::MAX)
        .execute(&mut *f.connection().await)
        .await
        .expect("owned unaffordable price");
    assert!(claim(&f, &draft).await.is_none());
    let failed = fixture::failure(&f, &draft).await;
    assert_eq!(
        failed.failure.expect("bounded billing refusal").error_code,
        afd_core::error_code::AUTHORING_BALANCE
    );
    assert_eq!(fixture::attempts(&f).await, 0);
    f.cleanup().await;
}

/// Rewrites every price tier of one catalogue model's input rate.
const SET_INPUT_PRICE: &str = "UPDATE model.platform_model_prices SET input_nanos_per_mtok = $2 \
     WHERE platform_model_id = $1::uuid";
