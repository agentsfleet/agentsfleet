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

use afd_api_wire::admin::{AdminState, RunnerAdminAction};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_runner::RunnerEdit;
use afd_wire::runner::{
    AssignedPolicy, BindMode, CapabilityReport, ExtraBind, NetworkPolicy, SandboxTier,
};
use tokio::sync::Barrier;

use self::requests::{enrolled_at, enrolment};
use self::support::Fixtures;

const ACTOR: &str = "fixture:platform-operator";

/// Enrols one development runner at the fixture instant and returns its id.
async fn enrol(fixtures: &Fixtures) -> Uuid7 {
    fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(enrolled_at()),
        )
        .await
        .expect("the runner enrols")
        .runner_id
}

/// Applies `action` to `runner` as [`ACTOR`], `offset` milliseconds after
/// enrolment.
async fn transition(
    fixtures: &Fixtures,
    runner: &Uuid7,
    action: RunnerAdminAction,
    offset: i64,
) -> afd_runner::Result<AdminState> {
    fixtures
        .runners()
        .transition(
            runner,
            action,
            ACTOR,
            UnixMillis::from_millis(enrolled_at() + offset),
        )
        .await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_admin_transitions() {
    let fixtures = Fixtures::create().await;
    let enrolled = enrol(&fixtures).await;
    let runner = enrolled.as_str().to_owned();

    transition(&fixtures, &enrolled, RunnerAdminAction::Cordon, 1)
        .await
        .expect("an active runner may be cordoned");
    transition(&fixtures, &enrolled, RunnerAdminAction::Cordon, 2)
        .await
        .expect("repeating a transition is idempotent");
    transition(&fixtures, &enrolled, RunnerAdminAction::Revoke, 3)
        .await
        .expect("a cordoned runner may be revoked");

    let refused = transition(&fixtures, &enrolled, RunnerAdminAction::Drain, 4)
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

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_identical_transitions_append_one_event() {
    let fixtures = Fixtures::create().await;
    let enrolled = enrol(&fixtures).await;
    let gate = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for actor in ["fixture:operator-a", "fixture:operator-b"] {
        let runners = fixtures.runners().clone();
        let runner = enrolled.clone();
        let ready = Arc::clone(&gate);
        tasks.push(tokio::spawn(async move {
            ready.wait().await;
            runners
                .transition(
                    &runner,
                    RunnerAdminAction::Cordon,
                    actor,
                    UnixMillis::from_millis(enrolled_at() + 1),
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
            AdminState::Cordoned
        );
    }
    assert_eq!(
        fixtures.events(enrolled.as_str()).await,
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
    let enrolled = enrol(&fixtures).await;
    let runner = enrolled.as_str();

    let first = assign(&fixtures, &enrolled, &reassignment(u32::MAX), 1)
        .await
        .expect("a live runner accepts a safe assignment");
    assert_eq!(
        first,
        Some(64),
        "the reply carries the clamped worker count"
    );
    assert_eq!(
        fixtures
            .runner_column(runner, "worker_count")
            .await
            .as_deref(),
        Some("64")
    );
    assert_degraded(&fixtures, runner, "true", Some("no capability report")).await;

    assign(&fixtures, &enrolled, &reassignment(64), 2)
        .await
        .expect("repeating stored values is a successful no-op");
    assert_eq!(
        fixtures.events(runner).await,
        vec!["runner_registered", "runner_policy_assigned"]
    );

    store_full_capability(&fixtures, runner).await;

    let mut changed = reassignment(2);
    changed.registry_allowlist.push(Cow::Borrowed("pypi.org"));
    assign(&fixtures, &enrolled, &changed, 3)
        .await
        .expect("the updated assignment reconciles with stored capability");
    assert_degraded(&fixtures, runner, "false", None).await;

    fixtures.cleanup().await;
}

/// Assigns `policy` to `runner`, `offset` milliseconds after enrolment,
/// answering the worker count as stored.
async fn assign(
    fixtures: &Fixtures,
    runner: &Uuid7,
    policy: &AssignedPolicy<'_>,
    offset: i64,
) -> afd_runner::Result<Option<u32>> {
    let edit = RunnerEdit {
        assigned_policy: Some(policy),
        labels: None,
    };
    fixtures
        .runners()
        .edit(
            runner,
            edit,
            UnixMillis::from_millis(enrolled_at() + offset),
        )
        .await
        .map(|stored| stored.assigned_policy.map(|policy| policy.worker_count))
}

/// Asserts the runner row's degraded flag and the reason it records.
async fn assert_degraded(fixtures: &Fixtures, runner: &str, degraded: &str, reason: Option<&str>) {
    assert_eq!(
        fixtures.runner_column(runner, "degraded").await.as_deref(),
        Some(degraded)
    );
    assert_eq!(
        fixtures
            .runner_column(runner, "degraded_reason")
            .await
            .as_deref(),
        reason
    );
}

/// Stores a capability report that can enforce every assigned posture, as a
/// heartbeat from a fully capable host would.
async fn store_full_capability(fixtures: &Fixtures, runner: &str) {
    let capability = serde_json::to_string(&CapabilityReport {
        build_id: None,
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
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_policy_assignment_refuses_revocation() {
    let fixtures = Fixtures::create().await;
    let enrolled = enrol(&fixtures).await;
    transition(&fixtures, &enrolled, RunnerAdminAction::Revoke, 1)
        .await
        .expect("the runner is revoked");

    let refused = assign(&fixtures, &enrolled, &reassignment(3), 2)
        .await
        .expect_err("revocation is terminal for policy assignment");
    assert!(refused.is_rejected());

    fixtures.cleanup().await;
}
