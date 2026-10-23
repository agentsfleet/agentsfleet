//! Each case owns its authenticated account and configured model.

use super::{mint, seed};
use crate::harness::Fleet;
use afd_auth::{
    credential::Presented,
    directory::Digest,
    scope::{Scope, ScopeSet},
};
use afd_core::id::Uuid7;
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase},
};
use axum::Router;
use serde_json::{Value, json};

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) owner: Uuid7,
    pub(super) tenant: Uuid7,
    pub(super) workspace: Uuid7,
    pub(super) model: Uuid7,
    pub(super) catalogue: Uuid7,
    pub(super) token: String,
    cleanup: OwnedRows,
    lane: TestDatabase,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let mut f = Self {
            db: lane.open(DbRole::Api, &[]).await,
            cleanup: OwnedRows::new(&lane),
            lane,
            owner: mint(),
            tenant: mint(),
            workspace: mint(),
            model: mint(),
            catalogue: mint(),
            token: format!("agt_t{}{}", mint(), mint()).replace('-', ""),
        };
        f.cleanup.record(
            "DELETE FROM authoring.replay_receipts WHERE workspace_id=$1::uuid",
            f.workspace.as_str(),
        );
        f.cleanup.record(
            "DELETE FROM fleet.fleets WHERE workspace_id=$1::uuid",
            f.workspace.as_str(),
        );
        f.cleanup.record(
            "DELETE FROM authoring.library_entries WHERE tenant_id=$1::uuid",
            f.tenant.as_str(),
        );
        f.cleanup.record(
            "DELETE FROM identity.tenants WHERE id=$1::uuid",
            f.tenant.as_str(),
        );
        f.cleanup.record(
            "DELETE FROM model.platform_models WHERE id=$1::uuid",
            f.catalogue.as_str(),
        );
        let digest = Digest::of(&Presented::new(&f.token).expect("fixture token"));
        seed::account(&f, digest.as_str()).await;
        seed::model(&f).await;
        f
    }

    pub(super) fn services(&self, scopes: &[Scope]) -> Fleet {
        Fleet::live(
            self.db.clone(),
            self.owner.as_str(),
            ScopeSet::from_scopes(scopes),
        )
        .at(afd_core::clock::now())
        .with_owned_workspace(self.workspace.clone())
    }

    pub(super) fn router(&self) -> Router {
        self.services(&Scope::ALL).router()
    }

    pub(super) fn owner_url(&self) -> String {
        self.lane.url()
    }

    pub(super) fn own_runner(&mut self, id: &Uuid7) {
        self.cleanup
            .record("DELETE FROM runner.runners WHERE id=$1::uuid", id.as_str());
    }

    /// Removes a second account a case seeds, with its people and keys.
    pub(super) fn own_tenant(&mut self, id: &Uuid7) {
        self.cleanup.record(
            "DELETE FROM identity.tenants WHERE id=$1::uuid",
            id.as_str(),
        );
    }

    pub(super) fn own_library(&mut self, id: &str) {
        self.cleanup.record(
            "DELETE FROM authoring.library_revisions WHERE library_entry_id=$1::uuid",
            id,
        );
    }

    pub(super) fn own_platform_library(&mut self, slug: &str) {
        self.cleanup.record("DELETE FROM authoring.library_revisions WHERE library_entry_id IN (SELECT id FROM authoring.library_entries WHERE platform_slug=$1)", slug);
        self.cleanup.record(
            "DELETE FROM authoring.library_entries WHERE platform_slug=$1",
            slug,
        );
    }

    pub(super) fn collection(&self) -> String {
        format!("/v1/workspaces/{}/drafts", self.workspace)
    }

    pub(super) fn request(&self, operation: &Uuid7) -> Value {
        json!({"source":{"kind":"definition","definition":{
            "schema_version":1, "description":"Explain failures.",
            "instructions_md":"Investigate.\r\n", "config":{"name":"checkout-watch","x-agentsfleet":{
                "triggers":[],"tools":[],"budget":{"daily_nanos":afd_core::money::NANOS_PER_USD.to_string()}
            }}
        }}, "authoring_model_ref":self.model, "operation_id":operation, "message":"Investigate checkout"})
    }

    pub(super) async fn cleanup(mut self) {
        self.cleanup
            .finish()
            .await
            .expect("remove only fixture-owned rows");
        self.db.close().await;
        self.lane.cleanup().await;
    }
}
