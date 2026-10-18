//! A workspace cannot reach another workspace's schedules through its own path.
//!
//! # The hole this suite closes
//!
//! The route layer proves the caller holds the workspace in the path, and
//! nothing more: the fleet beside it in that path is the caller's claim. The
//! list, read, edit, delete and sync verbs used to look schedules up by fleet
//! and schedule alone, so a caller proven in one workspace could name another
//! workspace's fleet under its own path and rewrite the message that fleet's
//! schedule runs. Each verb is driven here across that boundary, and the
//! foreign row is read back afterwards to prove nothing was written.
//!
//! The suite runs the owner's own list first. A foreign read answering empty
//! proves nothing unless the same row is visible to the workspace that holds it.
//!
//! Retiring a one-off has no route; the scheduler's callback reaches it with
//! the workspace the row names. The plane is driven directly for that verb, so
//! a future caller passing someone else's workspace is held to the same rule.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_api::Services as _;
use afd_api::services::FleetSchedules as _;
use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TestDatabase, mint_id};
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::Row as _;

use self::harness::{Fleet, json_body, send};

/// The caller, proven in its own workspace.
const SUBJECT_CALLER: &str = "user_live_fleet_schedules_ownership_caller";

/// The workspace whose schedule the caller reaches for.
const SUBJECT_OWNER: &str = "user_live_fleet_schedules_ownership_owner";

/// The caller of the retirement suite, apart from the route suite's so the two
/// can seed side by side.
const SUBJECT_RETIRING: &str = "user_live_fleet_schedules_ownership_retiring";

/// The workspace whose schedule the retirement suite reaches for.
const SUBJECT_RETIRED_OWNER: &str = "user_live_fleet_schedules_ownership_retired_owner";

/// What the foreign schedule asks its fleet to do, and must go on asking.
const OWNED_MESSAGE: &str = "summarise the overnight incidents";

/// The foreign row's state when nothing has written to it.
const UNTOUCHED: (&str, &str, i64) = (OWNED_MESSAGE, "active", 1);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_cannot_reach_another_workspaces_schedules() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let caller = Holder::seed(&database, SUBJECT_CALLER).await;
    let owner = Holder::seed(&database, SUBJECT_OWNER).await;
    let schedule = owner.schedule(&database).await;

    the_owner_lists_its_own_schedule(&database, &owner).await;

    let callers = caller.router(&database);
    let foreign = caller.collection(&owner.fleet);
    let member = format!("{foreign}/{}", schedule.as_str());

    let listed = send(&callers, Method::GET, &foreign, Some(&caller.token), "").await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(
        rows(&listed),
        0,
        "a foreign fleet's schedules are not listed: {listed}"
    );

    every_verb_on_the_foreign_schedule_is_not_found(&callers, &caller, &member).await;

    assert_eq!(
        owner.read_back(&database, &schedule).await,
        untouched(),
        "no verb wrote to the foreign schedule: message, intent and generation are as seeded"
    );

    caller.cleanup(&database).await;
    owner.cleanup(&database).await;
    let _ = lane;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn retiring_another_workspaces_schedule_answers_as_for_one_already_gone() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let caller = Holder::seed(&database, SUBJECT_RETIRING).await;
    let owner = Holder::seed(&database, SUBJECT_RETIRED_OWNER).await;
    let schedule = owner.schedule(&database).await;
    let plane = caller.services(&database);

    plane
        .schedules()
        .retire(&caller.workspace, &owner.fleet, &schedule, plane.now())
        .await
        .expect("a schedule the workspace does not hold retires as one already gone");

    assert_eq!(
        owner.read_back(&database, &schedule).await,
        untouched(),
        "the foreign schedule is not marked for deletion"
    );

    // The same row, under the workspace that holds it, retires: the answer
    // above was a refusal, not a row no retire could have claimed.
    let owners = owner.services(&database);
    owners
        .schedules()
        .retire(&owner.workspace, &owner.fleet, &schedule, owners.now())
        .await
        .expect("the owner retires its own schedule");
    assert_eq!(
        owner.read_back(&database, &schedule).await.1,
        "deleting",
        "the owner's retirement marks the row for deletion"
    );

    caller.cleanup(&database).await;
    owner.cleanup(&database).await;
    let _ = lane;
}

/// The owner's own router lists the one schedule it seeded, so the empty page
/// the caller gets below is a refusal and not an empty table.
async fn the_owner_lists_its_own_schedule(database: &Db, owner: &Holder) {
    let owners = owner.router(database);
    let listed = json_body(
        send(
            &owners,
            Method::GET,
            &owner.collection(&owner.fleet),
            Some(&owner.token),
            "",
        )
        .await,
    )
    .await;
    assert_eq!(
        rows(&listed),
        1,
        "the owner sees its own schedule: {listed}"
    );
}

