//! Unresolved provider obligations and private deletion cannot erase accounting receipts.

use super::{ANSWER, fixture::AuthoringFixture};
use af_core::{error_code, id::Uuid7};
use af_wire::authoring::{FailureReason, Knowledge, Outcome, Report};
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn unknown_completion_keeps_its_obligation_until_the_same_report_can_settle() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, true).await;
    let lease = fixture.id();
    let model = fixture.model();
    let report = unknown_report(&fixture);
    let (result, ()) = tokio::join!(fixture.session.authoring_model(&lease, &model), async {
        fixture.provider.state.arrived().await;
        let first = fixture
            .session
            .authoring_report(&lease, &report)
            .await
            .expect("pending accounting acknowledgement");
        assert!(!first.settled && !first.replayed);
        assert!(
            fixture.charges().await.is_empty(),
            "unknown provider cost cannot be invented"
        );
        let replayed = fixture
            .session
            .authoring_report(&lease, &report)
            .await
            .expect("pending report replay");
        assert!(!replayed.settled && replayed.replayed);
        fixture.provider.state.resume();
    });
    assert_eq!(
        result
            .expect_err("private model result withheld after terminal accounting")
            .code(),
        error_code::AUTHORING_FENCE
    );
    let settled = fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("same report settles authoritative usage");
    assert!(settled.settled && settled.replayed);
    assert_eq!(fixture.charges().await.len(), 2);
    assert_eq!(fixture.receipts().await, (1, 0, 1));
    assert_eq!(fixture.provider.state.count(), 1);
    assert!(fixture.answer().await.is_none());
    fixture.finish(supervisor).await;
}

fn unknown_report(fixture: &AuthoringFixture) -> Report {
    let mut accounting = fixture.report(1).accounting().clone();
    accounting.outcome = Outcome::Failed;
    accounting.failure_reason = Some(FailureReason::ProviderUsageUnknown);
    accounting.usage.knowledge = Knowledge::Unknown;
    Report::Accounting { accounting }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn completion_accounting_replays_after_private_draft_deletion_without_another_charge() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    let lease = fixture.id();
    fixture
        .session
        .authoring_model(&lease, &fixture.model())
        .await
        .expect("native model result");
    let report = fixture.report(1);
    let first = fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("native terminal report");
    assert!(first.settled);
    assert_eq!(fixture.answer().await.as_deref(), Some(ANSWER));
    let charged = fixture.charges().await;
    let draft = Uuid7::parse(&fixture.lease.draft_id).expect("draft identity");
    fixture
        .drafts
        .delete(fixture.access(), &draft)
        .await
        .expect("erase private draft");
    let recovery = Report::Accounting {
        accounting: report.accounting().clone(),
    };
    let replayed = fixture
        .session
        .authoring_report(&lease, &recovery)
        .await
        .expect("content-free completion replay");
    assert!(replayed.settled && replayed.replayed);
    assert_eq!(replayed.state, first.state);
    assert_eq!(fixture.charges().await, charged);
    assert_eq!(fixture.provider.state.count(), 1);
    assert!(
        fixture.dispatch().await.0.is_none(),
        "private model result is erased"
    );
    fixture.finish(supervisor).await;
}
