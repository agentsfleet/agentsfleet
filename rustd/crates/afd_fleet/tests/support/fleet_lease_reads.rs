//! The lease-side reads and the row-chain a claim needs to exist.
//!
//! `fleet_fixtures.rs` is named for `fleet.runners` and reads that table; these
//! touch `fleet.runner_leases`, `fleet.runner_affinity`, and the
//! tenant → workspace → fleet chain underneath them. Different table family,
//! different file — and it keeps both under the length cap without either
//! becoming a grab bag.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use afd_db::test_util::FleetSeed;
use sqlx::{AssertSqlSafe, Row as _};

use crate::support::Fixtures;

/// The native setup a seeded fleet is saved with: no trigger, no tool, a
/// dollar a day. Suites that need another install their own over it.
const SEEDED_NATIVE_CONFIG: &str = r#"{"name":"seeded","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// The `fleet.runner_affinity` column a claim's expiry lives in.
pub(crate) const COLUMN_LEASED_UNTIL: &str = "leased_until";

/// The `fleet.runner_affinity` sticky hint: which runner last leased the fleet.
pub(crate) const COLUMN_LAST_RUNNER: &str = "last_runner_id";

/// The `fleet.runner_affinity` metering column a fresh lease resets.
pub(crate) const COLUMN_METERED_INPUT: &str = "metered_input_tokens";

/// The `fleet.runner_affinity` column the runtime meter's last advance lives in.
pub(crate) const COLUMN_LAST_METERED: &str = "last_metered_at";

impl Fixtures {
    /// One column of a `fleet.runner_leases` row, as text.
    pub(crate) async fn lease_column(&self, lease: &str, column: &str) -> Option<String> {
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.runner_leases WHERE id = $1::uuid"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(lease)
            .fetch_optional(&mut *connection)
            .await
            .expect("the lease read must run")
            .map(|row| row.try_get(0).expect("the column must be readable as text"))
    }

    /// Expires one lease in place, as the liveness sweep would once its holder
    /// stopped renewing.
    pub(crate) async fn expire_lease(&self, lease: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE fleet.runner_leases SET status = $2::text WHERE id = $1::uuid")
            .bind(lease)
            .bind("expired")
            .execute(&mut *connection)
            .await
            .expect("the lease write must run");
    }

    /// Removes one lease row, as the retention sweep does once it is old
    /// enough (`afd_runner`'s `DELETE FROM fleet.runner_leases`).
    ///
    /// The race it stands in for is narrow and real: a report loads its lease,
    /// the sweep removes the row, and the claim then matches nothing for a
    /// reason that is neither a lost fence nor a settled lease.
    pub(crate) async fn delete_lease(&self, lease: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("DELETE FROM fleet.runner_leases WHERE id = $1::uuid")
            .bind(lease)
            .execute(&mut *connection)
            .await
            .expect("the lease delete must run");
    }

