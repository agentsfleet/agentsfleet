//! A setup run whose runner stops renewing is rerun: its turn is assigned
//! again at the next fence, up to `EVENT_RUNS_MAX` runs, and then fails as a
//! lost turn instead of being retried forever.

use af_core::limits::EVENT_RUNS_MAX;
use afd_fleet::run::sql::RUN_STATUS_EXPIRED;

use super::Case;

// Value: protects=a dead runner neither strands a setup turn nor reruns it forever;
// fails_when=the reassignment skips the fence bump, the lapse cap is dropped, or the
// capped turn is not failed; why_new=the draft run path had no expiry case;
// seam=the production draft assignment over live Postgres, the runs aged as a silent
// holder leaves them.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_expired_setup_run_reruns_the_turn() {
    let case = Case::queued().await;
    let first = case.take_on_host().await;
    case.lapse(&first.run_id()).await;
    for fence in 2..=u64::from(EVENT_RUNS_MAX) {
        let rerun = case.take_on_host().await;
        assert_eq!(rerun.fence, fence, "each rerun takes the next fence");
        assert_eq!(rerun.turn, first.turn, "the same turn, run again");
        case.lapse(&rerun.run_id()).await;
    }

    assert!(
        case.take(&case.running.runner).await.is_none(),
        "past {EVENT_RUNS_MAX} lapsed runs the turn is not assigned again"
    );
    let turn = case.turn(&first.turn_id()).await;
    assert_eq!(turn.status, "failed", "{turn:?}");
    assert_eq!(
        turn.failure_reason.as_deref(),
        Some("provider_usage_unknown"),
        "{turn:?}"
    );
    assert_eq!(turn.answer, None);
    let expired: Vec<(i64, String)> = (1..=i64::from(EVENT_RUNS_MAX))
        .map(|fence| (fence, RUN_STATUS_EXPIRED.to_owned()))
        .collect();
    assert_eq!(
        case.runs().await,
        expired,
        "every run lapsed, and none was issued past the cap"
    );
    let (candidates, _head) = case.candidates().await;
    assert_eq!(candidates, 1, "a lost turn writes no candidate");
    assert!(
        case.take(&case.running.runner).await.is_none(),
        "a failed turn is never taken again"
    );
    case.cleanup().await;
}
