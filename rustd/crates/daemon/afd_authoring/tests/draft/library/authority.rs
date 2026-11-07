//! Publication preserves creator privacy and rejects busy or obsolete candidates.

use super::{Case, mint, now};
use afd_authoring::types::PublishLibrary;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_member_cannot_publish_someone_elses_private_candidate() {
    let case = Case::create().await;
    let draft = case.draft().await;
    let error = case
        .setup
        .publish(
            case.base.member_access(),
            &mint(),
            PublishLibrary {
                draft_candidate_id: draft.candidate.expect("candidate").id,
                label: "foreign draft".into(),
            },
            now(),
        )
        .await
        .expect_err("creator boundary");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-001");
    assert_eq!(case.counts().await, (0, 0, 0));
    case.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn queued_messages_and_old_candidate_identity_refuse_publication() {
    let case = Case::create().await;
    let draft = case
        .base
        .drafts
        .create(
            case.base.access(),
            case.base.request(mint(), Some("Keep checking")),
            now(),
        )
        .await
        .expect("queued draft")
        .draft;
    let error = case
        .setup
        .publish(
            case.base.access(),
            &mint(),
            PublishLibrary {
                draft_candidate_id: draft.candidate.expect("candidate").id,
                label: "busy".into(),
            },
            now(),
        )
        .await
        .expect_err("busy draft");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-002");
    let idle = case.draft().await;
    case.edit(&idle).await;
    let error = case
        .setup
        .publish(
            case.base.access(),
            &mint(),
            PublishLibrary {
                draft_candidate_id: idle.candidate.expect("old candidate").id,
                label: "obsolete".into(),
            },
            now(),
        )
        .await
        .expect_err("candidate changed");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-002");
    assert_eq!(case.counts().await, (0, 0, 0));
    case.base.cleanup().await;
}
