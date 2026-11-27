//! A runner killed while its save is still publishing leaves the turn on the
//! last committed generation, and another host restores exactly that.
//!
//! ```text
//!   save 1   pin ─► resume ─► begin ─► register ─► PUT ─► commit   the head
//!   save 2   pin ─► resume ─► begin ─► register …                  held
//!   SIGKILL                                     the head is still save 1
//!   host two restores save 1, byte for byte
//! ```
//!
//! The drill holds the workspace's checkpoint storage lock, which a
//! registration waits on and a begin does not take, so the second save is
//! past its resume and short of its commit when the kill lands.

use super::chain::{content, head, moved, note, workspace};
use super::{
    COMPLETED, Checked, NOTE_PATH, custody::Custody, model::Asked, model::Model, proof, seed,
    worker::Worker,
};
use crate::e2e::Scenario;
use af_core::id::Uuid7;
use afr_tools::catalog::FILE_WRITE;
use agentsfleetd::supervisor::Supervisor;
use sqlx::Connection as _;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The tool rounds the drill takes at most to commit a save, then to catch
/// one publishing.
const ROUNDS: usize = 10;
/// How long a round waits for its save's generation to show. A registration
/// held at the lock is answered nothing for longer than this, so a
/// generation seen within it is still publishing.
const PENDING_WAIT: Duration = Duration::from_secs(3);

/// Every entry under a workspace by its path within it: a file's bytes, and
/// nothing for a directory.
type Tree = BTreeMap<PathBuf, Option<Vec<u8>>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs real processes, Postgres, NATS and the CI R2 bucket: make test-integration-rustd"]
async fn a_runner_killed_between_resume_and_commit_keeps_the_previous_generation() -> Checked {
    let custody = Custody::from_env()?;
    let mut supervisor = Supervisor::new();
    let scene = crate::e2e_clock::scenario_with(&mut supervisor, &custody.knobs()).await;
    custody.seed(&scene).await?;
    let mut model = Model::start().await?;
    let (replacement, token) = seed::configure(&scene, &model.base()).await?;
    let mut first = Worker::start(&scene, &scene.token, &model)?;
    let (step, committed, round) = saved(&scene, &mut first, &mut model).await?;
    let before = tree(&workspace(first.home.path())?)?;
    let mut connection = scene.booted.database.acquire().await?;
    let mut held = connection.begin().await?;
    sqlx::query("SELECT id FROM identity.lock_workspace_storage($1)")
        .bind(&scene.workspace)
        .execute(&mut *held)
        .await?;
    let (unanswered, uncommitted) =
        publishing(&scene, &mut first, &mut model, (step, round)).await?;
    first.kill().await?;
    held.rollback().await?;
    drop(connection);
    assert_eq!(head(&scene).await?.as_ref(), Some(&committed));
    let deadline = proof::deadline(&scene).await?;
    let mut second = Worker::start(&scene, &token, &model)?;
    proof::no_early_issue(&scene, &replacement, deadline).await?;
    let resumed = second.next(&mut model).await?;
    let note = before.get(Path::new(NOTE_PATH)).cloned().flatten();
    assert_eq!(note, Some(content(round).into_bytes()));
    assert_eq!(tree(&workspace(second.home.path())?)?, before);
    assert_eq!(ready(&scene, &uncommitted).await?, None);
    proof::replacement(&scene, &replacement, deadline).await?;
    resumed.answer(COMPLETED)?;
    processed(&scene).await?;
    let second_log = second.stop().await?;
    assert!(second_log.contains("respawn_checkpoint_restored"));
    assert!(second_log.contains(committed.as_str()));
    drop(unanswered);
    model.finish().await?;
    supervisor.shutdown().await;
    scene.cleanup().await;
    Ok(())
}

/// Writes the note, a new version each round, until a save of it commits:
/// the model request that round left unanswered, the head, and the round.
async fn saved(
    scene: &Scenario,
    first: &mut Worker,
    model: &mut Model,
) -> Checked<(Asked, Uuid7, usize)> {
    let mut step = first.next(model).await?;
    let (mut rounds, mut committed) = (0, None);
    while committed.is_none() {
        assert!(rounds < ROUNDS, "{ROUNDS} rounds committed no save");
        step.call(FILE_WRITE.name(), &note(rounds))?;
        step = first.next(model).await?;
        moved(scene, None).await?;
        committed = head(scene).await?;
        rounds += 1;
    }
    Ok((step, committed.ok_or("a committed save")?, rounds - 1))
}

/// Writes the note again, a new version each round after `round`, until a
/// save's generation shows, begun and neither committed nor abandoned: the
/// model request the run asked after it resumed, and that generation.
async fn publishing(
    scene: &Scenario,
    first: &mut Worker,
    model: &mut Model,
    (mut step, round): (Asked, usize),
) -> Checked<(Asked, Uuid7)> {
    let (mut rounds, mut shown) = (0, None);
    while shown.is_none() {
        assert!(rounds < ROUNDS, "{ROUNDS} rounds left no save publishing");
        rounds += 1;
        step.call(FILE_WRITE.name(), &note(round + rounds))?;
        // The run asks its model again without waiting for the publication.
        step = first.next(model).await?;
        shown = pending(scene).await?;
    }
    Ok((step, shown.ok_or("a publishing save")?))
}

/// The turn's generation that is neither committed nor abandoned, once it
/// shows; none when a round's save was skipped.
async fn pending(scene: &Scenario) -> Checked<Option<Uuid7>> {
    let shown = async {
        loop {
            let mut connection = scene.booted.database.acquire().await?;
            let found: Option<Uuid7> = sqlx::query_scalar(
                "SELECT id FROM runner.checkpoints
                 WHERE turn_id=$1 AND ready_at IS NULL AND abandoned_at IS NULL",
            )
            .bind(&scene.turn_id)
            .fetch_optional(&mut *connection)
            .await?;
            drop(connection);
            if found.is_some() {
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(found);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    // A round whose save was skipped shows nothing within the wait.
    let found = tokio::time::timeout(PENDING_WAIT, shown).await.ok();
    Ok(found.transpose()?.flatten())
}

/// The generation's identifier when it ever committed; none when it is
/// still unready, or gone.
async fn ready(scene: &Scenario, generation: &Uuid7) -> Checked<Option<Uuid7>> {
    let mut connection = scene.booted.database.acquire().await?;
    Ok(
        sqlx::query_scalar(
            "SELECT id FROM runner.checkpoints WHERE id=$1 AND ready_at IS NOT NULL",
        )
        .bind(generation)
        .fetch_optional(&mut *connection)
        .await?,
    )
}

/// Waits for the turn to be processed, as the replacement's answer ends it.
async fn processed(scene: &Scenario) -> Checked {
    let settled = async {
        loop {
            let mut connection = scene.booted.database.acquire().await?;
            let done: Option<Uuid7> =
                sqlx::query_scalar("SELECT id FROM thread.turns WHERE id=$1 AND status=$2")
                    .bind(&scene.turn_id)
                    .bind(af_core::turn::status::PROCESSED)
                    .fetch_optional(&mut *connection)
                    .await?;
            drop(connection);
            if done.is_some() {
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(60), settled).await?
}

/// Every entry under `root`, read whole.
fn tree(root: &Path) -> Checked<Tree> {
    let mut found = Tree::new();
    let mut unread = vec![root.to_owned()];
    while let Some(directory) = unread.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            let name = path.strip_prefix(root)?.to_owned();
            if path.is_dir() {
                found.insert(name, None);
                unread.push(path);
            } else {
                found.insert(name, Some(std::fs::read(&path)?));
            }
        }
    }
    Ok(found)
}
