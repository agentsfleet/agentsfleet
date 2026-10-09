//! Renewal coverage decisions that need live rows but no broad lane orchestration.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::queue;
use crate::report_seed;
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_wire::report::RenewRequest;

use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

use self::report_seed::{DEEP_POOL, Held, held, held_in};

const ONE_NANO_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":0.000000001}}}"#;
const DEEP_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1000}}}"#;

/// The status a kill leaves on the fleet row.
///
/// `Leases::installed` answers `Ok(None)` for it, as for every status that is
/// not `active`. The renewal's ceiling read must not: the cases below kill a
/// fleet while its run holds a lease and renew that lease.
const FLEET_KILLED: &str = "killed";

async fn set_fleet_config(held: &Held, config: &str) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid")
        .bind(&held.fleet)
        .bind(config)
        .execute(&mut *connection)
        .await
        .expect("the live fleet config is replaced");
}

async fn remove_wallet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM billing.tenant_wallet WHERE tenant_id = $1::uuid")
        .bind(&held.tenant)
        .execute(&mut *connection)
        .await
        .expect("the wallet fixture is removed");
}

async fn exhaust_fleet_budget(held: &Held) {
    held.fixtures
        .seed_wallet(&held.tenant, DEEP_POOL, held.now.as_millis())
        .await;
    set_fleet_config(held, ONE_NANO_DAILY_BUDGET).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE billing.usage_ledger SET credit_deducted_nanos = 2 WHERE event_id = $1")
        .bind(&held.event_id)
        .execute(&mut *connection)
        .await
        .expect("the scoped event spends past the ceiling");
}

async fn make_budget_unreadable(held: &Held) {
    set_fleet_config(held, "{}").await;
}

async fn kill_fleet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET status = $2 WHERE id = $1::uuid")
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
    let lease_id = held.issued.lease_id.as_str();

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
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
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
    let lease_id = held.issued.lease_id.as_str();

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
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A kill never cancels a run that still has room: the lease in flight renews.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_killed_mid_run_with_room_still_renews() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();

    held.fixtures
        .seed_wallet(&held.tenant, DEEP_POOL, held.now.as_millis())
        .await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    kill_fleet(&held).await;
    plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("a killed fleet's run with room left keeps renewing");

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Tokens a renewal could not price are charged by the next renewal that can.
///
/// Three renewals at one instant, so no runtime accrues and every nano charged
/// is a token's. The first prices 10k input tokens; the second reports 25k
/// while the catalogue is offline; the third prices 40k and must charge the
/// 30k since the first, three times the first slice. Charging the second
/// slice's real counts at run-fee rates would move the token cursor to 25k,
/// and the third would charge only 15k.
///
/// A private database, because taking the catalogue offline breaks the rate
/// read for every test sharing it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_during_a_catalogue_fault_charges_its_tokens_later() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    held.fixtures
        .seed_model_rate(PROVIDER, MODEL, held.now.as_millis())
        .await;
    let reported = |input_tokens| RenewRequest {
        input_tokens,
        ..RenewRequest::default()
    };

    let (_, first) = plane
        .renew(&held.runner, lease_id, reported(10_000), held.now)
        .await
        .expect("a priced renewal");
    assert!(
        !first.is_zero(),
        "the fixture rate prices ten thousand tokens"
    );

    held.fixtures.set_catalogue_readable(false).await;
    let log = Capture::install();
    let (_, unpriced) = plane
        .renew(&held.runner, lease_id, reported(25_000), held.now)
        .await
        .expect("a catalogue fault does not stop the run");
    // The renewal itself says it held the tokens, not only the helper it calls.
    let warned = log.only("renew_tokens_held_for_pricing");
    assert_eq!(warned.field("input_tokens"), Some("25000"));
    assert_eq!(warned.field("lease_id"), Some(lease_id));
    drop(log);
    assert!(
        unpriced.is_zero(),
        "no runtime accrued, and no token was priced while the catalogue was offline"
    );
    held.fixtures.set_catalogue_readable(true).await;

    let (_, later) = plane
        .renew(&held.runner, lease_id, reported(40_000), held.now)
        .await
        .expect("a priced renewal");
    assert_eq!(
        later.as_i64(),
        3 * first.as_i64(),
        "the tokens the fault held are charged with the next priced slice"
    );

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
