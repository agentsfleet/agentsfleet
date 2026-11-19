//! Private model and setup identities isolate the drill from the shared platform default.

use super::Checked;
use crate::e2e::Scenario;
use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_db::test_util::{ModelSeed, mint_id};
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::json;
use std::sync::Arc;

pub(super) async fn model(
    scene: &Scenario,
    base: &str,
    workspace: &Uuid7,
    tenant: &Uuid7,
) -> Checked<Uuid7> {
    let entry = mint_id();
    let model = mint_id();
    let name = format!("recovery-check-{model}");
    let settings = json!({"api":"chat","base_url":base,"max_output_tokens":1024}).to_string();
    let mut connection = scene.booted.database.acquire().await?;
    ModelSeed::new(&model, "openai-compatible", &name, 32_000)
        .inference(&settings)
        .rates(1_000_000, 0, 1_000_000)
        .insert(&mut *connection)
        .await?;
    drop(connection);
    let vault = Vault::new(
        scene.booted.database.clone(),
        Arc::new(Kek::from_hex(crate::e2e::GOOD_KEK)?),
        Entropy::new(),
    );
    let key = json!({"provider":"openai-compatible","base_url":base,"api_key":mint_id().as_str()});
    let key = serde_json::value::RawValue::from_string(key.to_string())?;
    vault
        .create(
            workspace,
            &SecretName::parse("recovery-check")?,
            &SecretBody::parse(&key)?,
            af_core::clock::now(),
        )
        .await?;
    let mut connection = scene.booted.database.acquire().await?;
    sqlx::query("INSERT INTO model.tenant_models (id,tenant_id,workspace_id,model_name,secret_ref,created_at,updated_at)
        VALUES ($1,$2,$3,$4,$5,now(),now())")
        .bind(&entry).bind(tenant).bind(workspace).bind(&name).bind("recovery-check")
        .execute(&mut *connection).await?;
    Ok(entry)
}

pub(super) async fn select(
    connection: &mut sqlx::PgConnection,
    fleet: &Uuid7,
    entry: &Uuid7,
) -> Checked {
    let definition = crate::e2e_native::definition(
        crate::e2e_seed::FLEET_CONFIG_JSON,
        "Run the recovery check.",
    );
    let stored = crate::e2e_native::Stored::of(&definition);
    sqlx::query("WITH next AS (SELECT coalesce(max(revision),0)+1 AS revision FROM fleet.revisions WHERE fleet_id=$1),
        appended AS (INSERT INTO fleet.revisions (id,fleet_id,revision,definition,canonical_digest,policy_semantics,runtime_model_ref,created_at)
        SELECT $2,$1,next.revision,$3::jsonb,$4,$5,$6,now() FROM next RETURNING id)
        UPDATE fleet.fleets SET head_revision_id=(SELECT id FROM appended),resource_version=resource_version+1,updated_at=now() WHERE id=$1")
        .bind(fleet).bind(mint_id()).bind(&stored.definition).bind(&stored.digest)
        .bind(af_fleet_runtime::definition::POLICY_SEMANTICS).bind(entry.as_str()).execute(connection).await?;
    Ok(())
}

pub(super) async fn admit(
    scene: &Scenario,
    fleet: &Uuid7,
    workspace: &Uuid7,
    message: &str,
) -> Checked<Uuid7> {
    let body = json!({"message":message}).to_string();
    let actor = format!("{}recovery-check", afd_turns::view::ACTOR_PREFIX);
    let accepted =
        afd_turns::Admissions::for_tests(scene.booted.database.clone(), scene.booted.bus.clone())
            .admit(afd_turns::Admission {
                producer: afd_turns::Producer::Steer,
                key: afd_turns::Key::Unrepeatable,
                fleet,
                workspace,
                actor: &actor,
                event_type: af_wire::event::EventType::Chat,
                request_json: &body,
                reply: afd_turns::Reply::None,
            })
            .await?;
    Ok(accepted.stored.id)
}
