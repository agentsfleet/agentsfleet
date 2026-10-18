//! An operator's transitions, edits and reads, against rows the tests enrol.

use std::borrow::Cow;

use afd_api_wire::admin::{AdminState, RunnerAdminAction, RunnerEventType};
use afd_core::clock;
use afd_runner::sql::NEVER_SEEN;
use afd_runner::{DETAIL_SELFTEST_REFUSED, KeysetCursor, RunnerEdit};
use afd_wire::runner::{BindMode, ExtraBind, RunnerLiveness};
use serde_json::Value;

use super::{Fixture, enrolment};

/// Who the operator transitions are recorded as.
const ACTOR: &str = "user_operator";

/// A bind the sandbox validator accepts.
const BIND_PATH: &str = "/srv/models";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_drain_records_its_status_and_a_self_test_ask() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let runners = &fixture.runners;

    let draining = runners
        .transition(&runner, RunnerAdminAction::Drain, ACTOR, clock::now())
        .await
        .expect("a drain applies");
    assert_eq!(draining, AdminState::Draining);
    let history = fixture.history(&runner).await;
    let drained = history.first().expect("the transition is newest");
    assert_eq!(drained.event_type, RunnerEventType::RunnerDraining);
    assert_eq!(
        drained.metadata.get("from_status"),
        Some(&Value::from("active"))
    );
    assert_eq!(
        drained.metadata.get("to_status"),
        Some(&Value::from("draining"))
    );
    assert_eq!(drained.metadata.get("actor_id"), Some(&Value::from(ACTOR)));

    let asked = runners
        .request_selftest(&runner)
        .await
        .expect("a self-test is asked of a live runner");
    assert_eq!(asked.status(), AdminState::Draining);
    let detail = runners
        .runner_detail(&runner)
        .await
        .expect("the detail reads");
    assert_eq!(detail.selftest_requested_at(), Some(asked.requested_at()));

    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_revoke_is_terminal_until_the_record_retires() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let runners = &fixture.runners;

    runners
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    let refused = runners
        .request_selftest(&runner)
        .await
        .expect_err("a revoked runner is asked nothing");
    assert_eq!(refused.detail(), DETAIL_SELFTEST_REFUSED);
    let reopened = runners
        .transition(&runner, RunnerAdminAction::Cordon, ACTOR, clock::now())
        .await
        .expect_err("revoked is terminal");
    assert!(reopened.is_rejected(), "{reopened}");

    runners
        .delete_revoked(&runner)
        .await
        .expect("a revoked runner's record retires");
    let gone = runners
        .self_record(&runner)
        .await
        .expect_err("the retired row is gone");
    assert!(gone.is_runner_vanished(), "{gone}");
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_operator_reads_derive_liveness_and_page_by_creation() {
    let mut fixture = Fixture::create().await;
    let older = fixture.enrol().await;
    let newer = fixture.enrol().await;
    let runners = &fixture.runners;

    let detail = runners
        .runner_detail(&newer)
        .await
        .expect("the detail reads");
    let item = detail.item();
    assert_eq!(item.status(), AdminState::Active);
    assert_eq!(item.liveness(), RunnerLiveness::Registered);
    assert_eq!(item.last_seen_at(), NEVER_SEEN);
    assert_eq!(detail.active_lease_count(), 0);
    let registered = fixture.history(&newer).await;
    let enrolment_event = registered.first().expect("the enrolment is recorded");
    assert_eq!(
        enrolment_event.occurred_at,
        item.created_at(),
        "the enrolment event takes the row's own stamp"
    );

    runners
        .heartbeat(&older, &afd_runner::NO_REPORT, None, clock::now())
        .await
        .expect("the older runner beats");
    let beaten = runners
        .runner_detail(&older)
        .await
        .expect("the detail reads");
    assert_eq!(beaten.item().liveness(), RunnerLiveness::Online);

    let boundary = KeysetCursor::new(item.created_at(), item.id().clone());
    let page = runners
        .list_runners(Some(&boundary), 1)
        .await
        .expect("a page after the cursor reads");
    let next = page
        .items()
        .first()
        .expect("the older runner lies behind it");
    assert!(
        (next.created_at(), next.id().as_str()) < (item.created_at(), item.id().as_str()),
        "a page resumes strictly after its cursor"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_edit_stores_its_binds_in_the_sandbox_configuration() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let bind = ExtraBind {
        path: Cow::Borrowed(BIND_PATH),
        mode: BindMode::ReadOnly,
        note: Cow::Borrowed("model cache"),
    };
    let mut policy = enrolment().assigned_policy;
    policy.extra_binds = vec![bind.clone()];
    let labels = [Cow::Borrowed("plane"), Cow::Borrowed("edited")];

    let edited = fixture
        .runners
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: Some(&policy),
                labels: Some(&labels),
            },
            clock::now(),
        )
        .await
        .expect("the edit applies");
    assert_eq!(edited.status, AdminState::Active);

    let stored = fixture
        .runners
        .self_record(&runner)
        .await
        .expect("the edited row reads");
    let decoded = stored.assignment.decode().expect("the assignment decodes");
    assert_eq!(decoded.extra_binds, vec![bind]);
    let history = fixture.history(&runner).await;
    assert_eq!(
        history.first().map(|event| event.event_type),
        Some(RunnerEventType::RunnerPolicyAssigned)
    );
    let detail = fixture
        .runners
        .runner_detail(&runner)
        .await
        .expect("the detail reads");
    assert_eq!(detail.item().labels(), ["plane", "edited"]);
    fixture.cleanup().await;
}
