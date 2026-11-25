//! Real identities, joined membership, a sealed provider key and a model
//! the workspace may run.

use super::fixture::{Fixture, now};
use af_core::id::Uuid7;
use af_core::money::NANOS_PER_USD;
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_db::test_util::{ModelSeed, TenantSeed, UserSeed, WorkspaceSeed};
use afd_vault::{SecretBody, SecretName, Vault};
use std::sync::Arc;

/// The model routes through the Responses API, as an `openai` model does.
const RESPONSES_INFERENCE: &str =
    r#"{"api":"responses","base_url":"https://models.example.com/v1","max_output_tokens":8192}"#;

/// The vault entry the workspace's models read their key from.
const MODEL_SECRET: &str = "fleet-model";

/// The member joining the owner's account.
const MEMBERSHIP: &str = "\
INSERT INTO identity.memberships(id,tenant_id,user_id,role,created_at)
VALUES ($1,$2,$3,'member',now())";

/// The owner's tenant model entry for one catalogue model.
const TENANT_MODEL: &str = "\
INSERT INTO model.tenant_models(id,tenant_id,model_name,secret_ref,workspace_id,selected,created_at,updated_at)
VALUES ($1,$2,$3,$4,$5,false,now(),now())";

pub(super) async fn people(f: &Fixture) {
    let mut connection = f.connection().await;
    for (tenant, name, person) in [
        (&f.tenant, "fleet-owner", &f.owner),
        (&f.member_tenant, "fleet-member", &f.member),
    ] {
        TenantSeed::new(tenant)
            .name(name)
            .insert(&mut *connection)
            .await
            .expect("seed an isolated account");
        UserSeed::new(person, tenant, person.as_str(), person.as_str())
            .insert(&mut *connection)
            .await
            .expect("seed its person");
    }
    WorkspaceSeed::new(&f.workspace, &f.tenant)
        .insert(&mut *connection)
        .await
        .expect("seed the owner's workspace");
    sqlx::query(MEMBERSHIP)
        .bind(super::mint())
        .bind(&f.tenant)
        .bind(&f.member)
        .execute(&mut *connection)
        .await
        .expect("seed the membership");
}

pub(super) async fn credential(f: &Fixture) {
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture encryption key");
    let raw = serde_json::value::RawValue::from_string(
        r#"{"provider":"openai","api_key":"fixture-fleet-key"}"#.to_owned(),
    )
    .expect("credential fixture JSON");
    let body = SecretBody::parse(&raw).expect("credential fixture shape");
    Vault::new(f.db.clone(), Arc::new(kek), Entropy::new())
        .create(
            &f.workspace,
            &SecretName::parse(MODEL_SECRET).expect("secret name"),
            &body,
            now(),
        )
        .await
        .expect("seal the fixture credential");
}

/// A priced catalogue model, registered for the owner as `entry`.
pub(super) async fn model(f: &Fixture, catalogue: &Uuid7, entry: &Uuid7) {
    let model = format!("workspace-model-{catalogue}");
    ModelSeed::new(catalogue, "openai", &model, 128_000)
        .rates(NANOS_PER_USD, NANOS_PER_USD / 10, 2 * NANOS_PER_USD)
        .verified_at(now())
        .inference(RESPONSES_INFERENCE)
        .insert(&mut *f.connection().await)
        .await
        .expect("seed a priced model");
    sqlx::query(TENANT_MODEL)
        .bind(entry)
        .bind(&f.tenant)
        .bind(&model)
        .bind(MODEL_SECRET)
        .bind(&f.workspace)
        .execute(&mut *f.connection().await)
        .await
        .expect("register the owner's model");
}
