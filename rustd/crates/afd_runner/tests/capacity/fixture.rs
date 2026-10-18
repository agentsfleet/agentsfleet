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

use crate::rows::{self, LeaseRow, POLICY};

/// How long a live seeded lease or attempt has left.
pub(super) const LIVE_FOR: Duration = Duration::from_mins(1);

/// How far past its expiry a lapsed seeded lease or attempt is.
pub(super) const LAPSED_BY: Duration = Duration::from_millis(1);

pub(super) struct Fixture {
    pub(super) db: Db,
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
            runner: Uuid7::parse(&mint_id()).expect("fixture runner UUIDv7"),
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
            lane,
        };
        let mut connection = fixture.db.acquire().await.expect("seed connection");
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
            afd_runner::sql::ADMIN_STATE_ACTIVE,
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
            afd_runner::sql::LEASE_STATUS_ACTIVE,
            until,
        )
        .await;
    }

    pub(super) async fn runtime(&self, until: Timestamp) {
        let mut connection = self.db.acquire().await.expect("runtime seed connection");
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
        let mut connection = self.db.acquire().await.expect("cleanup connection");
        // Attempts name the runner with no cascade, so they go first.
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
        self.lane.cleanup().await;
    }
}
