//! Persisted progress must survive reconnect without accepting stale ownership.

use afd_wire::activity::{
    ActivityFrame, TaskActivity, TaskActivityHistory, TaskActivityOperation, TaskActivityPhase,
    TaskActivityRecord, TaskTransferBytes,
};
use sqlx::types::Json;

use crate::{
    report_seed::{Held, held},
    rows::{LAPSE, Row},
};

fn progress(id: &str, completed: u32) -> TaskActivity {
    TaskActivity {
        activity_id: id.to_owned(),
        task_id: None,
        source_admission_id: None,
        operation: TaskActivityOperation::Capture,
        phase: TaskActivityPhase::Uploading,
        provider: None,
        model: None,
        completed,
        total: Some(4),
        bytes: Some(TaskTransferBytes {
            acknowledged: u64::from(completed) * 100,
            missing: u64::from(4 - completed) * 100,
            reused: 0,
        }),
        sequence: Some(999_999),
    }
}

async fn saved(run: &Held) -> TaskActivityHistory {
    let Json(history) = sqlx::query_scalar::<_, Json<TaskActivityHistory>>(
        "SELECT task_activity FROM fleet.events WHERE fleet_id=$1::uuid AND event_id=$2",
    )
    .bind(&run.fleet)
    .bind(&run.event_id)
    .fetch_one(
        &mut *run
            .fixtures
            .database
            .acquire()
            .await
            .expect("history connection"),
    )
    .await
    .expect("saved progress is readable");
    history
}

/// Execute the actual write with a previously observed owner, bypassing only
/// the preflight so the regression exercises the write's ownership defense.
async fn write(run: &Held, progress: TaskActivity) -> Option<Json<TaskActivityHistory>> {
    let target = afd_fleet::lease::activity::Target {
        fleet_id: afd_core::id::Uuid7::parse(&run.fleet).expect("fixture fleet"),
        event_id: run.event_id.clone(),
        lease_created_at: run.now,
        event_created_at: run.now,
        timing_eligible: true,
        fence: run.fence.as_i64(),
    };
    run.leases
        .retain_task_activity(
            &target,
            &run.runner,
            run.issued.lease_id.as_str(),
            &[TaskActivityRecord::TaskTransferProgress(progress)],
        )
        .await
        .expect("cosmetic write is a bounded verdict")
        .map(Json)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn daemon_sequence_and_acknowledgements_survive_reordered_updates() {
    let run = held().await;
    let id = afd_db::test_util::mint_id();
    for completed in [2, 1, 3] {
        run.fixtures
            .plane()
            .activity(
                &run.runner,
                run.issued.lease_id.as_str(),
                &[ActivityFrame::TaskTransferProgress(progress(
                    &id, completed,
                ))],
            )
            .await
            .expect("active progress accepted");
        let history = saved(&run).await;
        assert_eq!(history.items.len(), 1, "one operation/stage is coalesced");
        let retained = history.items.first().expect("saved progress").progress();
        let expected = if completed == 1 { 2 } else { completed };
        assert_eq!(retained.completed, expected);
        assert_eq!(
            retained.bytes.as_ref().expect("byte counters").acknowledged,
            u64::from(expected) * 100
        );
        assert_eq!(
            retained.sequence,
            Some(if completed == 3 { 3 } else { 1 }),
            "the daemon replaces worker sequence and preserves a newer counter's order"
        );
    }
    run.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn history_write_rejects_owner_that_expired_after_preflight() {
    let run = held().await;
    let target = run
        .leases
        .load_activity_target(run.issued.lease_id.as_str(), &run.runner)
        .await
        .expect("preflight query")
        .expect("held target");
    assert!(target.timing_eligible);
    run.fixtures
        .age(
            Row::Lease(run.issued.lease_id.as_str()),
            "lease_expires_at",
            LAPSE,
        )
        .await;
    assert!(
        write(&run, progress(&afd_db::test_util::mint_id(), 1))
            .await
            .is_none()
    );
    assert!(
        saved(&run).await.items.is_empty(),
        "stale preflight cannot authorize saved progress"
    );
    run.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn contended_event_drops_progress_without_waiting_or_consuming_sequence() {
    let run = held().await;
    let mut connection = run
        .fixtures
        .database
        .acquire()
        .await
        .expect("lock connection");
    let mut tx = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("lock transaction");
    sqlx::query("SELECT id FROM fleet.events WHERE fleet_id=$1::uuid AND event_id=$2 FOR UPDATE")
        .bind(&run.fleet)
        .bind(&run.event_id)
        .fetch_one(&mut *tx)
        .await
        .expect("hold event");
    let id = afd_db::test_util::mint_id();
    let dropped = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        write(&run, progress(&id, 1)),
    )
    .await
    .expect("contended cosmetic progress must not wait");
    assert!(dropped.is_none());
    tx.rollback().await.expect("release event");
    drop(connection);
    let Json(history) = write(&run, progress(&id, 2))
        .await
        .expect("uncontended progress persists");
    assert_eq!(
        history
            .items
            .first()
            .expect("one retained fact")
            .progress()
            .sequence,
        Some(1)
    );
    run.fixtures.cleanup().await;
}
