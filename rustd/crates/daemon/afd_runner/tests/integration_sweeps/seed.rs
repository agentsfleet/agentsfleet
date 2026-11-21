//! The rows [`Fixture::seed`] writes: the stale, draining, cordoned and
//! unknown-status runners, and the fleets and runs they hold.
//!
//! Split from [`super`] at the line between writing the fixture and reading it
//! back; the test and its assertions stay there.

use std::time::Duration;

use af_core::clock;
use afd_db::test_util::{FleetSeed, RunnerSeed, TenantSeed, WorkspaceSeed};
use afd_state::sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE};

use super::{Fixture, OLD};
use crate::rows::{self, POLICY, RunRow};

/// How far ahead an active run's thread is held.
const THREAD_HELD_FOR: Duration = Duration::from_mins(1);

/// A status this daemon does not model, which the pass must skip, not guess.
const UNMODELLED_STATUS: &str = "future_state";

impl Fixture {
    /// The tenant, its workspace, four runners in four states, three fleets
    /// and their runs.
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
        self.seed_runs(&mut connection).await;
    }

    /// The stale runner last beat [`OLD`]; the other three beat just now, so
    /// only their status brings them into the pass.
    async fn seed_runners(&self, connection: &mut sqlx::PgConnection) {
        let rows = [
            (&self.stale, afd_state::sql::RUNNER_STATUS_ACTIVE, Some(OLD)),
            (&self.draining, afd_state::sql::RUNNER_STATUS_DRAINING, None),
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

    async fn seed_runs(&self, connection: &mut sqlx::PgConnection) {
        let runs = [
            (&self.stale, &self.stale_fleet, &self.stale_run),
            (&self.cordoned, &self.cordoned_fleet, &self.cordoned_run),
            (&self.unknown, &self.unknown_fleet, &self.unknown_run),
        ];
        let held_until = clock::after(clock::now(), THREAD_HELD_FOR);
        for (runner, fleet, run) in runs {
            rows::payload(
                connection,
                RunRow {
                    id: run,
                    runner,
                    fleet,
                    workspace: &self.workspace,
                    tenant: &self.tenant,
                    status: RUN_STATUS_ACTIVE,
                    expires_at: OLD,
                    stamped_at: Some(OLD),
                },
            )
            .await;
            rows::thread(connection, (fleet, runner), held_until, None, Some(OLD)).await;
        }
    }
}
