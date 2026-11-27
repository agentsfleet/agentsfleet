//! A run whose tenant processes met the sandbox's process cap is logged and
//! counted once; one that never met it says nothing.

use af_core::test_util::trace::Capture;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::{EVENT_PIDS_LIMIT_HIT, pids_limit_hit};

#[tokio::test]
async fn a_run_whose_forks_were_refused_at_the_cap_is_logged_and_counted() {
    let capture = Capture::install();
    let (tally, recorded) = Tally::new();

    scoped(tally, async {
        pids_limit_hit("run-refused", 3, 512);
        pids_limit_hit("run-quiet", 0, 512);
    })
    .await;

    let hit = capture.only(EVENT_PIDS_LIMIT_HIT);
    assert_eq!(hit.field("run_id"), Some("run-refused"));
    assert_eq!(hit.field("refused"), Some("3"));
    assert_eq!(hit.field("pids_max"), Some("512"));
    assert_eq!(
        recorded.try_iter().collect::<Vec<_>>(),
        vec![Recorded::PidsLimitHit],
        "one count for the run that met the cap, none for the one that did not"
    );
}
