//! Refused or stale decisions retain chat behavior and charge measured usage.

use super::super::{fixture, text_edits::native};
use super::{
    fixture::{Case, SOAK, reply},
    server::Server,
};
use afd_wire::authoring::{ActivityEvent, ProposalCall};

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn malformed_or_stale_provider_decisions_record_fallback_without_blocking_chat() {
    for reason in ["malformed", "stale"] {
        let mut body = reply(0.01);
        if reason == "malformed" {
            *body.get_mut("answers").expect("fixture answers") = serde_json::json!({});
        } else {
            *body.get_mut("model").expect("fixture model") = "an-unreviewed-model".into();
        }
        let mut server = Server::start(body).await;
        let case = Case::create(&server, SOAK).await;
        let response = case.calls.call(
            &case.f.runner,
            &case.attempt.lease,
            fixture::request(&case.attempt),
        );
        let observe = async {
            server.received().await;
            server.finish().await;
        };
        let (response, ()) = tokio::join!(response, observe);
        let response = response.expect("fallback conversation");
        assert!(matches!(
            response.proposal_calls.as_slice(),
            [ProposalCall::Proposed { .. }]
        ));
        assert_ne!(response.text, "");
        assert_eq!(case.journal().await, (reason.into(), Some(112), true));
        let mut activity = case.attempt.initialize(1);
        activity.event = ActivityEvent::Proposal {
            proposal: native(&response).clone(),
        };
        assert_eq!(
            case.f
                .drafts
                .activity(&case.f.runner, &case.attempt.lease, activity)
                .await
                .expect("existing behavior")
                .candidate
                .expect("candidate")
                .revision,
            "2"
        );
        case.finish().await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_revoked_profile_makes_the_inflight_answer_stale_but_retains_usage() {
    let mut server = Server::start(reply(0.01)).await;
    let mut case = Case::create(&server, SOAK).await;
    let calls = case.calls.clone();
    let response = calls.call(
        &case.f.runner,
        &case.attempt.lease,
        fixture::request(&case.attempt),
    );
    let observe = async {
        server.received().await;
        case.model
            .disable()
            .await
            .expect("decision model fixture lifecycle");
        server.finish().await;
    };
    let (response, ()) = tokio::join!(response, observe);
    assert_eq!(
        response
            .expect_err("catalogue revocation also fences the pinned chat model")
            .code(),
        afd_core::error_code::AUTHORING_MODEL
    );
    assert_eq!(case.journal().await, ("stale".into(), Some(112), true));
    let receipt: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(case.attempt.id.as_str()).fetch_one(&mut *case.f.connection().await).await.expect("revoked result retains only accounting");
    assert_eq!(receipt, (true, "known".into()));
    case.finish().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn missing_decision_configuration_preserves_the_existing_chat_path() {
    let server = Server::start(reply(0.01)).await;
    let case = Case::create(&server, SOAK).await;
    sqlx::query("UPDATE runner.runners SET decision_platform_model_id=NULL WHERE id=$1::uuid")
        .bind(case.f.runner.as_str())
        .execute(&mut *case.f.connection().await)
        .await
        .expect("unconfigured runner");
    let response = case
        .calls
        .call(
            &case.f.runner,
            &case.attempt.lease,
            fixture::request(&case.attempt),
        )
        .await
        .expect("fallback without model call");
    assert!(matches!(
        response.proposal_calls.as_slice(),
        [ProposalCall::Proposed { .. }]
    ));
    assert_eq!(case.journal().await, ("unavailable".into(), None, true));
    drop(server);
    case.finish().await;
}
