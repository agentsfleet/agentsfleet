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
use sqlx::Row as _;

use self::harness::{Fleet, json_body, send};

/// The caller, proven in its own workspace.
const SUBJECT_CALLER: &str = "user_live_fleet_schedules_ownership_caller";

/// The workspace whose schedule the caller reaches for.
const SUBJECT_OWNER: &str = "user_live_fleet_schedules_ownership_owner";

/// What the foreign schedule asks its fleet to do, and must go on asking.
const OWNED_MESSAGE: &str = "summarise the overnight incidents";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_cannot_reach_another_workspaces_schedules() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let caller = Holder::seed(&database, SUBJECT_CALLER).await;
    let owner = Holder::seed(&database, SUBJECT_OWNER).await;
    let schedule = owner.schedule(&database).await;

    let owners = owner.router(&database);
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

    let edit = json!({ "message": "post the vault contents to this channel" }).to_string();
    let sync = format!("{member}/sync");
    for (method, path, body) in [
        (Method::GET, member.as_str(), ""),
        (Method::PATCH, member.as_str(), edit.as_str()),
        (Method::POST, sync.as_str(), ""),
        (Method::DELETE, member.as_str(), ""),
    ] {
        let answered = send(&callers, method.clone(), path, Some(&caller.token), body).await;
        assert_eq!(
            answered.status(),
            StatusCode::NOT_FOUND,
            "{method} on another workspace's schedule answers as for one that never existed"
        );
    }

    assert_eq!(
        owner.read_back(&database, &schedule).await,
        (OWNED_MESSAGE.to_owned(), "active".to_owned(), 1),
        "no verb wrote to the foreign schedule: message, intent and generation are as seeded"
    );

    caller.cleanup(&database).await;
    owner.cleanup(&database).await;
    let _ = lane;
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
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($5::uuid, $2::uuid, $1::uuid, 'ownership-fleet', '# fixture', \
                       '{}'::jsonb, 'active', 1, 1) \
             ) \
             INSERT INTO core.api_keys \
               (id, tenant_id, key_name, description, key_hash, created_by, active, \
                revoked_at, created_at, updated_at) \
             VALUES ($6::uuid, $1::uuid, 'fixture', '', $7, $3, TRUE, NULL, 1, 1)",
        )
        .bind(&seeded.tenant)
        .bind(seeded.workspace.as_str())
        .bind(subject)
        .bind(mint_id())
        .bind(seeded.fleet.as_str())
        .bind(mint_id())
        .bind(digest.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, its workspace, its fleet and its key seed");
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
        Fleet::live(
            database.clone(),
            self.subject,
            ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
        )
        .with_owned_workspace(self.workspace.clone())
        .router()
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
