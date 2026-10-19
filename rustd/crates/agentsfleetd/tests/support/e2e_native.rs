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

use afd_db::test_util::SetupRevision;
use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{POLICY_SEMANTICS, canonical_digest};
use serde_json::{Value, json};

/// Appends the next revision and makes it the fleet's head, in one statement,
/// the way an Apply changes a saved fleet's setup.
const APPEND_HEAD_REVISION: &str = "\
WITH next AS (
    SELECT coalesce(max(revision), 0) + 1 AS revision
    FROM fleet.revisions WHERE fleet_id = $1::uuid
), appended AS (
    INSERT INTO fleet.revisions
      (id, fleet_id, revision, definition, canonical_digest, policy_semantics,
       runtime_model_ref, created_at)
    SELECT $2::uuid, $1::uuid, next.revision, $3::jsonb, $4, $5, $6, now() FROM next
    RETURNING id
)
UPDATE fleet.fleets SET head_revision_id = (SELECT id FROM appended),
    resource_version = resource_version + 1, updated_at = now()
WHERE id = $1::uuid";

/// What a seeded fleet's setup says about itself.
const FIXTURE_DESCRIPTION: &str = "End-to-end fixture.";

/// A parsed definition as the columns that store it.
pub(crate) struct Stored {
    /// The definition, as stored JSON.
    pub(crate) definition: String,
    /// Its canonical digest.
    pub(crate) digest: String,
}

impl Stored {
    /// `definition`, serialized and digested by the setup authority's own code.
    pub(crate) fn of(definition: &Definition) -> Self {
        Self {
            definition: serde_json::to_string(definition).expect("Definition JSON"),
            digest: canonical_digest(definition).expect("canonical digest"),
        }
    }

    /// The revision a fleet seed selects, on the platform default model.
    pub(crate) fn revision(&self) -> SetupRevision<'_> {
        SetupRevision {
            definition: &self.definition,
            digest: &self.digest,
            semantics: POLICY_SEMANTICS,
            model_ref: Some(afd_credential::provider::PLATFORM_MODEL_REF),
        }
    }
}

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

/// Appends `definition` as `fleet`'s next revision and selects it, the way an
/// Apply changes a saved fleet's setup.
pub(crate) async fn reselect(
    connection: &mut sqlx::PgConnection,
    fleet: &str,
    definition: &Definition,
) {
    let stored = Stored::of(definition);
    sqlx::query(APPEND_HEAD_REVISION)
        .bind(fleet)
        .bind(afd_db::test_util::mint_id())
        .bind(&stored.definition)
        .bind(&stored.digest)
        .bind(POLICY_SEMANTICS)
        .bind(afd_credential::provider::PLATFORM_MODEL_REF)
        .execute(connection)
        .await
        .expect("append and select the setup");
}
