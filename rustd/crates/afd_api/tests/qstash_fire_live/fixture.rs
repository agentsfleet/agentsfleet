//! One workspace holding one fleet and one schedule, over both live stores.
//!
//! `Fleet::live` alone would not do: its schedules plane appends to a Redis
//! nothing resolves, and the accepted fire is the one path that writes. This
//! fixture arranges the queue too — see [`harness::Fleet::with_live_fire`].

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::id::Uuid7;
use afd_cron::DesiredStatus;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_fleet_lifecycle::FleetStatus;
use afd_redis::Redis;

use crate::harness;

/// What every fixture person's subject starts with.
const SUBJECT_PREFIX: &str = "user_live_qstash_fire_";

/// The expression the seeded schedule carries.
///
/// Never evaluated: nothing in this lane computes a next fire, because the
/// callback IS the tick. The column is `NOT NULL`, so it holds something real
/// rather than a placeholder a later reader would take for a bug.
const NIGHTLY: &str = "0 3 * * *";

/// What the seeded schedule asks its fleet to do.
const MESSAGE: &str = "run the nightly sweep";

/// The scheduler this daemon registered the schedule with.
const SOURCE: &str = "qstash";

/// A workspace holding one fleet and one schedule that fires at it.
pub(super) struct Fixture {
    lane: TestDatabase,
    database: Db,
    queue: Redis,
    subject: String,
    tenant: String,
    workspace: Uuid7,
    user: String,
    fleet: Uuid7,
    /// The schedule a fire names in its header.
    pub(super) schedule: Uuid7,
}

impl Fixture {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            queue: harness::connect_redis().await,
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: Uuid7::parse(&mint_id()).expect("a minted workspace is canonical"),
            user: mint_id(),
            fleet: Uuid7::parse(&mint_id()).expect("a minted fleet is canonical"),
            schedule: Uuid7::parse(&mint_id()).expect("a minted schedule is canonical"),
            lane,
        }
    }

    /// The production router, resolving through Postgres AND the queue.
    pub(super) fn router(&self) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
        )
        .with_schedule_keys(
            crate::webhook_qstash_route::CURRENT_KEY,
            crate::webhook_qstash_route::NEXT_KEY,
        )
        .with_live_fire(self.database.clone(), self.queue.clone())
        .router()
    }

    /// Seeds the tenant, its workspace, the person, the fleet and the schedule.
    ///
    /// Both states a fire turns on are arguments, because the three drop arms
    /// differ only in which of them the row holds.
    pub(super) async fn seed(&self, fleet: FleetStatus, desired: DesiredStatus) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'QStash fire live', 1, 1) \
             ), workspace AS ( \
               INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
               VALUES ($2::uuid, $1::uuid, 'qstash-fire-live', $3, 1) \
             ), person AS ( \
               INSERT INTO core.users \
                 (id, tenant_id, oidc_subject, email, created_at, updated_at) \
               VALUES ($4::uuid, $1::uuid, $3, 'qstash-fire@example.test', 1, 1) \
             ), fleet AS ( \
               INSERT INTO core.fleets \
                 (id, workspace_id, tenant_id, name, source_markdown, config_json, \
                  status, created_at, updated_at) \
               VALUES ($5::uuid, $2::uuid, $1::uuid, 'qstash-fire-fleet', '# fixture', \
                       '{}'::jsonb, $6, 1, 1) \
             ) \
             INSERT INTO core.fleet_schedules \
               (id, fleet_id, source, source_key, cron_expression, timezone, message, \
                desired_status, sync_status, generation, created_at, updated_at) \
             VALUES ($7::uuid, $5::uuid, $8, $7::text, $9, 'UTC', $10, $11, 'synced', 1, 1, 1)",
        )
        .bind(&self.tenant)
        .bind(self.workspace.as_str())
        .bind(&self.subject)
        .bind(&self.user)
        .bind(self.fleet.as_str())
        .bind(fleet.as_str())
        .bind(self.schedule.as_str())
        .bind(SOURCE)
        .bind(NIGHTLY)
        .bind(MESSAGE)
        .bind(desired.as_str())
        .execute(&mut *connection)
        .await
        .expect("the tenant, workspace, person, fleet and schedule seed");
    }

    /// Removes this run's rows — the schedule cascades with its fleet.
    pub(super) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the tenant cascades away");
        drop(connection);
        drop(self.lane);
    }
}
