//! Staging for the platform Library is one mutation per key: two stagings of
//! one candidate under one key stage one entry, and the operator list reads it.

use super::{Case, mint, now};
use afd_authoring::types::{DraftPatch, PublishLibrary};
use serde_json::json;
use sqlx::Connection as _;

/// Holds the draft row the way a staging's capture locks it.
const LOCK_DRAFT: &str =
    "SELECT pg_backend_pid() FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn two_stagings_under_one_key_stage_one_platform_entry() {
    let mut c = Case::create().await;
    let draft = c.draft().await;
    // The slug is the definition's name: this proof's own, so no other
    // staging in the lane can collide with it.
    let slug = format!(
        "stage-{}",
        mint().to_string().replace('-', "").get(20..).unwrap_or("x")
    );
    let rename: DraftPatch = serde_json::from_value(json!({"config_ops":[
        {"op":"replace","path":"/config/name","value":slug}]}))
    .expect("rename input");
    let renamed = c
        .base
        .drafts
        .patch(c.base.access(), &draft.id, Some(&draft.etag), rename, now())
        .await
        .expect("the candidate takes the proof's slug");
    c.base.own_platform_library(&slug);
    let request = PublishLibrary {
        draft_candidate_id: renamed.candidate.expect("renamed candidate").id,
        label: "Staged once".into(),
    };
    let key = mint();
    let mut holder = c.base.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar(LOCK_DRAFT)
        .bind(draft.id.as_str())
        .fetch_one(&mut *held)
        .await
        .expect("hold the draft");
    let stage = || {
        c.setup
            .stage_platform_library(c.base.access(), &key, request.clone(), now())
    };
    let (first, second, ()) = tokio::join!(stage(), stage(), async {
        c.base.waits_on(pid, 2).await;
        held.rollback().await.expect("release the draft");
    });
    drop(holder);
    let (first, second) = (
        first.expect("one staging lands"),
        second.expect("the other replays it"),
    );
    assert_eq!(first.revision_id, second.revision_id);
    assert_eq!(first.library_id, slug);
    let listed = c
        .setup
        .platform_libraries()
        .await
        .expect("the operator list reads");
    let staged: Vec<_> = listed
        .iter()
        .filter(|entry| entry.card.id == slug)
        .collect();
    assert_eq!(staged.len(), 1, "one entry for one key");
    assert!(
        staged.iter().all(|entry| !entry.published),
        "staged, not yet published"
    );
    c.base.cleanup().await;
}
