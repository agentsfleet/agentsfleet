//! The lease-side writes and the row chain a claim needs to exist.
//!
//! `fleet_fixtures.rs` reads columns of any row through [`Row`]; these stage
//! `runner.leases`, `runner.fleet_slots`, and the tenant → workspace → fleet
//! chain underneath them, through the shared `afd_db` seeds where one exists.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use afd_db::test_util::{FleetSeed, SetupRevision, TenantSeed, WorkspaceSeed};
use afd_fleet_runtime::definition::{POLICY_SEMANTICS, canonical_digest};
use jiff::Timestamp;

use crate::rows::Row;
use crate::support::Fixtures;

/// The native setup a seeded fleet is saved with: no trigger, no tool, a
/// dollar a day. Suites that need another install their own over it.
const SEEDED_NATIVE_CONFIG: &str = r#"{"name":"seeded","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// The `runner.fleet_slots` column a claim's expiry lives in.
pub(crate) const COLUMN_LEASED_UNTIL: &str = "lease_expires_at";

/// The `runner.fleet_slots` sticky hint: which runner last leased the fleet.
pub(crate) const COLUMN_LAST_RUNNER: &str = "last_runner_id";

/// The `runner.leases` column a lease's runtime meter last advanced to.
pub(crate) const COLUMN_LAST_METERED: &str = "last_metered_at";

/// The runtime cursor of `fleet`'s newest lease.
const NEWEST_LEASE_METERED: &str = "SELECT last_metered_at FROM runner.leases \
     WHERE fleet_id = $1::uuid ORDER BY fencing_token DESC LIMIT 1";

/// Whether the workspace a seed would write already exists.
const WORKSPACE_EXISTS: &str =
    "SELECT EXISTS (SELECT 1 FROM identity.workspaces WHERE id = $1::uuid)";

/// The database clock, rounded as a `TIMESTAMPTZ(3)` column rounds it, so a
/// stamp the code under test stored compares against it exactly.
const DATABASE_NOW: &str = "SELECT clock_timestamp()::timestamptz(3)";

/// Stands a slot up with `runner` as its last holder, its claim long expired.
const MAKE_STICKY: &str = "\
INSERT INTO runner.fleet_slots
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, 1, now() - interval '1 day', NULL, now(), now())";

impl Fixtures {
    /// Expires one lease in place, as the liveness sweep would once its holder
    /// stopped renewing.
    pub(crate) async fn expire_lease(&self, lease: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("UPDATE runner.leases SET status = $2::text WHERE id = $1::uuid")
            .bind(lease)
            .bind(afd_state::sql::LEASE_STATUS_EXPIRED)
            .execute(&mut *connection)
            .await
            .expect("the lease write must run");
    }

    /// Removes one lease row, as the retention sweep does once it is old
    /// enough.
    ///
    /// The race it stands in for is narrow and real: a report loads its lease,
    /// the sweep removes the row, and the claim then matches nothing for a
    /// reason that is neither a lost fence nor a settled lease.
    pub(crate) async fn delete_lease(&self, lease: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("DELETE FROM runner.leases WHERE id = $1::uuid")
            .bind(lease)
            .execute(&mut *connection)
            .await
            .expect("the lease delete must run");
    }

