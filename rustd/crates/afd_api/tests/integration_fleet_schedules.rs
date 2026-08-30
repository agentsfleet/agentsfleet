//! The schedules CRUD surface, over the migrated schema.
//!
//! # Two behaviours here read as bugs until you know why
//!
//! **A create answers 201 even when the external scheduler refused.** The row
//! is saved either way and the answer carries the sync state, so a caller can
//! see which happened. Answering 502 would tell a person their schedule was not
//! created when it was — and the next reconcile would then repair a schedule
//! they believe does not exist.
//!
//! **A delete does not delete.** It sets `desired_status = deleting` and pushes;
//! the row goes only once the scheduler confirms. A row removed first would
//! leave a schedule firing at a callback this daemon can no longer resolve to a
//! fleet.
//!
//! This suite is where both get asserted, because both are exactly the kind of
//! thing a later reader "fixes".
//!
//! # The scheduler is unreachable here, and that is the interesting state
//!
//! `Fleet::live` gives a real Postgres and a scheduler nothing resolves — which
//! is precisely the create-saved-but-not-registered case above. What this suite
//! cannot reach is a create that registered upstream, and that needs a vendor.

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
use serde_json::{Value, json};

use self::harness::{Fleet, json_body, send};

/// The subject the lifecycle case authenticates as.
///
/// One per TEST rather than one per file, because `core.users.oidc_subject` is
/// globally unique and both cases here seed their own tenant. Two tests sharing
/// a subject is a duplicate-key failure in whichever loses the race, and the
/// other then fails for a reason that has nothing to do with what it asserts.
const SUBJECT_LIFECYCLE: &str = "user_live_fleet_schedules_lifecycle";

/// The subject the refusal case authenticates as.
const SUBJECT_REFUSALS: &str = "user_live_fleet_schedules_refusals";

/// An expression this daemon registers.
const NIGHTLY: &str = "0 3 * * *";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_schedules_surface_completes_its_real_lifecycle() {
    let fixture = Fixture::create(SUBJECT_LIFECYCLE).await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_LIFECYCLE,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    a_new_fleet_has_no_schedules(&router, &fixture).await;
    let created = a_create_saves_the_row_and_says_upstream_does_not_know(&router, &fixture).await;
    the_created_schedule_is_readable_and_listed(&router, &fixture, &created).await;
    a_partial_edit_leaves_the_untouched_fields_alone(&router, &fixture, &created).await;
    a_delete_marks_the_row_rather_than_removing_it(&router, &fixture, &created).await;

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_schedules_surface_refuses_what_it_will_not_register() {
    let fixture = Fixture::create(SUBJECT_REFUSALS).await;
    fixture.seed().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_REFUSALS,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    an_expression_this_daemon_will_not_register_is_refused(&router, &fixture).await;
    a_zone_this_daemon_will_not_pass_upstream_is_refused(&router, &fixture).await;
    a_message_that_would_wake_a_fleet_with_nothing_is_refused(&router, &fixture).await;
    a_body_that_is_not_a_schedule_is_refused(&router, &fixture).await;
    a_schedule_this_fleet_does_not_hold_is_not_found(&router, &fixture).await;
    nothing_was_written_by_any_refusal(&router, &fixture).await;

    fixture.cleanup().await;
}

// ── The lifecycle ────────────────────────────────────────────────────────────

async fn a_new_fleet_has_no_schedules(router: &axum::Router, fixture: &Fixture) {
    let response = send(
        router,
        Method::GET,
        &fixture.collection(),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        schedules(&document).len(),
        0,
        "a fleet that has registered nothing answers an empty page, not a 404"
    );
}

/// The documented 201-despite-refusal, asserted where it actually happens.
async fn a_create_saves_the_row_and_says_upstream_does_not_know(
    router: &axum::Router,
    fixture: &Fixture,
) -> String {
    let response = send(
        router,
        Method::POST,
        &fixture.collection(),
        Some(&fixture.token),
        &json!({ "cron": NIGHTLY, "message": "run the nightly repair" }).to_string(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the row is saved even though the scheduler could not be reached"
    );
    let document = json_body(response).await;

    assert_eq!(field(&document, "cron"), NIGHTLY);
    assert_eq!(
        field(&document, "timezone"),
        "UTC",
        "a schedule naming no zone is not an error; UTC surprises its author least"
    );
    assert_eq!(
        field(&document, "status"),
        "active",
        "the operator's intent"
    );
    assert_ne!(
        field(&document, "sync"),
        "synced",
        "the scheduler was never reached, and a view claiming otherwise would \
         report a schedule as live when it fires nowhere"
    );

    field(&document, "schedule_id").to_owned()
}

async fn the_created_schedule_is_readable_and_listed(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::GET,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(field(&document, "schedule_id"), schedule);
    assert_eq!(field(&document, "fleet_id"), fixture.fleet.as_str());

    let listed = json_body(
        send(
            router,
            Method::GET,
            &fixture.collection(),
            Some(&fixture.token),
            "",
        )
        .await,
    )
    .await;
    let rows = schedules(&listed);
    assert_eq!(rows.len(), 1);
    let only = rows
        .first()
        .expect("the page carries the one row asserted above");
    assert_eq!(field(only, "schedule_id"), schedule);
}

/// An absent field is left alone — a patch is not a whole replacement.
async fn a_partial_edit_leaves_the_untouched_fields_alone(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::PATCH,
        &fixture.one(schedule),
        Some(&fixture.token),
        &json!({ "paused": true }).to_string(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        field(&document, "status"),
        "paused",
        "the field that was sent"
    );
    assert_eq!(
        field(&document, "cron"),
        NIGHTLY,
        "a patch naming no expression must not blank the one that is there"
    );
    assert_eq!(
        field(&document, "message"),
        "run the nightly repair",
        "nor the message"
    );
}

/// The one that reads as a bug: DELETE leaves the row.
async fn a_delete_marks_the_row_rather_than_removing_it(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::DELETE,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert!(
        response.status().is_success(),
        "the delete is accepted: {}",
        response.status()
    );

    let after = send(
        router,
        Method::GET,
        &fixture.one(schedule),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(
        after.status(),
        StatusCode::OK,
        "the row survives until the scheduler confirms — one removed first would \
         leave a schedule firing at a callback that resolves to no fleet"
    );
    assert_eq!(
        field(&json_body(after).await, "status"),
        "deleting",
        "the intent is recorded, and the reconcile is what finishes it"
    );
}

// ── The refusals ─────────────────────────────────────────────────────────────

async fn an_expression_this_daemon_will_not_register_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for expression in ["@daily", "*/61 * * * *", "5-2 * * * *", "MON * * * *", ""] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": expression,
                "message": "run the nightly repair",
            }),
        )
        .await;
        assert_eq!(
            refused.0,
            StatusCode::BAD_REQUEST,
            "`{expression}` must be refused before a row is written, because a \
             failed registration is a state somebody has to clear"
        );
    }
}

