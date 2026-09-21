//! The authenticated workspace every fleet-lifecycle live case runs against.
//!
//! Split out of the parent suite rather than written there: the parent was at
//! the 350-line cap with its exercisers, and a fixture is the cut that frees
//! the most lines for the least risk. Nothing here asserts — it seeds rows and
//! hands back the identifiers the cases address them by.
//!
//! Every instance mints its own tenant, workspace and token, so two cases that
//! run in the same lane never see each other's rows.

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};

pub(super) struct Fixture {
    lane: TestDatabase,
    pub(super) database: Db,
    tenant: String,
    pub(super) workspace: Uuid7,
    user: String,
    key: String,
    pub(super) library: String,
    pub(super) grant: String,
    pub(super) token: String,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            key: mint_id(),
            library: mint_id(),
            grant: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    pub(super) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Fleet lifecycle', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'fleets', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'fleet-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(super::SUBJECT)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace seeds");
        sqlx::query(
            "INSERT INTO core.fleet_library \
               (id, name, description, source_repo, source_path, source_ref, \
                required_credentials, required_credentials_reasons, required_tools, \
                network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
                created_at, updated_at) \
             VALUES ($1, 'live-fleet', 'fixture', 'repo', 'path', 'main', '[]', '{}', \
                     '[]', '[]', 'public', $2, $3, $4, 1, 1)",
        )
        .bind(&self.library)
        .bind(format!("sha256:{}", self.library))
        .bind("---\nname: live-fleet\ndescription: Live fleet.\nversion: 1.0.0\n---\nRun.\n")
        .bind("---\nname: live-fleet\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n")
        .execute(&mut *connection)
        .await
        .expect("the platform library entry seeds");
    }

    pub(super) async fn seed_event_and_grant(&self, fleet: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, \
                response_text, tokens, wall_ms, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'steer:api', 'chat', 'completed', \
                     '{\"prompt\":\"ship\"}', 'done', 7, 12, 10, 11)",
        )
        .bind(fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(super::EVENT)
        .execute(&mut *connection)
        .await
        .expect("the completed event seeds");
        sqlx::query(
            "INSERT INTO core.integration_grants \
               (id, fleet_id, service, status, requested_reason, approved_at, created_at) \
             VALUES ($1::uuid, $2::uuid, 'github', 'approved', 'fixture', 10, 9)",
        )
        .bind(&self.grant)
        .bind(fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the approved grant seeds");
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        sqlx::query("DELETE FROM core.fleet_library WHERE id = $1")
            .bind(&self.library)
            .execute(&mut *connection)
            .await
            .expect("the platform library fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
