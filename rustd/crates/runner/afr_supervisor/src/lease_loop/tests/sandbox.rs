#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afr_sandbox::Limits;
use tokio::sync::mpsc;

use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, OUTCOME, PROCESSED, Rig, daemon, lease,
    reported,
};

/// Native sandbox work uses the same runner and daemon boundary.
fn rig(engine: FakeEngine) -> Rig {
    Rig::new(
        daemon(|_call| None),
        engine,
        FakeAgent::new(Behaviour::Answer),
    )
}

/// The wire carries no sandbox size, so a lease builds its sandbox with this
/// host's own limits.
#[tokio::test(start_paused = true)]
async fn a_lease_builds_the_hosts_limits() {
    let (asked, mut received) = mpsc::unbounded_channel();
    let mut rig = rig(FakeEngine {
        asked: Some(asked),
        ..FakeEngine::default()
    });
    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    let built: Vec<Limits> = std::iter::from_fn(|| received.try_recv().ok()).collect();
    assert_eq!(built, [Limits::default()]);
    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
}
