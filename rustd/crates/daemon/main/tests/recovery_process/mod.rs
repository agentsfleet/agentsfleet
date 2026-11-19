//! A killed supervisor resumes authenticated ordinary history through the real daemon.
#![cfg(all(feature = "test-util", target_os = "linux", debug_assertions))]

mod model;
mod population;
mod process;
mod proof;
mod seed;
mod worker;

use agentsfleetd::supervisor::Supervisor;
use model::Model;
use worker::Worker;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ACCEPTED: &str = "accepted answer A";
const COMPLETED: &str = "completed answer B";
const SAVED_INPUT: &str = "follow-up captured before the crash";
const LATER_INPUT: &str = "follow-up admitted after the checkpoint";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs real processes, Postgres and NATS: make test-integration-rustd"]
async fn killed_runner_mid_turn_restarts_on_another_host() -> Checked {
    let mut supervisor = Supervisor::new();
    let run = crate::e2e::scenario(&mut supervisor).await;
    let mut model = Model::start().await?;
    let (replacement, token) = seed::configure(&run, &model.base()).await?;
    let mut first = Worker::start(&run, &run.token, &model)?;
    let initial = first.next(&mut model).await?;
    let covered = seed::fold(&run, SAVED_INPUT).await?;
    initial.answer(ACCEPTED)?;
    let interrupted = first.next(&mut model).await?;
    proof::prefix(&interrupted.body, &[ACCEPTED, SAVED_INPUT]);
    let checkpoint = proof::checkpoint(&run, &covered).await?;
    let later = seed::fold(&run, LATER_INPUT).await?;
    let first_log = first.kill().await?;
    assert!(first_log.contains("respawn_checkpoint_committed"));
    let deadline = proof::deadline(&run).await?;
    let mut second = Worker::start(&run, &token, &model)?;
    proof::no_early_issue(&run, &replacement, deadline).await?;
    let resumed = second.next(&mut model).await?;
    proof::prefix(&resumed.body, &[ACCEPTED, SAVED_INPUT, LATER_INPUT]);
    let winner = proof::replacement(&run, &replacement, deadline).await?;
    proof::adopted(&run, &winner, &[covered.clone(), later.clone()]).await?;
    resumed.answer(COMPLETED)?;
    proof::settled(&run, &winner, &[covered, later]).await?;
    proof::quarantined(&run).await?;
    let second_log = second.stop().await?;
    assert!(second_log.contains("respawn_checkpoint_restored"));
    assert!(second_log.contains(checkpoint.as_str()));
    drop(interrupted);
    model.finish().await?;
    supervisor.shutdown().await;
    run.cleanup().await;
    Ok(())
}
