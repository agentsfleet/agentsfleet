//! A Save names one exact candidate under this daemon's policy and one unused
//! key; anything else is refused before a fleet is written, and what it does
//! write follows the candidate exactly, its schedules included.

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

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_draft_already_saving_refuses_a_second_key() {
    let c = Case::create().await;
    let (_, request) = c.request("second-key", Start::None).await;
    let first = c.save(&mint(), request.clone()).await;
    assert_eq!(
        first.status, "installing",
        "unfunded, so the draft stays saving"
    );
    let refused = c
        .creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect_err("one draft becomes one fleet");
    assert_eq!(refused.current_state(), Some("saving"));
    assert_eq!(c.fleet_count().await, 1);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_save_that_loses_its_key_to_a_lapsed_claim_saves_no_fleet() {
    let c = Case::create().await;
    let (_, request) = c.request("lost-key", Start::None).await;
    let key = mint();
    let scope = format!("{}:fleet:create:{}", c.base.workspace, c.base.owner);
    let (holder, pid) = c.base.foreign_receipt(&scope, &key).await;
    let (refused, ()) = tokio::join!(
        c.creations.save(c.base.access(), &key, request, now()),
        async {
            c.base.waits_on(pid, 1).await;
            crate::draft::locks::commit_receipt(holder).await;
        }
    );
    let refused = refused.expect_err("the key went to a claim whose receipt has lapsed");
    assert_eq!(refused.code(), error_code::AUTHORING_GONE);
    assert_eq!(c.fleet_count().await, 0, "the reservation rolled back");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn only_a_saved_cron_trigger_files_a_schedule() {
    let c = Case::create().await;
    let (draft, mut request) = c.request("triggered", Start::None).await;
    let current = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("current draft");
    let patch: DraftPatch = serde_json::from_value(json!({"config_ops":[{"op":"replace",
    "path":"/config/x-agentsfleet/triggers","value":[
        {"type":"webhook","source":"github"},
        {"type":"cron","schedule":"0 * * * *","timezone":"UTC","message":"Review evidence"}
    ]}]}))
    .expect("declare both triggers");
    c.base
        .drafts
        .patch(c.base.access(), &draft, Some(&current.etag), patch, now())
        .await
        .expect("declared triggers");
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("review the triggers");
    request.source = SaveSource::Draft {
        draft_candidate_id: reviewed.candidate.expect("candidate").id,
    };
    request.review_digest = reviewed.review.expect("live review").review_digest;
    let saved = c.save(&mint(), request).await;
    let schedules: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.schedules WHERE fleet_id=$1::uuid")
            .bind(saved.id.as_str())
            .fetch_one(&mut *c.base.connection().await)
            .await
            .expect("the fleet's schedules");
    assert_eq!(schedules, 1, "the webhook files none, the cron one");
    c.cleanup().await;
}
