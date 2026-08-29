//! Renewal coverage decisions that need live rows but no broad lane orchestration.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::queue;
use crate::report_seed;
use afd_core::error_code;
use afd_wire::report::RenewRequest;

use self::report_seed::{DEEP_POOL, Held, held};

const ONE_NANO_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":0.000000001}}}"#;
const DEEP_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1000}}}"#;

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
