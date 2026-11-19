//! A lapsed slot requalifies the same row and runs it again.

use af_core::id::Uuid7;
use afd_fleet::run::{Acquired, Billed, Delivery, Kind, Runs};

use super::{enqueue, next};
use crate::rows::Row;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_reclaimed_row_is_rund_as_redelivered() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &holder).await.expect("first claim");
    assert!(!first.redelivered);
    assert_eq!(first.kind, Kind::Fresh);
    assert_eq!(first.earlier_runs, 0, "nothing was issued before it");
    assert_eq!(first.opened.received.delivery, Delivery::First);
    let issued = issue(&fixtures, &runs, &first, &holder, &tenant).await;
    fixtures.lapse_holder(&issued, &fleet).await;

    let reclaimed = next(&runs, &poller)
        .await
        .expect("expired slot requalifies");
    assert_eq!(reclaimed.event_id, event, "no sweeper or stream repair ran");
    assert!(reclaimed.redelivered);
    assert!(reclaimed.fence > first.fence);
    assert_eq!(reclaimed.kind, Kind::Reclaim);
    assert_eq!(reclaimed.earlier_runs, 1, "the lapsed holder's run");
    assert_eq!(
        fixtures.text(Row::Run(&issued), "status").await.as_deref(),
        Some(afd_fleet::run::sql::RUN_STATUS_EXPIRED)
    );
    assert!(
        next(&runs, &holder).await.is_none(),
        "new slot remains exclusive"
    );
    drop(runs);
    fixtures.cleanup().await;
}

pub(super) async fn issue(
    fixtures: &Fixtures,
    runs: &Runs,
    acquired: &Acquired,
    holder: &Uuid7,
    tenant: &Uuid7,
) -> Uuid7 {
    runs.issue(
        holder,
        acquired,
        Billed {
            tenant_id: tenant,
            posture: POSTURE,
            provider: PROVIDER,
            model: MODEL,
        },
        fixtures.enrolled_at(),
    )
    .await
    .expect("issue query")
    .expect("live fence")
    .run_id
}
