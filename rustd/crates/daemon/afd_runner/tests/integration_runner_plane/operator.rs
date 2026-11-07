//! An operator's transitions, edits and reads, against rows the tests enrol.

use std::borrow::Cow;

use afd_api_wire::admin::{AdminState, RunnerAdminAction, RunnerEventType};
use afd_core::clock;
use afd_core::test_util::trace::Capture;
use afd_runner::sql::NEVER_SEEN;
use afd_runner::{DETAIL_SELFTEST_REFUSED, KeysetCursor, RunnerEdit};
use afd_wire::runner::{BindMode, ExtraBind, RunnerLiveness};
use serde_json::Value;

use super::{Fixture, enrolment};
use crate::rows::{self, AttemptRow};
use afd_db::test_util::mint_id;

/// Who the operator transitions are recorded as.
const ACTOR: &str = "user_operator";

/// What the retired runner's attempt held against the wallet.
const RESERVED_NANOS: i64 = 4_200;

/// Whether an attempt still names a runner, and what it reserved.
const READ_ATTEMPT: &str = "\
SELECT runner_id IS NULL AS runner_gone, reserved_nanos
FROM authoring.attempts WHERE id = $1::uuid";

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

/// The deletion matrix: a revoked runner's record retires, and the attempts it
/// ran stay with their money, naming no runner.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_retiring_a_runner_keeps_the_attempts_it_ran_and_their_reservations() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let mut connection = fixture.owner.acquire().await.expect("a seed connection");
    let attempt = rows::attempt(
        &mut connection,
        (runner.as_str(), &mint_id(), &mint_id()),
        AttemptRow {
            status: afd_runner::sql::LEASE_STATUS_EXPIRED,
            lease_expires_at: clock::now(),
            reserved_nanos: RESERVED_NANOS,
        },
    )
    .await;

    fixture
        .runners
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    fixture
        .runners
        .delete_revoked(&runner)
        .await
        .expect("a runner that ran an attempt still retires");

    let (runner_gone, reserved): (bool, i64) = sqlx::query_as(READ_ATTEMPT)
        .bind(&attempt)
        .fetch_one(&mut *connection)
        .await
        .expect("the attempt outlives its runner");
    assert!(runner_gone, "the attempt names no runner once it retires");
    assert_eq!(reserved, RESERVED_NANOS, "its reservation is untouched");
    sqlx::query("DELETE FROM authoring.attempts WHERE id = $1::uuid")
        .bind(&attempt)
        .execute(&mut *connection)
        .await
        .expect("the seeded attempt cleans up");
    drop(connection);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_edit_carrying_nothing_is_refused_before_any_write() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let before = fixture.history(&runner).await.len();

    let refused = fixture
        .runners
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: None,
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect_err("an edit with no policy and no labels changes nothing");

    assert!(refused.is_rejected(), "{refused}");
    assert_eq!(
        fixture.history(&runner).await.len(),
        before,
        "a refused edit appends no event"
    );
    fixture.cleanup().await;
}

/// A rotation answers the new credential once and records it; a revoked
/// runner's is refused inside the same statement, so it records nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_rotation_lands_for_a_live_runner_and_is_refused_once_revoked() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let capture = Capture::install();

    fixture
        .runners
        .rotate_token(&runner, ACTOR, clock::now())
        .await
        .expect("a live runner rotates");
    let rotated = capture.only("runner_token_rotated");
    assert_eq!(rotated.field("runner_id"), Some(runner.as_str()));
    let history = fixture.history(&runner).await;
    assert_eq!(
        history.first().map(|event| event.event_type),
        Some(RunnerEventType::RunnerTokenRotated)
    );

    fixture
        .runners
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    let refused = fixture
        .runners
        .rotate_token(&runner, ACTOR, clock::now())
        .await
        .expect_err("a revoked runner holds no credential to rotate");

    assert!(refused.is_rejected(), "{refused}");
    assert_eq!(
        fixture
            .history(&runner)
            .await
            .first()
            .map(|event| event.event_type),
        Some(RunnerEventType::RunnerRevoked),
        "the refused rotation appended nothing after the revoke"
    );
    drop(capture);
    fixture.cleanup().await;
}
