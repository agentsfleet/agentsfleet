//! A settled run outlives its fleet, and says where an operator can go.
//!
//! `thread.runs` holds no foreign key to the fleet or its event, so deleting
//! the fleet row (which takes its events with it) leaves the run listed: the
//! fleet and event columns read null, the outcome is no longer provable, and
//! the destination is `fleet_deleted`. A live fleet in another account is
//! `not_permitted`: a name alone never makes a destination.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_api_wire::operator::{RunDestination, RunOutcome, RunnerRunItem};
use afd_db::config::DbRole;
use afd_db::test_util::{FleetSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_fleet_ops::RunnerRunHistory;
use afd_state::sql::FLEET_STATUS_ACTIVE;

/// One runner, one processed event on the fleet, one reported run of it and
/// one usage line the run logged.
/// `$1` tenant · `$2` workspace · `$3` fleet · `$4` runner · `$5` run ·
/// `$6` event id · `$7` host · `$8` token hash · `$9` the lines' work type ·
/// `$10` the line.
const SEED: &str = r"
WITH runner AS (
    INSERT INTO runner.runners
      (id, host_id, token_hash, sandbox_tier, status, labels, network_policy,
       registry_allowlist, worker_count, run_version, last_seen_at, created_at, updated_at,
       platform_owned)
    VALUES ($4, $7, $8, 'dev_none', 'active', '[]'::jsonb, 'deny_all_egress',
            '[]'::jsonb, 1, 0, now(), now(), now(), true)
), event AS (
    INSERT INTO thread.turns
      (thread_id, workspace_id, id, actor, kind, status, request_json, wall_ms,
       failure_reason, failure_detail, opened_at, settled_at, created_at, updated_at)
    VALUES ($3, $2, $6, 'user:a', 'chat', 'processed', '{}'::jsonb, 900,
            NULL, NULL, now(), now(), now(), now())
), line AS (
    INSERT INTO billing.usage_lines
      (id, work_type, work_id, seq, kind, tenant_id, workspace_id, fleet_id, fleet_name, turn_id,
       run_id, provider, model, posture, requested_at, input_token_count,
       cached_input_token_count, cache_write_token_count, cache_write_5m_token_count,
       cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id,
       unpriced_token_count, charged_nanos, created_at, updated_at)
    VALUES ($10, $9, $5, 0, 'model', $1, $2, $3, 'Doomed', $6, NULL,
            'anthropic', 'model-a', 'platform', now(), 10, 2, 0, 0, 0, 3, 0, NULL, 15, 0,
            now(), now())
)
INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at, fencing_token, run_expires_at, status, posted_message_count,
   created_at, updated_at)
VALUES ($5, $4, $3, $2, $1, $6, now(), 'platform',
        'anthropic', 'model-a', now(), 1, now(), 'reported', 0, now(), now())
";

/// Deletes the fleet row the way a purge's final statement does.
const DELETE_FLEET: &str = "DELETE FROM fleet.fleets WHERE id = $1";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_settled_run_outlives_its_fleet_and_names_its_destination() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let mut connection = database.acquire().await.expect("an API connection");
    let (tenant, workspace, fleet, runner, run) =
        (mint_id(), mint_id(), mint_id(), mint_id(), mint_id());
    TenantSeed::new(&tenant)
        .name("Retention test")
        .insert(&mut *connection)
        .await
        .expect("the tenant seeds");
    WorkspaceSeed::new(&workspace, &tenant)
        .name("primary")
        .insert(&mut *connection)
        .await
        .expect("the workspace seeds");
    FleetSeed::new(&fleet, &workspace, &tenant, FLEET_STATUS_ACTIVE)
        .name("Doomed")
        .insert(&mut *connection)
        .await
        .expect("the fleet seeds");
    sqlx::query(SEED)
        .bind(&tenant)
        .bind(&workspace)
        .bind(&fleet)
        .bind(&runner)
        .bind(&run)
        .bind(mint_id())
        .bind(format!("host-{run}"))
        .bind(format!("hash-{run}"))
        .bind(afd_billing::sql::work::FLEET_RUN)
        .bind(mint_id())
        .execute(&mut *connection)
        .await
        .expect("the runner, event and run seed");
    let history = RunnerRunHistory::new(database.clone());
    let only =
        |page: afd_api_wire::operator::RunnerRunsResponse<'static>| -> RunnerRunItem<'static> {
            assert_eq!(page.total, 1, "the runner holds one run");
            page.items.into_iter().next().expect("the run lists")
        };

    let elsewhere = only(
        history
            .list(&mint_id(), &runner, None, None, None, 10)
            .await
            .expect("the history reads"),
    );
    assert_eq!(elsewhere.destination, RunDestination::NotPermitted);
    assert_eq!(
        elsewhere.fleet_name.as_deref(),
        Some("Doomed"),
        "a name is listed but does not make a destination"
    );

    sqlx::query(DELETE_FLEET)
        .bind(&fleet)
        .execute(&mut *connection)
        .await
        .expect("the fleet row deletes");
    let by_id = fleet.to_string();
    let retained = only(
        history
            .list(&tenant, &runner, None, Some(by_id.as_str()), None, 10)
            .await
            .expect("a purged fleet's run still reads, filtered by its fleet id"),
    );
    assert_eq!(retained.id, run.as_str());
    assert_eq!(
        (
            retained.metered_input_tokens,
            retained.metered_cached_tokens,
            retained.metered_output_tokens
        ),
        (10, 2, 3),
        "the run's tokens are what its usage lines record, past its fleet's purge"
    );
    assert_eq!(retained.destination, RunDestination::FleetDeleted);
    assert_eq!(retained.fleet_name, None);
    assert_eq!(retained.event_type, None, "the event went with its fleet");
    assert_eq!(retained.actor, None);
    assert_eq!(
        retained.outcome,
        RunOutcome::Unknown,
        "a reported run whose event is gone cannot prove success or failure"
    );

    drop(connection);
    drop(history);
    drop(database);
    lane.cleanup().await;
}
