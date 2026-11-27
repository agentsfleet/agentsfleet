//! A killed runner whose turn saved a full manifest and deltas over it
//! restores the whole chain on another host.
//!
//! The workspace is seeded with enough entries that its full manifest spans
//! more pages than the deltas the drill writes, so the save keeps writing
//! deltas instead of a full manifest again.

use super::{
    ACCEPTED, COMPLETED, Checked, LATER_INPUT, NOTE, NOTE_PATH, SAVED_INPUT, custody::Custody,
    model::Model, proof, seed, worker::Worker,
};
use crate::e2e::Scenario;
use af_core::id::Uuid7;
use afr_tools::catalog::FILE_WRITE;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the seeded entries go, inside the workspace.
const SEED_DIRECTORY: &str = "seeded";
/// Each seeded name's length: long, so the bytes a page holds bound how many
/// entries it lists.
const SEED_NAME_BYTES: usize = 200;
/// The deltas the drill needs over its full manifest before the kill.
const DELTAS: usize = 3;
/// The tool rounds the drill takes at most to reach them.
const ROUNDS: usize = 10;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs real processes, Postgres, NATS and the CI R2 bucket: make test-integration-rustd"]
async fn a_killed_runner_restores_a_delta_chain_on_another_host() -> Checked {
    let custody = Custody::from_env()?;
    let mut supervisor = Supervisor::new();
    let scene = crate::e2e_clock::scenario_with(&mut supervisor, &custody.knobs()).await;
    custody.seed(&scene).await?;
    let mut model = Model::start().await?;
    let (replacement, token) = seed::configure(&scene, &model.base()).await?;
    let mut first = Worker::start(&scene, &scene.token, &model)?;
    let mut step = first.next(&mut model).await?;
    step.call(FILE_WRITE.name(), &note(0))?;
    step = first.next(&mut model).await?;
    let seeded = seed_entries(&workspace(first.home.path())?)?;
    let mut round = 1;
    while deltas(&scene).await? < DELTAS {
        assert!(
            round < ROUNDS,
            "{ROUNDS} saves left fewer than {DELTAS} deltas"
        );
        let head = head(&scene).await?;
        step.call(FILE_WRITE.name(), &note(round))?;
        step = first.next(&mut model).await?;
        moved(&scene, head.as_ref()).await?;
        round += 1;
    }
    let written = json!(content(round - 1));
    let covered = seed::fold(&scene, SAVED_INPUT).await?;
    step.answer(ACCEPTED)?;
    let interrupted = first.next(&mut model).await?;
    let checkpoint = proof::checkpoint(&scene, &covered).await?;
    assert!(deltas(&scene).await? >= DELTAS, "the head is a delta chain");
    let later = seed::fold(&scene, LATER_INPUT).await?;
    first.kill().await?;
    let deadline = proof::deadline(&scene).await?;
    let mut second = Worker::start(&scene, &token, &model)?;
    proof::no_early_issue(&scene, &replacement, deadline).await?;
    let resumed = second.next(&mut model).await?;
    proof::prefix(&resumed.body, &[ACCEPTED, SAVED_INPUT, LATER_INPUT]);
    let restored = workspace(second.home.path())?;
    assert_eq!(
        json!(std::fs::read_to_string(restored.join(NOTE_PATH))?),
        written,
        "the last delta's note came back"
    );
    let seeds = std::fs::read_dir(restored.join(SEED_DIRECTORY))?.count();
    assert_eq!(seeds, seeded, "every entry of the full manifest came back");
    let winner = proof::replacement(&scene, &replacement, deadline).await?;
    proof::adopted(&scene, &winner, &[covered.clone(), later.clone()]).await?;
    resumed.answer(COMPLETED)?;
    proof::settled(&scene, &winner, &[covered, later]).await?;
    let second_log = second.stop().await?;
    assert!(second_log.contains(checkpoint.as_str()));
    drop(interrupted);
    model.finish().await?;
    supervisor.shutdown().await;
    scene.cleanup().await;
    Ok(())
}

/// The `file_write` arguments of the note's `round`th version.
fn note(round: usize) -> serde_json::Value {
    json!({"path": NOTE_PATH, "content": content(round)})
}

/// What the note's `round`th version holds.
fn content(round: usize) -> String {
    format!("{NOTE}round {round}\n")
}

/// The one sandbox workspace a runner's storage home holds.
fn workspace(home: &Path) -> Checked<PathBuf> {
    let sandboxes = afr_supervisor::StorageHome::open(home)?.sandboxes();
    let mut found = std::fs::read_dir(sandboxes)?
        .map(|sandbox| Ok::<_, std::io::Error>(sandbox?.path().join("workspace")))
        .filter(|path| path.as_ref().is_ok_and(|path| path.is_dir()));
    let only = found.next().ok_or("no sandbox workspace")??;
    Ok(only)
}

/// Writes empty files whose entries fill more turn manifest pages than the
/// deltas the drill keeps, with the fold's, so a full manifest of the tree
/// outnumbers the chain.
fn seed_entries(workspace: &Path) -> Checked<usize> {
    let directory = workspace.join(SEED_DIRECTORY);
    std::fs::create_dir(&directory)?;
    let pages = DELTAS + 2;
    let count = pages * af_wire::run::TURN_CHECKPOINT_PAGE_BYTES / SEED_NAME_BYTES + 1;
    for index in 0..count {
        let name = format!("{index:0SEED_NAME_BYTES$}");
        std::fs::File::create(directory.join(name))?;
    }
    Ok(count)
}

/// The turn's head generation, when it has one.
async fn head(scene: &Scenario) -> Checked<Option<Uuid7>> {
    let mut connection = scene.booted.database.acquire().await?;
    Ok(
        sqlx::query_scalar("SELECT head_checkpoint_id FROM thread.turns WHERE id=$1")
            .bind(&scene.turn_id)
            .fetch_one(&mut *connection)
            .await?,
    )
}

/// Waits for the head to move off `before`, as the round's publication
/// commits after the run resumed; a save that skipped leaves it, and the
/// drill goes on.
async fn moved(scene: &Scenario, before: Option<&Uuid7>) -> Checked {
    let wait = async {
        while head(scene).await?.as_ref() == before {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    };
    match tokio::time::timeout(Duration::from_secs(30), wait).await {
        Ok(moved) => moved,
        Err(_skipped) => Ok(()),
    }
}

/// The deltas between the turn's head and the full manifest its chain starts
/// from, which has no parent.
async fn deltas(scene: &Scenario) -> Checked<usize> {
    let mut connection = scene.booted.database.acquire().await?;
    let parents: Vec<Option<Uuid7>> = sqlx::query_scalar(
        "WITH RECURSIVE chain(id, parent) AS (
            SELECT c.id, c.parent_checkpoint_id FROM thread.turns t
            JOIN runner.checkpoints c ON c.id=t.head_checkpoint_id WHERE t.id=$1
            UNION ALL
            SELECT c.id, c.parent_checkpoint_id FROM runner.checkpoints c
            JOIN chain ON c.id=chain.parent)
        SELECT parent FROM chain",
    )
    .bind(&scene.turn_id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(parents.iter().filter(|parent| parent.is_some()).count())
}
