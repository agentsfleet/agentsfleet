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
            body["answers"] = serde_json::json!({});
        } else {
            body["model"] = "an-unreviewed-model".into();
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
            &response.proposal_calls[..],
            [ProposalCall::Proposed { .. }]
        ));
        assert!(!response.text.is_empty());
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
        case.model.disable().await;
        server.finish().await;
    };
    let (response, ()) = tokio::join!(response, observe);
    let response = response.expect("stale decision fallback");
    assert!(matches!(
        &response.proposal_calls[..],
        [ProposalCall::Proposed { .. }]
    ));
    assert_eq!(case.journal().await, ("stale".into(), Some(112), true));
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
        &response.proposal_calls[..],
        [ProposalCall::Proposed { .. }]
    ));
    assert_eq!(case.journal().await, ("unavailable".into(), None, true));
    drop(server);
    case.finish().await;
}
