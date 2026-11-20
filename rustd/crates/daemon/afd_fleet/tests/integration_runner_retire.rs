//! Retiring a runner's record against live Postgres.
//!
//! Split from [`super::integration_runner_admin`] by outcome: everything here
//! is about the row going away, and the two refusals in front of it. The
//! transitions that keep the row live stay next door.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_wire::runner::{NetworkPolicy, SandboxTier};
use afd_api_wire::admin::RunnerAdminAction;
use afd_fleet::run::Billed;

use crate::requests::enrolment;
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use crate::support::Fixtures;

/// The operator every retirement here is attributed to.
const ACTOR: &str = "fixture:platform-operator";

/// A record is retired only from the terminal state, and only once.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_runner_record_is_retired_only_once_revoked() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            "fixture:platform-operator",
            fixtures.enrolled_at(),
        )
        .await
        .expect("the runner enrols");

    let in_service = fixtures
        .runners()
        .platform()
        .delete_revoked(&enrolled.runner_id)
        .await
        .err()
        .map(|error| error.code());
    assert_eq!(
        in_service,
        Some(af_core::error_code::RUNNER_MUST_REVOKE_FIRST),
        "a runner still in service keeps its record; the revoke is the destructive step"
    );

    fixtures
        .runners()
        .platform()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            ACTOR,
            af_core::clock::after(fixtures.enrolled_at(), std::time::Duration::from_millis(1)),
        )
        .await
        .expect("an active runner may be revoked");
    fixtures
        .runners()
        .platform()
        .delete_revoked(&enrolled.runner_id)
        .await
        .expect("a revoked runner's record is retired");

    let gone = fixtures
        .runners()
        .platform()
        .delete_revoked(&enrolled.runner_id)
        .await
        .err()
        .map(|error| error.code());
    assert_eq!(
        gone,
        Some(af_core::error_code::RUNNER_NOT_FOUND),
        "the row is the verdict: retired once, it is not found the second time"
    );
}

/// A revoked runner keeps its record while a run of its is still active.
///
/// The run row is what the liveness sweep releases the fleet's thread
/// through; retiring the runner under it would strand the thread until the
/// run's own expiry. Once the run is gone the record retires as usual.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_assigned_runner_keeps_its_record_until_the_run_is_gone() {
    // The one retirement test that takes a run, so the one whose run path
    // publishes on the fixture's bus.
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        tenant,
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let now = af_core::clock::after(fixtures.enrolled_at(), std::time::Duration::from_millis(1));
    // Select finds the work, the narrative log opens, and issue writes the
    // row. The retirement predicate reads the row, so every step is needed,
    // as `integration_run_issue` does them.
    // Narrowed to THIS fleet: the readiness cursor is process-global, and the
    // issue below charges this tenant's wallet for whatever was acquired.
    let held = crate::seed::select_fleet(&fixtures.runs(), &runner, &fleet)
        .await
        .expect("a ready fleet holding an event is leasable");
    // The run row references the event's row, which the assignment opened.
    let issued = fixtures
        .runs()
        .issue(
            &runner,
            &held,
            Billed {
                tenant_id: &tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the run row must be written")
        .expect("the assignment is still held");
    let run = issued.run_id;

    fixtures
        .runners()
        .platform()
        .transition(
            &runner,
            RunnerAdminAction::Revoke,
            ACTOR,
            af_core::clock::after(fixtures.enrolled_at(), std::time::Duration::from_millis(2)),
        )
        .await
        .expect("an active runner may be revoked");

    let still_running = fixtures
        .runners()
        .platform()
        .delete_revoked(&runner)
        .await
        .err();
    assert_eq!(
        still_running.as_ref().map(afd_runner::Error::code),
        Some(af_core::error_code::RUNNER_MUST_REVOKE_FIRST)
    );
    assert_eq!(
        still_running.as_ref().map(afd_runner::Error::detail),
        Some(afd_runner::DETAIL_RUNNER_STILL_RUNNING),
        "the refusal names the run, not the revocation the operator already did"
    );

    fixtures.expire_run(&run).await;
    let unbilled = fixtures
        .runners()
        .platform()
        .delete_revoked(&runner)
        .await
        .err();
    assert_eq!(
        unbilled.as_ref().map(afd_runner::Error::code),
        Some(af_core::error_code::RUNNER_MUST_REVOKE_FIRST),
        "an expired run whose runtime tail is unbilled still holds the record"
    );
    fixtures.bill_run_tail(&run).await;
    fixtures
        .runners()
        .platform()
        .delete_revoked(&runner)
        .await
        .expect("once its run is gone and billed, a revoked runner's record retires");

    fixtures.cleanup().await;
}
