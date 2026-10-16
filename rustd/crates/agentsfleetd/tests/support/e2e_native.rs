//! The native setup a seeded fleet row selects, as production Save writes it.
//!
//! A lease reads a fleet's policy only from its current immutable definition
//! revision, so a row without one is a fleet no runner can ever run. Every
//! scenario that seeds a fleet row directly selects one here, through the same
//! parser and digest the setup authority uses.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{POLICY_SEMANTICS, canonical_digest};
use serde_json::{Value, json};

/// The setup revision a fleet's first selection carries.
const FIRST_REVISION: i64 = 1;

/// What a seeded fleet's setup says about itself.
const FIXTURE_DESCRIPTION: &str = "End-to-end fixture.";

/// A complete definition for `config`, whose own `name` names the fleet.
pub(crate) fn definition(config: &str, instructions: &str) -> Definition {
    let config: Value = serde_json::from_str(config).expect("fixture config is JSON");
    Definition::parse(
        &json!({
            "schema_version": 1,
            "description": FIXTURE_DESCRIPTION,
            "instructions_md": instructions,
            "config": config,
        })
        .to_string(),
    )
    .expect("a valid native setup")
}

/// Appends `definition` as `fleet`'s first revision on the platform default
/// model, and selects it.
pub(crate) async fn select(
    connection: &mut sqlx::PgConnection,
    fleet: &str,
    definition: &Definition,
    at: i64,
) {
    select_revision(connection, fleet, definition, FIRST_REVISION, at).await;
}

/// Appends `definition` as `fleet`'s next revision and selects it, the way an
/// Apply changes a saved fleet's setup.
pub(crate) async fn reselect(
    connection: &mut sqlx::PgConnection,
    fleet: &str,
    definition: &Definition,
    at: i64,
) {
    let next: i64 = sqlx::query_scalar(
        "SELECT coalesce(setup_version, 0) + 1 FROM core.fleets WHERE id = $1::uuid",
    )
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("the fleet's setup version reads");
    select_revision(connection, fleet, definition, next, at).await;
}

/// Appends `definition` as `fleet`'s revision `number` and selects it.
async fn select_revision(
    connection: &mut sqlx::PgConnection,
    fleet: &str,
    definition: &Definition,
    number: i64,
    at: i64,
) {
    // Idempotent like the row seed it follows: a fleet seeded twice keeps the
    // selection it has rather than failing on the revision number.
    let inserted: Option<String> = sqlx::query_scalar(
        "INSERT INTO core.fleet_definition_revisions
           (id, fleet_id, revision, definition, canonical_digest, policy_semantics, created_at,
            runtime_model_ref)
         VALUES ($1::uuid, $2::uuid, $3, $4::jsonb, $5, $6, $7, $8)
         ON CONFLICT (fleet_id, revision) DO NOTHING
         RETURNING id::text",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(fleet)
    .bind(number)
    .bind(serde_json::to_string(definition).expect("Definition JSON"))
    .bind(canonical_digest(definition).expect("canonical digest"))
    .bind(POLICY_SEMANTICS)
    .bind(at)
    .bind(afd_credential::provider::PLATFORM_MODEL_REF)
    .fetch_optional(&mut *connection)
    .await
    .expect("append the immutable setup");
    let Some(revision) = inserted else {
        return;
    };
    sqlx::query(
        "UPDATE core.fleets SET current_definition_revision_id = $2::uuid,
           setup_version = $3, resource_version = $3, config_json = $4::jsonb
         WHERE id = $1::uuid",
    )
    .bind(fleet)
    .bind(&revision)
    .bind(number)
    .bind(definition.config().value().to_string())
    .execute(connection)
    .await
    .expect("select the setup");
}
