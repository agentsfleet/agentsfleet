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
use afd_cron::MAX_SCHEDULES_PER_FLEET;
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

/// The subject the ceiling case authenticates as.
const SUBJECT_CEILING: &str = "user_live_fleet_schedules_ceiling";

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
    an_unpause_is_the_same_verb_carrying_the_other_intent(&router, &fixture, &created).await;
    a_second_create_in_the_same_instant_is_refused_as_a_duplicate(&router, &fixture).await;
    a_sync_reconciles_the_row_it_names(&router, &fixture, &created).await;
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
    an_edit_is_validated_before_the_row_is_looked_up(&router, &fixture).await;
    a_sync_of_a_schedule_this_fleet_does_not_hold_is_not_found(&router, &fixture).await;
    a_schedule_for_a_fleet_that_does_not_exist_is_refused(&router, &fixture).await;
    nothing_was_written_by_any_refusal(&router, &fixture).await;

    fixture.cleanup().await;
}

/// The per-fleet ceiling is refused at the store, before the row is written.
///
/// `MAX_SCHEDULES_PER_FLEET` bounds the fan-out of a single fleet, and the
/// count is taken inside the same statement that would insert — so the refusal
/// cannot race a concurrent create past the limit. The ceiling is seeded here
/// rather than driven through the API because the suite's clock is frozen: a
/// second create over HTTP collides on the upstream key long before it could
/// reach the thirty-third, which is the duplicate case, not this one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleet_already_holding_the_ceiling_refuses_one_more() {
    let fixture = Fixture::create(SUBJECT_CEILING).await;
    fixture.seed().await;
    fixture.fill_to_the_ceiling().await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT_CEILING,
        ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
    )
    .with_owned_workspace(fixture.workspace.clone())
    .router();

    let refused = create_with(
        &router,
        &fixture,
        json!({ "cron": NIGHTLY, "message": "one too many" }),
    )
    .await;

    assert_eq!(
        refused.0,
        StatusCode::CONFLICT,
        "a fleet at its ceiling refuses the next schedule rather than growing \
         a fan-out nothing bounds"
    );

    let listed = json_body(
        send(
            &router,
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
        MAX_SCHEDULES_PER_FLEET,
        "the refused create wrote nothing: the fleet still holds exactly the \
         ceiling it held before"
    );

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

/// Unpausing is `paused: false`, and it has to be graded separately.
///
/// The pause case above covers one side of the same `if`; this is the other,
/// and they are not symmetric enough to assume. `paused` is an Option<bool>,
/// so the handler distinguishes three things a caller can mean — pause, resume,
/// and say nothing about it — and only the third leaves the desired status
/// alone. A resume that fell through to "say nothing" would look correct in the
/// response, which echoes the stored row, and would leave a schedule the
/// operator believes they restarted still paused.
async fn an_unpause_is_the_same_verb_carrying_the_other_intent(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::PATCH,
        &fixture.one(schedule),
        Some(&fixture.token),
        &json!({ "paused": false }).to_string(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = json_body(response).await;
    assert_eq!(
        field(&document, "status"),
        "active",
        "`paused: false` is a resume; a schedule left paused here is one the \
         operator believes they restarted and did not"
    );
}

/// Two creates in one instant collide, and the collision is the daemon's own.
///
/// `create` names the row upstream with `{fleet}-{now}` before the scheduler
/// has answered with an id of its own. That key has to be unique per fleet, and
/// the clock is what makes it so — which means the duplicate this refuses is
/// not an operator sending the same schedule twice, it is two arriving inside
/// the same millisecond. The suite's clock is frozen, so every create in this
/// test is "the same millisecond" and the second one is the case.
async fn a_second_create_in_the_same_instant_is_refused_as_a_duplicate(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let refused = create_with(
        router,
        fixture,
        json!({ "cron": NIGHTLY, "message": "run the nightly repair" }),
    )
    .await;

    assert_eq!(
        refused.0,
        StatusCode::CONFLICT,
        "a second create inside the same millisecond reuses the upstream key \
         the first one claimed, and is refused rather than overwriting it"
    );
}

/// A sync asks for the reconcile the background pass would have made.
///
/// The scheduler is unreachable in this suite, which is the state the module
/// note calls out: the reconcile runs, fails to register, and records that on
/// the row. It still answers 200 with the schedule, because the caller asked
/// for a reconcile and got one — what it produced is in the `sync` field, and
/// a 502 here would tell an operator their schedule is gone when it is not.
async fn a_sync_reconciles_the_row_it_names(
    router: &axum::Router,
    fixture: &Fixture,
    schedule: &str,
) {
    let response = send(
        router,
        Method::POST,
        &format!("{}/sync", fixture.one(schedule)),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the reconcile ran; whether it reached the scheduler is the `sync` \
         field's business, not the status code's"
    );
    let document = json_body(response).await;
    assert_ne!(
        field(&document, "sync"),
        "synced",
        "nothing resolves the scheduler here, so a synced verdict would mean \
         the suite is grading a reconcile that did not happen"
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

/// Every field `PATCH` accepts is validated, and validated BEFORE the lookup.
///
/// The ordering is the point, not an incidental. `patch` checks the fields it
/// was given and only then asks the store for the row, so an edit carrying an
/// expression the scheduler would reject is refused whether or not the schedule
/// exists — which is why this case can assert all three refusals against a
/// schedule id that was never created, and why the fleet still holds nothing
/// afterwards. Were the lookup to move first, these would answer "not found"
/// and the validation they exist to grade would never run.
async fn an_edit_is_validated_before_the_row_is_looked_up(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent = mint_id();

    for (field, body) in [
        ("cron", json!({ "cron": "not an expression" })),
        ("timezone", json!({ "timezone": "Mars/Olympus" })),
        ("message", json!({ "message": "" })),
    ] {
        let response = send(
            router,
            Method::PATCH,
            &fixture.one(&absent),
            Some(&fixture.token),
            &body.to_string(),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a patch carrying an unusable `{field}` is refused for THAT reason, \
             not as a missing row — the validation runs before the lookup"
        );
    }
}

/// A sync names a schedule, and a name matching nothing is a 404.
///
/// `sync` is the one schedule verb with no body to get wrong, so the only
/// refusal it owns is this one: the reconcile answers `None` for a row this
/// fleet does not hold, and `held_or` turns that into a not-found rather than
/// letting an absent row read as a successful no-op.
async fn a_sync_of_a_schedule_this_fleet_does_not_hold_is_not_found(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let response = send(
        router,
        Method::POST,
        &format!("{}/sync", fixture.one(&mint_id())),
        Some(&fixture.token),
        "",
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a sync of a schedule this fleet does not hold is a 404, not a silent OK"
    );
}

/// A create names a fleet, and the store is what knows the fleet is real.
///
/// The workspace here is owned and the request is authorised, so nothing before
/// the store has cause to refuse: the path parses, the body validates, and the
/// insert is the first thing that consults `fleets`. What it answers is a
/// refusal and not a database error, because a schedule for a fleet nobody
/// created is a caller mistake rather than an incident — and rendering it as a
/// 500 would page somebody for a typo.
async fn a_schedule_for_a_fleet_that_does_not_exist_is_refused(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let absent_fleet = mint_id();
    let response = send(
        router,
        Method::POST,
        &format!(
            "/v1/workspaces/{}/fleets/{absent_fleet}/schedules",
            fixture.workspace.as_str()
        ),
        Some(&fixture.token),
        &json!({ "cron": NIGHTLY, "message": "run the nightly repair" }).to_string(),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a schedule for a fleet that was never created is a 404, not a 500"
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

    /// Fills this fleet to [`MAX_SCHEDULES_PER_FLEET`], bypassing the API.
    ///
    /// Written straight to the table for the reason the calling test states:
    /// the frozen clock makes a second create over HTTP a key collision, so the
    /// ceiling is not reachable through the surface being graded. Each row
    /// carries its own `source_key`, which is the uniqueness the table actually
    /// enforces, and `generation` starts at one because zero is banned by a
    /// check constraint that exists to keep "never synced" distinguishable.
    ///
    /// `sync_status` is `failed` rather than any word meaning "not yet tried":
    /// the column holds one of three states this build parses, and a row
    /// carrying anything else is unreadable — the list answers an error rather
    /// than a page, which is how the first draft of this helper failed. Failed
    /// is also the honest state here, since nothing resolves the scheduler.
    async fn fill_to_the_ceiling(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        for slot in 0..MAX_SCHEDULES_PER_FLEET {
            sqlx::query(
                "INSERT INTO core.fleet_schedules \
                   (id, fleet_id, source, source_key, cron_expression, timezone, \
                    message, desired_status, sync_status, generation, created_at, \
                    updated_at) \
                 VALUES ($1::uuid, $2::uuid, 'api', $3, $4, 'UTC', 'seeded', \
                         'active', 'failed', 1, 1, 1)",
            )
            .bind(mint_id())
            .bind(self.fleet.as_str())
            .bind(format!("seeded-{slot}"))
            .bind(NIGHTLY)
            .execute(&mut *connection)
            .await
            .expect("a seeded schedule row");
        }
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
