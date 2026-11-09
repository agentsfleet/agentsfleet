//! One workspace holding one fleet and one schedule, over both live stores.
//!
//! `Fleet::live` alone would not do: its schedules plane notifies a bus nothing
//! reaches, and the accepted fire is the one path that writes. This fixture
//! arranges the bus too — see [`harness::Fleet::with_live_fire`].

use af_core::id::Uuid7;
use afd_auth::scope::{Scope, ScopeSet};
use afd_cron::{DesiredStatus, Source};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, UserSeed, WorkspaceSeed, mint_id};
use afd_fleet_lifecycle::FleetStatus;

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

/// A workspace holding one fleet and one schedule that fires at it.
pub(crate) struct Fixture {
    lane: TestDatabase,
    database: Db,
    bus: afd_bus::Handle,
    subject: String,
    tenant: Uuid7,
    workspace: Uuid7,
    user: Uuid7,
    fleet: Uuid7,
    /// The schedule a fire names in its header.
    pub(crate) schedule: Uuid7,
}

impl Fixture {
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            bus: harness::bus(),
            subject: format!("{SUBJECT_PREFIX}{}", mint_id()),
            tenant: mint_id(),
            workspace: mint_id(),
            user: mint_id(),
            fleet: mint_id(),
            schedule: mint_id(),
            lane,
        }
    }

    /// The production router, resolving through Postgres AND the bus.
    pub(crate) fn router(&self) -> axum::Router {
        harness::Fleet::live(
            self.database.clone(),
            &self.subject,
            ScopeSet::from_scopes(&[Scope::ScheduleRead, Scope::ScheduleWrite]),
        )
        .with_schedule_keys(
            crate::webhook_qstash_route::CURRENT_KEY,
            crate::webhook_qstash_route::NEXT_KEY,
        )
        .with_live_fire(self.database.clone(), self.bus.clone())
        .router()
    }

    /// Seeds the tenant, its workspace, the person, the fleet and the schedule.
    ///
    /// Both states a fire turns on are arguments, because the three drop arms
    /// differ only in which of them the row holds.
    pub(crate) async fn seed(&self, fleet: FleetStatus, desired: DesiredStatus) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("QStash fire live")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("qstash-fire-live")
            .insert(&mut *connection)
            .await
            .expect("its workspace seeds");
        UserSeed::new(
            &self.user,
            &self.tenant,
            &self.subject,
            "qstash-fire@example.test",
        )
        .insert(&mut *connection)
        .await
        .expect("its person seeds");
        FleetSeed::new(&self.fleet, &self.workspace, &self.tenant, fleet.as_str())
            .name("qstash-fire-fleet")
            .at(afd_db::test_util::SEEDED_AT)
            .native_setup()
            .insert(&mut *connection)
            .await
            .expect("the fleet seeds");
        sqlx::query(
            "INSERT INTO fleet.schedules \
               (id, fleet_id, source, provider_schedule_id, cron_expression, timezone, \
                message, once, desired_status, sync_status, resource_version, created_at, \
                updated_at) \
             VALUES ($1, $2, $3, $1::text, $4, 'UTC', $5, false, $6, 'synced', 1, \
                     now(), now())",
        )
        .bind(&self.schedule)
        .bind(&self.fleet)
        // A person's schedule: `source` names who wrote it, and a retirement
        // decodes the whole row, so it must hold a value the store reads back.
        .bind(Source::Api.as_str())
        .bind(NIGHTLY)
        .bind(MESSAGE)
        .bind(desired.as_str())
        .execute(&mut *connection)
        .await
        .expect("the schedule seeds");
    }

    /// Makes the seeded schedule a `once` schedule, which retires on its
    /// moment.
    pub(crate) async fn mark_once(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("UPDATE fleet.schedules SET once = true WHERE id = $1")
            .bind(&self.schedule)
            .execute(&mut *connection)
            .await
            .expect("the schedule becomes a one-off");
    }

    /// The seeded schedule's desired status, or `None` once its row is gone.
    pub(crate) async fn desired_status(&self) -> Option<String> {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query_scalar("SELECT desired_status FROM fleet.schedules WHERE id = $1")
            .bind(&self.schedule)
            .fetch_optional(&mut *connection)
            .await
            .expect("the schedule row reads")
    }

    /// Removes this run's rows — the schedule cascades with its fleet.
    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("the tenant cascades away");
        drop(connection);
        drop(self.lane);
    }
}