    /// Stands a metering cursor up mid-slice, as a dying holder would leave it.
    pub(crate) async fn set_metered_input(&self, fleet: &str, tokens: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "UPDATE fleet.runner_affinity SET metered_input_tokens = $2 WHERE fleet_id = $1::uuid",
        )
        .bind(fleet)
        .bind(tokens)
        .execute(&mut *connection)
        .await
        .expect("the meter update must run");
    }

    /// Seeds the tenant → workspace → fleet chain one affinity claim needs.
    ///
    /// `fleet.runner_affinity.fleet_id` is a foreign key to `core.fleets`, and
    /// a fleet carries its workspace's tenant through a COMPOSITE key — so a
    /// claim cannot be tested without all three rows existing and agreeing.
    /// Seeded through plain statements rather than a store verb because no
    /// store verb in this crate creates a fleet: that is the tenant plane's
    /// job (M178), and inventing one here to serve a test would put a
    /// write-path this milestone does not own into the shipping crate.
    ///
    /// Every id is a caller-supplied v7 spelling: the tables CHECK the version
    /// nibble, so a random UUID would be refused by the schema rather than by
    /// the code under test.
    /// Seeds a fleet placeable only by a runner carrying `tag`.
    ///
    /// The tag is what keeps the lease suites apart on one shared database.
    /// `Leases::select` has no workspace or tenant in it — it peeks the GLOBAL
    /// readiness set and filters candidates by `required_tags <@ labels` — so a
    /// fleet seeded with no tag is placeable by every other test's runner, and
    /// two suites polling at once trade fleets. Minting identifiers cannot fix
    /// that: the collision is over the candidate SET, not over the names in it.
    /// Isolating through the production filter costs one array element and
    /// leaves the assignment pass under test rather than around it.
    pub(crate) async fn seed_fleet(
        &self,
        fleet: &str,
        workspace: &str,
        tenant: &str,
        tag: &str,
        now: i64,
    ) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at)
             VALUES ($1::uuid, $2, $3, $3)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(tenant)
        .bind("fixture-tenant")
        .bind(now)
        .execute(&mut *connection)
        .await
        .expect("the tenant row must insert");

        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_at)
             VALUES ($1::uuid, $2::uuid, $3, $4)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(workspace)
        .bind(tenant)
        .bind("fixture-workspace")
        .bind(now)
        .execute(&mut *connection)
        .await
        .expect("the workspace row must insert");

        FleetSeed::new(
            fleet,
            workspace,
            tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        // The fleet's own id as its NAME. `uq_fleets_workspace_id_name` is
        // unique per workspace — so a constant name let one workspace hold
        // exactly one fixture fleet. The fairness suite seeds twenty in one
        // workspace and failed on the second, reporting a constraint no test
        // mentions (ISO-1: mint every identifier a test writes).
        .name(fleet)
        .tags(&[tag])
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("the fleet row must insert");
        drop(connection);
        // Every saved fleet carries the setup revision Save selected, and
        // admission refuses a fleet whose revision was written under other
        // policy semantics -- or that has none. A fixture fleet without one
        // is a fleet production cannot hold, so it gets the revision too.
        self.native_config(fleet, SEEDED_NATIVE_CONFIG).await;
    }

    /// One column of a `fleet.runner_affinity` row, as text; `None` for no row
    /// or a NULL.
    pub(crate) async fn affinity_column(&self, fleet: &str, column: &str) -> Option<String> {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.runner_affinity WHERE fleet_id = $1::uuid"
        ));
        sqlx::query(statement)
            .bind(fleet)
            .fetch_optional(&mut *connection)
            .await
            .expect("the affinity read must run")
            .and_then(|row| row.try_get(0).expect("the column must be readable as text"))
    }

    /// Makes `runner` the fleet's last holder, its claim long expired, so the
    /// candidate scan sorts the fleet sticky for that runner and nothing holds
    /// its slot.
    pub(crate) async fn make_sticky(&self, fleet: &str, runner: &str, at: i64) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO fleet.runner_affinity
               (fleet_id, last_runner_id, fencing_seq, leased_until,
                metered_input_tokens, metered_cached_tokens, metered_output_tokens,
                last_metered_at, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, 1, 0, 0, 0, 0, $3, $3, $3)",
        )
        .bind(fleet)
        .bind(runner)
        .bind(at)
        .execute(&mut *connection)
        .await
        .expect("the affinity row must insert");
    }
}

impl Fixtures {
    /// One admission row's receipt, or `None` when the queue never confirmed it.
    ///
    /// Addressed the way the production stamp addresses it — the fleet plus the
    /// logical event id's two integers, parsed by the ledger's own
    /// `logical_parts` — so a suite cannot drift from the key the real
    /// statement uses.
    pub(crate) async fn admission_receipt(&self, fleet: &str, event_id: &str) -> Option<String> {
        self.admission_column(fleet, event_id, "receipt").await
    }

