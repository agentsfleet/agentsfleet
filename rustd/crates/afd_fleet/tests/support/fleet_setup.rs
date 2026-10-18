//! Runtime fixtures append the same native revisions that production Save selects.

#![expect(clippy::expect_used, reason = "fixture preconditions must fail loudly")]
#![allow(
    dead_code,
    reason = "the shared fixture is used by two integration binaries"
)]

use crate::support::Fixtures;
use afd_fleet_runtime::{
    Definition,
    definition::{POLICY_SEMANTICS, canonical_digest},
};
use serde_json::{Value, json};

/// Appends the next revision and makes it the fleet's head, in one statement.
///
/// Revisions are append-only, so a fixture that changes a fleet's setup writes
/// a new one exactly as Save does.
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
UPDATE fleet.fleets SET head_revision_id = (SELECT id FROM appended), updated_at = now()
WHERE id = $1::uuid";

/// The fleet's head definition with its config replaced by `$2`, unparsed.
const CORRUPT_HEAD_DEFINITION: &str = "\
SELECT jsonb_set(r.definition, '{config}', $2::jsonb)::text
FROM fleet.fleets f JOIN fleet.revisions r ON r.id = f.head_revision_id
WHERE f.id = $1::uuid";

/// A native setup definition wrapping `config`, as Save would parse it.
pub(crate) fn definition(config: &str) -> Definition {
    let config: Value = serde_json::from_str(config).expect("fixture config JSON");
    Definition::parse(
        &json!({"schema_version":1, "description":"Runtime fixture.",
        "instructions_md":"Run the fixture.","config":config})
        .to_string(),
    )
    .expect("valid native setup")
}

impl Fixtures {
    /// Makes `config` the fleet's head revision without parsing it, as a
    /// stored row written by an older daemon could hold.
    pub(crate) async fn corrupt_native_config(&self, fleet: &str, config: &str) {
        let mut connection = self.database.acquire().await.expect("fault connection");
        let corrupt: String = sqlx::query_scalar(CORRUPT_HEAD_DEFINITION)
            .bind(fleet)
            .bind(config)
            .fetch_one(&mut *connection)
            .await
            .expect("the fleet has a head revision to corrupt");
        drop(connection);
        let model = Some(afd_credential::provider::PLATFORM_MODEL_REF);
        self.append_head(
            fleet,
            &corrupt,
            "corrupt-fixture-digest",
            POLICY_SEMANTICS,
            model,
        )
        .await;
    }

    pub(crate) async fn native_config(&self, fleet: &str, config: &str) {
        self.native_definition(fleet, &definition(config)).await;
    }

    pub(crate) async fn native_definition(&self, fleet: &str, definition: &Definition) {
        let stored = serde_json::to_string(definition).expect("Definition JSON");
        let digest = canonical_digest(definition).expect("canonical digest");
        let model = Some(afd_credential::provider::PLATFORM_MODEL_REF);
        self.append_head(fleet, &stored, &digest, POLICY_SEMANTICS, model)
            .await;
    }

    /// Appends `definition` as the fleet's head revision, stored exactly as
    /// given: a test of a revision Save would refuse writes it here.
    pub(crate) async fn append_head(
        &self,
        fleet: &str,
        definition: &str,
        digest: &str,
        semantics: &str,
        model_ref: Option<&str>,
    ) {
        let mut connection = self.database.acquire().await.expect("setup connection");
        sqlx::query(APPEND_HEAD_REVISION)
            .bind(fleet)
            .bind(afd_db::test_util::mint_id())
            .bind(definition)
            .bind(digest)
            .bind(semantics)
            .bind(model_ref)
            .execute(&mut *connection)
            .await
            .expect("append the fleet's head revision");
    }
}
