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

use sqlx::{AssertSqlSafe, Row as _};

use crate::support::Fixtures;

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

        sqlx::query(
            "INSERT INTO core.fleets
               (id, workspace_id, tenant_id, name, source_markdown, config_json,
                status, created_at, updated_at, required_tags)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8, $9)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(fleet)
        .bind(workspace)
        .bind(tenant)
        // The fleet's own id as its NAME. `uq_fleets_workspace_id_name` is
        // unique per workspace, and the `ON CONFLICT (id)` arm cannot see a
        // name collision — so a constant name let one workspace hold exactly
        // one fixture fleet. The fairness suite seeds twenty in one workspace
        // and failed on the second, reporting a constraint no test mentions
        // (ISO-1: mint every identifier a test writes).
        .bind(fleet)
        .bind("# fixture")
        .bind("{}")
        .bind("active")
        .bind(now)
        .bind(vec![tag.to_owned()])
        .execute(&mut *connection)
        .await
        .expect("the fleet row must insert");
    }

    /// One column of a `fleet.runner_affinity` row, as text.
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
            .map(|row| row.try_get(0).expect("the column must be readable as text"))
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
