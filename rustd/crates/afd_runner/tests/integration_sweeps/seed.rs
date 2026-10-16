//! The rows [`Fixture::seed`] writes: the stale, draining, cordoned and
//! unknown-state runners, the fleets and leases they hold, and the aged event
//! retention retires.
//!
//! Split from [`super`] at the line between writing the fixture and reading it
//! back; the test and its assertions stay there.

use afd_core::clock;
use afd_db::test_util::FleetSeed;
use afd_state::sql::FLEET_STATUS_ACTIVE;

use super::{Fixture, NEVER_SEEN, OLD};

impl Fixture {
    /// The tenant, its workspace, four runners in four states, three fleets,
    /// their leases, and one per-work event old enough to retire.
    pub(super) async fn seed(&self) {
        let now = clock::now().as_millis();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Sweep test', $2, $2) \
               RETURNING id \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $3::uuid, id, 'sweep', 'test', $2 FROM tenant",
        )
        .bind(&self.tenant)
        .bind(now)
        .bind(&self.workspace)
        .execute(&mut *connection)
        .await
        .expect("the tenant and workspace seed");
        self.seed_runners(&mut connection, now).await;
        self.seed_fleets(&mut connection, now).await;
        self.seed_events_and_leases(&mut connection, now).await;
    }

    async fn seed_runners(&self, connection: &mut sqlx::PgConnection, now: i64) {
        sqlx::query(
            "INSERT INTO fleet.runners \
               (id, host_id, token_hash, sandbox_tier, admin_state, labels, \
                network_policy, registry_allowlist, last_seen_at, created_at, updated_at) VALUES \
             ($1::uuid, $1, $1, 'dev_none', 'active', '[]', 'deny_all_egress', '[]', $5, $5, $5), \
             ($2::uuid, $2, $2, 'dev_none', 'draining', '[]', 'deny_all_egress', '[]', $6, $5, $5), \
             ($3::uuid, $3, $3, 'dev_none', 'cordoned', '[]', 'deny_all_egress', '[]', $6, $5, $5), \
             ($4::uuid, $4, $4, 'dev_none', 'future_state', '[]', 'deny_all_egress', '[]', $6, $5, $5)",
        )
        .bind(&self.stale)
        .bind(&self.draining)
        .bind(&self.cordoned)
        .bind(&self.unknown)
        .bind(OLD)
        .bind(NEVER_SEEN.max(now))
        .execute(connection)
        .await
        .expect("the runner rows seed");
    }

    async fn seed_fleets(&self, connection: &mut sqlx::PgConnection, now: i64) {
        for (index, fleet) in [&self.stale_fleet, &self.cordoned_fleet, &self.unknown_fleet]
            .into_iter()
            .enumerate()
        {
            let name = format!("sweep-{index}");
            FleetSeed::new(fleet, &self.workspace, &self.tenant, FLEET_STATUS_ACTIVE)
                .name(&name)
                .at(now)
                .insert(&mut *connection)
                .await
                .expect("the fleet row seeds");
        }
    }

    async fn seed_events_and_leases(&self, connection: &mut sqlx::PgConnection, now: i64) {
        let rows = [
            (
                &self.stale,
                &self.stale_fleet,
                &self.stale_lease,
                &self.stale_event,
                "active",
            ),
            (
                &self.cordoned,
                &self.cordoned_fleet,
                &self.cordoned_lease,
                &self.cordoned_event,
                "active",
            ),
            (
                &self.unknown,
                &self.unknown_fleet,
                &self.unknown_lease,
                &self.unknown_event,
                "active",
            ),
            (
                &self.stale,
                &self.stale_fleet,
                &self.reported_lease,
                &self.reported_event,
                "reported",
            ),
        ];
        for (runner, fleet, lease, event, status) in rows {
            self.seed_lease(
                connection,
                LeaseSeed {
                    runner,
                    fleet,
                    lease,
                    event,
                    status,
                    now,
                },
            )
            .await;
        }
        sqlx::query(
            "INSERT INTO fleet.runner_events \
               (id, runner_id, event_type, metadata, created_at) \
             VALUES ($1::uuid, $2::uuid, 'lease_acquired', '{}', $3)",
        )
        .bind(&self.old_event)
        .bind(&self.stale)
        .bind(OLD)
        .execute(connection)
        .await
        .expect("the aged per-work event seeds");
    }

    async fn seed_lease(&self, connection: &mut sqlx::PgConnection, row: LeaseSeed<'_>) {
        sqlx::query(
            "WITH event AS ( \
               INSERT INTO core.fleet_events \
               (fleet_id, workspace_id, event_id, actor, event_type, status, request_json, created_at, updated_at) \
               VALUES ($1::uuid, $2::uuid, $3, 'test', 'chat', 'received', '{}', $4, $4) \
               RETURNING event_id \
             ) \
             INSERT INTO fleet.runner_leases \
               (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor, event_type, \
                event_created_at, posture, provider, model, metered_input_tokens, metered_cached_tokens, \
                metered_output_tokens, last_metered_at, fencing_token, lease_expires_at, status, created_at, updated_at, \
                receipt) \
             SELECT $5::uuid, $6::uuid, $1::uuid, $2::uuid, $7::uuid, event_id, \
                    'test', 'chat', $4, 'platform', 'test', 'test', 0, 0, 0, $4, \
                    1, $4, $8, $4, $4, event_id FROM event",
        )
        .bind(row.fleet)
        .bind(&self.workspace)
        .bind(row.event)
        .bind(OLD)
        .bind(row.lease)
        .bind(row.runner)
        .bind(&self.tenant)
        .bind(row.status)
        .execute(&mut *connection)
        .await
        .expect("the event and lease seed");
        if row.status == "active" {
            sqlx::query(
                "INSERT INTO fleet.runner_affinity \
                   (fleet_id, last_runner_id, fencing_seq, leased_until, metered_input_tokens, \
                    metered_cached_tokens, metered_output_tokens, last_metered_at, created_at, updated_at) \
                 VALUES ($1::uuid, $2::uuid, 1, $3, 0, 0, 0, $4, $4, $4)",
            )
            .bind(row.fleet)
            .bind(row.runner)
            .bind(row.now.saturating_add(60_000))
            .bind(OLD)
            .execute(connection)
            .await
            .expect("the affinity slot seeds");
        }
    }
}

struct LeaseSeed<'a> {
    runner: &'a str,
    fleet: &'a str,
    lease: &'a str,
    event: &'a str,
    status: &'a str,
    now: i64,
}
