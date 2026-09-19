//! `GET /v1/users/me` against the rows it actually reads.
//!
//! `tenant_current_user.rs` proves the GUARD: which credential classes reach
//! this route, that it gates on no capability, that a runner token is refused.
//! Every case there builds `Fleet::new()`, which has no Postgres behind it, so
//! every one of them asserts `503` — they prove a person gets THROUGH, and
//! nothing about what comes back. The render was zero covered lines.
//!
//! That is the whole reason this file exists. The response is six fields, five
//! of them columns off one join and one derived from the proven principal, and
//! a harness with no database cannot grade any of them. A stub deciding what
//! `user_of` returns would be a test grading its own fixture.
//!
//! # Why the absent display name is its own act
//!
//! `display_name` is the only nullable column in the projection, and the
//! handler renders it through `Option::map` — so a row with one and a row
//! without take different branches. The second act NULLs it on the same person
//! rather than seeding a second one, because the claim is about the COLUMN and
//! a separate fixture would let an unrelated difference explain the result.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints this without a
//! datastore; `make test-integration-rustd` is the only lane that runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The route under test.
const CURRENT_USER: &str = "/v1/users/me";

/// What this suite's person is named before its own identifier is appended.
///
/// A prefix rather than a fixed subject: `core.users.oidc_subject` is unique
/// deployment-wide, so a fixture spelling it once would collide with its own
/// leftovers the moment a run failed before cleanup.
const SUBJECT_PREFIX: &str = "user_live_current_user_";

/// The person's stored email, tenant name and display name.
const EMAIL: &str = "current-user@example.test";
/// See [`EMAIL`].
const TENANT_NAME: &str = "Current user live";
/// See [`EMAIL`].
const DISPLAY_NAME: &str = "Ada Fixture";

/// The class an `agt_t` credential resolves to, as `afd_wire` spells it.
const TENANT_API_KEY: &str = "tenant_api_key";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_identity_read_answers_the_rows_behind_the_credential() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        &fixture.subject,
        ScopeSet::from_scopes(&[Scope::FleetRead]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    a_named_person_is_rendered_in_full(&router, &fixture).await;
    a_person_with_no_display_name_omits_the_field(&router, &fixture).await;

    fixture.cleanup().await;
}

/// Every field, against the row that produced it.
///
/// Asserted by VALUE and not by presence: the identifiers are minted per run,
/// so a handler rendering the wrong person's row — or the tenant's id where its
/// name belongs — fails here rather than passing on shape.
async fn a_named_person_is_rendered_in_full(router: &axum::Router, fixture: &Fixture) {
    let response = send(router, Method::GET, CURRENT_USER, Some(&fixture.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = json_body(response).await;
    assert_eq!(field(&body, "user_id"), &Value::from(fixture.user.as_str()));
    assert_eq!(field(&body, "email"), &Value::from(EMAIL));
    assert_eq!(field(&body, "display_name"), &Value::from(DISPLAY_NAME));
    assert_eq!(
        field(&body, "tenant_id"),
        &Value::from(fixture.tenant.as_str())
    );
    assert_eq!(field(&body, "tenant_name"), &Value::from(TENANT_NAME));
    assert_eq!(
        field(&body, "credential"),
        &Value::from(TENANT_API_KEY),
        "the class is read from the proven principal, never from what the caller sent"
    );
    assert_eq!(
        field(&body, "scopes"),
        &Value::from(vec![Value::from(Scope::FleetRead.wire())]),
        "the capabilities resolved server-side are what the terminal is told it holds"
    );
}

/// The nullable column's other branch.
async fn a_person_with_no_display_name_omits_the_field(router: &axum::Router, fixture: &Fixture) {
    fixture.clear_display_name().await;

    let response = send(router, Method::GET, CURRENT_USER, Some(&fixture.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = json_body(response).await;
    assert_eq!(
        field(&body, "display_name"),
        &Value::Null,
        "a person who never set a display name has none, not an empty one"
    );
    assert_eq!(
        field(&body, "email"),
        &Value::from(EMAIL),
        "the rest of the row is unaffected"
    );
}

/// A tenant, its workspace, the person acting, and their key.
struct Fixture {
    lane: TestDatabase,
    database: Db,
    tenant: String,
    workspace: Uuid7,
    subject: String,
    user: String,
    key: String,
    token: String,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let first = mint_id().replace('-', "");
        let second = mint_id().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{first}{second}"),
            lane,
        }
    }

    /// The tenant, workspace, person and credential the read joins over.
    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the fixture token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, $7, 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'current-user', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, $8, $9, 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .bind(TENANT_NAME)
        .bind(EMAIL)
        .bind(DISPLAY_NAME)
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace, person and credential seed");
    }

    /// Returns the person to having never set a display name.
    async fn clear_display_name(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE core.users SET display_name = NULL WHERE id = $1::uuid")
            .bind(&self.user)
            .execute(&mut *connection)
            .await
            .expect("the display name clears");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        for statement in [
            "DELETE FROM core.api_keys WHERE tenant_id = $1::uuid",
            "DELETE FROM core.users WHERE tenant_id = $1::uuid",
            "DELETE FROM core.workspaces WHERE tenant_id = $1::uuid",
            "DELETE FROM core.tenants WHERE id = $1::uuid",
        ] {
            sqlx::query(statement)
                .bind(&self.tenant)
                .execute(&mut *connection)
                .await
                .expect("the fixture rows clear");
        }
        drop(connection);
        drop(self.lane);
    }
}

/// One field of the response, named rather than indexed.
///
/// `clippy::indexing_slicing` is denied across this workspace, and the lint is
/// right here for a reason beyond the panic: a missing field indexed with `[]`
/// reads as `null`, which is exactly what the absent-display-name act asserts.
/// Indexing would let a field that VANISHED pass that assertion.
fn field<'a>(body: &'a Value, name: &str) -> &'a Value {
    body.get(name)
        .expect("the identity response carries every field it declares")
}
