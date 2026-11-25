//! Each case owns its account, a member of it, and a configured model.

use super::{json_body, mint, seed, send};
use crate::harness::Fleet;
use af_core::id::Uuid7;
use afd_auth::{
    credential::Presented,
    directory::Digest,
    scope::{Scope, ScopeSet},
};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{OwnedRows, TestDatabase},
};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

/// A browser session's bearer; the harness verifier accepts any unmarked one.
pub(super) const SESSION: &str = "fixture.header.payload";

/// The first message every created fleet is sent.
pub(super) const FIRST_MESSAGE: &str = "Investigate checkout";

pub(super) struct Fixture {
    pub(super) db: Db,
    pub(super) owner: Uuid7,
    pub(super) tenant: Uuid7,
    pub(super) workspace: Uuid7,
    pub(super) model: Uuid7,
    pub(super) catalogue: Uuid7,
    /// The owner's API key: a machine acting for the account.
    pub(super) token: String,
    /// A second person, a member of the owner's account.
    pub(super) member: Uuid7,
    member_tenant: Uuid7,
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
            member: mint(),
            member_tenant: mint(),
            token: format!("agt_t{}{}", mint(), mint()).replace('-', ""),
        };
        f.cleanup.record_text(
            "DELETE FROM thread.replay_receipts WHERE scope LIKE $1",
            &format!("{}:%", f.workspace),
        );
        f.cleanup.record(
            "DELETE FROM fleet.fleets WHERE workspace_id=$1",
            &f.workspace,
        );
        f.cleanup
            .record("DELETE FROM identity.tenants WHERE id=$1", &f.tenant);
        f.cleanup
            .record("DELETE FROM identity.tenants WHERE id=$1", &f.member_tenant);
        f.cleanup.record(
            "DELETE FROM model.platform_models WHERE id=$1",
            &f.catalogue,
        );
        let digest = Digest::of(&Presented::new(&f.token).expect("fixture token"));
        seed::account(&f, digest.as_str()).await;
        seed::member(&f, &f.member_tenant).await;
        seed::model(&f).await;
        f
    }

    /// The router an API key of the owner's reaches, holding `scopes`.
    pub(super) fn services(&self, scopes: &[Scope]) -> Fleet {
        Fleet::live(
            self.db.clone(),
            self.owner.as_str(),
            ScopeSet::from_scopes(scopes),
        )
        .at(af_core::clock::now())
        .with_owned_workspace(self.workspace.clone())
    }

    /// The router the owner's API key reaches, holding every scope.
    pub(super) fn router(&self) -> Router {
        self.services(&Scope::ALL).router()
    }

    /// The router `subject`'s browser session reaches, holding every scope:
    /// send [`SESSION`] as its bearer.
    pub(super) fn signed_in(&self, subject: &str) -> Router {
        let scopes = ScopeSet::from_scopes(&Scope::ALL);
        Fleet::live(self.db.clone(), subject, scopes)
            .at(af_core::clock::now())
            .with_owned_workspace(self.workspace.clone())
            .with_dashboard_holding(subject, scopes)
            .router()
    }

    pub(super) fn fleets(&self) -> String {
        format!("/v1/workspaces/{}/fleets", self.workspace)
    }

    pub(super) fn fleet(&self, fleet: &str) -> String {
        format!("{}/{fleet}", self.fleets())
    }

    /// Creates a fleet from its first message through `router` as `bearer`,
    /// answering the create's body.
    pub(super) async fn create_fleet(&self, router: &Router, bearer: &str) -> Value {
        let body = json!({
            "message": FIRST_MESSAGE,
            "operation_id": mint(),
            "model_ref": self.model,
        });
        let created = send(
            router,
            Method::POST,
            &self.fleets(),
            Some(bearer),
            &body.to_string(),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        json_body(created).await
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
