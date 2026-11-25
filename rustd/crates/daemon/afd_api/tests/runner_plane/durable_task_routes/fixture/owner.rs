//! The account, workspace, wallet, coding model and fleet one case owns,
//! written through the shared seeds and nothing else.

use af_core::id::Uuid7;
use afd_db::{
    Db,
    test_util::{FleetSeed, ModelSeed, TenantSeed, WalletSeed, WorkspaceSeed, mint_id},
};

/// What the wallet holds before any decision is charged.
pub(in super::super) const BALANCE: i64 = 1_000_000_000;

/// The coding model's provider.
pub(in super::super) const PROVIDER: &str = "anthropic";

/// The person's message the run holds; a task starts from its whole text.
pub(in super::super) const MESSAGE: &str = "Inspect PR 123 and explain its failed checks.";

pub(in super::super) struct Owner {
    pub tenant: Uuid7,
    pub workspace: Uuid7,
    pub fleet: Uuid7,
    /// The placement tag the fleet and its runners share, so no other suite's
    /// runner takes the case's work.
    pub tag: String,
    pub coding_id: Uuid7,
    pub coding_model: String,
}

impl Owner {
    pub(in super::super) fn new() -> Self {
        Self {
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            tag: format!("durable-task-http-{}", mint_id()),
            coding_id: mint_id(),
            coding_model: format!("durable-task-http-coding-{}", mint_id()),
        }
    }

    pub(in super::super) async fn seed(&self, database: &Db) {
        let mut connection = database.acquire().await.expect("owner seed connection");
        TenantSeed::new(&self.tenant)
            .name("Durable task routes")
            .insert(&mut *connection)
            .await
            .expect("tenant");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("Durable task routes")
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
            .expect("priced coding model");
        let tags = [self.tag.as_str()];
        FleetSeed::new(
            &self.fleet,
            &self.workspace,
            &self.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("Pull request review")
        .tags(&tags)
        .insert(&mut *connection)
        .await
        .expect("the fleet");
    }
}
