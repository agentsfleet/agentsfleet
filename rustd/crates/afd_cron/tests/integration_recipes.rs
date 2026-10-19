//! Replacing a fleet's inherited schedules: the refusals ahead of any write,
//! the slot a shorter Definition drops, and a revision that cannot advance.
//!
//! Every case runs the replacement on its own transaction, as the setup write
//! does, and reads the table back afterwards: a refusal is only worth its name
//! if the rows it refused to touch are untouched.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_cron::{DesiredStatus, MAX_SCHEDULES_PER_FLEET, Recipe, RecipeSet, Refused, SyncStatus};
use afd_db::test_util::mint_id;
use sqlx::Connection as _;

use self::support::{CronLane, SEED_AT};

/// The fleet's recipe rows, slot first: `(slot, desired, sync)`.
const RECIPE_ROWS: &str = "SELECT (recipe->>'slot')::bigint, desired_status, sync_status \
FROM fleet.schedules WHERE fleet_id = $1::uuid AND recipe IS NOT NULL ORDER BY 1";

/// Pushes a slot's stored recipe revision to the largest a counter holds.
const REVISION_AT_CEILING: &str = "UPDATE fleet.schedules \
SET recipe = jsonb_set(recipe, '{recipe_revision}', to_jsonb(9223372036854775807::bigint)) \
WHERE fleet_id = $1::uuid AND recipe IS NOT NULL";

/// The stored recipe revision of every recipe row on the fleet.
const REVISIONS: &str = "SELECT (recipe->>'recipe_revision')::bigint FROM fleet.schedules \
WHERE fleet_id = $1::uuid AND recipe IS NOT NULL";

/// `count` distinct recipes, one per hour of the day.
fn recipes(count: usize) -> Vec<Recipe> {
    (0..count)
        .map(|hour| {
            Recipe::parse(
                &format!("0 {} * * *", hour % 24),
                "UTC",
                "run the inherited repair",
            )
            .expect("a recipe")
        })
        .collect()
}

/// Replaces the fleet's recipes under `workspace`, committing only on success.
async fn replace_in(
    lane: &CronLane,
    workspace: &Uuid7,
    recipes: &[Recipe],
) -> afd_cron::Result<()> {
    let fleet = lane.fleet_id();
    let mut connection = lane.connection().await;
    let mut tx = connection.begin().await.expect("recipe transaction");
    lane.store
        .replace_recipes_on(
            &mut tx,
            RecipeSet {
                workspace,
                fleet: &fleet,
                setup_revision: 1,
                recipes,
                now: SEED_AT,
            },
        )
        .await?;
    tx.commit().await.expect("commit the recipes");
    Ok(())
}

async fn replace(lane: &CronLane, recipes: &[Recipe]) -> afd_cron::Result<()> {
    replace_in(lane, &lane.workspace_id(), recipes).await
}

/// The fleet's recipe rows as `(slot, desired, sync)`, slot first.
async fn rows(lane: &CronLane) -> Vec<(i64, String, String)> {
    sqlx::query_as(RECIPE_ROWS)
        .bind(lane.fleet.as_str())
        .fetch_all(&mut *lane.connection().await)
        .await
        .expect("read the recipe rows")
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_fleet_outside_the_callers_workspace_is_refused_as_absent() {
    let lane = CronLane::open().await;
    let elsewhere = Uuid7::parse(&mint_id()).expect("a minted workspace id is canonical");

    let refused = replace_in(&lane, &elsewhere, &recipes(1))
        .await
        .expect_err("a fleet in another workspace must not take recipes");

    assert_eq!(refused.code(), Refused::NoSuchFleet.code());
    assert_eq!(refused.detail(), Refused::NoSuchFleet.detail());
    assert!(rows(&lane).await.is_empty(), "nothing was written for it");
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn recipes_past_the_fleet_ceiling_are_refused_before_any_row() {
    let lane = CronLane::open().await;
    // One independent schedule already counts against the ceiling, so a
    // Definition that alone would fit exactly is one too many beside it.
    lane.settled("key-recipe-ceiling", "0 1 * * *").await;

    let refused = replace(&lane, &recipes(MAX_SCHEDULES_PER_FLEET))
        .await
        .expect_err("the ceiling counts the fleet's own schedules too");

    assert_eq!(refused.code(), Refused::TooMany.code());
    assert!(
        rows(&lane).await.is_empty(),
        "the refusal wrote no recipe row"
    );

    replace(&lane, &recipes(MAX_SCHEDULES_PER_FLEET - 1))
        .await
        .expect("the ceiling itself is admitted");
    assert_eq!(rows(&lane).await.len(), MAX_SCHEDULES_PER_FLEET - 1);
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_slot_the_new_definition_drops_is_marked_for_deletion_not_deleted() {
    let lane = CronLane::open().await;
    replace(&lane, &recipes(2)).await.expect("two recipes land");

    replace(&lane, &recipes(1))
        .await
        .expect("the shorter definition lands");

    let rows = rows(&lane).await;
    assert_eq!(
        rows.len(),
        2,
        "the dropped slot keeps its row until the scheduler agrees"
    );
    let deleting = DesiredStatus::Deleting.as_str();
    let syncing = SyncStatus::Syncing.as_str();
    let kept = rows.first().expect("slot 0 is still stored");
    assert_eq!(kept.0, 0);
    assert_ne!(kept.1, deleting, "the kept slot stays wanted");
    assert_eq!(
        rows.get(1),
        Some(&(1, deleting.to_owned(), syncing.to_owned())),
        "the dropped slot waits on the scheduler, marked for deletion"
    );
}

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_recipe_revision_at_its_ceiling_refuses_a_changed_base() {
    let lane = CronLane::open().await;
    replace(&lane, &recipes(1))
        .await
        .expect("the first recipe lands");
    sqlx::query(REVISION_AT_CEILING)
        .bind(lane.fleet.as_str())
        .execute(&mut *lane.connection().await)
        .await
        .expect("push the stored revision to its ceiling");

    // Same slot, different base: the revision has to advance and cannot.
    let changed = [Recipe::parse("30 2 * * *", "UTC", "run the changed repair").expect("a recipe")];
    let refused = replace(&lane, &changed)
        .await
        .expect_err("a revision that cannot advance must not be overwritten");

    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        !refused.is_retryable(),
        "the counter will be no lower next time"
    );
    let revisions: Vec<i64> = sqlx::query_scalar(REVISIONS)
        .bind(lane.fleet.as_str())
        .fetch_all(&mut *lane.connection().await)
        .await
        .expect("read the revisions");
    assert_eq!(revisions, [i64::MAX], "the refused replacement rolled back");
}
