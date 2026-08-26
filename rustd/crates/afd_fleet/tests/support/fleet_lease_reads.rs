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
    pub(crate) async fn seed_fleet(&self, fleet: &str, workspace: &str, tenant: &str, now: i64) {
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
                status, created_at, updated_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(fleet)
        .bind(workspace)
        .bind(tenant)
        .bind("fixture-fleet")
        .bind("# fixture")
        .bind("{}")
        .bind("active")
        .bind(now)
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
