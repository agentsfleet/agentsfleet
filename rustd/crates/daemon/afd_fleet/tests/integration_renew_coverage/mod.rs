//! Renewal coverage decisions that need live rows but no broad lane orchestration.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use af_core::error_code;
use af_wire::report::RenewRequest;

use crate::report_seed::{DEEP_POOL, Held, held};

const ONE_NANO_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1"}}}"#;
const DEEP_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#;

/// The status a kill leaves on the fleet row.
///
/// `Leases::installed` answers `Ok(None)` for it, as for every status that is
/// not `active`. The renewal's ceiling read must not: the cases below kill a
/// fleet while its run holds a lease and renew that lease.
const FLEET_KILLED: &str = "killed";

async fn set_fleet_config(held: &Held, config: &str) {
    held.fixtures.native_config(&held.fleet, config).await;
}

async fn remove_wallet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM billing.wallets WHERE tenant_id = $1")
        .bind(&held.tenant)
        .execute(&mut *connection)
        .await
        .expect("the wallet fixture is removed");
}

async fn exhaust_fleet_budget(held: &Held) {
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(held, ONE_NANO_DAILY_BUDGET).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_lines \
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, \
            event_id, provider, model, posture, requested_at, input_token_count, \
            cached_input_token_count, cache_write_token_count, cache_write_5m_token_count, \
            cache_write_1h_token_count, output_token_count, runtime_ms, unpriced_token_count, \
            charged_nanos, created_at, updated_at) \
         SELECT $1, $2, $1, 0, f.tenant_id, f.workspace_id, f.id, f.name, $3, \
                'anthropic', 'claude-fixture', 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, 2, \
                now(), now() \
           FROM fleet.fleets f WHERE f.id = $4",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(&held.event_id)
    .bind(&held.fleet)
    .execute(&mut *connection)
    .await
    .expect("the fleet spends past the ceiling");
}

async fn make_budget_unreadable(held: &Held) {
    held.fixtures.corrupt_native_config(&held.fleet, "{}").await;
}

async fn kill_fleet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1")
        .bind(&held.fleet)
        .bind(FLEET_KILLED)
        .execute(&mut *connection)
        .await
        .expect("the live fleet is killed");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_wallet_admits_while_exhausted_and_unreadable_budgets_refuse() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = &held.issued.lease_id;

    // The wallet and fleet ceiling are independent gates. Give the latter
    // explicit room so this assertion can prove only the absent-wallet arm;
    // the generic seed's `{}` config is deliberately not a budget fixture.
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    remove_wallet(&held).await;
    plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("an absent wallet remains the documented fail-open case");

    exhaust_fleet_budget(&held).await;
    let exhausted = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("a live ceiling at equality refuses the renewal");
    assert_eq!(exhausted.code(), error_code::RUN_BUDGET_EXCEEDED);

    make_budget_unreadable(&held).await;
    let malformed = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("an unreadable stored ceiling fails closed");
    assert_eq!(malformed.code(), error_code::RUN_BUDGET_EXCEEDED);

    drop(plane);
    held.fixtures.cleanup().await;
}

/// A fleet killed mid-run keeps the ceiling its run was admitted under.
///
/// Refuses FIRST while the fleet is active, against the same breached ceiling,
/// so the refusal after the kill is proved to come from that ceiling and not
/// from the kill itself.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_killed_mid_run_keeps_its_breached_ceiling() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = &held.issued.lease_id;

    exhaust_fleet_budget(&held).await;
    let refused = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("the breached ceiling refuses while the fleet is still active");
    assert_eq!(refused.code(), error_code::RUN_BUDGET_EXCEEDED);

    kill_fleet(&held).await;
    let still_refused = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("a kill does not lift the ceiling of the run in flight");
    assert_eq!(still_refused.code(), error_code::RUN_BUDGET_EXCEEDED);

    drop(plane);
    held.fixtures.cleanup().await;
}

/// A kill never cancels a run that still has room: the lease in flight renews.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_killed_mid_run_with_room_still_renews() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = &held.issued.lease_id;

    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    kill_fleet(&held).await;
    plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("a killed fleet's run with room left keeps renewing");

    drop(plane);
    held.fixtures.cleanup().await;
}

mod catalogue;
