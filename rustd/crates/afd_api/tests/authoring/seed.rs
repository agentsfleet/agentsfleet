//! Real credential directory, encrypted provider key and tenant registry entry.

use super::{fixture::Fixture, mint};
use afd_api::Services as _;
use afd_auth::Scope;
use afd_core::clock;
use afd_core::money::NANOS_PER_USD;
use afd_vault::{SecretBody, SecretName};

/// The fixture model's prices: a dollar per million input tokens, a tenth of
/// that cached, two dollars per million output tokens.
pub(super) const INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD;
/// See [`INPUT_NANOS_PER_MTOK`].
const CACHED_INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD / 10;
/// See [`INPUT_NANOS_PER_MTOK`].
pub(super) const OUTPUT_NANOS_PER_MTOK: i64 = 2 * NANOS_PER_USD;

const ACCOUNT: &str = "\
WITH tenant AS (
 INSERT INTO core.tenants(id,name,created_at,updated_at) VALUES ($1::uuid,'authoring-http',$6,$6)
), workspace AS (
 INSERT INTO core.workspaces(id,tenant_id,created_at) VALUES ($2::uuid,$1::uuid,$6)
), person AS (
 INSERT INTO core.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
 VALUES ($3::uuid,$1::uuid,$3,$3,$6,$6)
)
INSERT INTO core.api_keys(id,tenant_id,key_name,description,key_hash,created_by,active,created_at,updated_at)
VALUES ($4::uuid,$1::uuid,'fixture','',$5,$3,true,$6,$6)";

pub(super) async fn account(f: &Fixture, digest: &str) {
    sqlx::query(ACCOUNT)
        .bind(f.tenant.as_str())
        .bind(f.workspace.as_str())
        .bind(f.owner.as_str())
        .bind(mint().as_str())
        .bind(digest)
        .bind(clock::now().as_millis())
        .execute(&mut *f.db.acquire().await.expect("seed connection"))
        .await
        .expect("seed authenticated account");
}

pub(super) async fn model(f: &Fixture) {
    let mut connection = f.db.acquire().await.expect("model connection");
    let model = format!("workspace-model-{}", f.catalogue);
    sqlx::query("INSERT INTO core.model_library(id,model_id,provider,context_cap_tokens,input_nanos_per_mtok, \
        cached_input_nanos_per_mtok,output_nanos_per_mtok,created_at,updated_at,inference) \
        VALUES ($1::uuid,$2,'openai',128000,$4,$5,$6,$3,$3,$7)")
        .bind(f.catalogue.as_str()).bind(&model).bind(clock::now().as_millis())
        .bind(INPUT_NANOS_PER_MTOK).bind(CACHED_INPUT_NANOS_PER_MTOK).bind(OUTPUT_NANOS_PER_MTOK)
        .bind(sqlx::types::Json(afd_wire::inference::ModelInference {
            api: afd_wire::inference::InferenceApi::Responses,
            base_url: "https://models.example.com/v1".into(), dialect: None, max_output_tokens: 8192,
        }))
        .execute(&mut *connection).await.expect("seed isolated priced model");
    sqlx::query("INSERT INTO core.tenant_model_entries(id,tenant_id,model_id,secret_ref,created_at,updated_at) \
        VALUES ($1::uuid,$2::uuid,$3,'authoring-model',$4,$4)")
        .bind(f.model.as_str()).bind(f.tenant.as_str()).bind(&model).bind(clock::now().as_millis())
        .execute(&mut *connection).await.expect("register owner's explicit model");
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
