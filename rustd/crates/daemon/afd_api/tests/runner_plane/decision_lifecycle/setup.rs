//! Existing native seeds create only this scenario's owned identities and setup.

use af_core::id::Uuid7;
use af_fleet_runtime::test_util::FixtureSetup;
use afd_db::{
    Db,
    test_util::{
        FleetSeed, ModelSeed, SetupRevision, TenantSeed, WalletSeed, WorkspaceSeed, mint_id,
    },
};
use serde_json::json;

pub(super) const BALANCE: i64 = 1_000_000_000;
pub(super) const PROVIDER: &str = "anthropic";
pub(super) const MESSAGE: &str = "Inspect PR 123 and explain its failed checks.";

pub(super) struct Owner {
    pub tenant: Uuid7,
    pub workspace: Uuid7,
    pub fleet: Uuid7,
    pub tag: String,
    pub coding_id: Uuid7,
    pub coding_model: String,
}

impl Owner {
    pub(super) fn new() -> Self {
        Self {
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            tag: format!("decision-http-{}", mint_id()),
            coding_id: mint_id(),
            coding_model: format!("decision-http-coding-{}", mint_id()),
        }
    }

    pub(super) async fn seed(&self, database: &Db) {
        let mut connection = database.acquire().await.expect("native owner seed");
        TenantSeed::new(&self.tenant)
            .name("Decision ownership")
            .insert(&mut *connection)
            .await
            .expect("tenant");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("Decision ownership")
            .insert(&mut *connection)
            .await
            .expect("workspace");
        WalletSeed::new(&self.tenant, BALANCE)
            .insert(&mut *connection)
            .await
            .expect("funded wallet");
        ModelSeed::new(&self.coding_id, PROVIDER, &self.coding_model, 200_000)
            .rates(3_000_000_000, 300_000_000, 15_000_000_000)
            .insert(&mut *connection)
            .await
            .expect("owned coding price");
        let policy: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../../tests/fixtures/fleets/goal-repair/decision-policy.json"
        ))
        .expect("shared owner policy");
        let setup = FixtureSetup::new(&json!({"name":"pull-request-review", "x-agentsfleet":{"tools":[],"triggers":[],"budget":{"daily_nanos":"1000000000"},"decision_policy":policy}}).to_string());
        FleetSeed::new(
            &self.fleet,
            &self.workspace,
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("Pull request review")
        .tags(&[self.tag.as_str()])
        .setup(SetupRevision {
            definition: &setup.definition,
            digest: &setup.digest,
            semantics: setup.semantics(),
            model_ref: Some("platform-default"),
        })
        .insert(&mut *connection)
        .await
        .expect("native saved fleet");
    }
}
