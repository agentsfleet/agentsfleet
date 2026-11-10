//! The lane's Postgres and NATS, and the row chain a schedule hangs from.
//!
//! `fleet.schedules.fleet_id` references `fleet.fleets`, which references a
//! workspace and a tenant, so no schedule test can seed a single row without
//! first seeding all three. That chain lives here rather than in each suite,
//! because getting it wrong fails with a constraint violation naming a table
//! the test never mentions.
//!
//! # Isolation, and what replaces it
//!
//! Postgres is the SHARED lane database — `TestDatabase::shared`, never one per
//! test, for the reason `make/test-integration-rustd.mk` states at length. What
//! replaces isolation is that every fixture mints its OWN workspace and fleet
//! identifiers, so two suites running in parallel address disjoint rows. Every
//! assertion here is scoped to this lane's own fleet for the same reason: a
//! count over the whole table would race whatever else the lane is running.

#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
// `allow`, not `expect`: this module is compiled once per test binary and each
// uses a different subset, so whether the lint fires depends on which binary is
// building. An `expect` is an error in whichever binary happens to use all of
// it, which makes the attribute a tripwire on test layout rather than on code.
#![allow(
    dead_code,
    reason = "test support: shared by two test binaries, each using a subset"
)]

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_cron::{NewSchedule, Refused, Schedule, Schedules, Source};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use jiff::Timestamp;

/// The instant the seeded rows are stamped with, and the one a create mints
/// its id from. Postgres stamps the schedules themselves.
pub(crate) const SEED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The status the fixture fleet is seeded live under.
const FLEET_ACTIVE: &str = "active";

/// A tenant, workspace and fleet nothing else in the suite addresses.
pub(crate) struct CronLane {
    lane: TestDatabase,
    pub(crate) database: Db,
    /// The lane's NATS, which every ledger this lane builds publishes on.
    bus: afd_bus::Handle,
    pub(crate) store: Schedules,
    pub(crate) tenant: Uuid7,
    pub(crate) workspace: Uuid7,
    pub(crate) fleet: Uuid7,
}

impl CronLane {
    /// Opens Postgres and NATS, and seeds the row chain a schedule needs.
    pub(crate) async fn open() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let bus = afd_bus::test_util::lane()
            .await
            .expect("the lane's NATS: run through make test-integration-rustd");
        let seeded = Self {
            lane,
            store: Schedules::new(database.clone(), Entropy::new()),
            database,
            bus,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        seeded.seed_fleet().await;
        seeded
    }

    /// This lane's fleet, as the store takes it.
    pub(crate) fn fleet_id(&self) -> Uuid7 {
        self.fleet.clone()
    }

    /// This lane's workspace, as the store takes it.
    pub(crate) fn workspace_id(&self) -> Uuid7 {
        self.workspace.clone()
    }

    /// A fresh syncer token, which is just a minted identifier.
    pub(crate) fn token() -> Uuid7 {
        mint_id()
    }

    /// Creates one schedule on this lane's fleet, expecting it to be admitted.
    ///
    /// The row comes back HELD by the token that created it, which is the state
    /// `create` actually leaves — the creating request holds the fence until it
    /// has pushed. A case that wants an unheld row wants [`Self::settled`].
    pub(crate) async fn create(&self, source_key: &str, cron: &str) -> Schedule {
        self.create_with(source_key, cron, &Self::token()).await
    }

    /// A schedule in the state one SITS in: registered upstream, fence released.
    ///
    /// Every fence case starts here rather than from `create`, and the reason is
    /// worth naming because it cost a full lane run to find: a freshly created
    /// schedule is already claimed by its creator, with a live lease. "Claim an
    /// unheld row" is not reachable from a bare create at the same instant, so a
    /// suite that tried it was not testing a contended claim — it was testing
    /// the born-held state and reading the answer as a failure.
    pub(crate) async fn settled(&self, source_key: &str, cron: &str) -> Schedule {
        let token = Self::token();
        let created = self.create_with(source_key, cron, &token).await;
        self.store
            .finalize_synced(&created, &token, None)
            .await
            .expect("the lane's Postgres must answer")
            .expect("the creator's own finalize must land")
    }

