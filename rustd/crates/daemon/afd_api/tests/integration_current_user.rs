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
//! # Why the log capture rides the same run
//!
//! The success path is the only one holding the person's email, and it needs
//! the rows this suite seeds; a capture over a refusal would search records
//! that could never have held it.
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
use afd_core::test_util::trace::Capture;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{ApiKeySeed, TenantSeed, TestDatabase, UserSeed, WorkspaceSeed, mint_id};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, json_body, send};

/// The route under test.
const CURRENT_USER: &str = "/v1/users/me";

/// What this suite's person is named before its own identifier is appended.
///
/// A prefix rather than a fixed subject: `identity.users.oidc_subject` is unique
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
    let capture = Capture::install();
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
    no_record_names_the_person(&capture);

    fixture.cleanup().await;
}

/// Nothing logged while the read answered carries the person it answered about.
///
/// The response is the one place an address may go. A log line is one field
/// away from publishing it to an operator's aggregator, whether the handler
/// writes it or a helper beneath it does, so every event and span recorded on
/// this thread is searched for the person's email and display name.
fn no_record_names_the_person(capture: &Capture) {
    let events = capture.events().into_iter().map(|event| event.fields);
    let spans = capture.spans().into_iter().map(|span| span.fields);
    for fields in events.chain(spans) {
        for (name, value) in &fields {
            for private in [EMAIL, DISPLAY_NAME] {
                assert!(
                    !value.contains(private),
                    "the `{name}` field carries `{private}`: {fields:?}"
                );
            }
        }
    }
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
        body.get("display_name"),
        None,
        "a person who never set one has NO display_name key — not a null, and \
         not an empty string a client would render as a name"
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
        TenantSeed::new(&self.tenant)
            .name(TENANT_NAME)
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(self.workspace.as_str(), &self.tenant)
            .name("current-user")
            .insert(&mut *connection)
            .await
            .expect("its workspace seeds");
        UserSeed::new(&self.user, &self.tenant, &self.subject, EMAIL)
            .display_name(DISPLAY_NAME)
            .insert(&mut *connection)
            .await
            .expect("its person seeds");
        ApiKeySeed::new(&self.key, &self.tenant, digest.as_str(), &self.subject)
            .insert(&mut *connection)
            .await
            .expect("their credential seeds");
    }

    /// Returns the person to having never set a display name.
    async fn clear_display_name(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE identity.users SET display_name = NULL WHERE id = $1::uuid")
            .bind(&self.user)
            .execute(&mut *connection)
            .await
            .expect("the display name clears");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        for statement in [
            "DELETE FROM identity.api_keys WHERE tenant_id = $1::uuid",
            "DELETE FROM identity.users WHERE tenant_id = $1::uuid",
            "DELETE FROM identity.workspaces WHERE tenant_id = $1::uuid",
            "DELETE FROM identity.tenants WHERE id = $1::uuid",
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
/// `clippy::indexing_slicing` is denied across this workspace, and the lint
/// earned its keep here. Indexing a missing key yields `null`, so the first
/// version of the absent-display-name act asserted `body["display_name"] ==
/// null` and passed — against a response that omits the key ENTIRELY. It would
/// have passed just as happily if the field had vanished from the wire. The
/// act now reads `get` directly and asserts `None`; everything else reads
/// through here, where a missing field is a failure and not a null.
fn field<'a>(body: &'a Value, name: &str) -> &'a Value {
    body.get(name)
        .expect("the identity response carries every field it declares")
}
