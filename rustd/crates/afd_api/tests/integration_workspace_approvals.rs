//! Approval inbox and decision HTTP lifecycle over live Postgres and Redis.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

pub(crate) const SUBJECT: &str = "user_live_approval_inbox";

/// The listing suite's own signed-in person.
///
/// A separate subject because `core.users` holds one row per OIDC subject and
/// these files run concurrently: two fixtures seeding one subject race, and the
/// loser reports a duplicate key rather than the behaviour under test.
pub(crate) const LISTING_SUBJECT: &str = "user_live_approval_listing";

/// The seeded gate's sweeper deadline, in the millisecond epoch the column
/// stores. Far enough past this fixture's `created_at` of 1 that no test in
/// this file races the timeout sweeper; no assertion reads it back.
const GATE_TIMEOUT_AT: i64 = 10_000;

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn approval_inbox_reads_and_resolves_a_live_gate() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let queue = harness::connect_redis().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .with_approval_queue(fixture.database.clone(), queue)
    .router();
    let collection = format!("/v1/workspaces/{}/approvals", fixture.workspace.as_str());
    let item = format!("{collection}/{}", fixture.gate);
    assert_approval_reads(&router, &fixture, &collection, &item).await;
    assert_approval_resolution(&router, &fixture.token, &item).await;
    fixture.cleanup().await;
}

async fn assert_approval_reads(
    router: &axum::Router,
    fixture: &Fixture,
    collection: &str,
    item: &str,
) {
    let listed = send(router, Method::GET, collection, Some(&fixture.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(
        listed.pointer("/items/0/gate_id").and_then(Value::as_str),
        Some(fixture.gate.as_str())
    );

    let detail = send(router, Method::GET, item, Some(&fixture.token), "").await;
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(
        json_body(detail)
            .await
            .get("action_id")
            .and_then(Value::as_str),
        Some(fixture.action.as_str())
    );
}

async fn assert_approval_resolution(router: &axum::Router, token: &str, item: &str) {
    let resolved = send(
        router,
        Method::POST,
        &format!("{item}/approve"),
        Some(token),
        r#"{"reason":"reviewed"}"#,
    )
    .await;
    let status = resolved.status();
    let resolved = json_body(resolved).await;
    assert_eq!(status, StatusCode::OK, "{resolved}");
    assert_eq!(
        resolved.get("outcome").and_then(Value::as_str),
        Some("approved")
    );
    assert_eq!(
        resolved.get("resolved_by").and_then(Value::as_str),
        Some(SUBJECT)
    );

    // The second answer is a 409 rather than a 200 reporting the first. Both
    // tell the caller the gate is resolved; only the conflict tells them it was
    // not resolved BY THEM, which is the difference between an audit trail and
    // a dashboard that credits the wrong person for a denial.
    let repeated = send(
        router,
        Method::POST,
        &format!("{item}/deny"),
        Some(token),
        "",
    )
    .await;
    let status = repeated.status();
    let refused = json_body(repeated).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert_eq!(
        refused.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_ALREADY_RESOLVED.as_str())
    );
    assert_eq!(
        refused.get("current_state").and_then(Value::as_str),
        Some("approved"),
        "the conflict names the standing outcome, so the caller refetches \
         rather than retrying a decision that cannot change"
    );
    assert!(
        !refused.to_string().contains(SUBJECT),
        "the resolver is an entity value and belongs on the gate, not in a \
         refusal sentence"
    );
}

pub(crate) struct Fixture {
    lane: TestDatabase,
    pub(crate) database: Db,
    tenant: String,
    pub(crate) workspace: Uuid7,
    pub(crate) fleet: String,
    user: String,
    key: String,
    pub(crate) gate: String,
    action: String,
    event: String,
    pub(crate) token: String,
    /// The signed-in person this fixture seeds and authenticates as.
    pub(crate) subject: &'static str,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        Self::create_as(SUBJECT).await
    }

    /// A fixture whose person is `subject`.
    pub(crate) async fn create_as(subject: &'static str) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: mint_id(),
            user: mint_id(),
            key: mint_id(),
            gate: mint_id(),
            action: mint_id(),
            event: mint_id(),
            token: format!("agt_t{token_bits}"),
            subject,
            lane,
        }
    }

    pub(crate) async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Approval live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'approval', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'approval-live@example.test', 1, 1) \
             ), credential AS ( \
               INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, active, \
                  revoked_at, created_at, updated_at) \
               VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1) \
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($7::uuid, $2::uuid, $1::uuid, 'approval', '# fixture', '{}', \
                       'active', 1, 1) \
             ) \
             INSERT INTO core.fleet_approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind, \
                proposed_action, evidence, blast_radius, timeout_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($8::uuid, $7::uuid, $2::uuid, $9, 'git', 'push', 'tool', \
                     'open a pull request', '{}', 'one repository', $11, '', 'pending', \
                     '', 1, NULL, $10, 0, 32)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(&self.fleet)
        .bind(&self.gate)
        .bind(&self.action)
        .bind(&self.event)
        .bind(GATE_TIMEOUT_AT)
        .execute(&mut *connection)
        .await
        .expect("the authenticated approval gate seeds");
    }

    /// A second pending gate, later and of another kind.
    ///
    /// One row cannot show a filter narrowing or a page resuming: both look
    /// identical to an unfiltered single-row read.
    pub(crate) async fn seed_second_gate(&self) -> String {
        let second = mint_id();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleet_approval_gates \
               (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_kind, \
                proposed_action, evidence, blast_radius, timeout_at, resolved_by, status, \
                detail, created_at, updated_at, event_id, spend_count, spend_ceiling) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'stripe', 'charge', 'spend', \
                     'refund a customer', '{}', 'one account', $6, '', 'pending', \
                     '', 2, NULL, $5, 0, 32)",
        )
        .bind(&second)
        .bind(&self.fleet)
        .bind(self.workspace.as_str())
        .bind(mint_id())
        .bind(mint_id())
        .bind(GATE_TIMEOUT_AT)
        .execute(&mut *connection)
        .await
        .expect("the second approval gate seeds");
        second
    }

    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .expect("the cleanup transaction opens");
        sqlx::query("SET LOCAL fleet.allow_gate_purge = 'on'")
            .execute(&mut *transaction)
            .await
            .expect("the fixture opts into the guarded approval purge");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *transaction)
            .await
            .expect("the scoped fixture cleans up");
        transaction
            .commit()
            .await
            .expect("the scoped cleanup commits");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}
