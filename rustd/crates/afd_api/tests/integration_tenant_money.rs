//! Priced catalogue and billing reads over the migrated schema.
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
use http::{Method, StatusCode, header};
use serde_json::Value;

use self::harness::{Fleet, json_body, send, send_with_headers};

const SUBJECT: &str = "user_live_money_catalogue";
const BALANCE: i64 = 42_000;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn catalogue_and_billing_reads_page_real_rows() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    exercise_catalogue(&router, &fixture).await;
    exercise_billing(&router, &fixture).await;
    fixture.cleanup().await;
}

async fn exercise_catalogue(router: &axum::Router, fixture: &Fixture) {
    let path = format!("/v1/models?provider={}&limit=1", fixture.provider);
    let first = send(router, Method::GET, &path, Some(&fixture.token), "").await;
    assert_eq!(first.status(), StatusCode::OK);
    let etag = first
        .headers()
        .get(header::ETAG)
        .expect("a catalogue answer has a validator")
        .to_str()
        .expect("the validator is visible ASCII")
        .to_owned();
    let first = json_body(first).await;
    assert_eq!(items(&first).len(), 1);
    assert_eq!(first.get("total"), Some(&Value::Null));
    let cursor = text(&first, "next_cursor").to_owned();

    let unchanged = send_with_headers(
        router,
        Method::GET,
        &path,
        Some(&fixture.token),
        "",
        &[(header::IF_NONE_MATCH, &etag)],
    )
    .await;
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        unchanged
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );

    let next = send(
        router,
        Method::GET,
        &format!("{path}&starting_after={cursor}"),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    let next = json_body(next).await;
    assert_eq!(items(&next).len(), 1);
    assert_eq!(next.get("next_cursor"), Some(&Value::Null));
}

async fn exercise_billing(router: &axum::Router, fixture: &Fixture) {
    let wallet = send(
        router,
        Method::GET,
        "/v1/tenants/me/billing",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(wallet.status(), StatusCode::OK);
    let wallet = json_body(wallet).await;
    assert_eq!(
        wallet.get("balance_nanos").and_then(Value::as_i64),
        Some(BALANCE)
    );
    assert_eq!(
        wallet.get("is_exhausted").and_then(Value::as_bool),
        Some(false)
    );

    let first = send(
        router,
        Method::GET,
        "/v1/tenants/me/billing/charges?limit=1",
        Some(&fixture.token),
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
        &format!("/v1/tenants/me/billing/charges?limit=1&cursor={cursor}"),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(next.status(), StatusCode::OK);
    assert_eq!(items(&json_body(next).await).len(), 1);

    fixture.remove_wallet().await;
    let missing = send(
        router,
        Method::GET,
        "/v1/tenants/me/billing",
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(missing.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

fn items(document: &Value) -> &[Value] {
    document
        .get(if document.get("models").is_some() {
            "models"
        } else {
            "items"
        })
        .and_then(Value::as_array)
        .expect("a page carries its collection")
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
    key: String,
    token: String,
    provider: String,
    model_rows: [String; 2],
    ledger_rows: [String; 2],
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let tenant = mint_id();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            provider: format!("live{}", tenant.replace('-', "")),
            tenant,
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            model_rows: [mint_id(), mint_id()],
            ledger_rows: [mint_id(), mint_id()],
            lane,
        }
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Money lifecycle', 1, 1) \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($2::uuid, $1::uuid, 'fixture', '', $3, $4, TRUE, NULL, 1, 1) \
             ) \
             INSERT INTO billing.tenant_wallet \
               (tenant_id, balance_nanos, grant_source, created_at, updated_at) \
             VALUES ($1::uuid, $5, 'fixture:seed', 10, 10)",
        )
        .bind(&self.tenant)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(SUBJECT)
        .bind(BALANCE)
        .execute(&mut *connection)
        .await
        .expect("the authenticated tenant and wallet seed");

        for (index, id) in self.model_rows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO core.model_library \
                   (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
                    cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
                 VALUES ($1::uuid, $2, $3, 200000, 5, 1, 25, 1, $4)",
            )
            .bind(id)
            .bind(format!("model-{index}"))
            .bind(&self.provider)
            .bind(1_745_884_800_000_i64 + i64::try_from(index).unwrap_or_default())
            .execute(&mut *connection)
            .await
            .expect("the priced model seeds");
        }

        for (index, id) in self.ledger_rows.iter().enumerate() {
            let instant = 100_i64 + i64::try_from(index).unwrap_or_default();
            sqlx::query(
                "INSERT INTO billing.usage_ledger \
                   (id, tenant_id, event_id, charge_type, posture, model, \
                    credit_deducted_nanos, token_count_input, token_count_cached_input, \
                    token_count_output, wall_ms, event_created_at, created_at, last_charged_at) \
                 VALUES ($1::uuid, $2::uuid, $3, 'stage', 'platform', $4, \
                         7, 11, 2, 13, 17, $5, $5, $5)",
            )
            .bind(id)
            .bind(&self.tenant)
            .bind(format!("live-charge-{}-{index}", self.tenant))
            .bind(format!("model-{index}"))
            .bind(instant)
            .execute(&mut *connection)
            .await
            .expect("the charge row seeds");
        }
    }

    async fn remove_wallet(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM billing.tenant_wallet WHERE tenant_id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the wallet can be removed for the invariant proof");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM billing.usage_ledger WHERE tenant_id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped ledger cleans up");
        sqlx::query("DELETE FROM core.model_library WHERE provider = $1")
            .bind(&self.provider)
            .execute(&mut *connection)
            .await
            .expect("the scoped catalogue cleans up");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
