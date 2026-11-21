//! A setup run whose frame never left the daemon is released: its draft turn
//! is assigned again at once, at the next fence, without waiting for a lapse.

use afd_state::sql::RUN_STATUS_ACTIVE;

use super::Case;

// Value: protects=a draft turn whose run never reached its runner is rerun at once;
// fails_when=the release misses a draft's thread, keyed by the draft, or leaves its run
// active; why_new=the gateway released nothing before; seam=the release statement, then
// the production draft assignment over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unsent_setup_run_is_released_and_its_turn_assigned_again() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    let released = case
        .running
        .fixtures
        .runs()
        .release_unsent(&first.run_id(), &case.running.runner, first.fence)
        .await
        .expect("the release reaches the datastore");
    assert!(released, "the draft run is released");

    let again = case.take_on_host().await;
    assert_eq!(again.turn, first.turn, "the same turn, assigned again");
    assert_eq!(again.fence, first.fence + 1, "at the next fence");
    assert_eq!(
        case.runs().await,
        [(2, RUN_STATUS_ACTIVE.to_owned())],
        "the never-sent run is gone, and its rerun holds the turn"
    );
    case.cleanup().await;
}
