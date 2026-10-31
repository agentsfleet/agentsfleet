//! An improvement chat starts from a saved fleet's numbered revision and
//! applies back onto it only while that fleet can still take a revision.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use afd_authoring::{
    SavedFleet,
    types::{ApplyRevision, DraftPatch, DraftState, SaveFleet, SaveSource, Source, Start},
};
use afd_core::{error_code, id::Uuid7};

/// Saves a funded fleet under `name` and answers it active.
async fn active(c: &Case, name: &str) -> SavedFleet {
    c.fund().await;
    let (_, request) = c.request(name, Start::None).await;
    let saved = c.save(&mint(), request).await;
    assert_eq!(saved.status, "active");
    saved
}

/// Opens an improvement chat on `saved`'s head and answers it with the apply
/// request its current review accepts.
async fn improvement(c: &Case, saved: &SavedFleet) -> (Uuid7, ApplyRevision) {
    let mut request = c.base.request(mint(), None);
    request.source = Source::Fleet {
        fleet_id: saved.id.clone(),
        setup_revision: saved.setup_revision.clone(),
    };
    let draft = c
        .base
        .drafts
        .create(c.base.access(), request, now())
        .await
        .expect("an improvement chat on the saved head")
        .draft;
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &draft.id)
        .await
        .expect("the improvement reviews");
    let apply = ApplyRevision {
        origin: None,
        draft_candidate_id: reviewed.candidate.expect("source candidate").id,
        review_digest: reviewed.review.expect("live review").review_digest,
    };
    (draft.id, apply)
}

async fn status(c: &Case, fleet: &Uuid7) -> String {
    sqlx::query_scalar("SELECT status FROM fleet.fleets WHERE id=$1::uuid")
        .bind(fleet.as_str())
        .fetch_one(&mut *c.base.connection().await)
        .await
        .expect("the saved fleet")
}

