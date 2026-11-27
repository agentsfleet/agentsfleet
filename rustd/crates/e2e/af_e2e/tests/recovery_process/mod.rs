//! A killed supervisor resumes authenticated ordinary history and its
//! workspace, checkpointed through real R2, on another host.
#![cfg(all(target_os = "linux", debug_assertions))]

mod chain;
mod custody;
mod handoff;
mod model;
mod population;
mod process;
mod proof;
mod seed;
mod uncommitted;
mod worker;

use afr_tools::catalog::FILE_WRITE;
use agentsfleetd::supervisor::Supervisor;
use custody::Custody;
use model::Model;
use serde_json::json;
use worker::Worker;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ACCEPTED: &str = "accepted answer A";
const COMPLETED: &str = "completed answer B";
const SAVED_INPUT: &str = "follow-up captured before the crash";
const LATER_INPUT: &str = "follow-up admitted after the checkpoint";
/// The file the first host writes, which only a restore can put on the second.
const NOTE_PATH: &str = "notes/handoff.md";
const NOTE: &str = "written on the first host before it died\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs real processes, Postgres, NATS and the CI R2 bucket: make test-integration-rustd"]
async fn killed_runner_mid_turn_restarts_on_another_host() -> Checked {
    let custody = Custody::from_env()?;
    let mut supervisor = Supervisor::new();
    let scene = crate::e2e_clock::scenario_with(&mut supervisor, &custody.knobs()).await;
    custody.seed(&scene).await?;
    let mut model = Model::start().await?;
    let (replacement, token) = seed::configure(&scene, &model.base()).await?;
    let mut first = Worker::start(&scene, &scene.token, &model)?;
    let note = json!({"path": NOTE_PATH, "content": NOTE});
    let initial = first.next(&mut model).await?;
    initial.call(FILE_WRITE.name(), &note)?;
    let written = first.next(&mut model).await?;
    proof::wrote(&written.body);
    let covered = seed::fold(&scene, SAVED_INPUT).await?;
    written.answer(ACCEPTED)?;
    let interrupted = first.next(&mut model).await?;
    proof::prefix(&interrupted.body, &[ACCEPTED, SAVED_INPUT]);
    let checkpoint = proof::checkpoint(&scene, &covered).await?;
    let later = seed::fold(&scene, LATER_INPUT).await?;
    let first_log = first.kill().await?;
    assert!(first_log.contains("respawn_checkpoint_committed"));
    let deadline = proof::deadline(&scene).await?;
    let mut second = Worker::start(&scene, &token, &model)?;
    proof::no_early_issue(&scene, &replacement, deadline).await?;
    let resumed = second.next(&mut model).await?;
    proof::prefix(&resumed.body, &[ACCEPTED, SAVED_INPUT, LATER_INPUT]);
    proof::restored(second.home.path())?;
    let winner = proof::replacement(&scene, &replacement, deadline).await?;
    proof::adopted(&scene, &winner, &[covered.clone(), later.clone()]).await?;
    resumed.answer(COMPLETED)?;
    proof::settled(&scene, &winner, &[covered, later]).await?;
    proof::quarantined(&scene).await?;
    let second_log = second.stop().await?;
    assert!(second_log.contains("respawn_checkpoint_restored"));
    assert!(second_log.contains(checkpoint.as_str()));
    drop(interrupted);
    model.finish().await?;
    supervisor.shutdown().await;
    scene.cleanup().await;
    Ok(())
}
