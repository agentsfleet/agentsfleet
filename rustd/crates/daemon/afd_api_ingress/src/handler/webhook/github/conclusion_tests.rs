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

use super::{Ingest, Policy, REASON_NON_FAILURE_CONCLUSION, classify};

/// The header GitHub names a workflow run in.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// A failed run, whose one `"failure"` is its conclusion.
const RUN_FAILURE: &str =
    include_str!("../../../../../../../../tests/fixtures/webhooks/github_run_failure.json");

/// 2026-08-26T12:00:00Z.
const RECEIVED_AT: jiff::Timestamp = jiff::Timestamp::constant(1_787_745_600, 0);

/// The failed run's delivery, classified as though it had ended `conclusion`.
fn run_ending(conclusion: &str) -> Ingest {
    let body = RUN_FAILURE.replace("\"failure\"", &format!("\"{conclusion}\""));
    classify(
        Policy::AppIngress,
        EVENT_WORKFLOW_RUN,
        body.as_bytes(),
        RECEIVED_AT,
    )
    .expect("the corpus delivery reads under any conclusion")
}

#[test]
fn a_run_that_timed_out_or_never_started_wakes_the_fleet_under_its_own_conclusion() {
    for conclusion in ["timed_out", "startup_failure"] {
        let Ingest::Accept(body) = run_ending(conclusion) else {
            panic!("a run that ended {conclusion} was not accepted");
        };
        let digest: serde_json::Value = serde_json::from_str(&body).expect("the digest is JSON");

        assert_eq!(
            digest.get("conclusion").and_then(serde_json::Value::as_str),
            Some(conclusion),
            "the fleet's prose reads which way the run ended"
        );
    }
}

#[test]
fn a_run_nobody_needs_to_investigate_is_dropped_with_its_reason() {
    for conclusion in [
        "success",
        "cancelled",
        "skipped",
        "neutral",
        "action_required",
        "stale",
    ] {
        let Ingest::Ignore(reason) = run_ending(conclusion) else {
            panic!("a run that ended {conclusion} was not ignored");
        };

        assert_eq!(reason, REASON_NON_FAILURE_CONCLUSION, "{conclusion}");
    }
}
