//! Private fencing preserves authoritative usage while refusing stale mutations.

use super::fixture::AuthoringFixture;
use af_core::{error_code, id::Uuid7};
use af_wire::authoring::{Outcome, Report};
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stale_connection_cannot_call_propose_renew_or_complete() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    let lease = fixture.id();
    let previous = fixture.connection().await;
    fixture.replace_authority().await;
    assert_ne!(previous, fixture.connection().await);
    assert_eq!(
        fixture
            .session
            .authoring_model(&lease, &fixture.model())
            .await
            .expect_err("stale model call")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    assert_eq!(
        fixture
            .session
            .authoring_activity(&lease, &fixture.proposal())
            .await
            .expect_err("stale proposal")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    assert_eq!(
        fixture
            .session
            .authoring_renew(&lease, &fixture.renewal(0))
            .await
            .expect_err("stale renewal")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    assert_eq!(
        fixture
            .session
            .authoring_report(&lease, &fixture.report(0))
            .await
            .expect_err("stale completion")
            .code(),
        error_code::AUTHORING_NOT_FOUND
    );
    assert_eq!(fixture.provider.state.count(), 0);
    assert_eq!(fixture.receipts().await, (0, 0, 0));
    assert_eq!(fixture.charges().await, Vec::new());
    assert!(fixture.answer().await.is_none());
    fixture.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn replacement_during_provider_http_withholds_text_and_keeps_known_usage() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, true).await;
    let lease = fixture.id();
    let model = fixture.model();
    let (result, ()) = tokio::join!(fixture.session.authoring_model(&lease, &model), async {
        fixture.provider.state.arrived().await;
        fixture.replace_authority().await;
        fixture.provider.state.resume();
    });
    assert_eq!(
        result.expect_err("stale result withheld").code(),
        error_code::AUTHORING_NOT_FOUND
    );
    let (private, usage) = fixture.dispatch().await;
    assert!(private.is_none(), "stale private model text is not saved");
    assert_eq!(
        usage
            .expect("known provider accounting")
            .0
            .get("knowledge")
            .and_then(serde_json::Value::as_str),
        Some("known")
    );
    let mut replacement = Session::connect(&fixture.config)
        .await
        .expect("replacement native connection");
    let report = Report::Accounting {
        accounting: fixture.report(1).accounting().clone(),
    };
    assert!(
        replacement
            .authoring_report(&lease, &report)
            .await
            .expect("current owner accounts known usage")
            .settled
    );
    assert_eq!(fixture.charges().await.len(), 2);
    assert_eq!(fixture.provider.state.count(), 1);
    assert!(fixture.answer().await.is_none());
    replacement.close().await.expect("replacement shutdown");
    fixture.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn cancellation_during_provider_http_preserves_accounting_without_private_answer() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, true).await;
    let lease = fixture.id();
    let model = fixture.model();
    let (result, ()) = tokio::join!(fixture.session.authoring_model(&lease, &model), async {
        fixture.provider.state.arrived().await;
        fixture
            .drafts
            .cancel(
                fixture.access(),
                &Uuid7::parse(&fixture.lease.draft_id).expect("draft id"),
                1,
            )
            .await
            .expect("cancel accepted message");
        fixture.provider.state.resume();
    });
    assert_eq!(
        result.expect_err("cancelled model answer").code(),
        error_code::AUTHORING_FENCE
    );
    let (private, usage) = fixture.dispatch().await;
    assert!(private.is_none());
    assert_eq!(
        usage
            .expect("known usage survives cancellation")
            .0
            .get("knowledge")
            .and_then(serde_json::Value::as_str),
        Some("known")
    );
    let mut accounting = fixture.report(1).accounting().clone();
    accounting.outcome = Outcome::Cancelled;
    let report = Report::Accounting { accounting };
    let ack = fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("cancelled accounting");
    assert!(ack.settled);
    assert_eq!(ack.state, Outcome::Cancelled);
    assert_eq!(fixture.charges().await.len(), 2);
    assert!(fixture.answer().await.is_none());
    assert_eq!(fixture.provider.state.count(), 1);
    fixture.finish(supervisor).await;
}
