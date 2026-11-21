//! What stops an assignment: a run that is over, a durable task, the per-assignment
//! cap, and a message that is already settled.

use afd_turns::Producer;

use super::support::{
    Assigned, Checked, STEER_JSON, STEER_TEXT, assign_as_holder, send, set_run_task,
    settle_directly, workspace_of,
};
use crate::report_seed::held;
use crate::rows::{LAPSE, Row};
use crate::run_reads::COLUMN_HELD_UNTIL;

/// A run past its own deadline is assigned nothing, though its thread still names
/// it: a holder that stopped renewing must not keep reading.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_run_past_its_deadline_is_assigned_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [sent.assigned(STEER_TEXT)]
    );
    let run = Row::Run(&held.issued.run_id);
    held.fixtures.age(run, COLUMN_HELD_UNTIL, LAPSE).await;
    assert_eq!(assign_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// A thread past its deadline is assigned nothing, though its run row is still
/// live: the thread is what a later holder displaces.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_thread_past_its_deadline_is_assigned_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [sent.assigned(STEER_TEXT)]
    );
    let thread = Row::Thread(&held.fleet);
    held.fixtures.age(thread, COLUMN_HELD_UNTIL, LAPSE).await;
    assert_eq!(assign_as_holder(&held, 0).await?, Vec::new());
    held.fixtures.cleanup().await;
    Ok(())
}

/// A durable task's run is assigned nothing here: its conversation is assigned by
/// the task's own input path. The same run is assigned again once it is not one.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_durable_task_run_is_assigned_nothing() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let sent = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    set_run_task(&held, Some(&afd_db::test_util::mint_id())).await?;
    assert_eq!(assign_as_holder(&held, 0).await?, Vec::new());
    set_run_task(&held, None).await?;
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [sent.assigned(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// A burst is assigned whole: every message the run has not read, oldest
/// first, as Codex reads all pending input at its next step; nothing is left
/// for a second assignment.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_assignment_takes_every_pending_message() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let mut sent = Vec::new();
    for _ in 0..40 {
        sent.push(send(&held, &workspace, Producer::Steer, STEER_JSON).await?);
    }
    let first = assign_as_holder(&held, 0).await?;
    let every: Vec<Assigned> = sent
        .iter()
        .map(|message| message.assigned(STEER_TEXT))
        .collect();
    assert_eq!(first, every);
    let read_through = first.last().ok_or("the assignment took messages")?.sequence;
    assert_eq!(assign_as_holder(&held, read_through).await?, []);
    held.fixtures.cleanup().await;
    Ok(())
}

/// A message an earlier run already settled is not assigned again; the one
/// behind it is.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_settled_message_is_not_assigned_again() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let done = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let waiting = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    settle_directly(&held, &done.id).await?;
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [waiting.assigned(STEER_TEXT)]
    );
    held.fixtures.cleanup().await;
    Ok(())
}

/// Privacy cleanup can remove the task reference while retaining its generation.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retained_task_generation_stops_the_ordinary_fold_prefix() -> Checked {
    let held = held().await;
    let workspace = workspace_of(&held).await?;
    let earlier = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let barrier = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let later = send(&held, &workspace, Producer::Steer, STEER_JSON).await?;
    let mut connection = held.fixtures.database.acquire().await?;
    sqlx::query("UPDATE thread.turns SET task_generation=1 WHERE id=$1 AND task_id IS NULL")
        .bind(&barrier.id)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert_eq!(
        assign_as_holder(&held, 0).await?,
        [earlier.assigned(STEER_TEXT)]
    );
    assert_eq!(assign_as_holder(&held, earlier.sequence).await?, Vec::new());
    let mut connection = held.fixtures.database.acquire().await?;
    let untouched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE id=ANY($1::uuid[])
        AND input_run_id IS NULL AND input_fence IS NULL AND settled_at IS NULL",
    )
    .bind([barrier.id, later.id].as_slice())
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(untouched, 2);
    Ok(())
}
