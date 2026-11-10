//! Changed identities cannot replace saved results or duplicate physical work and billing.

use super::{ANSWER, fixture::AuthoringFixture};
use af_core::error_code::AUTHORING_REPLAY;
use af_wire::authoring::{ActivityEvent, Report};
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn changed_replay_content_cannot_dispatch_publish_or_charge_again() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, false).await;
    refuse_changed_model(&fixture).await;
    refuse_changed_proposal(&fixture).await;
    refuse_changed_completion(&fixture).await;
    assert_eq!(fixture.provider.state.count(), 1);
    assert_eq!(fixture.receipts().await, (1, 1, 1));
    assert_eq!(fixture.answer().await.as_deref(), Some(ANSWER));
    fixture.finish(supervisor).await;
}

async fn refuse_changed_model(fixture: &AuthoringFixture) {
    let lease = fixture.id();
    let mut model = fixture.model();
    fixture
        .session
        .authoring_model(&lease, &model)
        .await
        .expect("initial model call");
    model.request.max_output_tokens += 1;
    assert_eq!(
        fixture
            .session
            .authoring_model(&lease, &model)
            .await
            .expect_err("changed model replay")
            .code(),
        AUTHORING_REPLAY
    );
}

async fn refuse_changed_proposal(fixture: &AuthoringFixture) {
    let lease = fixture.id();
    let mut proposal = fixture.proposal();
    fixture
        .session
        .authoring_activity(&lease, &proposal)
        .await
        .expect("initial proposal");
    proposal.event = ActivityEvent::Progress {
        text: "changed replay".into(),
    };
    assert_eq!(
        fixture
            .session
            .authoring_activity(&lease, &proposal)
            .await
            .expect_err("changed proposal replay")
            .code(),
        AUTHORING_REPLAY
    );
}

async fn refuse_changed_completion(fixture: &AuthoringFixture) {
    let lease = fixture.id();
    let mut report = fixture.report(1);
    fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("initial completion");
    let charged = fixture.charges().await;
    if let Report::Terminal { answer, .. } = &mut report {
        *answer = Some("changed answer".into());
    }
    assert_eq!(
        fixture
            .session
            .authoring_report(&lease, &report)
            .await
            .expect_err("changed completion replay")
            .code(),
        AUTHORING_REPLAY
    );
    assert_eq!(fixture.charges().await, charged);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn disconnect_keeps_the_provider_receipt_for_native_reconnect_without_redispatch() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, true).await;
    let lease = fixture.id();
    let model = fixture.model();
    let (result, ()) = tokio::join!(fixture.session.authoring_model(&lease, &model), async {
        fixture.provider.state.arrived().await;
        fixture.session.disconnect();
        fixture.provider.state.resume();
    });
    assert!(
        result.is_err(),
        "the interrupted request cannot receive an acknowledgement"
    );
    fixture.wait_for_known_receipt().await;
    let mut replacement = afr_supervisor::test_util::Session::connect(&fixture.config)
        .await
        .expect("new native connection");
    let saved = replacement
        .authoring_model(&lease, &model)
        .await
        .expect("replay after reconnect");
    assert!(saved.replayed);
    assert_eq!(saved.text, ANSWER);
    assert_eq!(fixture.provider.state.count(), 1);
    assert!(
        replacement
            .authoring_report(&lease, &fixture.report(1))
            .await
            .expect("completion after reconnect")
            .settled
    );
    replacement.close().await.expect("new connection shutdown");
    assert_eq!(fixture.answer().await.as_deref(), Some(ANSWER));
    assert_eq!(fixture.charges().await.len(), 2);
    fixture.finish(supervisor).await;
}
