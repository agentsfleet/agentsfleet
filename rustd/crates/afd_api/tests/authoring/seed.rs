//! Real credential directory, encrypted provider key and tenant registry entry.

use super::{fixture::Fixture, mint};
use afd_api::Services as _;
use afd_auth::Scope;
use afd_core::clock;
use afd_core::money::NANOS_PER_USD;
use afd_tenant::test_util::{Signup, sign_up};
use afd_vault::{SecretBody, SecretName};

/// The fixture model's prices: a dollar per million input tokens, a tenth of
/// that cached, two dollars per million output tokens.
pub(super) const INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD;
/// See [`INPUT_NANOS_PER_MTOK`].
const CACHED_INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD / 10;
/// See [`INPUT_NANOS_PER_MTOK`].
pub(super) const OUTPUT_NANOS_PER_MTOK: i64 = 2 * NANOS_PER_USD;

/// The account's API key, the credential the router's requests present.
const API_KEY: &str = "\
INSERT INTO identity.api_keys(id,tenant_id,name,description,key_hash,created_by_subject,created_at,updated_at)
VALUES ($1::uuid,$2::uuid,'fixture','',$3,$4,now(),now())";

pub(super) async fn account(f: &Fixture, digest: &str) {
    sign_up(
        &f.db,
        &Signup {
            tenant: f.tenant.as_str(),
            user: f.owner.as_str(),
            subject: f.owner.as_str(),
            email: f.owner.as_str(),
            name: "authoring-http",
            display_name: None,
            workspace: f.workspace.as_str(),
        },
    )
    .await;
    sqlx::query(API_KEY)
        .bind(mint().as_str())
        .bind(f.tenant.as_str())
        .bind(digest)
        .bind(f.owner.as_str())
        .execute(&mut *f.db.acquire().await.expect("seed connection"))
        .await
        .expect("seed authenticated account");
}

pub(super) async fn model(f: &Fixture) {
    let mut connection = f.db.acquire().await.expect("model connection");
    let model = format!("gpt-4o-{}", f.catalogue);
    sqlx::query("INSERT INTO model.platform_models(id,model_name,provider,context_cap_tokens,input_nanos_per_mtok, \
        cached_input_nanos_per_mtok,output_nanos_per_mtok,created_at,updated_at) \
        VALUES ($1::uuid,$2,'openai',128000,$3,$4,$5,now(),now())")
        .bind(f.catalogue.as_str()).bind(&model)
        .bind(INPUT_NANOS_PER_MTOK).bind(CACHED_INPUT_NANOS_PER_MTOK).bind(OUTPUT_NANOS_PER_MTOK)
        .execute(&mut *connection).await.expect("seed isolated priced model");
    sqlx::query(
        "INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,created_at,updated_at) \
        VALUES ($1::uuid,$2::uuid,$3,'authoring-model',now(),now())",
    )
    .bind(f.model.as_str())
    .bind(f.tenant.as_str())
    .bind(&model)
    .execute(&mut *connection)
    .await
    .expect("register owner's explicit model");
    drop(connection);
    let raw = serde_json::value::RawValue::from_string(
        r#"{"provider":"openai","api_key":"fixture-authoring-key"}"#.to_owned(),
    )
    .expect("credential JSON");
    f.services(&Scope::ALL)
        .secrets()
        .create(
            &f.workspace,
            &SecretName::parse("authoring-model").expect("fixture name"),
            &SecretBody::parse(&raw).expect("fixture credential"),
            clock::now(),
        )
        .await
        .expect("seal real credential using the router's own vault");
}
