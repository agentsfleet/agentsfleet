//! A sandbox the host ends under its run ends that run.

use std::sync::Arc;

use af_core::test_util::trace::Capture;
use afr_executor::Unavailable;

use super::*;
use crate::test_support::Parking;

/// The failure a run whose sandbox ended under it reports.
const TRANSPORT_LOSS: &str = "transport_loss";
/// The outcome of a run that did not end as the fleet meant it to.
const FLEET_ERROR: &str = "fleet_error";

/// A sandbox that reads ended once built ends its run at the turn's first
/// sandbox-side call: the call answers `sandbox_unavailable` as lost, the run
/// reports the loss with no task transition, and the sandbox is destroyed,
/// never parked.
#[tokio::test(start_paused = true)]
async fn test_a_sandbox_ended_under_its_run_ends_the_run_unavailable() {
    let capture = Capture::install();
    let engine = FakeEngine {
        parking: Parking::Lost,
        ..FakeEngine::default()
    };
    let (parked, destroyed) = (Arc::clone(&engine.parked), Arc::clone(&engine.destroyed));
    let mut rig = rig(healthy, engine, Behaviour::Answer);

    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    assert_eq!(rig.lent(), [Some(Unavailable::Lost)]);
    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], FLEET_ERROR, "{report}");
    assert_eq!(report[FAILURE_REASON], TRANSPORT_LOSS, "{report}");
    assert_eq!(parked.load(Ordering::SeqCst), 0, "nothing is parked");
    assert_eq!(destroyed.load(Ordering::SeqCst), 1, "destroyed once");
    let lost = capture.only("run_sandbox_lost");
    assert_eq!(lost.field("run_id"), Some(RUN_ID));
}
