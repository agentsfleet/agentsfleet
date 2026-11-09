use super::*;

#[tokio::test(start_paused = true)]
async fn completed_report_carries_memory_and_fence_without_a_separate_push() {
    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();
    let calls = rig.calls();
    let report = reported(&calls);
    assert!(position(&calls, Verb::Capture).is_none());
    assert_eq!(report["fencing_token"], FENCING);
    assert_eq!(report["memory"][0]["key"], "k");
    assert_eq!(report[OUTCOME], PROCESSED);
    assert!(position(&calls, Verb::Activity).is_some());
    assert_eq!(rig.destroyed.load(Ordering::SeqCst), 1);
    assert!(
        !rig.home
            .sandboxes()
            .parent()
            .unwrap()
            .join("spool")
            .exists()
    );
}

#[tokio::test(start_paused = true)]
async fn the_report_settles_before_a_stalled_live_tail_and_the_wait_is_bounded() {
    let stalled: Special = |call| (call.verb == Verb::Activity).then_some(Answer::Stall);
    let mut rig = rig(stalled, FakeEngine::default(), Behaviour::Answer);
    let started = Instant::now();

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let waited = started.elapsed();
    assert!(
        waited >= ACTIVITY_DRAIN_WAIT && waited < ACTIVITY_DRAIN_WAIT * 2,
        "{waited:?}"
    );
}

/// The frames a run emitted that `recorded` counts as abandoned.
fn abandoned(recorded: &std::sync::mpsc::Receiver<afr_telemetry::testing::Recorded>) -> Vec<u64> {
    use afr_telemetry::labels::FrameDrop;
    use afr_telemetry::testing::Recorded;

    recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            Recorded::FramesDropped(FrameDrop::Abandoned, frames) => Some(frames),
            _other => None,
        })
        .collect()
}

/// A live tail the lease stopped waiting for counts the frames it still held
/// as abandoned: the fake engine's one chunk, stalled in its post.
#[tokio::test(start_paused = true)]
async fn a_live_tail_abandoned_at_the_drain_wait_counts_what_it_held() {
    use afr_telemetry::testing::{Tally, scoped};

    let stalled: Special = |call| (call.verb == Verb::Activity).then_some(Answer::Stall);
    let mut rig = rig(stalled, FakeEngine::default(), Behaviour::Answer);
    let (tally, recorded) = Tally::new();

    scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(
        abandoned(&recorded),
        vec![1],
        "the one frame the run emitted"
    );
}

/// A live tail that drained abandons nothing.
#[tokio::test(start_paused = true)]
async fn a_drained_live_tail_abandons_nothing() {
    use afr_telemetry::testing::{Tally, scoped};

    let mut rig = rig(healthy, FakeEngine::default(), Behaviour::Answer);
    let (tally, recorded) = Tally::new();

    scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    let unfinished = abandoned(&recorded);
    assert!(unfinished.is_empty(), "{unfinished:?}");
}
