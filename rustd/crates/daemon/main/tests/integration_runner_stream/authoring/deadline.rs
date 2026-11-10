//! An expired answer wait cannot discard provider accounting already known by the daemon.

use super::{decision::Boundary, fixture::AuthoringFixture};
use af_core::{error_code, id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::authoring::{FailureReason, Outcome, Report};
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::error::Error as _;
use tokio::{sync::Semaphore, time::Instant};

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn known_usage_survives_the_model_answer_deadline_during_its_edit_check() {
    let boundary = Boundary::open().await;
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::with_decision(&mut supervisor, &boundary).await;
    propose_edit(&fixture).await;
    let lease = fixture.id();
    let model = fixture.model();
    let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
    let expired = Semaphore::new(0);
    let (result, ()) = tokio::join!(
        async {
            let result = fixture
                .session
                .authoring_model_until(&lease, &model, deadline)
                .await;
            expired.add_permits(1);
            result
        },
        cross_deadline(&fixture, &boundary, deadline, &expired)
    );
    let error =
        result.expect_err("the caller's answer budget expires while the decision is blocked");
    assert_eq!(error.code(), error_code::RUN_STREAM_DISCONNECTED);
    assert!(
        error
            .source()
            .is_some_and(<dyn std::error::Error + 'static>::is::<tokio::time::error::Elapsed>)
    );
    fixture.wait_for_known_receipt().await;
    assert!(
        fixture.dispatch().await.0.is_none(),
        "replacement still fences private output"
    );
    verify_accounting(&fixture, &model).await;
    assert_eq!(fixture.provider.state.count(), 1);
    assert_eq!(boundary.count(), 1);
    fixture.finish(supervisor).await;
    boundary.close().await;
}

async fn cross_deadline(
    fixture: &AuthoringFixture,
    boundary: &Boundary,
    deadline: Instant,
    expired: &Semaphore,
) {
    fixture.provider.state.arrived().await;
    fixture.provider.state.resume();
    boundary.arrived().await;
    assert!(
        Instant::now() < deadline,
        "the edit check precedes the caller deadline"
    );
    // This direct authority replacement emits no notification that could precede the answer timeout.
    fixture.replace_authority().await;
    expired
        .acquire()
        .await
        .expect("caller deadline observed")
        .forget();
    boundary.resume();
}

async fn verify_accounting(fixture: &AuthoringFixture, model: &af_wire::authoring::ModelCall) {
    let lease = fixture.id();
    let mut replacement = Session::connect(&fixture.config)
        .await
        .expect("current native owner");
    let mut accounting = fixture.report(1).accounting().clone();
    accounting.outcome = Outcome::Failed;
    accounting.failure_reason = Some(FailureReason::ProviderUsageUnknown);
    let report = Report::Accounting { accounting };
    assert!(
        replacement
            .authoring_report(&lease, &report)
            .await
            .expect("known accounting")
            .settled
    );
    let charged = fixture.charges().await;
    assert_eq!(
        charged.len(),
        3,
        "runtime, primary provider and edit decision"
    );
    assert_eq!(charged.get(1).map(|line| (line.1, line.2)), Some((1, 1)));
    assert_eq!(charged.get(2).map(|line| (line.1, line.2)), Some((100, 3)));
    assert_eq!(fixture.receipts().await, (1, 1, 1));
    assert!(
        replacement
            .authoring_report(&lease, &report)
            .await
            .expect("accounting replay")
            .replayed
    );
    assert_eq!(fixture.charges().await, charged);
    assert_eq!(
        replacement
            .authoring_model(&lease, model)
            .await
            .expect_err("settled work cannot redispatch")
            .code(),
        error_code::AUTHORING_FENCE
    );
    replacement.close().await.expect("replacement shutdown");
}

async fn propose_edit(fixture: &AuthoringFixture) {
    fixture
        .session
        .authoring_activity(&fixture.id(), &fixture.proposal())
        .await
        .expect("initial candidate");
    let draft = Uuid7::parse(&fixture.lease.draft_id).expect("draft identity");
    let candidate = fixture
        .drafts
        .read(fixture.access(), &draft)
        .await
        .expect("current setup")
        .candidate
        .expect("initialized candidate");
    fixture
        .provider
        .state
        .propose(json!({"kind":"edit","base_candidate_id":candidate.id,
        "base_candidate_revision":candidate.revision,"config_ops":[],
        "instruction_edits":[{"kind":"append","text":"\nPrefer clarity."}]}));
}
