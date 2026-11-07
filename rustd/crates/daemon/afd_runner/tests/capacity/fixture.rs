//! Each proof owns a runner and workspace; counts never inspect other tests.

use std::time::Duration;

use afd_core::{clock, id::Uuid7};
use afd_db::{
    Db,
    config::DbRole,
    test_util::{FleetSeed, RunnerSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id},
};
use jiff::Timestamp;
use sqlx::{Connection as _, Postgres, Transaction};

use crate::rows::{self, AttemptRow, LeaseRow, POLICY};

/// How long a live seeded lease or attempt has left.
pub(super) const LIVE_FOR: Duration = Duration::from_mins(1);

/// How far past its expiry a lapsed seeded lease or attempt is.
pub(super) const LAPSED_BY: Duration = Duration::from_millis(1);

pub(super) struct Fixture {
    /// The API login's pool, which the code under test runs on.
    pub(super) db: Db,
    /// The lane owner's pool, for the rows a test seeds and removes around it.
    owner: Db,
    pub(super) runner: Uuid7,
    tenant: String,
    workspace: String,
    fleet: String,
    lane: TestDatabase,
}

impl Fixture {
    pub(super) async fn create(workers: i32) -> Self {
        let lane = TestDatabase::shared();
        let fixture = Self {
            db: lane.open(DbRole::Api, &[]).await,
            owner: lane.open(DbRole::Migrator, &[]).await,
            runner: Uuid7::parse(&mint_id()).expect("fixture runner UUIDv7"),
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            lane,
        };
        let mut connection = fixture.owner.acquire().await.expect("seed connection");
        TenantSeed::new(&fixture.tenant)
            .name("capacity")
            .insert(&mut *connection)
            .await
            .expect("seed the capacity tenant");
        WorkspaceSeed::new(&fixture.workspace, &fixture.tenant)
            .insert(&mut *connection)
            .await
            .expect("seed the capacity workspace");
        RunnerSeed::new(
            fixture.runner.as_str(),
            afd_runner::sql::RUNNER_STATUS_ACTIVE,
            POLICY,
        )
        .workers(workers)
        .insert(&mut *connection)
        .await
        .expect("seed isolated capacity host");
        FleetSeed::new(
            &fixture.fleet,
            &fixture.workspace,
            &fixture.tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .name("capacity")
        .insert(&mut *connection)
        .await
        .expect("seed isolated capacity fleet");
        drop(connection);
        fixture
    }

    pub(super) async fn authoring(&self, tx: &mut Transaction<'_, Postgres>, until: Timestamp) {
        rows::attempt(
            tx,
            (self.runner.as_str(), &self.workspace, &self.tenant),
            AttemptRow {
                status: afd_runner::sql::LEASE_STATUS_ACTIVE,
                lease_expires_at: until,
                reserved_nanos: 0,
            },
        )
        .await;
    }

    pub(super) async fn runtime(&self, until: Timestamp) {
        let mut connection = self.owner.acquire().await.expect("runtime seed connection");
        rows::lease(
            &mut connection,
            LeaseRow {
                id: &mint_id(),
                runner: self.runner.as_str(),
                fleet: &self.fleet,
                workspace: &self.workspace,
                tenant: &self.tenant,
                status: afd_runner::sql::LEASE_STATUS_ACTIVE,
                expires_at: until,
                stamped_at: None,
            },
        )
        .await;
    }

    /// Moves this host's runtime lease to lapse `after` from now on the
    /// database clock, the clock the census compares against.
    pub(super) async fn lapse_runtime_in(&self, after: Duration) {
        let millis = i64::try_from(after.as_millis()).expect("a test interval fits");
        let mut connection = self.owner.acquire().await.expect("lease update connection");
        sqlx::query(
            "UPDATE runner.leases \
             SET lease_expires_at = clock_timestamp() + $2 * interval '1 millisecond' \
             WHERE runner_id = $1::uuid",
        )
        .bind(self.runner.as_str())
        .bind(millis)
        .execute(&mut *connection)
        .await
        .expect("the runtime lease's expiry moves");
    }

    pub(super) async fn try_claim(&self) -> bool {
        let mut connection = self.db.acquire().await.expect("claim connection");
        let mut tx = connection.begin().await.expect("claim transaction");
        let capacity = afd_runner::capacity::lock(&mut tx, &self.runner)
            .await
            .expect("lock host")
            .expect("isolated host exists");
        let accepted = capacity.can_claim();
        if accepted {
            self.authoring(&mut tx, clock::after(capacity.now(), LIVE_FOR))
                .await;
        }
        tx.commit().await.expect("commit the guarded claim");
        accepted
    }

    /// Takes the host row's lock on a connection of its own, the way a
    /// concurrent claimant on another replica holds it.
    pub(super) async fn lock_host(&self, tx: &mut Transaction<'_, Postgres>) {
        sqlx::query("SELECT 1 FROM runner.runners WHERE id = $1::uuid FOR UPDATE")
            .bind(self.runner.as_str())
            .execute(&mut **tx)
            .await
            .expect("the competing claimant locks the host");
    }

    pub(super) async fn cleanup(self) {
        let mut connection = self.owner.acquire().await.expect("cleanup connection");
        // Attempts outlive their runner by design, so a test removes the ones
        // it seeded itself rather than leaving them in the lane.
        sqlx::query("DELETE FROM authoring.attempts WHERE runner_id = $1::uuid")
            .bind(self.runner.as_str())
            .execute(&mut *connection)
            .await
            .expect("remove only this test's attempts");
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
            .bind(&self.tenant)
            .execute(&mut *connection)
            .await
            .expect("remove only this test's rows");
        sqlx::query("DELETE FROM runner.runners WHERE id = $1::uuid")
            .bind(self.runner.as_str())
            .execute(&mut *connection)
            .await
            .expect("remove only this test's host");
        drop(connection);
        self.db.close().await;
        self.owner.close().await;
        self.lane.cleanup().await;
    }
}
