//! A beat whose every write is refused still answers: the policy read is the
//! one statement allowed to fail it, and each refused write says so at `warn`.
//!
//! The refusal is a trigger scoped to this test's runner, so the statements
//! fail where production would see an aborted write, and every other runner in
//! the lane beats through untouched.

use afd_core::clock;
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_runner::sql::NEVER_SEEN;
use afd_wire::runner::{CapabilityReport, HeartbeatRequest, HeldFleets};

use super::Fixture;
use super::refusal::Refusal;

/// Every update to the runner row the trigger names fails.
const REFUSE: &str = "RAISE EXCEPTION 'fixture refusal'";

/// A report proving nothing: enough for the beat to write its verdict.
fn bare_report() -> CapabilityReport<'static> {
    CapabilityReport {
        build_id: None,
        landlock: false,
        seccomp: false,
        cgroup_controllers: Vec::new(),
        bubblewrap: false,
        egress_enforcement: false,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_beat_whose_writes_are_refused_still_answers_and_logs_each() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let refusal = Refusal::install(
        &fixture.lane.url(),
        "runner.runners",
        "beat",
        runner.as_str(),
        &format!("NEW.id = '{runner}'::uuid"),
        REFUSE,
    )
    .await;
    let beat = HeartbeatRequest {
        capability_report: Some(bare_report()),
        selftest: None,
        holds: HeldFleets(Vec::new()),
        closing: false,
    };
    let capture = Capture::install();

    fixture
        .runners
        .heartbeat(&runner, &beat, None, clock::now())
        .await
        .expect("a refused best-effort write does not fail the beat");

    let events = capture.events();
    let failed: Vec<_> = events
        .iter()
        .filter(|event| {
            matches!(
                event.field("event"),
                Some("heartbeat_bump_failed" | "verdict_persist_failed")
            )
        })
        .collect();
    // The verdict write, the liveness bump with its event, and the bare touch
    // it falls back to: three writes, three lines.
    assert_eq!(failed.len(), 3, "{events:?}");
    for event in failed {
        assert_eq!(
            event.field("error_code"),
            Some(error_code::INTERNAL_DB_QUERY.as_str())
        );
        assert_eq!(event.field("runner_id"), Some(runner.as_str()));
        assert!(
            event
                .field("reason")
                .is_some_and(|reason| reason.contains("fixture refusal")),
            "the line carries Postgres's own reason: {event:?}"
        );
    }
    drop(capture);
    drop(refusal);

    let stored = fixture
        .runners
        .self_record(&runner)
        .await
        .expect("the runner reads");
    assert_eq!(stored.last_seen_at, NEVER_SEEN, "no liveness write landed");
    fixture.cleanup().await;
}
