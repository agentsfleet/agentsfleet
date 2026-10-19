//! Heartbeat recovery and stale-state reconciliation.

#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::error_code::INTERNAL_DB_QUERY;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_db::test_util::mint_id;
use afd_runner::heartbeat::holds::prove;
use afd_runner::reconcile::REASON_LANDLOCK_UNAVAILABLE;
use afd_runner::{Beat, NO_REPORT};
use afd_wire::runner::{
    CapabilityReport, HeartbeatRequest, HeldFleets, NetworkPolicy, SandboxTier,
};
use jiff::Timestamp;

use super::Verdict;
use super::requests::{ONE_BEAT, capable, enrolment};
use super::support::Fixtures;
use crate::rows::Row;

/// What a hold reconcile that did not land is logged under.
const HOLDS_RECONCILE_FAILED: &str = "runner_holds_reconcile_failed";

/// Moves the table the release read joins out of its way, in a database of
/// the test's own; the beat's other statements never read it.
const MOVE_FLEETS: &str = "ALTER TABLE fleet.fleets RENAME TO fleets_moved";

/// `runner` beats once at `now`, listing `fleet` as held.
async fn beats_holding(fixtures: &Fixtures, runner: &Uuid7, fleet: &str, now: Timestamp) -> Beat {
    let request = HeartbeatRequest {
        holds: HeldFleets(vec![Cow::Borrowed(fleet)]),
        ..NO_REPORT
    };
    let held = prove(request.holds.clone());
    fixtures
        .runners()
        .heartbeat(runner, &request, held.as_ref(), now)
        .await
        .expect("a release read that fails does not fail the beat")
}

/// A host that loses a mechanism is degraded on the next beat, and named.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_lost_guarantee_degrades_the_row_on_the_next_beat() {
    let fixtures = Fixtures::create().await;
    let request = enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 1);
    let enrolled = fixtures
        .runners()
        .register(&request, fixtures.enrolled_at())
        .await
        .expect("enrolment must succeed");
    let runner = enrolled.runner_id.as_str().to_owned();

    for (offset, report) in [
        (ONE_BEAT, capable()),
        (
            ONE_BEAT * 2,
            CapabilityReport {
                build_id: None,
                landlock: false,
                ..capable()
            },
        ),
    ] {
        fixtures
            .runners()
            .heartbeat(
                &enrolled.runner_id,
                &HeartbeatRequest {
                    capability_report: Some(report),
                    selftest: None,
                    holds: afd_wire::runner::HeldFleets::default(),
                    closing: false,
                },
                None,
                fixtures.after(offset),
            )
            .await
            .expect("the capability transition must land");
    }

    assert_eq!(
        fixtures.text(Row::Runner(&runner), "degraded_reason").await,
        Some(REASON_LANDLOCK_UNAVAILABLE.to_owned()),
        "the reason is on the row an operator inspects"
    );
    fixtures.cleanup().await;
}

/// A stored report repairs a stale verdict without needing a fresh report.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_stored_report_repairs_a_stale_verdict() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::LandlockFull, NetworkPolicy::AllowAll, 1),
            fixtures.enrolled_at(),
        )
        .await
        .expect("enrolment must succeed");
    let report = serde_json::to_string(&capable()).expect("the report shape serializes");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "UPDATE runner.runners SET capability_report = $2::jsonb, \
         degraded_reason = 'stale fixture' WHERE id = $1::uuid",
    )
    .bind(enrolled.runner_id.as_str())
    .bind(report)
    .execute(&mut *connection)
    .await
    .expect("the stale verdict fixture lands");
    drop(connection);

    let answered = fixtures
        .runners()
        .heartbeat(
            &enrolled.runner_id,
            &NO_REPORT,
            None,
            fixtures.after(ONE_BEAT),
        )
        .await
        .expect("the stored report remains usable");

    assert_eq!(answered.verdict, Verdict::Healthy);
    assert_eq!(
        fixtures
            .text(Row::Runner(enrolled.runner_id.as_str()), "degraded_reason")
            .await,
        None,
        "a healthy verdict clears the reason"
    );
    fixtures.cleanup().await;
}

/// A token that authenticated against a row since reaped fails closed.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_vanished_runner_is_its_own_failure() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            fixtures.enrolled_at(),
        )
        .await
        .expect("enrolment must succeed");
    let phantom = afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000ff")
        .expect("the fixture identifier is canonical");

    let read = fixtures.runners().self_record(&phantom).await;
    let beat = fixtures
        .runners()
        .heartbeat(&phantom, &NO_REPORT, None, fixtures.enrolled_at())
        .await;

    for outcome in [read.err(), beat.err()] {
        let error = outcome.expect("a phantom runner must not resolve");
        assert!(error.is_runner_vanished());
        assert_eq!(error.code().as_str(), "UZ-RUN-001");
        assert_eq!(error.detail(), "runner not found");
    }
    fixtures
        .runners()
        .self_record(&enrolled.runner_id)
        .await
        .expect("a phantom's failure must not disturb a real row");
    fixtures.cleanup().await;
}

/// A release read that fails answers no release, and says why: the runner
/// keeps its holds one beat longer while the beat itself lands. The same
/// listed fleet, which no slot of this runner's holds, is answered back for
/// release while the read works, so the empty answer is the failure's.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_release_read_that_fails_releases_nothing_and_warns() {
    let fixtures = Fixtures::create_isolated().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            fixtures.enrolled_at(),
        )
        .await
        .expect("enrolment must succeed");
    let runner = &enrolled.runner_id;
    let fleet = mint_id();
    let working = beats_holding(&fixtures, runner, &fleet, fixtures.after(ONE_BEAT)).await;
    let mut connection = fixtures.database.acquire().await.expect("a connection");
    sqlx::query(MOVE_FLEETS)
        .execute(&mut *connection)
        .await
        .expect("the private database's table moves");
    drop(connection);

    let log = Capture::install();
    let failed = beats_holding(&fixtures, runner, &fleet, fixtures.after(ONE_BEAT * 2)).await;
    let warned = log.only(HOLDS_RECONCILE_FAILED).fields;
    drop(log);

    assert_eq!(working.release_holds, [fleet], "the read works first");
    assert!(
        failed.release_holds.is_empty(),
        "{:?}",
        failed.release_holds
    );
    assert_eq!(
        warned.get("error_code").map(String::as_str),
        Some(INTERNAL_DB_QUERY.as_str())
    );
    assert_eq!(
        warned.get("runner_id").map(String::as_str),
        Some(runner.as_str())
    );
    fixtures.cleanup().await;
}
