//! Recovery assertions read the actual lease, logical event and provider-request charges.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::e2e::Scenario;
use af_core::{clock, id::Uuid7, timing::STREAM_RECOVERY_BUDGET};

pub(super) struct Snapshot {
    pub(super) lease: Uuid7,
    pub(super) expires_at: jiff::Timestamp,
    pub(super) owner: Uuid7,
}

pub(super) async fn snapshot(run: &Scenario) -> Snapshot {
    let (lease, expires_at, owner): (Uuid7,jiff_sqlx::Timestamp,Uuid7) = sqlx::query_as(
        "SELECT l.id,l.lease_expires_at,r.connection_id FROM runner.leases l JOIN runner.runners r ON r.id=l.runner_id WHERE l.event_id=$1 AND l.runner_id=$2 ORDER BY l.created_at DESC LIMIT 1"
    ).bind(&run.event_id).bind(&run.runner_id)
    .fetch_one(&mut *run.booted.database.acquire().await.expect("lease observer"))
    .await.expect("real issued lease and connection owner");
    Snapshot {
        lease,
        expires_at: expires_at.to_jiff(),
        owner,
    }
}

pub(super) async fn reported(run: &Scenario, lease: &Uuid7) -> bool {
    sqlx::query_scalar("SELECT status=$2 FROM runner.leases WHERE id=$1")
        .bind(lease)
        .bind(afd_state::sql::LEASE_STATUS_REPORTED)
        .fetch_one(
            &mut *run
                .booted
                .database
                .acquire()
                .await
                .expect("report observer"),
        )
        .await
        .expect("issued lease still exists")
}

pub(super) async fn wait_reported(run: &Scenario, lease: &Uuid7, bound: std::time::Duration) {
    tokio::time::timeout(bound, async {
        loop {
            if reported(run, lease).await {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the retained report settles after connection recovery");
}

pub(super) async fn wait_renewed_before(
    run: &Scenario,
    before: &Snapshot,
    bound: std::time::Duration,
) -> Snapshot {
    let left = std::time::Duration::try_from(before.expires_at.duration_since(clock::now()))
        .expect("the original confirmed lease is still live");
    tokio::time::timeout(bound.min(left), async {
        loop {
            let current = snapshot(run).await;
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

pub(super) async fn assert_settled_once(run: &Scenario) {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE event_id=$1")
        .bind(&run.event_id)
        .fetch_one(
            &mut *run
                .booted
                .database
                .acquire()
                .await
                .expect("lease count observer"),
        )
        .await
        .expect("real lease count");
    assert_eq!(
        count, 1,
        "connection recovery never redispatches this completed turn"
    );
    wait_for_billing(run).await;
    let lines: Vec<(i64,i64,i64,bool)> = sqlx::query_as("SELECT seq,input_token_count,output_token_count,settled_at IS NOT NULL FROM billing.usage_lines WHERE event_id=$1 AND seq=$2")
        .bind(&run.event_id).bind(0_i64)
        .fetch_all(&mut *run.booted.database.acquire().await.expect("provider receipt observer"))
        .await.expect("real provider receipt");
    assert_eq!(lines, vec![(0, 12000, 4000, true)]);
    assert_eq!(
        crate::reads::event_column(run, &run.event_id, "status")
            .await
            .as_deref(),
        Some(af_core::event::status::PROCESSED)
    );
    assert_eq!(
        crate::reads::event_column(run, &run.event_id, "response_text")
            .await
            .as_deref(),
        Some(super::runner::ANSWER)
    );
}

pub(super) async fn process_stopped_by(pid: u32, expires_at: jiff::Timestamp) {
    let left = std::time::Duration::try_from(expires_at.duration_since(clock::now()))
        .expect("a live confirmed lease deadline");
    tokio::time::timeout(left, async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the physical tool process ends before the confirmed lease expires");
}

async fn wait_for_billing(run: &Scenario) {
    tokio::time::timeout(STREAM_RECOVERY_BUDGET, async {
        loop {
            let remaining: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM billing.usage_lines WHERE event_id=$1 AND settled_at IS NULL",
            )
            .bind(&run.event_id)
            .fetch_one(
                &mut *run
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
