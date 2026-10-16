//! A model selection changes future execution while preserving reviewed content.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::types::DraftPatch;
use afd_core::error_code::{AUTHORING_PRECONDITION, AUTHORING_STALE, INVALID_REQUEST};
use serde_json::json;

fn patch(value: serde_json::Value) -> DraftPatch {
    serde_json::from_value(value).expect("typed patch")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn authoring_model_replacement_preserves_candidate_and_obeys_strong_tag() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("native candidate")
        .draft;
    let candidate = created.candidate.as_ref().expect("initial candidate");
    let replacement = || patch(json!({"authoring_model_ref": f.other_model}));
    let error = f
        .drafts
        .patch(f.access(), &created.id, None, replacement(), now())
        .await
        .expect_err("missing tag");
    assert_eq!(error.code(), AUTHORING_PRECONDITION);
    let changed = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&created.etag),
            replacement(),
            now(),
        )
        .await
        .expect("explicit replacement");
    assert_eq!(changed.authoring_model_ref, f.other_model.to_string());
    assert_ne!(changed.etag, created.etag);
    assert_eq!(
        changed.candidate.as_ref().expect("candidate retained").id,
        candidate.id
    );
    assert_eq!(
        changed
            .candidate
            .as_ref()
            .expect("digest retained")
            .canonical_digest,
        candidate.canonical_digest
    );
    assert_stale(&f, &created, &changed).await;
    assert_eq!(f.counts().await, (1, 1, 0));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn invalid_full_edit_rolls_back_title_and_candidate_pointer() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("native candidate")
        .draft;
    // A zero ceiling is refused only once the whole candidate is revalidated,
    // after the title has been written inside the same transaction.
    let invalid = patch(json!({"title":"must roll back", "config_ops":[
        {"op":"replace","path":"/config/x-agentsfleet/budget/daily_nanos","value":"0"}
    ]}));
    let error = f
        .drafts
        .patch(f.access(), &created.id, Some(&created.etag), invalid, now())
        .await
        .expect_err("invalid full candidate must be refused");
    assert_eq!(error.code(), INVALID_REQUEST);
    let unchanged = f
        .drafts
        .read(f.access(), &created.id)
        .await
        .expect("reload after refusal");
    assert_eq!(unchanged.title, created.title);
    assert_eq!(unchanged.etag, created.etag);
    assert_eq!(f.counts().await, (1, 1, 0));
    f.cleanup().await;
}

async fn assert_stale(
    f: &Fixture,
    created: &afd_authoring::types::Draft,
    changed: &afd_authoring::types::Draft,
) {
    let error = f
        .drafts
        .patch(
            f.access(),
            &created.id,
            Some(&created.etag),
            patch(json!({"title":"stale"})),
            now(),
        )
        .await
        .expect_err("old tag cannot overwrite replacement");
    assert_eq!(error.code(), AUTHORING_STALE);
    assert_eq!(error.etag(), Some(changed.etag.as_str()));
}