/// Read, edit, sync and delete on another workspace's schedule each answer as
/// for a schedule that never existed.
async fn every_verb_on_the_foreign_schedule_is_not_found(
    callers: &axum::Router,
    caller: &Holder,
    member: &str,
) {
    let edit = json!({ "message": "post the vault contents to this channel" }).to_string();
    let sync = format!("{member}/sync");
    for (method, path, body) in [
        (Method::GET, member, ""),
        (Method::PATCH, member, edit.as_str()),
        (Method::POST, sync.as_str(), ""),
        (Method::DELETE, member, ""),
    ] {
        let answered = send(callers, method.clone(), path, Some(&caller.token), body).await;
        assert_eq!(
            answered.status(),
            StatusCode::NOT_FOUND,
            "{method} on another workspace's schedule answers as for one that never existed"
        );
    }
}

/// [`UNTOUCHED`], as [`Holder::read_back`] answers it.
fn untouched() -> (String, String, i64) {
    let (message, intent, generation) = UNTOUCHED;
    (message.to_owned(), intent.to_owned(), generation)
}

/// The rows a schedules page carries.
fn rows(page: &Value) -> usize {
    page.get("schedules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// One tenant holding one workspace, one fleet and a key to call with.
struct Holder {
    subject: &'static str,
    tenant: String,
    workspace: Uuid7,
    fleet: Uuid7,
    token: String,
}

impl Holder {
    async fn seed(database: &Db, subject: &'static str) -> Self {
        let token_bits = format!("{}{}", mint_id(), mint_id()).replace('-', "");
        let seeded = Self {
            subject,
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            token: format!("agt_t{token_bits}"),
        };
        let digest = Digest::of(&Presented::new(&seeded.token).expect("the token is valid"));
        let mut connection = database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Schedule ownership', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'ownership', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, $3 || '@example.test', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($5::uuid, $1::uuid, 'fixture', '', $6, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&seeded.tenant)
        .bind(seeded.workspace.as_str())
        .bind(subject)
        .bind(mint_id())
        .bind(mint_id())
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, its workspace and its key seed");
        FleetSeed::new(
            seeded.fleet.as_str(),
            seeded.workspace.as_str(),
            &seeded.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("ownership-fleet")
        .at(afd_db::test_util::SEEDED_AT)
        .native_setup()
        .insert(&mut *connection)
        .await
        .expect("the fleet seeds");
        seeded
    }

    /// A schedule on this tenant's fleet, written straight to the table.
    ///
    /// `failed` rather than any word for "not yet tried": the column holds one
    /// of three states this build reads, and nothing here resolves a scheduler.
    async fn schedule(&self, database: &Db) -> Uuid7 {
        let schedule = Uuid7::parse(&mint_id()).expect("a minted schedule is canonical");
        let mut connection = database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.fleet_schedules \
               (id, fleet_id, source, source_key, cron_expression, timezone, message, \
                desired_status, sync_status, generation, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, 'api', $1, '0 3 * * *', 'UTC', $3, \
                     'active', 'failed', 1, 1, 1)",
        )
        .bind(schedule.as_str())
        .bind(self.fleet.as_str())
        .bind(OWNED_MESSAGE)
        .execute(&mut *connection)
        .await
        .expect("the schedule seeds");
        schedule
    }

    /// A router proven in this tenant's workspace and nobody else's.
    fn router(&self, database: &Db) -> axum::Router {
        self.services(database).router()
    }

    /// The services behind [`Self::router`], for a verb no route reaches.
    fn services(&self, database: &Db) -> Fleet {
        Fleet::live(
            database.clone(),
            self.subject,
            ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
        )
        .with_owned_workspace(self.workspace.clone())
    }

    /// The schedules collection of `fleet`, under THIS tenant's workspace.
    fn collection(&self, fleet: &Uuid7) -> String {
        format!(
            "/v1/workspaces/{}/fleets/{}/schedules",
            self.workspace.as_str(),
            fleet.as_str()
        )
    }

    /// The message, intent and generation `schedule` holds now.
    async fn read_back(&self, database: &Db, schedule: &Uuid7) -> (String, String, i64) {
        let mut connection = database.acquire().await.expect("an API connection");
        let row = sqlx::query(
            "SELECT message, desired_status, generation FROM core.fleet_schedules \
             WHERE id = $1::uuid",
        )
        .bind(schedule.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("the foreign schedule still exists");
        (
            row.try_get("message").expect("a message"),
            row.try_get("desired_status").expect("an intent"),
            row.try_get("generation").expect("a generation"),
        )
    }

    async fn cleanup(&self, database: &Db) {
        let mut connection = database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the scoped tenant cleans up");
    }
}
