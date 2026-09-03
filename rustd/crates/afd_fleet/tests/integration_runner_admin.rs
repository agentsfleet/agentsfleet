//! Operator mutations against live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::requests;
use crate::support;
use std::borrow::Cow;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_wire::admin::RunnerAdminAction;
use afd_wire::runner::{
    AssignedPolicy, BindMode, CapabilityReport, ExtraBind, NetworkPolicy, SandboxTier,
};
use tokio::sync::Barrier;

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;
use crate::seed::seeded;

const ACTOR: &str = "fixture:platform-operator";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_admin_transitions() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    let runner = enrolled.runner_id.as_str().to_owned();

    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Cordon,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("an active runner may be cordoned");
    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Cordon,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("repeating a transition is idempotent");
    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 3),
        )
        .await
        .expect("a cordoned runner may be revoked");

    let refused = fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Drain,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 4),
        )
        .await
        .expect_err("revocation is terminal");
    assert!(refused.is_rejected());
    assert_eq!(
        fixtures.runner_column(&runner, "admin_state").await,
        Some("revoked".to_owned())
    );
    assert_eq!(
        fixtures.events(&runner).await,
        vec![
            "runner_registered".to_owned(),
            "runner_cordoned".to_owned(),
            "runner_revoked".to_owned(),
        ],
        "each real transition appends one event; repeats and refusals append none"
    );
    assert_eq!(
        fixtures.admin_event_actors(&runner).await,
        vec![ACTOR.to_owned(), ACTOR.to_owned()]
    );

    fixtures.cleanup().await;
}

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
    let fixtures = Fixtures::create().await;
    let seeded = seeded::<1>(&fixtures).await;
    let [runner] = seeded.runners;
    let answer = fixtures
        .plane()
        .lease(&runner, false, UnixMillis::from_millis(ENROLLED_AT + 1))
        .await
        .expect("the seeded fleet has work to lease");
    let document: serde_json::Value =
        serde_json::from_str(&answer).expect("the lease answer is a document");
    let lease = document
        .get("lease")
        .and_then(|lease| lease.get("lease_id"))
        .and_then(serde_json::Value::as_str)
        .expect("the seeded event is leased to the runner")
        .to_owned();

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

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_identical_transitions_append_one_event() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    let gate = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for actor in ["fixture:operator-a", "fixture:operator-b"] {
        let runners = fixtures.runners().clone();
        let runner = enrolled.runner_id.clone();
        let ready = Arc::clone(&gate);
        tasks.push(tokio::spawn(async move {
            ready.wait().await;
            runners
                .transition(
                    &runner,
                    RunnerAdminAction::Cordon,
                    actor,
                    UnixMillis::from_millis(ENROLLED_AT + 1),
                )
                .await
        }));
    }
    gate.wait().await;
    for task in tasks {
        assert_eq!(
            task.await
                .expect("the transition task completes")
                .expect("identical transitions are idempotent"),
            afd_wire::admin::AdminState::Cordoned
        );
    }
    assert_eq!(
        fixtures.events(enrolled.runner_id.as_str()).await,
        vec!["runner_registered".to_owned(), "runner_cordoned".to_owned(),],
        "the row lock lets only one contender append the state-change event"
    );

    fixtures.cleanup().await;
}

fn reassignment(worker_count: u32) -> AssignedPolicy<'static> {
    AssignedPolicy {
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: NetworkPolicy::AllowListEgress,
        registry_allowlist: vec![Cow::Borrowed("registry.npmjs.org")],
        worker_count,
        extra_binds: vec![ExtraBind {
            path: Cow::Borrowed("/srv/models"),
            mode: BindMode::ReadOnly,
            note: Cow::Borrowed("shared model cache"),
        }],
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_policy_assignment_is_atomic_idempotent_and_reconciled() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    let runner = enrolled.runner_id.as_str();

    let first = fixtures
        .runners()
        .assign_policy(
            &enrolled.runner_id,
            &reassignment(u32::MAX),
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("a live runner accepts a safe assignment");
    assert_eq!(first.worker_count(), 64);
    assert_eq!(
        fixtures
            .runner_column(runner, "worker_count")
            .await
            .as_deref(),
        Some("64")
    );
    assert_eq!(
        fixtures.runner_column(runner, "degraded").await.as_deref(),
        Some("true")
    );
    assert_eq!(
        fixtures
            .runner_column(runner, "degraded_reason")
            .await
            .as_deref(),
        Some("no capability report")
    );

    fixtures
        .runners()
        .assign_policy(
            &enrolled.runner_id,
            &reassignment(64),
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("repeating stored values is a successful no-op");
    assert_eq!(
        fixtures.events(runner).await,
        vec!["runner_registered", "runner_policy_assigned"]
    );

    let capability = serde_json::to_string(&CapabilityReport {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec![
            Cow::Borrowed("cpu"),
            Cow::Borrowed("memory"),
            Cow::Borrowed("pids"),
        ],
        bubblewrap: true,
        egress_enforcement: true,
    })
    .expect("the capability fixture serializes");
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.runners SET capability_report = $2::jsonb WHERE id = $1::uuid")
        .bind(runner)
        .bind(capability)
        .execute(&mut *connection)
        .await
        .expect("the capability fixture lands");
    drop(connection);

    let mut changed = reassignment(2);
    changed.registry_allowlist.push(Cow::Borrowed("pypi.org"));
    fixtures
        .runners()
        .assign_policy(
            &enrolled.runner_id,
            &changed,
            UnixMillis::from_millis(ENROLLED_AT + 3),
        )
        .await
        .expect("the updated assignment reconciles with stored capability");
    assert_eq!(
        fixtures.runner_column(runner, "degraded").await.as_deref(),
        Some("false")
    );
    assert_eq!(
        fixtures.runner_column(runner, "degraded_reason").await,
        None
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_policy_assignment_refuses_revocation() {
    let fixtures = Fixtures::create().await;
    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    fixtures
        .runners()
        .transition(
            &enrolled.runner_id,
            RunnerAdminAction::Revoke,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("the runner is revoked");

    let refused = fixtures
        .runners()
        .assign_policy(
            &enrolled.runner_id,
            &reassignment(3),
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect_err("revocation is terminal for policy assignment");
    assert!(refused.is_rejected());

    fixtures.cleanup().await;
}
