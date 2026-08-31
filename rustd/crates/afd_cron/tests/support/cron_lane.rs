//! The lane's Postgres, and the row chain a schedule hangs from.
//!
//! `core.fleet_schedules.fleet_id` references `core.fleets`, which references a
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
    clippy::panic,
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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_cron::{NewSchedule, Refused, Schedule, Schedules, Source};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

/// The instant the seeded rows are stamped with.
const SEED_MS: i64 = 1_760_000_000_000;

/// The knob `make test-integration-rustd` exports the lane's Redis under.
const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";

/// The knob carrying the lane's CA bundle, where the lane speaks TLS.
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// How long anything crossing a datastore is given before the test fails.
const REQUEST_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// A tenant, workspace and fleet nothing else in the suite addresses.
pub(crate) struct CronLane {
    lane: TestDatabase,
    pub(crate) database: Db,
    pub(crate) store: Schedules,
    pub(crate) tenant: String,
    pub(crate) workspace: String,
    pub(crate) fleet: String,
}

impl CronLane {
    /// Opens Postgres and seeds the row chain a schedule needs.
    pub(crate) async fn open() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let seeded = Self {
            lane,
            store: Schedules::new(database.clone(), Entropy::new()),
            database,
            tenant: mint_id(),
            workspace: mint_id(),
            fleet: mint_id(),
        };
        seeded.seed_fleet().await;
        seeded
    }

    /// This lane's fleet, as the store takes it.
    pub(crate) fn fleet_id(&self) -> Uuid7 {
        Uuid7::parse(&self.fleet).expect("the minted fleet id is canonical")
    }

    /// This lane's workspace, as the store takes it.
    pub(crate) fn workspace_id(&self) -> Uuid7 {
        Uuid7::parse(&self.workspace).expect("the minted workspace id is canonical")
    }

    /// A fresh syncer token, which is just a minted identifier.
    pub(crate) fn token() -> Uuid7 {
        Uuid7::parse(&mint_id()).expect("a minted token is canonical")
    }

    /// The instant every case measures leases against.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(SEED_MS)
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
            .finalize_synced(&created, &token, None, Self::now())
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
    ) -> core::result::Result<Schedule, Refused> {
        self.try_create_with(source_key, cron, &Self::token()).await
    }

    async fn try_create_with(
        &self,
        source_key: &str,
        cron: &str,
        token: &Uuid7,
    ) -> core::result::Result<Schedule, Refused> {
        let fleet = self.fleet_id();
        self.store
            .create(
                &self.workspace_id(),
                NewSchedule {
                    fleet: &fleet,
                    source: Source::Api,
                    source_key,
                    cron,
                    timezone: "UTC",
                    message: "run the nightly repair",
                },
                token,
                Self::now(),
            )
            .await
            .expect("the lane's Postgres must answer")
    }

    /// The lane's Redis, opened on demand.
    ///
    /// Not held on the struct: only the fire suite needs a queue, and opening
    /// one for every store and fence case would make a datastore lane out of a
    /// Postgres lane for no gain.
    pub(crate) async fn queue() -> Redis {
        afd_redis::test_util::connect_live(&Self::redis())
            .await
            .expect("the lane's Redis must be reachable")
    }

    /// The lane's Redis configuration.
    pub(crate) fn redis() -> RedisConfig {
        let url = std::env::var(REDIS_URL_KNOB).unwrap_or_else(|_unset| {
            panic!("{REDIS_URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        RedisConfig::from_url(RedisRole::Default, url)
            .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
            .with_request_timeout(REQUEST_BUDGET)
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
    /// A test cannot wait out `SYNC_LEASE_MS` — thirty seconds per case would
    /// be the slowest suite in the lane — and sleeping would prove only that
    /// the clock advances. Moving the stored deadline is the same state a died
    /// syncer leaves behind, reached in one statement.
    pub(crate) async fn expire_lease(&self, schedule: &Uuid7) {
        let mut connection = self.connection().await;
        sqlx::query("UPDATE core.fleet_schedules SET sync_lease_until = $2 WHERE id = $1::uuid")
            .bind(schedule.as_str())
            .bind(SEED_MS - 1)
            .execute(&mut *connection)
            .await
            .expect("expiring a lease");
    }

    /// The rows this lane's fleet currently holds.
    pub(crate) async fn count(&self) -> i64 {
        let mut connection = self.connection().await;
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id = $1::uuid",
        )
        .bind(self.fleet.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("counting this fleet's schedules")
    }

    async fn seed_fleet(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, 'cron-fixture', $2, $2)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        // The NAME carries the id: `uq_workspaces_tenant_id_name` is unique per
        // tenant, and a constant name would collide where `ON CONFLICT (id)`
        // cannot see it.
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
             VALUES ($1::uuid, $2::uuid, $1::text, 'cron-fixture', $3)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");

        sqlx::query(
            "INSERT INTO core.fleets
               (id, workspace_id, tenant_id, name, source_markdown, config_json,
                status, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, 'cron-fixture-fleet',
                     '# fixture', '{}'::jsonb, 'active', $4, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.fleet.as_str())
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(SEED_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a fleet");

        let _ = &self.lane;
    }
}
