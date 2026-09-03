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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::Billed;
use afd_wire::admin::RunnerAdminAction;
use afd_wire::runner::{NetworkPolicy, SandboxTier};

use crate::requests::{ENROLLED_AT, enrolment};
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use crate::support::Fixtures;

/// The operator every retirement here is attributed to.
const ACTOR: &str = "fixture:platform-operator";

/// A record is retired only from the terminal state, and only once.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_runner_record_is_retired_only_once_revoked() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");

    let in_service = fixtures
        .runners()
        .delete_revoked(&enrolled.runner_id)
        .await
        .err()
        .map(|error| error.code());
    assert_eq!(
        in_service,
        Some(afd_core::error_code::RUNNER_MUST_REVOKE_FIRST),
        "a runner still in service keeps its record; the revoke is the destructive step"
    );

    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("an active runner may be revoked");
    fixtures
        .runners()
        .delete_revoked(&enrolled.runner_id)
        .await
        .expect("a revoked runner's record is retired");

    let gone = fixtures
        .runners()
        .delete_revoked(&enrolled.runner_id)
        .await
        .err()
        .map(|error| error.code());
    assert_eq!(
        gone,
        Some(afd_core::error_code::RUNNER_NOT_FOUND),
        "the row is the verdict: retired once, it is not found the second time"
    );
}

/// A revoked runner keeps its record while a lease of its is still active.
///
/// The lease row is what the liveness sweep releases the fleet's slot
/// through; retiring the runner under it would strand the slot until the
/// lease's own expiry. Once the lease is gone the record retires as usual.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_leased_runner_keeps_its_record_until_the_lease_is_gone() {
    // With a queue: this is the one retirement test that takes a lease, and
    // the lease path publishes to it.
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT + 1);
    // Select finds the work; issue writes the row. The retirement predicate
    // reads the row, so both steps are needed, as `integration_lease_issue`
    // does them.
    let held = fixtures
        .leases()
        .select(&runner, now)
        .await
        .expect("the assignment pass must not fault")
        .expect("a ready fleet holding an event is leasable");
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = fixtures
        .leases()
        .issue(
            &runner,
            &held,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");
    let lease = issued.lease_id.as_str().to_owned();

    fixtures
        .runners()
        .transition(
            &runner,
            RunnerAdminAction::Revoke,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("an active runner may be revoked");

    let still_leased = fixtures.runners().delete_revoked(&runner).await.err();
    assert_eq!(
        still_leased.as_ref().map(afd_runner::Error::code),
        Some(afd_core::error_code::RUNNER_MUST_REVOKE_FIRST)
    );
    assert_eq!(
        still_leased.as_ref().map(afd_runner::Error::detail),
        Some(afd_runner::DETAIL_RUNNER_STILL_LEASED),
        "the refusal names the lease, not the revocation the operator already did"
    );

    fixtures.expire_lease(&lease).await;
    fixtures
        .runners()
        .delete_revoked(&runner)
        .await
        .expect("once its lease is gone, a revoked runner's record retires");

    fixtures.cleanup().await;
}
