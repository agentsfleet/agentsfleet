//! Real identities, joined membership and encrypted provider metadata.

use super::fixture::{Fixture, now};
use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use std::sync::Arc;

/// The fixture model's prices: a dollar per million input tokens, a tenth of
/// that cached, two dollars per million output tokens.
const INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD;
/// See [`INPUT_NANOS_PER_MTOK`].
const CACHED_INPUT_NANOS_PER_MTOK: i64 = NANOS_PER_USD / 10;
/// See [`INPUT_NANOS_PER_MTOK`].
const OUTPUT_NANOS_PER_MTOK: i64 = 2 * NANOS_PER_USD;

const PEOPLE: &str = "\
WITH tenants AS (
 INSERT INTO core.tenants(id,name,created_at,updated_at)
 VALUES ($1::uuid,'authoring-owner',$6,$6),($2::uuid,'authoring-member',$6,$6)
), users AS (
 INSERT INTO core.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
 VALUES ($3::uuid,$1::uuid,$3,$3,$6,$6),($4::uuid,$2::uuid,$4,$4,$6,$6)
 RETURNING id
), workspace AS (
 INSERT INTO core.workspaces(id,tenant_id,created_at) VALUES ($5::uuid,$1::uuid,$6)
)
INSERT INTO core.memberships(id,tenant_id,user_id,role,created_at)
SELECT $7::uuid,$1::uuid,id,'member',$6 FROM users WHERE id=$4::uuid";

pub(super) async fn people(f: &Fixture) {
    sqlx::query(PEOPLE)
        .bind(f.tenant.as_str())
        .bind(f.member_tenant.as_str())
        .bind(f.owner.as_str())
        .bind(f.member.as_str())
        .bind(f.workspace.as_str())
        .bind(now().as_millis())
        .bind(super::mint().as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("seed isolated people and membership");
}

pub(super) async fn credential(f: &Fixture) {
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key");
    let raw = serde_json::value::RawValue::from_string(
        r#"{"provider":"openai","api_key":"fixture-authoring-key"}"#.to_owned(),
    )
    .expect("credential fixture JSON");
    let body = SecretBody::parse(&raw).expect("credential fixture shape");
    Vault::new(f.db.clone(), Arc::new(kek), Entropy::new())
        .create(
            &f.workspace,
            &SecretName::parse("authoring-model").expect("secret name"),
            &body,
            now(),
        )
        .await
        .expect("seal real fixture credential");
}

pub(super) async fn model(f: &Fixture, catalogue: &Uuid7, entry: &Uuid7) {
    let model = format!("gpt-4o-{catalogue}");
    sqlx::query("INSERT INTO core.model_library(id,model_id,provider,context_cap_tokens, \
        input_nanos_per_mtok,cached_input_nanos_per_mtok,output_nanos_per_mtok,created_at,updated_at) \
        VALUES($1::uuid,$2,'openai',128000,$4,$5,$6,$3,$3)")
        .bind(catalogue.as_str()).bind(&model).bind(now().as_millis())
        .bind(INPUT_NANOS_PER_MTOK).bind(CACHED_INPUT_NANOS_PER_MTOK).bind(OUTPUT_NANOS_PER_MTOK)
        .execute(&mut *f.connection().await).await.expect("seed isolated priced model");
    sqlx::query("INSERT INTO core.tenant_model_entries(id,tenant_id,model_id,secret_ref,created_at,updated_at) \
        VALUES($1::uuid,$2::uuid,$3,'authoring-model',$4,$4)")
        .bind(entry.as_str()).bind(f.tenant.as_str()).bind(&model).bind(now().as_millis())
        .execute(&mut *f.connection().await).await.expect("register workspace owner's model");
}