/// The stored first revision's runtime choice, and whether its definition
/// names a model of its own.
const REVISION_MODEL: &str = "SELECT runtime_model_ref, \
     definition->'config'->'x-agentsfleet' ? 'model' FROM fleet.revisions WHERE fleet_id=$1::uuid";

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_improvement_chat_inherits_the_model_its_fleet_resolved_at_save() {
    let c = Case::create().await;
    let draft = c
        .base
        .drafts
        .create(c.base.access(), c.base.request(mint(), None), now())
        .await
        .expect("a chat that names no runtime model")
        .draft;
    let named: DraftPatch = serde_json::from_value(serde_json::json!({
        "instance_name": "improve-default"
    }))
    .expect("an instance name alone");
    c.base
        .drafts
        .patch(c.base.access(), &draft.id, Some(&draft.etag), named, now())
        .await
        .expect("name the instance, leave the model");
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &draft.id)
        .await
        .expect("review resolves the owner's default");
    let request = SaveFleet {
        origin: None,
        source: SaveSource::Draft {
            draft_candidate_id: reviewed.candidate.expect("candidate").id,
        },
        name: "improve-default".to_owned(),
        review_digest: reviewed.review.expect("live review").review_digest,
        start: Start::None,
    };
    let saved = c.save(&mint(), request).await;
    let (resolved, named): (Option<String>, bool) = sqlx::query_as(REVISION_MODEL)
        .bind(saved.id.as_str())
        .fetch_one(&mut *c.base.connection().await)
        .await
        .expect("the first revision");
    assert!(
        !named,
        "the saved definition leaves the model to the default"
    );
    let resolved = resolved.expect("Save records the model it resolved");
    let (chat, _) = improvement(&c, &saved).await;
    let improving = c
        .setup
        .read_draft(c.base.access(), &chat)
        .await
        .expect("the improvement reviews");
    let candidate = improving.candidate.expect("source candidate");
    assert_eq!(
        candidate.definition.runtime().model(),
        Some(resolved.as_str()),
        "the chat edits the setup the fleet actually runs"
    );
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_paused_fleet_takes_the_revision_without_reinstalling_and_closes_the_chat() {
    let c = Case::create().await;
    let saved = active(&c, "improve-paused").await;
    let (draft, apply) = improvement(&c, &saved).await;
    sqlx::query("UPDATE fleet.fleets SET status='paused' WHERE id=$1::uuid")
        .bind(saved.id.as_str())
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an operator paused the fleet");
    let applied = c
        .creations
        .apply_revision(
            c.base.access(),
            &saved.id,
            Some(&saved.setup_etag),
            &mint(),
            apply,
            now(),
        )
        .await
        .expect("a paused fleet takes a revision");
    assert_eq!(applied.setup_revision, "2");
    assert_eq!(
        status(&c, &saved.id).await,
        "paused",
        "no reinstall is started"
    );
    let closed = c.base.drafts.read(c.base.access(), &draft).await;
    assert_eq!(closed.expect("closed chat").state, DraftState::Closed);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_killed_fleet_refuses_the_revision_and_keeps_its_head() {
    let c = Case::create().await;
    let saved = active(&c, "improve-killed").await;
    let (_, apply) = improvement(&c, &saved).await;
    let killed = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await
        .expect("Kill");
    assert_eq!(
        killed.setup_etag, saved.setup_etag,
        "Kill moves no revision"
    );
    let refused = c
        .creations
        .apply_revision(
            c.base.access(),
            &saved.id,
            Some(&saved.setup_etag),
            &mint(),
            apply,
            now(),
        )
        .await
        .expect_err("a killed fleet takes no revision");
    assert_eq!(refused.current_state(), Some("killed"));
    let read = c.creations.read(c.base.access(), &saved.id).await;
    assert_eq!(read.expect("killed fleet").setup_revision, "1");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_candidate_under_retired_policy_semantics_cannot_be_applied() {
    let c = Case::create().await;
    let saved = active(&c, "improve-policy").await;
    let (chat, _) = improvement(&c, &saved).await;
    let retired = c.base.retire_head(&chat).await;
    let reviewed = c
        .setup
        .read_draft(c.base.access(), &chat)
        .await
        .expect("the retired candidate still reviews");
    let apply = ApplyRevision {
        origin: None,
        draft_candidate_id: retired,
        review_digest: reviewed.review.expect("live review").review_digest,
    };
    let refused = c
        .creations
        .apply_revision(
            c.base.access(),
            &saved.id,
            Some(&saved.setup_etag),
            &mint(),
            apply,
            now(),
        )
        .await
        .expect_err("stale policy needs a fresh review");
    assert_eq!(refused.code(), error_code::AUTHORING_POLICY);
    let read = c.creations.read(c.base.access(), &saved.id).await;
    assert_eq!(read.expect("unchanged fleet").setup_revision, "1");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn two_applies_under_one_key_write_one_revision_and_the_waiter_replays_it() {
    use sqlx::Connection as _;
    let c = Case::create().await;
    let saved = active(&c, "improve-twice").await;
    let (chat, apply) = improvement(&c, &saved).await;
    let key = mint();
    let mut holder = c.base.connection().await;
    let mut held = holder.begin().await.expect("holder transaction");
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE",
    )
    .bind(chat.as_str())
    .fetch_one(&mut *held)
    .await
    .expect("hold the chat the way a capture locks it");
    let run = || {
        c.creations.apply_revision(
            c.base.access(),
            &saved.id,
            Some(&saved.setup_etag),
            &key,
            apply.clone(),
            now(),
        )
    };
    let (winner, waiter, ()) = tokio::join!(run(), run(), async {
        c.base.waits_on(pid, 2).await;
        held.rollback().await.expect("release the chat");
    });
    drop(holder);
    let (winner, waiter) = (
        winner.expect("one apply lands"),
        waiter.expect("the waiter replays it"),
    );
    assert_eq!(winner.setup_revision, "2");
    assert_eq!(waiter.setup_revision, winner.setup_revision);
    let revisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.revisions WHERE fleet_id=$1::uuid")
            .bind(saved.id.as_str())
            .fetch_one(&mut *c.base.connection().await)
            .await
            .expect("the fleet's revisions");
    assert_eq!(revisions, 2, "one revision for one key");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_apply_that_loses_its_key_to_a_lapsed_claim_writes_no_revision() {
    let c = Case::create().await;
    let saved = active(&c, "improve-lost-key").await;
    let (_, apply) = improvement(&c, &saved).await;
    let key = mint();
    let scope = format!("{}:fleet:{}:revision", c.base.workspace, saved.id);
    let (holder, pid) = c.base.foreign_receipt(&scope, &key).await;
    let (refused, ()) = tokio::join!(
        c.creations.apply_revision(
            c.base.access(),
            &saved.id,
            Some(&saved.setup_etag),
            &key,
            apply,
            now(),
        ),
        async {
            c.base.waits_on(pid, 1).await;
            crate::draft::locks::commit_receipt(holder).await;
        }
    );
    let refused = refused.expect_err("the key went to a claim whose receipt has lapsed");
    assert_eq!(refused.code(), error_code::AUTHORING_GONE);
    let read = c.creations.read(c.base.access(), &saved.id).await;
    assert_eq!(read.expect("unchanged fleet").setup_revision, "1");
    c.cleanup().await;
}
