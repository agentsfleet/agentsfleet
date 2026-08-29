//! Fleet-memory paging and forgetting over the production roles and schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;
use sqlx::Acquire as _;

use self::harness::{Fleet, json_body, send};

const SUBJECT: &str = "user_live_memory_operator";
const FIRST_KEY: &str = "deployment-style";
const SECOND_KEY: &str = "review-style";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn fleet_memories_page_search_and_forget_real_rows() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();
    let collection = fixture.collection();

    assert_memory_pages(&router, &fixture.token, &collection).await;
    assert_memory_filters(&router, &fixture.token, &collection).await;
    assert_memory_forget(&router, &fixture.token, &collection).await;
    assert_missing_fleet(&router, &fixture).await;
    fixture.cleanup().await;
}

async fn assert_memory_pages(router: &axum::Router, token: &str, collection: &str) {
    let first = send(
        router,
        Method::GET,
        &format!("{collection}?limit=1"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first = json_body(first).await;
    assert_eq!(items(&first).len(), 1);
    let cursor = text(&first, "next_cursor").to_owned();

    let next = send(
        router,
        Method::GET,
        &format!("{collection}?limit=1&starting_after={cursor}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert_eq!(items(&json_body(next).await).len(), 1);
}

async fn assert_memory_filters(router: &axum::Router, token: &str, collection: &str) {
    let category = send(
        router,
        Method::GET,
        &format!("{collection}?category=core"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(category.status(), StatusCode::OK);
    assert_eq!(items(&json_body(category).await).len(), 1);

    let no_match = send(
        router,
        Method::GET,
        &format!("{collection}?query=absent-pattern"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(no_match.status(), StatusCode::OK);
    assert!(items(&json_body(no_match).await).is_empty());
}

async fn assert_memory_forget(router: &axum::Router, token: &str, collection: &str) {
    let forgotten = send(
        router,
        Method::DELETE,
        &format!("{collection}/{FIRST_KEY}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(forgotten.status(), StatusCode::NO_CONTENT);
    let repeated = send(
        router,
        Method::DELETE,
        &format!("{collection}/{FIRST_KEY}"),
        Some(token),
        "",
    )
    .await;
    assert_eq!(repeated.status(), StatusCode::NOT_FOUND);
}

async fn assert_missing_fleet(router: &axum::Router, fixture: &Fixture) {
    let missing_fleet = send(
        router,
        Method::GET,
        &format!(
            "/v1/workspaces/{}/fleets/{}/memories",
            fixture.workspace,
            mint_id()
        ),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(missing_fleet.status(), StatusCode::NOT_FOUND);
}

fn items(document: &Value) -> &[Value] {
    document
        .get("items")
        .and_then(Value::as_array)
        .expect("a memory page carries items")
}

fn text<'value>(document: &'value Value, field: &str) -> &'value str {
    document
        .get(field)
        .and_then(Value::as_str)
        .expect("the response field is a string")
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: afd_core::id::Uuid7,
    fleet: String,
    key: String,
    token: String,
    entries: [String; 2],
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: afd_core::id::Uuid7::parse(&mint_id())
                .expect("a minted workspace is canonical"),
            fleet: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            entries: [mint_id(), mint_id()],
            lane,
        }
    }

    fn collection(&self) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/memories",
            self.workspace.as_str(),
            self.fleet,
        )
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Memory operator', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'memory', $3, 1) \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, 'fixture', '', $5, $3, TRUE, NULL, 1, 1) \
             ) \
             INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($6::uuid, $2::uuid, $1::uuid, 'memory', '# fixture', '{}', \
                     'active', 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(SUBJECT)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(&self.fleet)
        .execute(&mut *connection)
        .await
        .expect("the authenticated fleet seeds");

        let mut transaction = connection.begin().await.expect("memory seed begins");
        sqlx::query("SET LOCAL ROLE memory_runtime")
            .execute(&mut *transaction)
            .await
            .expect("the memory role is available");
        for (index, (id, key, category, content)) in [
            (&self.entries[0], FIRST_KEY, "core", "deploy with a canary"),
            (
                &self.entries[1],
                SECOND_KEY,
                "scratch",
                "review before merging",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let instant = 10_i64 + i64::try_from(index).unwrap_or_default();
            sqlx::query(
                "INSERT INTO memory.memory_entries \
                   (id, key, content, category, fleet_id, created_at, updated_at) \
                 VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, $6)",
            )
            .bind(id)
            .bind(key)
            .bind(content)
            .bind(category)
            .bind(&self.fleet)
            .bind(instant)
            .execute(&mut *transaction)
            .await
            .expect("the memory entry seeds");
        }
        transaction.commit().await.expect("memory seed commits");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
