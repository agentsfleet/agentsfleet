//! The `qstash` bag in the admin workspace's vault is what a call spends and
//! what a fire is checked against, read when it is used.
//!
//! So a rotation is a bag update: the next call spends the new bearer and goes
//! to the new base, and the next fire is checked against the new keys, with no
//! restart between. Real Postgres underneath; a loopback stand-in for the
//! scheduler on top.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

#[path = "support/fire_token.rs"]
mod fire_token;

#[path = "support/stub_scheduler.rs"]
mod stub_scheduler;

#[path = "support/vault_bag.rs"]
mod vault_bag;

use af_core::error_code::INTERNAL_OPERATION_FAILED;
use afd_cron::{Attempts, Scheduler as _};
use afd_vault::bag::BagFault;
use serde_json::{Value, json};

use self::fire_token::{BODY, FireClaims, mint};
use self::stub_scheduler::StubScheduler;
use self::support::CronLane;
use self::vault_bag::{OTHER_KEY, SEALING_KEY, bag, replace, scheduler_over, store, vault_under};

/// The request line a removal of the fixture schedule puts on the wire.
const REMOVAL: &str = "DELETE /schedules/scd_fixture HTTP/1.1";

/// A bag for the suites that only verify, so its base is left to the default.
fn keys_bag(current: &str, next: &str) -> Value {
    json!({"token": "tok", "current_signing_key": current, "next_signing_key": next})
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_replaced_bag_is_the_bearer_and_the_base_of_the_very_next_call() {
    let lane = CronLane::open().await;
    let (mut before, mut after) = (StubScheduler::start(), StubScheduler::start());
    let vault = vault_under(&lane, SEALING_KEY);
    let admin = lane.workspace_id();
    store(
        &vault,
        &admin,
        &bag("tok-1", before.base(), "cur-1", "nxt-1"),
    )
    .await;
    let scheduler = scheduler_over(&vault, &admin);

    before.answer(200);
    scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect("the call goes out under the stored bag");
    replace(
        &vault,
        &admin,
        &bag("tok-2", after.base(), "cur-2", "nxt-2"),
    )
    .await;
    after.answer(200);
    scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect("the next call goes out under the replaced bag");

    assert_eq!(
        before.drained(),
        [REMOVAL],
        "the first call went to the first base"
    );
    assert_eq!(
        after.drained(),
        [REMOVAL],
        "the next call went to the replaced base"
    );
    assert_eq!(
        before.authorizations(),
        [Some("Bearer tok-1".to_owned())],
        "and spent the bearer the bag held then"
    );
    assert_eq!(
        after.authorizations(),
        [Some("Bearer tok-2".to_owned())],
        "no restart stood between the replacement and the call"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_fire_is_checked_against_the_keys_the_bag_holds_now() {
    let lane = CronLane::open().await;
    let vault = vault_under(&lane, SEALING_KEY);
    let admin = lane.workspace_id();
    store(&vault, &admin, &keys_bag("key-current-1", "key-next-1")).await;
    let scheduler = scheduler_over(&vault, &admin);
    let signed_by = |key: &str| mint(&FireClaims::good(), key);

    let fire = scheduler
        .verify(&signed_by("key-current-1"), BODY)
        .await
        .expect("a token signed by the current key is believed");
    assert_eq!(fire.message_id, "msg_fixture_0001");

    replace(&vault, &admin, &keys_bag("key-current-2", "key-next-2")).await;

    let stale = scheduler
        .verify(&signed_by("key-current-1"), BODY)
        .await
        .expect_err("a key the bag no longer holds verifies nothing");
    assert_eq!(stale.refusal_word(), "signature_invalid");
    scheduler
        .verify(&signed_by("key-next-2"), BODY)
        .await
        .expect("the promoted next key is believed at once");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn an_unusable_bag_stops_the_call_before_any_request_leaves() {
    let lane = CronLane::open().await;
    let mut stub = StubScheduler::start();
    let vault = vault_under(&lane, SEALING_KEY);
    let admin = lane.workspace_id();
    let scheduler = scheduler_over(&vault, &admin);

    let nothing_stored = scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("no bag in the admin workspace");
    assert_eq!(nothing_stored.bag_fault(), Some(BagFault::Absent));

    store(
        &vault,
        &admin,
        &json!({"token": "tok", "url": stub.base(), "current_signing_key": "cur"}),
    )
    .await;
    let half_done = scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("a bag with one key is a rotation half done");
    assert_eq!(
        half_done.bag_fault(),
        Some(BagFault::Missing("next_signing_key"))
    );
    assert!(!half_done.is_retryable(), "waiting does not complete a bag");
    assert!(stub.drained().is_empty(), "no request left the process");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres and NATS"]
async fn a_bag_that_will_not_open_is_a_config_fault_and_not_an_outage() {
    let lane = CronLane::open().await;
    let mut stub = StubScheduler::start();
    let admin = lane.workspace_id();
    store(
        &vault_under(&lane, SEALING_KEY),
        &admin,
        &bag("tok", stub.base(), "cur", "nxt"),
    )
    .await;
    let wrong_key = scheduler_over(&vault_under(&lane, OTHER_KEY), &admin);

    for attempts in [Attempts::Once, Attempts::Retried] {
        let unreadable = wrong_key
            .remove("scd_fixture", attempts)
            .await
            .expect_err("a bag sealed under another key does not open");

        assert!(
            !unreadable.is_retryable(),
            "retrying a wrong key is a retry storm, not a repair"
        );
        assert_eq!(
            unreadable.bag_fault(),
            None,
            "no field was read, so none is at fault"
        );
        assert_eq!(unreadable.code(), INTERNAL_OPERATION_FAILED);
        assert!(
            stub.drained().is_empty(),
            "no request left the process: {attempts:?}"
        );
    }
}
