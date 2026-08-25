//! §1 against a live datastore — what a BEAT moves.
//!
//! The sibling of `integration_runner_row.rs`, which proves what enrolment
//! WRITES. The two were one file until it outgrew RULE FLL's cap, and the seam
//! they were cut along is the same one that file's own documentation named:
//! enrolment writes a row, a beat moves it. The shared fixtures and the
//! enrolment/capability builders both halves need live in `support/`.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_fleet::runner::reconcile::{REASON_LANDLOCK_UNAVAILABLE, REASON_NO_CAPABILITY_REPORT};
use afd_fleet::{NO_REPORT, Verdict};
use afd_wire::runner::{CapabilityReport, HeartbeatRequest, NetworkPolicy, SandboxTier};

use self::requests::{ENROLLED_AT, ONE_BEAT_MS, capable, enrolment};
use self::support::Fixtures;

/// The first beat bumps liveness, stores the report, clears the verdict, and
/// emits exactly one transition event.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_first_beat_reports_and_comes_online() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowListEgress, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();

    let beat = HeartbeatRequest {
        capability_report: Some(capable()),
        selftest: None,
    };
    let first = fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &beat,
            UnixMillis::from_millis(ENROLLED_AT + ONE_BEAT_MS),
        )
        .await
        .expect("a beat against a reachable datastore");

    assert_eq!(
        first.verdict,
        Verdict::Healthy,
        "a capable host clears its verdict"
    );
    assert!(!first.selftest_requested);
    assert_eq!(
        fixtures.runner_column(&runner, "degraded").await,
        Some("false".to_owned())
    );
    assert_eq!(
        fixtures.runner_column(&runner, "last_seen_at").await,
        Some((ENROLLED_AT + ONE_BEAT_MS).to_string())
    );
    assert!(
        fixtures
            .runner_column(&runner, "capability_report")
            .await
            .is_some_and(|report| report.contains("landlock")),
        "the report the verdict was reconciled against is what gets stored"
    );
    assert_eq!(
        fixtures.events(&runner).await,
        vec!["runner_registered".to_owned(), "runner_online".to_owned()],
        "a runner that had never been seen transitions exactly once"
    );

    fixtures.cleanup().await;
}

/// A steady beat writes liveness and writes NO history.
///
/// The property that keeps the event stream readable: an idle fleet beats every
/// ten seconds per host, and a transition row per beat would bury every real
/// transition under them.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_steady_beat_writes_no_second_event() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();

    // The first beat transitions; the second is a heartbeat inside the window.
    for beat in 1..=2_i64 {
        fixtures
            .runners()
            .heartbeat(
                &enrolled.runner_id,
                &NO_REPORT,
                UnixMillis::from_millis(ENROLLED_AT + beat * ONE_BEAT_MS),
            )
            .await
            .expect("a beat must not fail");
    }

    assert_eq!(
        fixtures.events(&runner).await,
        vec!["runner_registered".to_owned(), "runner_online".to_owned()],
        "the second beat inside the freshness window is liveness, not history"
    );

    // A beat after the lapse threshold IS a transition, and gets its event.
    let lapsed = ENROLLED_AT + 2 * ONE_BEAT_MS + RUNNER_OFFLINE_AFTER_MS + 1;
    fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &NO_REPORT,
            UnixMillis::from_millis(lapsed),
        )
        .await
        .expect("a beat after a lapse must not fail");

    assert_eq!(
        fixtures.events(&runner).await.len(),
        3,
        "a host that went silent past the lapse threshold and came back is a \
         real transition, and the stream says so"
    );

    fixtures.cleanup().await;
}

/// A beat carrying an unreadable report still counts as liveness.
///
/// A runner token must not be able to fail its own liveness by sending
/// nonsense: a host that cannot beat is a host the fleet reads as dead, and
/// that is a far worse outcome than a report this daemon ignores.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_out_of_bounds_report_does_not_fail_the_beat() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();

    // Past `MAX_REPORT_CONTROLLERS`, which is a persistence-amplification
    // attempt rather than a capability claim.
    let flooded = CapabilityReport {
        cgroup_controllers: vec![Cow::Borrowed("cpu"); 64],
        ..capable()
    };
    let beat = HeartbeatRequest {
        capability_report: Some(flooded),
        selftest: None,
    };

    let answered = fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &beat,
            UnixMillis::from_millis(ENROLLED_AT + ONE_BEAT_MS),
        )
        .await
        .expect("a malformed report must not fail the beat");

    assert_eq!(
        answered.verdict,
        Verdict::Degraded {
            reason: REASON_NO_CAPABILITY_REPORT
        },
        "the report was dropped, so the row still has no proven capability"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "capability_report").await,
        None,
        "nothing out of bounds reaches the column"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "last_seen_at").await,
        Some((ENROLLED_AT + ONE_BEAT_MS).to_string()),
        "liveness landed anyway — that is the whole point of the leniency"
    );

    fixtures.cleanup().await;
}

/// A host that loses a mechanism is degraded on the next beat, and named.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_lost_guarantee_degrades_the_row_on_the_next_beat() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();

    let healthy = HeartbeatRequest {
        capability_report: Some(capable()),
        selftest: None,
    };
    fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &healthy,
            UnixMillis::from_millis(ENROLLED_AT + ONE_BEAT_MS),
        )
        .await
        .expect("the first beat clears the verdict");

    // The kernel was rebuilt without filesystem isolation.
    let lost = HeartbeatRequest {
        capability_report: Some(CapabilityReport {
            landlock: false,
            ..capable()
        }),
        selftest: None,
    };
    let answered = fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &lost,
            UnixMillis::from_millis(ENROLLED_AT + 2 * ONE_BEAT_MS),
        )
        .await
        .expect("the second beat must not fail");

    assert_eq!(
        answered.verdict,
        Verdict::Degraded {
            reason: REASON_LANDLOCK_UNAVAILABLE
        }
    );
    assert_eq!(
        fixtures.runner_column(&runner, "degraded_reason").await,
        Some(REASON_LANDLOCK_UNAVAILABLE.to_owned()),
        "the reason is on the ROW, because that is what an operator greps"
    );

    fixtures.cleanup().await;
}

/// A token that authenticated against a row since reaped fails closed.
///
/// NOT collapsed into a bad-token rejection: the credential is real and the
/// enrolment is gone, so the remedy is to re-enrol the host rather than retry.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_vanished_runner_is_its_own_failure() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let phantom = afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000ff")
        .expect("the fixture identifier is canonical");

    let read = fixtures.runners().self_record(&phantom).await;
    let beat = fixtures
        .runners()
        .heartbeat(&phantom, &NO_REPORT, UnixMillis::from_millis(ENROLLED_AT))
        .await;

    for outcome in [read.err(), beat.err()] {
        let error = outcome.expect("a phantom runner must not resolve");
        assert!(error.is_runner_vanished());
        assert_eq!(error.code().as_str(), "UZ-RUN-001");
        assert_eq!(error.detail(), "runner not found");
    }
    // The real runner is untouched by the phantom's failure.
    fixtures
        .runners()
        .self_record(&enrolled.runner_id)
        .await
        .expect("a phantom's failure must not disturb a real row");

    fixtures.cleanup().await;
}
