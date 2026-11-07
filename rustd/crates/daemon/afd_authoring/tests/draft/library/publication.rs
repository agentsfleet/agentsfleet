//! Explicit publication leaves its source chat open and retries one snapshot.

use super::{Case, mint, now};
use afd_authoring::types::{DraftState, PublishLibrary};
use afd_core::id::Uuid7;

/// The code a publication under an already-used `key` is refused with.
async fn refusal(case: &Case, key: &Uuid7, request: PublishLibrary) -> String {
    case.setup
        .publish(case.base.access(), key, request, now())
        .await
        .expect_err("changed key body")
        .code()
        .as_str()
        .to_owned()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn explicit_publication_is_replayable_and_keeps_the_draft_editable() {
    let mut case = Case::create().await;
    let draft = case.draft().await;
    let request = PublishLibrary {
        draft_candidate_id: draft.candidate.as_ref().expect("candidate").id.clone(),
        label: "ARGO".into(),
    };
    let key = mint();
    let first = case
        .setup
        .publish(case.base.access(), &key, request.clone(), now())
        .await
        .expect("publish");
    case.base.own_library(&first.library_id);
    let replay = case
        .setup
        .publish(case.base.access(), &key, request.clone(), now())
        .await
        .expect("exact replay");
    assert_eq!(replay.revision_id, first.revision_id);
    let current = case
        .base
        .drafts
        .read(case.base.access(), &draft.id)
        .await
        .expect("private source");
    assert_eq!(current.state, DraftState::Editing);
    assert_eq!(
        current
            .publication
            .as_ref()
            .expect("verified receipt")
            .revision_id,
        first.revision_id
    );
    assert_eq!(
        case.counts().await,
        (1, 1, 0),
        "publication does not create a fleet"
    );
    let mut changed = request;
    changed.label = "different body".into();
    assert_eq!(refusal(&case, &key, changed).await, "UZ-AUTHOR-005");
    case.edit(&current).await;
    case.base.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn one_hundred_concurrent_publications_share_one_entry_and_revision() {
    let mut case = Case::create().await;
    let draft = case.draft().await;
    let request = PublishLibrary {
        draft_candidate_id: draft.candidate.expect("candidate").id,
        label: "ARGO".into(),
    };
    let key = mint();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(100));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let (setup, workspace, subject, key, request, barrier) = (
            case.setup.clone(),
            case.base.workspace.clone(),
            case.base.owner.clone(),
            key.clone(),
            request.clone(),
            std::sync::Arc::clone(&barrier),
        );
        tasks.spawn(async move {
            barrier.wait().await;
            setup
                .publish(
                    afd_authoring::Access {
                        workspace: &workspace,
                        subject: subject.as_str(),
                    },
                    &key,
                    request,
                    now(),
                )
                .await
        });
    }
    let mut identity = None;
    while let Some(result) = tasks.join_next().await {
        let result = result
            .expect("publication task")
            .expect("concurrent replay");
        case.base.own_library(&result.library_id);
        let accepted = (result.library_id, result.revision_id);
        match &identity {
            Some(first) => assert_eq!(&accepted, first),
            None => identity = Some(accepted),
        }
    }
    assert_eq!(case.counts().await, (1, 1, 0));
    assert_eq!(
        case.base.reported(super::funnel::PUBLISHED).len(),
        1,
        "a hundred racing replays report one publication"
    );
    case.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_candidate_under_retired_policy_semantics_publishes_nothing() {
    let case = Case::create().await;
    let draft = case.draft().await;
    let retired = case.base.retire_head(&draft.id).await;
    let refused = case
        .setup
        .publish(
            case.base.access(),
            &mint(),
            PublishLibrary {
                draft_candidate_id: retired,
                label: "ARGO".into(),
            },
            now(),
        )
        .await
        .expect_err("stale policy needs a fresh review before it is shared");
    assert_eq!(refused.code(), afd_core::error_code::AUTHORING_POLICY);
    assert_eq!(case.counts().await, (0, 0, 0));
    case.base.cleanup().await;
}
