//! Saved durable-task preference reaches every assignment read without changing support.

use super::{Fixture, enrolment};
use af_core::{clock, id::Uuid7};
use af_wire::runner::AssignedPolicy;
use afd_runner::{KeysetCursor, NO_REPORT, RunnerEdit};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn registration_preserves_explicit_durable_policy_in_all_projections() {
    let mut fixture = Fixture::create().await;
    for enabled in [true, false] {
        let mut request = enrolment();
        request.assigned_policy.durable_tasks_enabled = enabled;
        let enrolled = fixture
            .runners
            .register(&request, clock::now())
            .await
            .expect("runner enrolled");
        fixture.enrolled.push(enrolled.runner_id.clone());
        assert_readers(&fixture, &enrolled.runner_id, enabled).await;
        let history = fixture.history(&enrolled.runner_id).await;
        let registered = history
            .iter()
            .find(|event| {
                event.event_type == afd_api_wire::admin::RunnerEventType::RunnerRegistered
            })
            .expect("registration event");
        assert_eq!(
            registered.metadata.get("durable_tasks_enabled"),
            Some(&serde_json::json!(enabled))
        );
    }
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn disabled_policy_survives_unrelated_edits_and_identical_save_adds_no_event() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    assert_readers(&fixture, &runner, true).await;
    let mut policy = enrolment().assigned_policy;
    policy.durable_tasks_enabled = false;
    assign(&fixture, &runner, &policy).await;
    let before = fixture.history(&runner).await.len();
    assign(&fixture, &runner, &policy).await;
    assert_eq!(fixture.history(&runner).await.len(), before);
    policy.worker_count = 2;
    assign(&fixture, &runner, &policy).await;
    assert_readers(&fixture, &runner, false).await;
    let history = fixture.history(&runner).await;
    let changed = history
        .iter()
        .find(|event| {
            event.event_type == afd_api_wire::admin::RunnerEventType::RunnerPolicyAssigned
        })
        .expect("policy event");
    assert_eq!(
        changed.metadata.get("durable_tasks_enabled"),
        Some(&serde_json::json!(false))
    );
    policy.durable_tasks_enabled = true;
    assign(&fixture, &runner, &policy).await;
    assert_readers(&fixture, &runner, true).await;
    fixture.cleanup().await;
}

async fn assign(fixture: &Fixture, runner: &Uuid7, policy: &AssignedPolicy<'_>) {
    let result = fixture
        .runners
        .edit(
            runner,
            RunnerEdit {
                assigned_policy: Some(policy),
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect("policy saved");
    assert_eq!(result.assigned_policy.as_ref(), Some(policy));
}

async fn assert_readers(fixture: &Fixture, runner: &Uuid7, expected: bool) {
    let record = fixture
        .runners
        .self_record(runner)
        .await
        .expect("self record");
    assert_eq!(
        record
            .assignment
            .decode()
            .expect("self policy")
            .durable_tasks_enabled,
        expected
    );
    let beat = fixture
        .runners
        .heartbeat(runner, &NO_REPORT, None, clock::now())
        .await
        .expect("heartbeat reply");
    assert_eq!(
        beat.assignment
            .decode()
            .expect("heartbeat policy")
            .durable_tasks_enabled,
        expected
    );
    let detail = fixture
        .runners
        .runner_detail(runner)
        .await
        .expect("runner detail");
    assert_eq!(
        detail.item().assigned_policy().durable_tasks_enabled,
        expected
    );
    let boundary = KeysetCursor::new(
        detail.item().created_at() + std::time::Duration::from_secs(1),
        runner.clone(),
    );
    for cursor in [None, Some(&boundary)] {
        let page = fixture
            .runners
            .list_runners(cursor, 100)
            .await
            .expect("operator list page");
        let item = page
            .items()
            .iter()
            .find(|item| item.id() == runner)
            .expect("owned runner on page");
        assert_eq!(item.assigned_policy().durable_tasks_enabled, expected);
    }
}
