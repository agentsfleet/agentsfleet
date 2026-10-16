//! Lost creation replies reuse one atomic native setup and first request.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_core::error_code::{AUTHORING_MODEL, AUTHORING_REPLAY};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn concurrent_creation_replays_normalized_text_without_duplication() {
    let f = Fixture::create().await;
    let operation = mint();
    let (left, right) = tokio::join!(
        f.drafts.create(
            f.access(),
            f.request(operation.clone(), Some("Investigate\r\ncheckout")),
            now()
        ),
        f.drafts.create(
            f.access(),
            f.request(operation.clone(), Some("Investigate\ncheckout")),
            now()
        )
    );
    let left = left.expect("first concurrent create answers");
    let right = right.expect("second concurrent create answers");
    assert_eq!(left.draft.id, right.draft.id);
    assert_ne!(
        left.accepted.expect("first message").replayed,
        right.accepted.expect("first message").replayed
    );
    assert_eq!(f.counts().await, (1, 1, 1));
    let error = f
        .drafts
        .create(
            f.access(),
            f.request(operation, Some("different text")),
            now(),
        )
        .await
        .expect_err("accepted keys cannot authorize changed text");
    assert_eq!(error.code(), AUTHORING_REPLAY);
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn refused_creation_leaves_no_receipt_candidate_or_message() {
    let f = Fixture::create().await;
    let operation = mint();
    let mut invalid = f.request(operation.clone(), Some("Investigate"));
    invalid.authoring_model_ref = mint().to_string();
    let error = f
        .drafts
        .create(f.access(), invalid, now())
        .await
        .expect_err("unknown model");
    assert_eq!(error.code(), AUTHORING_MODEL);
    assert_eq!(f.counts().await, (0, 0, 0));
    let created = f
        .drafts
        .create(f.access(), f.request(operation, Some("Investigate")), now())
        .await
        .expect("a refused operation was never accepted");
    assert!(!created.accepted.expect("first message").replayed);
    assert_eq!(f.counts().await, (1, 1, 1));
    f.cleanup().await;
}