    /// Seeds a fleet placeable only by a runner carrying `tag`, with the
    /// tenant and workspace it belongs to.
    ///
    /// The tag is what keeps the lease suites apart on one shared database.
    /// `Leases::select` peeks the GLOBAL readiness set and filters candidates
    /// by `required_tags <@ labels`, so a fleet seeded with no tag is placeable
    /// by every other test's runner, and two suites polling at once trade
    /// fleets. Isolating through the production filter leaves the assignment
    /// pass under test rather than around it.
    ///
    /// The tenant and workspace are written once, by the first fleet that
    /// names them: a suite seeds many fleets in one workspace.
    pub(crate) async fn seed_fleet(&self, fleet: &str, workspace: &str, tenant: &str, tag: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        let owned: bool = sqlx::query_scalar(WORKSPACE_EXISTS)
            .bind(workspace)
            .fetch_one(&mut *connection)
            .await
            .expect("the workspace probe must run");
        if !owned {
            TenantSeed::new(tenant)
                .insert(&mut *connection)
                .await
                .expect("the tenant row must insert");
            WorkspaceSeed::new(workspace, tenant)
                .insert(&mut *connection)
                .await
                .expect("the workspace row must insert");
        }
        let definition = crate::setup::definition(SEEDED_NATIVE_CONFIG);
        let stored = serde_json::to_string(&definition).expect("Definition JSON");
        let digest = canonical_digest(&definition).expect("canonical digest");
        // Every saved fleet carries the setup revision Save selected, and
        // admission refuses a fleet whose revision was written under other
        // policy semantics, or that has none.
        FleetSeed::new(
            fleet,
            workspace,
            tenant,
            afd_state::sql::FLEET_STATUS_ACTIVE,
        )
        .tags(&[tag])
        .setup(SetupRevision {
            definition: &stored,
            digest: &digest,
            semantics: POLICY_SEMANTICS,
            model_ref: Some(afd_credential::provider::PLATFORM_MODEL_REF),
        })
        .insert(&mut *connection)
        .await
        .expect("the fleet row must insert");
    }

    /// Makes `runner` the fleet's last holder, its claim long expired, so the
    /// candidate scan sorts the fleet sticky for that runner and nothing holds
    /// its slot.
    pub(crate) async fn make_sticky(&self, fleet: &str, runner: &str) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(MAKE_STICKY)
            .bind(fleet)
            .bind(runner)
            .execute(&mut *connection)
            .await
            .expect("the slot row must insert");
    }

    /// How many admission rows one fleet holds.
    ///
    /// Fleet-scoped on purpose: the ledger is deployment-wide and a total would
    /// count whatever a sibling suite admitted in parallel (ISO-1).
    pub(crate) async fn admissions_for(&self, fleet: &str) -> i64 {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid")
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the ledger answers")
    }

    /// The database's clock now.
    ///
    /// Issue, renewal and the host census read this clock rather than the
    /// caller's, so an expectation about the deadlines and stamps they write is
    /// stated against it.
    pub(crate) async fn database_now(&self) -> Timestamp {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        let now: jiff_sqlx::Timestamp = sqlx::query_scalar(DATABASE_NOW)
            .fetch_one(&mut *connection)
            .await
            .expect("the database clock reads");
        now.to_jiff()
    }

    /// Whether `fleet`'s slot is free: its claim's deadline is already behind
    /// the database clock, so the next claim may take it.
    pub(crate) async fn slot_is_free(&self, fleet: &str) -> bool {
        let deadline = self
            .instant(Row::Slot(fleet), COLUMN_LEASED_UNTIL)
            .await
            .expect("the fleet has a slot row");
        deadline < self.database_now().await
    }

    /// Runs `write`, answering its output and the database clock read just
    /// before and just after it: the span a stamp it took from `now()` lies in.
    pub(crate) async fn clock_around<T>(
        &self,
        write: impl std::future::Future<Output = T>,
    ) -> (T, std::ops::RangeInclusive<Timestamp>) {
        let before = self.database_now().await;
        let output = write.await;
        let after = self.database_now().await;
        (output, before..=after)
    }

    /// The instant `fleet`'s newest lease's runtime meter last advanced to,
    /// which issue and renewal stamp from the database clock. A slice a test
    /// reports is measured from here, as the report's own charge is.
    pub(crate) async fn metered_since(&self, fleet: &str) -> Timestamp {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(NEWEST_LEASE_METERED)
            .bind(fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the fleet has a lease with a meter stamp")
            .to_jiff()
    }
}
