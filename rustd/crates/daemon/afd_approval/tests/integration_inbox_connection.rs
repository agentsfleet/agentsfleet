//! Approval continuation and retry release their outer connection.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]
use crate::lane::{Lane, Window};
use afd_approval::{Decision, Inbox, Resolution};
use afd_turns::Turns;
const OPERATOR: &str = "human:fixture";
const NOTE: &str = "approved";

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_single_connection_pool_resolves_and_retries_continuations() {
    let lane = Lane::isolated().await;
    let database = afd_db::test_util::TestDatabase::shared();
    let pool = database
        .open(
            afd_db::config::DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await;
    let inbox = Inbox::new(
        pool.clone(),
        lane.bus.clone(),
        Turns::for_tests(pool.clone(), lane.bus.clone()),
    );
    let action = lane.seed_gate(Window::Open).await;
    let first = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        inbox.resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None),
    )
    .await
    .expect("one pooled connection suffices")
    .expect("resolution");
    let resumed = match first {
        Resolution::Resolved(row) => row.continuation_turn_id,
        _ => None,
    }
    .expect("the winning decision records its continuation");
    let retry = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        inbox.resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None),
    )
    .await
    .expect("retry releases its read connection")
    .expect("retry");
    let retried = match retry {
        Resolution::AlreadyResolved(row) => row.continuation_turn_id,
        _ => None,
    }
    .expect("the retry observes the committed continuation");
    assert_eq!(retried, resumed);
    assert_eq!(pool.size(), 1);
    pool.close().await;
}