    /// When a runner was handed this admission, or `None` when none has been.
    ///
    /// Read as text and parsed back, so the assertion is about the column
    /// holding a value at all rather than about what this suite would have
    /// decoded a bigint into.
    pub(crate) async fn admission_delivered_at(&self, fleet: &str, event_id: &str) -> Option<i64> {
        self.admission_column(fleet, event_id, "delivered_at")
            .await
            .map(|stamp| stamp.parse().expect("delivered_at is a bigint"))
    }

    /// How many times the replay sweeper has re-appended this admission.
    pub(crate) async fn admission_replays(&self, fleet: &str, event_id: &str) -> i64 {
        self.admission_column(fleet, event_id, "replay_count")
            .await
            .expect("replay_count is NOT NULL")
            .parse()
            .expect("replay_count is a bigint")
    }

    /// How many admission rows one fleet holds.
    ///
    /// Fleet-scoped on purpose: the ledger is deployment-wide and a total would
    /// count whatever a sibling suite admitted in parallel (ISO-1).
    pub(crate) async fn admissions_for(&self, fleet: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("SELECT count(*) FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger answers")
            .try_get(0)
            .expect("count answers a bigint")
    }

    /// One nullable column of the admission row `event_id` names.
    async fn admission_column(&self, fleet: &str, event_id: &str, column: &str) -> Option<String> {
        let (created_at, seq) =
            afd_admission::logical_parts(event_id).expect("the ledger minted this id");
        let statement = AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_admissions
             WHERE fleet_id = $1::uuid AND created_at = $2 AND seq = $3"
        ));
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(statement)
            .bind(fleet)
            .bind(created_at)
            .bind(seq)
            .fetch_one(&mut *connection)
            .await
            .expect("the admitted row must exist")
            .try_get(0)
            .expect("the column must be readable as text")
    }
}

/// Reads the database clock, the one issue, renewal and the host census read.
const DATABASE_NOW: &str = const_format::concatcp!("SELECT ", afd_db::statement::NOW_MS);

/// Moves a slot's deadline to just behind the database clock.
const LAPSE_SLOT: &str = const_format::concatcp!(
    "UPDATE fleet.runner_affinity SET leased_until = ",
    afd_db::statement::NOW_MS,
    " - 1 WHERE fleet_id = $1::uuid"
);

/// Moves a lease's deadline to just behind the database clock.
const LAPSE_LEASE: &str = const_format::concatcp!(
    "UPDATE fleet.runner_leases SET lease_expires_at = ",
    afd_db::statement::NOW_MS,
    " - 1 WHERE id = $1::uuid"
);

impl Fixtures {
    /// The database's clock now.
    ///
    /// Issue, renewal and the host census read this clock rather than the
    /// caller's, so an expectation about the deadlines and stamps they write is
    /// stated against it.
    pub(crate) async fn database_now(&self) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar(DATABASE_NOW)
            .fetch_one(&mut *connection)
            .await
            .expect("the database clock reads")
    }

    /// The instant `fleet`'s runtime meter last advanced to, which issue and
    /// renewal stamp from the database clock. A slice a test reports is
    /// measured from here, as the report's own charge is.
    pub(crate) async fn metered_since(&self, fleet: &str) -> afd_core::clock::UnixMillis {
        let stamp = self
            .affinity_column(fleet, COLUMN_LAST_METERED)
            .await
            .expect("the slot carries a meter stamp");
        afd_core::clock::UnixMillis::from_millis(stamp.parse().expect("an integer stamp"))
    }

    /// Lapses `fleet`'s slot by the database clock: its claim has passed its
    /// deadline, which is what a holder that stopped renewing leaves.
    pub(crate) async fn lapse_slot(&self, fleet: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(LAPSE_SLOT)
            .bind(fleet)
            .execute(&mut *connection)
            .await
            .expect("the slot write must run");
    }

    /// Lapses one lease by the database clock, its status unchanged.
    pub(crate) async fn lapse_lease(&self, lease: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(LAPSE_LEASE)
            .bind(lease)
            .execute(&mut *connection)
            .await
            .expect("the lease write must run");
    }
}
