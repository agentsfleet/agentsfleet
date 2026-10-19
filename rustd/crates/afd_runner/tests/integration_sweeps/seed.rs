//! The rows [`Fixture::seed`] writes: the stale, draining, cordoned and
//! unknown-status runners, the fleets and leases they hold, and the aged event
//! retention retires.
//!
//! Split from [`super`] at the line between writing the fixture and reading it
//! back; the test and its assertions stay there.

use std::time::Duration;

use afd_core::clock;
use afd_db::test_util::{FleetSeed, RunnerSeed, TenantSeed, WorkspaceSeed};
use afd_state::sql::{FLEET_STATUS_ACTIVE, LEASE_STATUS_ACTIVE, LEASE_STATUS_REPORTED};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::{Fixture, OLD};
use crate::rows::{self, LeaseRow, POLICY};

/// How far ahead an active lease's slot is held.
const SLOT_HELD_FOR: Duration = Duration::from_mins(1);

/// A status this daemon does not model, which the pass must skip, not guess.
const UNMODELLED_STATUS: &str = "future_state";

impl Fixture {
    /// The tenant, its workspace, four runners in four states, three fleets,
    /// their leases, and one per-work event old enough to retire.
    pub(super) async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(&self.tenant)
            .name("Sweep test")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&self.workspace, &self.tenant)
            .name("sweep")
            .insert(&mut *connection)
            .await
            .expect("the workspace seeds");
        self.seed_runners(&mut connection).await;
        self.seed_fleets(&mut connection).await;
        self.seed_events_and_leases(&mut connection).await;
    }

    /// The stale runner last beat [`OLD`]; the other three beat just now, so
    /// only their status brings them into the pass.
    async fn seed_runners(&self, connection: &mut sqlx::PgConnection) {
        let rows = [
            (
                &self.stale,
                afd_runner::sql::RUNNER_STATUS_ACTIVE,
                Some(OLD),
            ),
            (
                &self.draining,
                afd_runner::sql::RUNNER_STATUS_DRAINING,
                None,
            ),
            (&self.cordoned, "cordoned", None),
            (&self.unknown, UNMODELLED_STATUS, None),
        ];
        for (id, status, seen) in rows {
            let mut seed = RunnerSeed::new(id, status, POLICY).at(OLD);
            if let Some(instant) = seen {
                seed = seed.seen(instant);
            }
            seed.insert(&mut *connection)
                .await
                .expect("the runner row seeds");
        }
    }

    async fn seed_fleets(&self, connection: &mut sqlx::PgConnection) {
        for (index, fleet) in [&self.stale_fleet, &self.cordoned_fleet, &self.unknown_fleet]
            .into_iter()
            .enumerate()
        {
            let name = format!("sweep-{index}");
            FleetSeed::new(fleet, &self.workspace, &self.tenant, FLEET_STATUS_ACTIVE)
                .name(&name)
                .insert(&mut *connection)
                .await
                .expect("the fleet row seeds");
        }
    }

    async fn seed_events_and_leases(&self, connection: &mut sqlx::PgConnection) {
        let leases = [
            (
                &self.stale,
                &self.stale_fleet,
                &self.stale_lease,
                LEASE_STATUS_ACTIVE,
            ),
            (
                &self.cordoned,
                &self.cordoned_fleet,
                &self.cordoned_lease,
                LEASE_STATUS_ACTIVE,
            ),
            (
                &self.unknown,
                &self.unknown_fleet,
                &self.unknown_lease,
                LEASE_STATUS_ACTIVE,
            ),
            (
                &self.stale,
                &self.stale_fleet,
                &self.reported_lease,
                LEASE_STATUS_REPORTED,
            ),
        ];
        let held_until = clock::after(clock::now(), SLOT_HELD_FOR);
        for (runner, fleet, lease, status) in leases {
            rows::lease(
                connection,
                LeaseRow {
                    id: lease,
                    runner,
                    fleet,
                    workspace: &self.workspace,
                    tenant: &self.tenant,
                    status,
                    expires_at: OLD,
                    stamped_at: Some(OLD),
                },
            )
            .await;
            if status == LEASE_STATUS_ACTIVE {
                rows::slot(connection, (fleet, runner), held_until, None, Some(OLD)).await;
            }
        }
        self.seed_aged_event(connection, OLD).await;
    }

    async fn seed_aged_event(&self, connection: &mut sqlx::PgConnection, at: Timestamp) {
        sqlx::query(
            "INSERT INTO runner.events (id, runner_id, event_type, metadata, created_at) \
             VALUES ($1::uuid, $2::uuid, 'lease_acquired', '{}', $3::timestamptz)",
        )
        .bind(&self.old_event)
        .bind(&self.stale)
        .bind(at.to_sqlx())
        .execute(connection)
        .await
        .expect("the aged per-work event seeds");
    }
}
