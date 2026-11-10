//! Revoked credentials and wrong attempt fences cannot mutate authoring state.

use super::fixture::AuthoringFixture;
use af_core::error_code::{AUTHORING_FENCE, AUTHORING_NOT_FOUND, ErrorCode};
use af_wire::authoring::Report;
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn changed_credential_fences_all_authoring_operations_on_the_existing_connection() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    let mut changed = fixture.token.clone();
    let last = changed.pop().expect("issued bearer is nonempty");
    changed.push(if last == 'A' { 'E' } else { 'A' });
    let presented = afd_auth::credential::Presented::new(&changed).expect("changed bearer shape");
    let digest = afd_auth::directory::Digest::of(&presented);
    sqlx::query("UPDATE runner.runners SET token_hash=$2 WHERE id=$1")
        .bind(&fixture.runner)
        .bind(digest.as_str())
        .execute(
            &mut *fixture
                .booted
                .database
                .acquire()
                .await
                .expect("credential rotation connection"),
        )
        .await
        .expect("replace opening credential digest");
    refused(&fixture, false, AUTHORING_NOT_FOUND).await;
    fixture.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn wrong_attempt_fence_refuses_all_authoring_operations_without_effects() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    refused(&fixture, true, AUTHORING_FENCE).await;
    fixture.finish(supervisor).await;
}

async fn refused(fixture: &AuthoringFixture, wrong_fence: bool, code: ErrorCode) {
    let lease = fixture.id();
    let mut model = fixture.model();
    let mut proposal = fixture.proposal();
    let mut renewal = fixture.renewal(0);
    let mut accounting = fixture.report(0).accounting().clone();
    if wrong_fence {
        for fence in [
            &mut model.fence,
            &mut proposal.fence,
            &mut renewal.fence,
            &mut accounting.fence,
        ] {
            *fence = "2".into();
        }
    }
    assert_refused(fixture.session.authoring_model(&lease, &model).await, code);
    assert_refused(
        fixture.session.authoring_activity(&lease, &proposal).await,
        code,
    );
    assert_refused(
        fixture.session.authoring_renew(&lease, &renewal).await,
        code,
    );
    let report = Report::Terminal {
        accounting,
        answer: Some(super::ANSWER.into()),
    };
    assert_refused(
        fixture.session.authoring_report(&lease, &report).await,
        code,
    );
    assert_eq!(fixture.provider.state.count(), 0);
    assert_eq!(fixture.receipts().await, (0, 0, 0));
    assert_eq!(fixture.charges().await, Vec::new());
    assert!(fixture.answer().await.is_none());
}

fn assert_refused<T: std::fmt::Debug>(result: afr_supervisor::Result<T>, code: ErrorCode) {
    assert_eq!(
        result
            .expect_err("authoring operation must be fenced")
            .code(),
        code
    );
}
