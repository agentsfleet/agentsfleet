//! All four operations share the opened session and replay their original receipts.

use super::{
    ANSWER,
    fixture::{AuthoringFixture, meter},
};
use af_wire::authoring::{ModelCall, ModelResult, ModelUsage, Outcome, Renew};
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn model_proposal_renewal_and_completion_share_one_native_session() {
    let mut supervisor = Supervisor::new();
    let fixture = AuthoringFixture::start(&mut supervisor, true).await;
    let connection = fixture.connection().await;
    let request = fixture.model();
    let model = invoke_and_renew(&fixture, &request).await;
    assert_eq!(model.text, ANSWER);
    assert!(matches!(
        model.usage,
        ModelUsage::Known {
            input_tokens: 1,
            output_tokens: 1,
            ..
        }
    ));
    let saved = fixture
        .dispatch()
        .await
        .0
        .expect("Postgres saves model result");
    assert_eq!(
        saved.0.get("text").and_then(serde_json::Value::as_str),
        Some(ANSWER)
    );
    let replayed = fixture
        .session
        .authoring_model(&fixture.id(), &request)
        .await
        .expect("saved model replay");
    assert!(replayed.replayed);
    assert_eq!(replayed.text, model.text);
    assert_eq!(fixture.provider.state.count(), 1);
    proposal_replay(&fixture).await;
    completion_replay(&fixture).await;
    assert_eq!(
        fixture.connection().await,
        connection,
        "no operation reopened the connection"
    );
    fixture.finish(supervisor).await;
}

async fn invoke_and_renew(fixture: &AuthoringFixture, request: &ModelCall) -> ModelResult {
    let lease = fixture.id();
    let (model, ()) = tokio::join!(fixture.session.authoring_model(&lease, request), async {
        fixture.provider.state.arrived().await;
        let renewed = fixture
            .session
            .authoring_renew(
                &lease,
                &Renew {
                    fence: fixture.lease.fence.clone(),
                    usage: meter(1),
                },
            )
            .await
            .expect("renewal proceeds while provider HTTP is blocked");
        assert!(!renewed.stop_requested);
        assert!(renewed.expires_at <= fixture.lease.deadline);
        assert_eq!(renewed.expires_at, renewed.model_access_expires_at);
        fixture.provider.state.resume();
    });
    model.expect("native model result")
}

async fn proposal_replay(fixture: &AuthoringFixture) {
    let lease = fixture.id();
    let proposal = fixture.proposal();
    let accepted = fixture
        .session
        .authoring_activity(&lease, &proposal)
        .await
        .expect("native proposal");
    assert!(accepted.accepted);
    let replayed = fixture
        .session
        .authoring_activity(&lease, &proposal)
        .await
        .expect("proposal receipt replay");
    assert!(replayed.replayed);
    assert_eq!(accepted.candidate, replayed.candidate);
    assert!(accepted.candidate.is_some());
}

async fn completion_replay(fixture: &AuthoringFixture) {
    let lease = fixture.id();
    let report = fixture.report(1);
    let completed = fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("native completion");
    assert!(completed.settled && !completed.replayed);
    assert_eq!(completed.state, Outcome::Completed);
    assert_eq!(fixture.answer().await.as_deref(), Some(ANSWER));
    let charged = fixture.charges().await;
    assert_eq!(charged.len(), 2, "provider and runtime lines");
    assert_eq!(charged.get(1).map(|line| (line.1, line.2)), Some((1, 1)));
    let replayed = fixture
        .session
        .authoring_report(&lease, &report)
        .await
        .expect("completion replay");
    assert!(replayed.settled && replayed.replayed);
    assert_eq!(fixture.charges().await, charged);
    assert_eq!(fixture.receipts().await, (1, 1, 1));
}
