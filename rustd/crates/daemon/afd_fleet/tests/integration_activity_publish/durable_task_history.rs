//! Persisted progress must survive reconnect without accepting stale ownership.

use af_wire::activity::{
    ActivityFrame, DurableTaskActivity, DurableTaskActivityHistory, DurableTaskActivityOperation,
    DurableTaskActivityPhase, DurableTaskActivityRecord, DurableTaskTransferBytes,
};
use sqlx::types::Json;

use crate::{
    report_seed::{Held, held},
    rows::{LAPSE, Row},
};

fn progress(id: &str, completed: u32) -> DurableTaskActivity {
    DurableTaskActivity {
        activity_id: id.to_owned(),
        durable_task_id: None,
        source_turn_id: None,
        operation: DurableTaskActivityOperation::Capture,
        phase: DurableTaskActivityPhase::Uploading,
        provider: None,
        model: None,
        completed,
        total: Some(4),
        bytes: Some(DurableTaskTransferBytes {
            acknowledged: u64::from(completed) * 100,
            missing: u64::from(4 - completed) * 100,
            reused: 0,
        }),
        sequence: Some(999_999),
    }
}

async fn saved(run: &Held) -> DurableTaskActivityHistory {
    let Json(history) = sqlx::query_scalar::<_, Json<DurableTaskActivityHistory>>(
        "SELECT jsonb_build_object('items', durable_task_activity) \
         FROM thread.turns WHERE thread_id=$1 AND id=$2",
    )
    .bind(&run.fleet)
    .bind(&run.turn_id)
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
async fn write(
    run: &Held,
    progress: DurableTaskActivity,
) -> Option<Json<DurableTaskActivityHistory>> {
    let target = afd_fleet::run::activity::Target {
        fleet_id: run.fleet.clone(),
        turn_id: run.turn_id.clone(),
        run_created_at: run.now,
        turn_created_at: run.now,
        timing_eligible: true,
        fence: run.fence.as_i64(),
    };
    run.runs
        .retain_durable_task_activity(
            &target,
            &run.runner,
            &run.issued.run_id,
            &[DurableTaskActivityRecord::DurableTaskTransferProgress(
                progress,
            )],
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
            .activity_for_target(
                &run.runner,
                &run.issued.run_id,
                &run.activity_target().await,
                &[ActivityFrame::DurableTaskTransferProgress(progress(
                    id.as_str(),
                    completed,
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
async fn history_retains_every_stage_in_monotonic_sequence() {
    let run = held().await;
    for _ in 0..130 {
        write(&run, progress(afd_db::test_util::mint_id().as_str(), 1))
            .await
            .expect("owned progress persists");
    }
    let history = saved(&run).await;
    assert_eq!(history.items.len(), 130, "nothing caps the history");
    let sequences: Vec<_> = history
        .items
        .iter()
        .map(|item| {
            item.progress()
                .sequence
                .expect("native observation sequence")
        })
        .collect();
    assert_eq!(sequences, (1..=130).collect::<Vec<_>>());
    run.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn history_write_rejects_owner_that_expired_after_preflight() {
    let run = held().await;
    let target = run.activity_target().await;
    assert!(target.timing_eligible);
    run.fixtures
        .age(Row::Run(&run.issued.run_id), "run_expires_at", LAPSE)
        .await;
    assert!(
        write(&run, progress(afd_db::test_util::mint_id().as_str(), 1))
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
    sqlx::query("SELECT id FROM thread.turns WHERE thread_id=$1 AND id=$2 FOR UPDATE")
        .bind(&run.fleet)
        .bind(&run.turn_id)
        .fetch_one(&mut *tx)
        .await
        .expect("hold event");
    let id = afd_db::test_util::mint_id();
    let dropped = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        write(&run, progress(id.as_str(), 1)),
    )
    .await
    .expect("contended cosmetic progress must not wait");
    assert!(dropped.is_none());
    tx.rollback().await.expect("release event");
    drop(connection);
    let Json(history) = write(&run, progress(id.as_str(), 2))
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

// Value: protects=a holder that lapsed cannot attach task progress to the live
// tail, yet its other frames still publish; fails_when=the lapsed branch
// publishes every frame, or drops the whole batch; why_new=the lapsed holder's
// task-frame filter had no live case; seam=`activity_for_target` over live
// Postgres and the lane's NATS, read back on the fleet's item subject.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_lapsed_holder_publishes_its_chunk_but_no_durable_task_progress() {
    use afd_bus::{Received, Subject};

    let run = held().await;
    run.fixtures
        .age(Row::Run(&run.issued.run_id), "run_expires_at", LAPSE)
        .await;
    let target = run.activity_target().await;
    assert!(!target.timing_eligible, "the lapsed run is not eligible");
    let mut tail = run
        .fixtures
        .bus()
        .subscribe(&Subject::ThreadItems(run.fleet.clone()))
        .await
        .expect("the tail subscribes");
    let chunk = ActivityFrame::FleetResponseChunk(af_wire::activity::FleetResponseChunk {
        text: std::borrow::Cow::Borrowed("still talking"),
        text_kind: af_wire::activity::StreamTextKind::Answer,
        first_chunk_after_ms: None,
        stream_start: false,
        stream_contiguous: false,
        stream_seq: 0,
    });
    let task = ActivityFrame::DurableTaskTransferProgress(progress(
        afd_db::test_util::mint_id().as_str(),
        1,
    ));

    run.fixtures
        .plane()
        .activity_for_target(&run.runner, &run.issued.run_id, &target, &[task, chunk])
        .await
        .expect("a lapsed holder's frames are answered, not refused");

    let first: serde_json::Value = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(Received::Message(message)) = tail.recv().await {
                return serde_json::from_str(&message.payload).expect("a frame is JSON");
            }
        }
    })
    .await
    .expect("the chunk reaches the tail");
    assert_eq!(
        first.get("kind").and_then(serde_json::Value::as_str),
        Some("chunk"),
        "the task frame is withheld and the chunk is first: {first}"
    );
    assert!(
        saved(&run).await.items.is_empty(),
        "a lapsed holder saves no progress"
    );
    run.fixtures.cleanup().await;
}
