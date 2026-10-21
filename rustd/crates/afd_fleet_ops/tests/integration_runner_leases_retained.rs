//! A settled lease outlives its fleet, and says where an operator can go.
//!
//! `runner.leases` holds no foreign key to the fleet or its event, so deleting
//! the fleet row (which takes its events with it) leaves the lease listed: the
//! fleet and event columns read null, the outcome is no longer provable, and
//! the destination is `fleet_deleted`. A live fleet in another account is
//! `not_permitted`: a name alone never makes a destination.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_api_wire::operator::{LeaseDestination, LeaseOutcome, RunnerLeaseItem};
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_uuid7};
use afd_fleet_ops::RunnerLeaseHistory;
use afd_state::sql::FLEET_STATUS_ACTIVE;

/// One runner, one processed event on the fleet, one reported lease of it and
/// one usage line the lease logged.
/// `$1` tenant · `$2` workspace · `$3` fleet · `$4` runner · `$5` event row ·
/// `$6` lease · `$7` event id · `$8` host · `$9` token hash · `$10` the lines'
/// work type · `$11` the line.
const SEED: &str = r"
WITH runner AS (
    INSERT INTO runner.runners
      (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
       registry_allowlist, worker_count, lease_version, last_seen_at, created_at, updated_at)
    VALUES ($4::uuid, $8, $9, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress',
            '[]'::jsonb, 1, 0, now(), now(), now())
), event AS (
    INSERT INTO fleet.events
      (id, fleet_id, workspace_id, event_id, actor, event_type, status, request_json, wall_ms,
       failure_reason, failure_detail, created_at, updated_at)
    VALUES ($5::uuid, $3::uuid, $2::uuid, $7, 'user:a', 'chat', 'processed', '{}'::jsonb, 900,
            NULL, NULL, now(), now())
), line AS (
    INSERT INTO billing.usage_lines
      (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
       authoring_attempt_id, provider, model, posture, requested_at, input_token_count,
       cached_input_token_count, cache_write_token_count, cache_write_5m_token_count,
       cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id,
       unpriced_token_count, charged_nanos, created_at, updated_at)
    VALUES ($11::uuid, $10, $6::uuid, 0, $1::uuid, $2::uuid, $3::uuid, 'Doomed', $7, NULL,
            'anthropic', 'model-a', 'platform', now(), 10, 2, 0, 0, 0, 3, 0, NULL, 15, 0,
            now(), now())
)
INSERT INTO runner.leases
  (id, runner_id, fleet_id, workspace_id, tenant_id, fleet_event_id, event_created_at,
   posture, provider, model, last_metered_at, fencing_token, lease_expires_at, status, posted_message_count,
   created_at, updated_at)
VALUES ($6::uuid, $4::uuid, $3::uuid, $2::uuid, $1::uuid, $5::uuid, now(), 'platform',
        'anthropic', 'model-a', now(), 1, now(), 'reported', 0, now(), now())
";

/// Deletes the fleet row the way a purge's final statement does.
const DELETE_FLEET: &str = "DELETE FROM fleet.fleets WHERE id = $1::uuid";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_settled_lease_outlives_its_fleet_and_names_its_destination() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let mut connection = database.acquire().await.expect("an API connection");
    let (tenant, workspace, fleet, runner, lease) = (
        mint_uuid7(),
        mint_uuid7(),
        mint_uuid7(),
        mint_uuid7(),
        mint_uuid7(),
    );
    TenantSeed::new(tenant.as_str())
        .name("Retention test")
        .insert(&mut *connection)
        .await
        .expect("the tenant seeds");
    WorkspaceSeed::new(workspace.as_str(), tenant.as_str())
        .name("primary")
        .insert(&mut *connection)
        .await
        .expect("the workspace seeds");
    FleetSeed::new(
        fleet.as_str(),
        workspace.as_str(),
        tenant.as_str(),
        FLEET_STATUS_ACTIVE,
    )
    .name("Doomed")
    .insert(&mut *connection)
    .await
    .expect("the fleet seeds");
    sqlx::query(SEED)
        .bind(tenant.as_str())
        .bind(workspace.as_str())
        .bind(fleet.as_str())
        .bind(runner.as_str())
        .bind(mint_uuid7().as_str())
        .bind(lease.as_str())
        .bind(format!("event-{lease}"))
        .bind(format!("host-{lease}"))
        .bind(format!("hash-{lease}"))
        .bind(afd_billing::sql::work::FLEET_LEASE)
        .bind(mint_uuid7().as_str())
        .execute(&mut *connection)
        .await
        .expect("the runner, event and lease seed");
    let history = RunnerLeaseHistory::new(database.clone());
    let only =
        |page: afd_api_wire::operator::RunnerLeasesResponse<'static>| -> RunnerLeaseItem<'static> {
            assert_eq!(page.total, 1, "the runner holds one lease");
            page.items.into_iter().next().expect("the lease lists")
        };

    let elsewhere = only(
        history
            .list(&mint_uuid7(), &runner, None, None, None, 10)
            .await
            .expect("the history reads"),
    );
    assert_eq!(elsewhere.destination, LeaseDestination::NotPermitted);
    assert_eq!(
        elsewhere.fleet_name.as_deref(),
        Some("Doomed"),
        "a name is listed but does not make a destination"
    );

    sqlx::query(DELETE_FLEET)
        .bind(fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the fleet row deletes");
    let by_id = fleet.to_string();
    let retained = only(
        history
            .list(&tenant, &runner, None, Some(by_id.as_str()), None, 10)
            .await
            .expect("a purged fleet's lease still reads, filtered by its fleet id"),
    );
    assert_eq!(retained.id, lease.as_str());
    assert_eq!(
        (
            retained.metered_input_tokens,
            retained.metered_cached_tokens,
            retained.metered_output_tokens
        ),
        (10, 2, 3),
        "the lease's tokens are what its usage lines record, past its fleet's purge"
    );
    assert_eq!(retained.destination, LeaseDestination::FleetDeleted);
    assert_eq!(retained.fleet_name, None);
    assert_eq!(retained.event_type, None, "the event went with its fleet");
    assert_eq!(retained.actor, None);
    assert_eq!(
        retained.outcome,
        LeaseOutcome::Unknown,
        "a reported lease whose event is gone cannot prove success or failure"
    );

    drop(connection);
    drop(history);
    drop(database);
    lane.cleanup().await;
}
