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

// Child of this suite, not a crate-root module: it reaches its parent
// through `super::`, so it must stay nested here. The path is relative
// to THIS file's directory, which the aggregator does not change.
#[path = "integration_runner_beat/recovery.rs"]
mod recovery;
use crate::requests;
use crate::support;
use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_crypto::entropy::Entropy;
use afd_runner::reconcile::REASON_NO_CAPABILITY_REPORT;
use afd_runner::{NO_REPORT, Runners, Verdict};
use afd_wire::runner::{
    CapabilityReport, HeartbeatRequest, NetworkPolicy, SandboxTier, SelftestCheck, SelftestReport,
};

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

/// Failures in optional beat work cannot turn a live host into an offline one.
///
/// This drives two independent failure seams in one beat: the self-test
/// summary contradicts its checks, and the event identifier's entropy source
/// refuses. The former must be ignored and the latter must fall back to the
/// liveness-only statement. Neither is allowed to fail the request.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_optional_heartbeat_failures_still_land_liveness() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();
    let (entropy, entropy_control) = Entropy::new_mocked();
    entropy_control.fail_next();
    let runners = Runners::new(fixtures.database.clone(), entropy);
    let inconsistent = HeartbeatRequest {
        capability_report: None,
        selftest: Some(SelftestReport {
            checks: vec![SelftestCheck {
                name: Cow::Borrowed("the sandbox launched"),
                ok: true,
                detail: Cow::Borrowed("the probe completed"),
            }],
            all_ok: false,
            sandbox_tier: Cow::Borrowed("dev_none"),
            network_policy: Cow::Borrowed("allow_all"),
        }),
    };
    let beat_at = ENROLLED_AT + ONE_BEAT_MS;

    let answered = runners
        .heartbeat(
            &enrolled.runner_id,
            &inconsistent,
            UnixMillis::from_millis(beat_at),
        )
        .await
        .expect("optional write failures must not fail liveness");

    assert!(
        !answered.selftest_requested,
        "a refused unsolicited verdict does not fabricate an operator request"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "selftest_checks").await,
        None,
        "a contradictory verdict is not persisted"
    );
    assert_eq!(
        fixtures.runner_column(&runner, "last_seen_at").await,
        Some(beat_at.to_string()),
        "entropy failure falls back to the liveness-only statement"
    );
    assert_eq!(
        fixtures.events(&runner).await,
        vec!["runner_registered".to_owned()],
        "without an event identifier, the fallback writes no false transition"
    );

    fixtures.cleanup().await;
}
