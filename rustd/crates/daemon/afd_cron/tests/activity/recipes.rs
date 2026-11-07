//! Setup recipe history shares the author's transaction and skips unchanged bases.

use afd_cron::{Recipe, RecipeSet};
use serde_json::json;

use super::fixture::*;
use crate::support::{CronLane, SEED_AT};

// Value: protects=setup-owned schedule writes/receipts commit or roll back together;
// fails_when=recipe writers omit receipts or persist them outside the caller transaction;
// seam=caller rollback is deliberate; no_claim=authoring HTTP or external registration.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rolled_back_setup_schedule_writes_leave_neither_state_nor_history() {
    let lane = CronLane::open().await;
    let recipe = Recipe::parse(EXPRESSION, ZONE, MESSAGE).expect("recipe");
    let (workspace, fleet) = (lane.workspace_id(), lane.fleet_id());
    let mut connection = lane.connection().await;
    let mut tx = sqlx::Transaction::<sqlx::Postgres>::begin(&mut *connection, None)
        .await
        .expect("caller transaction");
    let changes = RecipeSet {
        workspace: &workspace,
        fleet: &fleet,
        setup_revision: 1,
        recipes: std::slice::from_ref(&recipe),
        now: SEED_AT,
    };
    let pending = lane
        .store
        .replace_recipes_on(&mut tx, changes)
        .await
        .expect("same-transaction recipe");
    assert_eq!((lane.count().await, receipts(&lane).await), (0, vec![]));
    tx.rollback().await.expect("rollback");
    drop(pending);
    assert_eq!((lane.count().await, receipts(&lane).await), (0, vec![]));
    drop(connection);
    no_work(&lane).await;
}

// Value: protects=unchanged setup saves retain one history fact and retirement stays pending;
// fails_when=question-only revisions duplicate schedule history or removal is called confirmed;
// seam=none; no_claim=external registration or authoring HTTP.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn setup_schedule_revisions_commit_their_activity_atomically() {
    let lane = CronLane::open().await;
    let recipe = Recipe::parse(EXPRESSION, ZONE, MESSAGE).expect("recipe");
    let fleet = lane.fleet_id();
    save(&lane, 1, std::slice::from_ref(&recipe)).await;
    let rows = lane.store.list(&fleet).await.expect("saved schedules");
    let row = rows.first().expect("one recipe");
    let id = &row.schedule_id;
    same_facts(
        receipts(&lane).await,
        vec![changed(id, "created", "active", MESSAGE)],
    );
    save(&lane, 2, std::slice::from_ref(&recipe)).await;
    same_facts(
        receipts(&lane).await,
        vec![changed(id, "created", "active", MESSAGE)],
    );
    save(&lane, 3, &[]).await;
    save(&lane, 4, &[]).await;
    same_facts(
        receipts(&lane).await,
        vec![
            changed(id, "created", "active", MESSAGE),
            changed(id, "removal_requested", "deleting", MESSAGE),
        ],
    );
    assert_eq!(
        lane.count().await,
        1,
        "upstream removal has not yet been confirmed"
    );
    no_work(&lane).await;
}

async fn save(lane: &CronLane, revision: i64, recipes: &[Recipe]) {
    let (workspace, fleet) = (lane.workspace_id(), lane.fleet_id());
    let mut tx = sqlx::Transaction::begin(lane.database.acquire().await.expect("connection"), None)
        .await
        .expect("author-owned transaction");
    let announcement = lane
        .store
        .replace_recipes_on(
            &mut tx,
            RecipeSet {
                workspace: &workspace,
                fleet: &fleet,
                setup_revision: revision,
                recipes,
                now: SEED_AT,
            },
        )
        .await
        .expect("replace pinned recipes");
    afd_events::activity::commit(tx, announcement, None)
        .await
        .expect("same history commit");
}

// Value: protects=an existing native edit includes its new text while stale tags do nothing;
// fails_when=ordinary API edits bypass receipts or a rejected edit still records one;
// seam=none; no_claim=HTTP validation or browser rendering.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn conditional_schedule_edits_record_the_saved_revision_only() {
    let lane = CronLane::open().await;
    let row = registered(&lane).await;
    let patch = serde_json::from_value(json!({"message":"Review the revised support queue"}))
        .expect("native patch");
    let tag = row.etag();
    let request = afd_cron::EditRequest {
        workspace: &lane.workspace_id(),
        fleet: &lane.fleet_id(),
        schedule: &row.schedule_id,
        expected: Some(&tag),
        patch: &patch,
    };
    let edited = lane
        .store
        .edit(request, &CronLane::token())
        .await
        .expect("conditional edit");
    let before = receipts(&lane).await;
    let latest = changed(
        &row.schedule_id,
        "changed",
        "active",
        "Review the revised support queue",
    );
    assert!(before.contains(&latest));
    let error = lane
        .store
        .edit(request, &CronLane::token())
        .await
        .expect_err("stale tag");
    assert_eq!(
        (error.code(), error.etag()),
        (
            afd_core::error_code::AUTHORING_STALE,
            Some(edited.etag().as_str())
        )
    );
    assert_eq!(stored(&lane, &row.schedule_id).await, Some(edited));
    same_facts(receipts(&lane).await, before);
    no_work(&lane).await;
}
