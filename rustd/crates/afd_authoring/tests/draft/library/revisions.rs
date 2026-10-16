//! Continued chat appends V2 to the selected entry and preserves V1.

use super::{Case, mint, now};
use afd_authoring::types::{PublishRevision, Published};
use afd_core::id::Uuid7;

/// Publishes `request` onto `first`'s entry under `first`'s tag — current on
/// the first call, stale on any replay after it advanced.
async fn revise(
    case: &Case,
    first: &Published,
    key: &Uuid7,
    request: PublishRevision,
) -> afd_authoring::Result<Published> {
    case.setup
        .publish_revision(
            case.base.access(),
            &first.library_id,
            Some(&first.etag),
            key,
            request,
            now(),
        )
        .await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn continued_draft_publishes_v2_without_rewriting_v1() {
    let mut case = Case::create().await;
    let draft = case.draft().await;
    let first = case.publish(&draft).await;
    let changed = case.edit(&draft).await;
    let candidate = changed.candidate.expect("revised candidate");
    let key = mint();
    let request = PublishRevision {
        draft_candidate_id: candidate.id,
    };
    let second = revise(&case, &first, &key, request.clone())
        .await
        .expect("publish V2");
    assert_eq!(second.library_id, first.library_id);
    assert_ne!(second.revision_id, first.revision_id);
    let old = case
        .setup
        .library(
            case.base.access(),
            &first.library_id,
            Some(&first.revision_id),
        )
        .await
        .expect("Inspect V1");
    assert_eq!(old.head.revision, "1");
    assert_eq!(
        old.head.definition.instructions_md(),
        "Investigate failures.\n"
    );
    let current = case
        .setup
        .library(case.base.access(), &first.library_id, None)
        .await
        .expect("Inspect head");
    assert_eq!(current.head.revision, "2");
    assert_eq!(current.head.definition, candidate.definition);
    assert_eq!(
        revise(&case, &first, &key, request)
            .await
            .expect("replay ignores stale original tag")
            .revision_id,
        second.revision_id
    );
    assert_eq!(case.counts().await, (1, 2, 0));
    case.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stale_library_tag_creates_no_revision_or_receipt() {
    let mut case = Case::create().await;
    let draft = case.draft().await;
    let first = case.publish(&draft).await;
    let changed = case.edit(&draft).await;
    let error = case
        .setup
        .publish_revision(
            case.base.access(),
            &first.library_id,
            Some("\"stale\""),
            &mint(),
            PublishRevision {
                draft_candidate_id: changed.candidate.expect("candidate").id,
            },
            now(),
        )
        .await
        .expect_err("stale tag");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-003");
    assert_eq!(error.etag(), Some(first.etag.as_str()));
    assert_eq!(case.counts().await, (1, 1, 0));
    case.base.cleanup().await;
}
