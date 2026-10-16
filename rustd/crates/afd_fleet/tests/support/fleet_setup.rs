//! Runtime fixtures append the same native revisions that production Save selects.

#![expect(clippy::expect_used, reason = "fixture preconditions must fail loudly")]
#![allow(
    dead_code,
    reason = "the shared fixture is used by two integration binaries"
)]

use crate::support::Fixtures;
use afd_core::clock;
use afd_fleet_runtime::{
    Definition,
    definition::{POLICY_SEMANTICS, canonical_digest},
};
use serde_json::{Value, json};
use sqlx::Connection as _;

impl Fixtures {
    pub(crate) async fn corrupt_native_config(&self, fleet: &str, config: &str) {
        self.native_config(fleet, r#"{"name":"probe","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#).await;
        sqlx::query("UPDATE core.fleet_definition_revisions SET definition=jsonb_set(definition,'{config}',$2::jsonb) WHERE id=(SELECT current_definition_revision_id FROM core.fleets WHERE id=$1::uuid)")
            .bind(fleet).bind(config).execute(&mut *self.database.acquire().await.expect("fault connection"))
            .await.expect("inject persisted native config corruption");
    }

    pub(crate) async fn native_config(&self, fleet: &str, config: &str) {
        let config: Value = serde_json::from_str(config).expect("fixture config JSON");
        let definition = Definition::parse(
            &json!({"schema_version":1, "description":"Runtime fixture.",
            "instructions_md":"Run the fixture.","config":config})
            .to_string(),
        )
        .expect("valid native setup");
        self.native_definition(fleet, &definition).await;
    }

    pub(crate) async fn native_definition(&self, fleet: &str, definition: &Definition) {
        let mut connection = self.database.acquire().await.expect("setup connection");
        let mut tx = connection.begin().await.expect("setup transaction");
        let number: i64 = sqlx::query_scalar(
            "SELECT coalesce(setup_version,0)+1 FROM core.fleets WHERE id=$1::uuid FOR UPDATE",
        )
        .bind(fleet)
        .fetch_one(&mut *tx)
        .await
        .expect("lock fixture setup");
        let revision = afd_db::test_util::mint_id();
        sqlx::query("INSERT INTO core.fleet_definition_revisions(id,fleet_id,revision,definition,canonical_digest,policy_semantics,created_at,runtime_model_ref) VALUES($1::uuid,$2::uuid,$3,$4::jsonb,$5,$6,$7,$8)")
            .bind(&revision).bind(fleet).bind(number).bind(serde_json::to_string(definition).expect("Definition JSON"))
            .bind(canonical_digest(definition).expect("canonical digest")).bind(POLICY_SEMANTICS)
            .bind(clock::now().as_millis()).bind(afd_credential::provider::PLATFORM_MODEL_REF)
            .execute(&mut *tx).await.expect("append immutable setup");
        sqlx::query("UPDATE core.fleets SET current_definition_revision_id=$2::uuid,setup_version=$3,config_json=$4::jsonb WHERE id=$1::uuid")
            .bind(fleet).bind(&revision).bind(number).bind(definition.config().value().to_string())
            .execute(&mut *tx).await.expect("select setup");
        tx.commit().await.expect("commit fixture setup");
    }
}
