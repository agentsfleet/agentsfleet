//! Command-line credential lifecycle through the production router and schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

const SUBJECT: &str = "user_live_cli_lifecycle";
const DASHBOARD_TOKEN: &str = "fixture.header.payload";
const MACHINE: &str = "live-integration-host";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_terminal_credential_can_be_replaced_and_revoke_itself() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_dashboard(SUBJECT)
    .router();

    let first = mint(&router).await;
    let first_id = text(&first, "id").to_owned();
    assert!(text(&first, "credential").starts_with("afc_"));

    let replacement = mint(&router).await;
    let replacement_id = text(&replacement, "id").to_owned();
    let replacement_token = text(&replacement, "credential").to_owned();
    assert_ne!(first_id, replacement_id, "a replacement is a new row");
    fixture.assert_replaced(&first_id, &replacement_id).await;

    let revoked = send(
        &router,
        Method::DELETE,
        &format!("/v1/cli-credentials/{replacement_id}"),
        Some(&replacement_token),
        "",
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let repeated = send(
        &router,
        Method::DELETE,
        &format!("/v1/cli-credentials/{replacement_id}"),
        Some(DASHBOARD_TOKEN),
        "",
    )
    .await;
    assert_eq!(repeated.status(), StatusCode::NOT_FOUND);

    fixture.cleanup().await;
}

async fn mint(router: &axum::Router) -> Value {
    let response = send(
        router,
        Method::POST,
        "/v1/cli-credentials",
        Some(DASHBOARD_TOKEN),
        &format!(r#"{{"machine_name":"{MACHINE}"}}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.headers().get(http::header::CACHE_CONTROL),
        Some(&http::HeaderValue::from_static("no-store, max-age=0"))
    );
    json_body(response).await
}

fn text<'value>(document: &'value Value, field: &str) -> &'value str {
    document
        .get(field)
        .expect("the response field exists")
        .as_str()
        .expect("the response field is a string")
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    user: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            user: mint_id(),
            lane,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'CLI lifecycle', 1, 1) \
             ) \
             INSERT INTO core.users \
               (id, tenant_id, oidc_subject, email, created_at, updated_at) \
             VALUES ($2::uuid, $1::uuid, $3, 'cli-live@example.test', 1, 1)",
        )
        .bind(&self.tenant)
        .bind(&self.user)
        .bind(SUBJECT)
        .execute(&mut *connection)
        .await
        .expect("the CLI principal seeds");
    }

    async fn assert_replaced(&self, first: &str, replacement: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let rows: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT id::text, revoked_at FROM core.cli_credentials \
             WHERE user_id = $1::uuid AND machine_name = $2 ORDER BY created_at, id",
        )
        .bind(&self.user)
        .bind(MACHINE)
        .fetch_all(&mut *connection)
        .await
        .expect("the credential rows load");

        assert_eq!(rows.len(), 2, "one replacement retains one audit row");
        assert!(
            rows.iter()
                .any(|(id, revoked)| id == first && revoked.is_some()),
            "the first credential is revoked atomically"
        );
        assert!(
            rows.iter()
                .any(|(id, revoked)| id == replacement && revoked.is_none()),
            "the replacement is the sole live credential"
        );
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
