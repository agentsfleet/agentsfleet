//! Recovery assertions read the actual run, logical event and provider-request charges.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::e2e::Scenario;
use af_core::{clock, id::Uuid7, timing::STREAM_RECOVERY_BUDGET};

pub(super) struct Snapshot {
    pub(super) run: Uuid7,
    pub(super) expires_at: jiff::Timestamp,
    pub(super) owner: Uuid7,
}

pub(super) async fn snapshot(scene: &Scenario) -> Snapshot {
    let (run, expires_at, owner): (Uuid7,jiff_sqlx::Timestamp,Uuid7) = sqlx::query_as(
        "SELECT l.id,l.run_expires_at,r.connection_id FROM thread.runs l JOIN runner.runners r ON r.id=l.runner_id WHERE l.turn_id=$1 AND l.runner_id=$2 ORDER BY l.created_at DESC LIMIT 1"
    ).bind(&scene.turn_id).bind(&scene.runner_id)
    .fetch_one(&mut *scene.booted.database.acquire().await.expect("run observer"))
    .await.expect("real issued run and connection owner");
    Snapshot {
        run,
        expires_at: expires_at.to_jiff(),
        owner,
    }
}

pub(super) async fn reported(scene: &Scenario, run: &Uuid7) -> bool {
    sqlx::query_scalar("SELECT status=$2 FROM thread.runs WHERE id=$1")
        .bind(run)
        .bind(afd_state::sql::RUN_STATUS_REPORTED)
        .fetch_one(
            &mut *scene
                .booted
                .database
                .acquire()
                .await
                .expect("report observer"),
        )
        .await
        .expect("issued run still exists")
}

pub(super) async fn wait_reported(scene: &Scenario, run: &Uuid7, bound: std::time::Duration) {
    tokio::time::timeout(bound, async {
        loop {
            if reported(scene, run).await {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the retained report settles after connection recovery");
}

pub(super) async fn wait_renewed_before(
    scene: &Scenario,
    before: &Snapshot,
    bound: std::time::Duration,
) -> Snapshot {
    let left = std::time::Duration::try_from(before.expires_at.duration_since(clock::now()))
        .expect("the original confirmed run is still live");
    tokio::time::timeout(bound.min(left), async {
        loop {
            let current = snapshot(scene).await;
            if current.owner != before.owner && current.expires_at > before.expires_at {
                assert!(clock::now() < before.expires_at);
                return current;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a new native owner renews before the original confirmed expiry")
}

pub(super) async fn process_survives_deadline(pid: u32, deadline: jiff::Timestamp) {
    let left = std::time::Duration::try_from(deadline.duration_since(clock::now()))
        .expect("recovery completed before the original deadline");
    tokio::time::sleep(left + std::time::Duration::from_millis(100)).await;
    assert!(
        std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "the runner consumed the new grant and kept the same physical process alive"
    );
}

pub(super) async fn assert_settled_once(scene: &Scenario) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE turn_id=$1")
        .bind(&scene.turn_id)
        .fetch_one(
            &mut *scene
                .booted
                .database
                .acquire()
                .await
                .expect("run count observer"),
        )
        .await
        .expect("real run count");
    assert_eq!(
        count, 1,
        "connection recovery never redispatches this completed turn"
    );
    wait_for_billing(scene).await;
    let lines: Vec<(i64,i64,i64,bool)> = sqlx::query_as("SELECT seq,input_token_count,output_token_count,settled_at IS NOT NULL FROM billing.usage_lines WHERE turn_id=$1 AND seq=$2 AND kind=$3")
        .bind(&scene.turn_id).bind(0_i64).bind(af_wire::usage::LineKind::Model.as_str())
        .fetch_all(&mut *scene.booted.database.acquire().await.expect("provider receipt observer"))
        .await.expect("real provider receipt");
    assert_eq!(lines, vec![(0, 12000, 4000, true)]);
    assert_eq!(
        crate::reads::event_column(scene, &scene.turn_id, "status")
            .await
            .as_deref(),
        Some(af_core::turn::status::PROCESSED)
    );
    assert_eq!(
        crate::reads::event_column(scene, &scene.turn_id, "answer_text")
            .await
            .as_deref(),
        Some(super::runner::ANSWER)
    );
}

pub(super) async fn process_stopped_by(pid: u32, expires_at: jiff::Timestamp) {
    let left = std::time::Duration::try_from(expires_at.duration_since(clock::now()))
        .expect("a live confirmed run deadline");
    tokio::time::timeout(left, async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the physical tool process ends before the confirmed run expires");
}

async fn wait_for_billing(scene: &Scenario) {
    tokio::time::timeout(STREAM_RECOVERY_BUDGET, async {
        loop {
            let remaining: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM billing.usage_lines WHERE turn_id=$1 AND settled_at IS NULL",
            )
            .bind(&scene.turn_id)
            .fetch_one(
                &mut *scene
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("billing completion observer"),
            )
            .await
            .expect("all scoped usage lines are visible");
            if remaining == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("saved report completes billing");
}