async fn a_zone_this_daemon_will_not_pass_upstream_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for zone in ["Mars/Olympus", "../../etc/passwd", ""] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": NIGHTLY,
                "timezone": zone,
                "message": "run the nightly repair",
            }),
        )
        .await;
        assert_eq!(refused.0, StatusCode::BAD_REQUEST, "zone `{zone}`");
    }
}

async fn a_message_that_would_wake_a_fleet_with_nothing_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    for message in ["", "   ", "\n\t "] {
        let refused = create_with(
            router,
            fixture,
            json!({
                "cron": NIGHTLY,
                "message": message,
            }),
        )
        .await;
        assert_eq!(
            refused.0,
            StatusCode::BAD_REQUEST,
            "a fire with nothing to do wakes a fleet to no purpose"
        );
    }
}

async fn a_body_that_is_not_a_schedule_is_refused(router: &axum::Router, fixture: &Fixture) {
    for body in ["", "{", "[]", r#"{"message":"no cron"}"#] {
        let response = send(
            router,
            Method::POST,
            &fixture.collection(),
            Some(&fixture.token),
            body,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "body `{body}` is not a schedule this daemon can read"
        );
    }
}

async fn a_schedule_this_fleet_does_not_hold_is_not_found(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent = mint_id();
    let response = send(
        router,
        Method::GET,
        &fixture.one(&absent),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn nothing_was_written_by_any_refusal(router: &axum::Router, fixture: &Fixture) {
    let listed = json_body(
        send(
            router,
            Method::GET,
            &fixture.collection(),
            Some(&fixture.token),
            "",
        )
        .await,
    )
    .await;

    assert_eq!(
        schedules(&listed).len(),
        0,
        "every case above was refused before the write, so this fleet still \
         holds nothing"
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// One create attempt, as a status and its document.
async fn create_with(router: &axum::Router, fixture: &Fixture, body: Value) -> (StatusCode, Value) {
    let response = send(
        router,
        Method::POST,
        &fixture.collection(),
        Some(&fixture.token),
        &body.to_string(),
    )
    .await;
    let status = response.status();
    (status, json_body(response).await)
}

/// One string field of a document, or the empty string where it is absent.
fn field<'d>(document: &'d Value, name: &str) -> &'d str {
    document
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The rows a page carries.
fn schedules(page: &Value) -> Vec<Value> {
    page.get("schedules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    subject: &'static str,
    tenant: String,
    workspace: Uuid7,
    fleet: Uuid7,
    user: String,
    key: String,
    token: String,
}

impl Fixture {
    async fn create(subject: &'static str) -> Self {
        let lane = TestDatabase::shared();
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            subject,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            user: mint_id(),
            key: mint_id(),
            token: format!("agt_t{token_bits}"),
            lane,
        }
    }

    fn collection(&self) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/schedules",
            self.workspace.as_str(),
            self.fleet.as_str()
        )
    }

    fn one(&self, schedule: &str) -> String {
        format!("{}/{schedule}", self.collection())
    }

    async fn seed(&self) {
        let digest = Digest::of(&Presented::new(&self.token).expect("the token is valid"));
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Fleet schedules', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'schedules', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'schedules-live@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(self.subject)
        .bind(&self.user)
        .bind(&self.key)
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the authenticated workspace seeds");

        sqlx::query(
            "INSERT INTO core.fleets \
               (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                status, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'schedules-fixture-fleet', \
                     '# fixture', '{}'::jsonb, 'active', 1, 1)",
        )
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(&self.tenant)
        .execute(&mut *connection)
        .await
        .expect("the fleet seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
        let _ = self.lane;
    }
}
