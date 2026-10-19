//! A Save names one exact candidate under this daemon's policy; anything else
//! is refused before a fleet identity is reserved.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use afd_authoring::types::{DraftPatch, SaveSource, Start};
use afd_core::error_code;
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_candidate_under_retired_policy_semantics_saves_no_fleet() {
    let c = Case::create().await;
    let (draft, mut request) = c.request("retired-policy", Start::None).await;
    let retired = c.base.retire_head(&draft).await;
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("the retired candidate still reviews");
    request.source = SaveSource::Draft {
        draft_candidate_id: retired,
    };
    request.review_digest = reviewed.review.expect("live review").review_digest;
    let refused = c
        .creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect_err("stale policy needs a fresh review");
    assert_eq!(refused.code(), error_code::AUTHORING_POLICY);
    assert_eq!(refused.current_state(), Some("review_required"));
    assert_eq!(c.fleet_count().await, 0);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_candidate_an_edit_replaced_saves_no_fleet() {
    let c = Case::create().await;
    let (draft, request) = c.request("replaced-candidate", Start::None).await;
    let current = c
        .base
        .drafts
        .read(c.base.access(), &draft)
        .await
        .expect("current tag");
    let patch: DraftPatch = serde_json::from_value(json!({"instruction_edits":[
        {"start_byte":0,"end_byte":0,"text":"Check the queue first.\n"}]}))
    .expect("edit input");
    c.base
        .drafts
        .patch(c.base.access(), &draft, Some(&current.etag), patch, now())
        .await
        .expect("the edit makes a newer candidate");
    let refused = c
        .creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect_err("the reviewed candidate is no longer the head");
    assert_eq!(refused.current_state(), Some("candidate_changed"));
    assert_eq!(c.fleet_count().await, 0);
    c.cleanup().await;
}