    /// The create every helper above goes through, under a caller's own token.
    pub(crate) async fn create_with(
        &self,
        source_key: &str,
        cron: &str,
        token: &Uuid7,
    ) -> Schedule {
        self.try_create_with(source_key, cron, token)
            .await
            .expect("the fixture's own create must be admitted")
    }

    /// The same, handing back the refusal where there is one.
    pub(crate) async fn try_create(
        &self,
        source_key: &str,
        cron: &str,
    ) -> Result<Schedule, Refused> {
        self.try_create_with(source_key, cron, &Self::token()).await
    }

    async fn try_create_with(
        &self,
        source_key: &str,
        cron: &str,
        token: &Uuid7,
    ) -> Result<Schedule, Refused> {
        let fleet = self.fleet_id();
        self.store
            .create(
                &self.workspace_id(),
                NewSchedule {
                    fleet: &fleet,
                    source: Source::Api,
                    provider_schedule_id: Some(source_key),
                    cron,
                    timezone: "UTC",
                    message: "run the nightly repair",
                    once: false,
                },
                token,
                SEED_AT,
            )
            .await
            .expect("the lane's Postgres must answer")
    }

    /// The admission ledger every fire in this suite writes through.
    ///
    /// Built from the lane's own pool and its bus, so a test names what it is
    /// exercising rather than restating how a ledger is assembled.
    pub(crate) fn admissions(&self) -> afd_admission::Admissions {
        afd_admission::Admissions::for_tests(self.database.clone(), self.bus.clone())
    }

    /// One connection on the lane, for the direct reads a case needs.
    pub(crate) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.database
            .acquire()
            .await
            .expect("the lane's Postgres must answer")
    }

    /// Pushes a claim's lease into the past, as an abandoned syncer leaves it.
    ///
    /// A test cannot wait out `SYNC_LEASE` — thirty seconds per case would be
    /// the slowest suite in the lane — and sleeping would prove only that the
    /// clock advances. Moving the stored deadline a millisecond behind the
    /// database's clock is the same state a died syncer leaves behind, reached
    /// in one statement.
    pub(crate) async fn expire_lease(&self, schedule: &Uuid7) {
        let mut connection = self.connection().await;
        sqlx::query(
            "UPDATE fleet.schedules \
             SET sync_lease_expires_at = clock_timestamp() - interval '1 millisecond' \
             WHERE id = $1",
        )
        .bind(schedule)
        .execute(&mut *connection)
        .await
        .expect("expiring a lease");
    }

    /// Moves a `once` schedule's instant to a minute before the row's last
    /// stamp, as a sync that ran past its minute sees it.
    ///
    /// Relative to the row's own `updated_at` rather than to a wall clock, so
    /// the case is decided by the same comparison the reconciler makes.
    pub(crate) async fn backdate_fire_at(&self, schedule: &Uuid7) {
        let mut connection = self.connection().await;
        sqlx::query(
            "UPDATE fleet.schedules SET fire_at = updated_at - interval '1 minute' \
             WHERE id = $1",
        )
        .bind(schedule)
        .execute(&mut *connection)
        .await
        .expect("backdating a fire instant");
    }

    /// The rows this lane's fleet currently holds.
    pub(crate) async fn count(&self) -> i64 {
        let mut connection = self.connection().await;
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM fleet.schedules WHERE fleet_id = $1")
            .bind(&self.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("counting this fleet's schedules")
    }

    async fn seed_fleet(&self) {
        let mut connection = self.connection().await;
        TenantSeed::new(&self.tenant)
            .name("cron-fixture")
            .at(SEED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a tenant");

        // Named after its own id: `uq_workspaces_tenant_id_name` is unique per
        // tenant, and a constant name would collide.
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .at(SEED_AT)
            .insert(&mut *connection)
            .await
            .expect("seeding a workspace");

        FleetSeed::new(&self.fleet, &self.workspace, &self.tenant, FLEET_ACTIVE)
            .name("Support triage")
            .at(SEED_AT)
            .native_setup()
            .insert(&mut *connection)
            .await
            .expect("seeding a fleet");

        let _ = &self.lane;
    }
}
