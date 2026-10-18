//! Deletion queued before replay must finish without a receipt/draft lock cycle.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_core::error_code::AUTHORING_GONE;
use sqlx::Connection as _;
use std::time::Duration;

const BLOCKERS: &str = "\
WITH RECURSIVE blocked(pid) AS (
 SELECT $1::integer UNION
 SELECT a.pid FROM pg_stat_activity a JOIN blocked b ON b.pid=ANY(pg_blocking_pids(a.pid))
)
SELECT count(*) FROM pg_stat_activity WHERE pid IN (SELECT pid FROM blocked WHERE pid<>$1)
AND query LIKE '%FROM authoring.drafts%'";
const LIMIT: Duration = Duration::from_secs(5);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn deletion_before_creation_replay_neither_deadlocks_nor_recreates_text() {
    let f = Fixture::create().await;
    let request = f.request(mint(), Some("Private content"));
    let created = f
        .drafts
        .create(f.access(), request.clone(), now())
        .await
        .expect("native draft");
    let mut connection = f.connection().await;
    let mut lock = connection
        .begin()
        .await
        .expect("hold draft before both requests");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .expect("lock owner");
    sqlx::query("SELECT id FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE")
        .bind(created.draft.id.as_str())
        .fetch_one(&mut *lock)
        .await
        .expect("hold private row");
    let delete = f.drafts.delete(f.access(), &created.draft.id);
    let replay = async {
        waiting(&f, pid, 1).await;
        let replay = f.drafts.create(f.access(), request, now());
        let release = async {
            waiting(&f, pid, 2).await;
            lock.commit()
                .await
                .expect("release both in their queued order");
        };
        let (outcome, ()) = tokio::join!(replay, release);
        outcome
    };
    let (deleted, replayed) =
        tokio::time::timeout(LIMIT, Box::pin(async { tokio::join!(delete, replay) }))
            .await
            .expect("both operations must finish without a lock cycle");
    deleted.expect("queued deletion commits first");
    assert_eq!(
        replayed
            .expect_err("replay observes the purge tombstone")
            .code(),
        AUTHORING_GONE
    );
    assert_eq!(f.counts().await, (0, 0, 0));
    drop(connection);
    f.cleanup().await;
}

/// Waits until `count` requests queue behind the transaction `pid` holds,
/// observed in `pg_stat_activity` rather than guessed with a sleep.
pub(super) async fn waiting(f: &Fixture, pid: i32, count: i64) {
    tokio::time::timeout(LIMIT, async {
        loop {
            let blocked: i64 = sqlx::query_scalar(BLOCKERS)
                .bind(pid)
                .fetch_one(&mut *f.connection().await)
                .await
                .expect("observe actual row-lock contention");
            if blocked >= count {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("request must reach the held row, without timing sleeps");
}
