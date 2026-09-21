//! The fixture the owned-collection cases run against.
//!
//! Split from the suite beside it because that file was over the 350-line cap
//! with the fixture inline. Nothing here asserts anything about the subject —
//! it seeds rows, sends requests and reports what came back.

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use super::{SOURCE_KIND, SUBJECT, items_of};
use crate::harness::{Fleet, json_body, send};

/// A seeded workspace, its live router, and a second workspace it cannot reach.
pub(super) struct Live {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    foreign: Uuid7,
    token: String,
    router: axum::Router,
    path: String,
}

impl Live {
    pub(super) async fn start() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        let live = Self {
            tenant: mint_id(),
            workspace: parse(&mint_id()),
            foreign: parse(&mint_id()),
            token: format!("agt_t{token_bits}"),
            router: axum::Router::new(),
            path: String::new(),
            database,
            lane,
        };
        live.seed_scope().await;
        let path = format!("/v1/workspaces/{}", live.workspace.as_str());
        let router = Fleet::live(
            live.database.clone(),
            SUBJECT,
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_owned_workspace(live.workspace.clone())
        .router();
        Self {
            router,
            path,
            ..live
        }
    }

    /// The tenant, both workspaces, the person and the key that authenticates.
    pub(super) async fn seed_scope(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Library entries', 1, 1) \
             ), mine AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'mine', $3, 1) \
             ), theirs AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($4::uuid, $1::uuid, 'theirs', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, $3, 'entries-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($6::uuid, $1::uuid, 'fixture', '', $7, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(SUBJECT)
        .bind(self.foreign.as_str())
        .bind(mint_id())
        .bind(mint_id())
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace seeds");
    }

    /// Published platform rows, in the order the gallery serves them.
    pub(super) async fn seed_platform_rows(&self, count: usize) -> Vec<String> {
        let mut seeded = Vec::with_capacity(count);
        for index in 0..count {
            let id = format!("platform-{}-{index}", self.tenant);
            let mut connection = self.database.acquire().await.expect("an API connection");
            sqlx::query(
                "INSERT INTO core.fleet_library ( \
                   id, name, description, source_repo, source_path, source_ref, \
                   required_credentials, required_credentials_reasons, required_tools, \
                   network_hosts, visibility, content_hash, skill_markdown, \
                   trigger_markdown, support_files_json, created_at, updated_at) \
                 VALUES ($1, $1, 'platform fixture', $1, '', 'main', '[]', '{}', '[]', \
                   '[]', 'public', $1, '# Fixture', NULL, '[]', $2, $2)",
            )
            .bind(&id)
            .bind(i64::try_from(index).unwrap_or_default() + 1)
            .execute(&mut *connection)
            .await
            .expect("the platform row seeds");
            seeded.push(id);
        }
        seeded.reverse();
        seeded
    }

    /// Rows the OTHER workspace owns, seeded where this router cannot write.
    pub(super) async fn seed_foreign_entries(&self, count: usize) {
        for index in 0..count {
            let mut connection = self.database.acquire().await.expect("an API connection");
            sqlx::query(
                "INSERT INTO core.tenant_fleet_library ( \
                   id, workspace_id, name, description, source_kind, source_ref, \
                   visibility, content_hash, skill_markdown, trigger_markdown, \
                   support_files_json, requirements_json, created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, $3, 'theirs', 'github', 'main', 'workspace', \
                   $3, '# Fixture', NULL, '[]', \
                   '{\"credentials\":[],\"tools\":[],\"network_hosts\":[],\"trigger_present\":false}', \
                   1, 1)",
            )
            .bind(mint_id())
            .bind(self.foreign.as_str())
            .bind(format!("theirs-{index}"))
            .execute(&mut *connection)
            .await
            .expect("the foreign row seeds");
        }
    }

    /// Onboards one upload bundle and answers its identifier.
    pub(super) async fn onboard(&self, slug: &str) -> String {
        let body = serde_json::json!({
            "source_kind": SOURCE_KIND,
            "source_ref": format!("unit/{slug}"),
            "skill_markdown": format!(
                "---\nname: {slug}\ndescription: Entries fixture.\nversion: 1.0.0\n---\nRun."
            ),
        })
        .to_string();
        let created = send(
            &self.router,
            Method::POST,
            &format!("{}/fleet-libraries", self.path),
            Some(&self.token),
            &body,
        )
        .await;
        let status = created.status();
        let created = json_body(created).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created
            .get("id")
            .and_then(Value::as_str)
            .expect("an onboarding answers with its identifier")
            .to_owned()
    }

    pub(super) async fn owned_page(&self, query: &str) -> Value {
        let (status, body) = self.owned_response(query).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    pub(super) async fn owned_response(&self, query: &str) -> (StatusCode, Value) {
        let listed = send(
            &self.router,
            Method::GET,
            &format!("{}/library-entries{query}", self.path),
            Some(&self.token),
            "",
        )
        .await;
        let status = listed.status();
        (status, json_body(listed).await)
    }

    pub(super) async fn remove_response(&self, entry: &str) -> (StatusCode, Vec<u8>) {
        let removed = send(
            &self.router,
            Method::DELETE,
            &format!("{}/library-entries/{entry}", self.path),
            Some(&self.token),
            "",
        )
        .await;
        let status = removed.status();
        let body = axum::body::to_bytes(removed.into_body(), usize::MAX)
            .await
            .expect("a refusal body is small and in memory");
        (status, body.to_vec())
    }

    /// The rows this workspace owns, counted in the table itself.
    pub(super) async fn owned_row_count(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM core.tenant_fleet_library WHERE workspace_id = $1::uuid",
        )
        .bind(self.workspace.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the count reads")
    }

    /// Every identifier the merged gallery serves, in its own order.
    pub(super) async fn gallery_ids(&self) -> Vec<String> {
        let gallery = send(
            &self.router,
            Method::GET,
            &format!("{}/fleet-libraries", self.path),
            Some(&self.token),
            "",
        )
        .await;
        assert_eq!(gallery.status(), StatusCode::OK);
        items_of(&json_body(gallery).await)
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        sqlx::query("DELETE FROM core.fleet_library WHERE id LIKE $1")
            .bind(format!("platform-{}-%", self.tenant))
            .execute(&mut *connection)
            .await
            .expect("the platform fixtures clean up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// The fixture identifier as the store takes it.
fn parse(id: &str) -> Uuid7 {
    Uuid7::parse(id).expect("the minted fixture id is UUIDv7")
}
