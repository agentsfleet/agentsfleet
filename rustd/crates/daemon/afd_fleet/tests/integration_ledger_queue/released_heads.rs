//! A head its pass freed without a run waits behind turns that queued before
//! the release, so a head that faults or is refused on every pass cannot lead
//! every ordinary pass.

use std::time::Duration;

use super::{next, ordering::enqueue_at};
use crate::requests::placement_tag;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

// Value: protects=a head freed without a run, as a gate fault, a retried payer
// gate or a refused issue leaves it, is offered after a younger runnable head;
// fails_when=ordinary order ignores the thread's release, so the same old head
// leads every pass; seam=the ledger selection and release on live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_head_freed_without_a_run_waits_behind_turns_queued_before_it() {
    let fixtures = Fixtures::create().await;
    let (stuck, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let runnable = afd_db::test_util::mint_id();
    fixtures
        .seed_fleet(&runnable, &workspace, &tenant, &placement_tag(&stuck))
        .await;
    let queued = |by| af_core::clock::before(fixtures.enrolled_at(), Duration::from_secs(by));
    let old = enqueue_at(&fixtures, &stuck, &workspace, queued(2)).await;
    let young = enqueue_at(&fixtures, &runnable, &workspace, queued(1)).await;
    let runs = fixtures.runs_with_dead_bus();

    let first = next(&runs, &runner).await.expect("both fleets wait");
    runs.release(&first.fleet_id, first.fence)
        .await
        .expect("the pass lets its assignment go");
    let second = next(&runs, &runner).await.expect("both fleets wait");

    assert_eq!(
        (first.turn_id, second.turn_id),
        (old, young),
        "the freed head waits behind the turn queued before its release"
    );
    fixtures.cleanup().await;
}
