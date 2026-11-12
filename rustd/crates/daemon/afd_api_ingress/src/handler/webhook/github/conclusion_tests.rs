//! Which workflow conclusions wake a fleet, against a delivery GitHub sent.
//!
//! Each case is the corpus's failed run with one word changed: its conclusion.
//! Everything else in the payload stays as GitHub wrote it, so a case proves
//! the decision and not a body shaped to pass it.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::tests::{EVENT_WORKFLOW_RUN, RECEIVED_AT, RUN_FAILURE};
use super::{Ingest, Policy, REASON_NON_FAILURE_CONCLUSION, classify};

#[test]
fn a_completed_run_wakes_the_fleet_only_when_it_failed_timed_out_or_never_started() {
    for (conclusion, wakes) in [
        ("failure", true),
        ("timed_out", true),
        ("startup_failure", true),
        ("success", false),
        ("cancelled", false),
        ("skipped", false),
        ("neutral", false),
        ("action_required", false),
        ("stale", false),
    ] {
        // The corpus's one `"failure"` is the run's conclusion.
        let body = RUN_FAILURE.replace("\"failure\"", &format!("\"{conclusion}\""));

        let ingest = classify(
            Policy::AppIngress,
            EVENT_WORKFLOW_RUN,
            body.as_bytes(),
            RECEIVED_AT,
        )
        .expect("the corpus delivery reads under any conclusion");

        match (ingest, wakes) {
            (Ingest::Accept(digest), true) => {
                let digest: serde_json::Value =
                    serde_json::from_str(&digest).expect("the digest is JSON");
                assert_eq!(
                    digest.get("conclusion").and_then(serde_json::Value::as_str),
                    Some(conclusion),
                    "the fleet's prose reads which way the run ended"
                );
            }
            (Ingest::Ignore(reason), false) => {
                assert_eq!(reason, REASON_NON_FAILURE_CONCLUSION, "{conclusion}");
            }
            (other, _) => panic!("a run that ended {conclusion} was classified {other:?}"),
        }
    }
}
