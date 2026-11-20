//! A lapsed thread requalifies the same row and runs it again.

use af_core::id::Uuid7;
use afd_fleet::run::{Acquired, Billed, Delivery, Kind, Runs};

use super::{enqueue, next};
use crate::rows::Row;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

/// A runner stops renewing: its run is marked expired, and the turn's next
/// run is assigned and written at the fence one above it.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_expired_run_is_followed_by_a_run_at_the_next_fence() {
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [holder, poller]) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let first = next(&runs, &holder).await.expect("first assignment");
    assert!(!first.redelivered);
    assert_eq!(first.kind, Kind::Fresh);
    assert_eq!(first.earlier_runs, 0, "nothing was issued before it");
    assert_eq!(first.opened.received.delivery, Delivery::First);
    let issued = issue(&fixtures, &runs, &first, &holder, &tenant).await;
    fixtures.lapse_holder(&issued, &fleet).await;

    let reassigned = next(&runs, &poller)
        .await
        .expect("expired thread requalifies");
    assert_eq!(reassigned.turn_id, event, "no sweeper or stream repair ran");
    assert!(reassigned.redelivered);
    assert_eq!(reassigned.fence.as_i64(), first.fence.as_i64() + 1);
    assert_eq!(reassigned.kind, Kind::Reassigned);
    assert_eq!(reassigned.earlier_runs, 1, "the lapsed holder's run");
    assert_eq!(
        fixtures.text(Row::Run(&issued), "status").await.as_deref(),
        Some(afd_fleet::run::sql::RUN_STATUS_EXPIRED)
    );
    let second = issue(&fixtures, &runs, &reassigned, &poller, &tenant).await;
    let fence = first.fence.as_i64() + 1;
    assert_eq!(
        (
            fixtures.text(Row::Run(&second), "turn_id").await,
            fixtures.text(Row::Run(&second), "fencing_token").await,
            fixtures.text(Row::Run(&second), "status").await,
        ),
        (
            Some(event.to_string()),
            Some(fence.to_string()),
            Some(afd_fleet::run::sql::RUN_STATUS_ACTIVE.to_owned()),
        ),
        "the second run serves the same turn at the next fence"
    );
    assert!(
        next(&runs, &holder).await.is_none(),
        "new thread remains exclusive"
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
