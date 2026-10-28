//! A hot chat must leave the pool available; cancellation must not admit a turn.

use super::{BLOCKED, CONTENTION_POLL, CONTENTION_TIMEOUT, Fixture, mint, now, queue};
use afd_authoring::{
    Drafts, Result,
    types::{Accepted, Draft, SendMessage},
};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use futures_util::poll;
use sqlx::{Connection as _, PgConnection};
use std::sync::Arc;

struct Held {
    fixture: Arc<Fixture>,
    pool: Db,
    hot: Draft,
    other: Draft,
    holder: PgConnection,
    monitor: PgConnection,
    pid: i32,
}

impl Held {
    async fn create() -> Self {
        let mut fixture = Fixture::create().await;
        let hot = queue(&fixture, "Hot chat").await;
        let other = queue(&fixture, "Independent chat").await;
        let pool = TestDatabase::shared()
            .open(
                DbRole::Api,
                &[
                    ("DATABASE_POOL_SIZE_API", "2"),
                    ("DATABASE_MIN_POOL_SIZE_API", "2"),
                ],
            )
            .await;
        fixture.drafts = Drafts::new(
            pool.clone(),
            Entropy::new(),
            crate::draft::fixture::ledger(&fixture.db),
        );
        let mut holder = PgConnection::connect(&fixture.owner_url())
            .await
            .expect("independent owner");
        sqlx::query("BEGIN")
            .execute(&mut holder)
            .await
            .expect("hold transaction");
        let pid = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut holder)
            .await
            .expect("holder identity");
        sqlx::query("SELECT id FROM authoring.drafts WHERE id=$1::uuid FOR UPDATE")
            .bind(hot.id.as_str())
            .execute(&mut holder)
            .await
            .expect("hot draft row hold");
        let monitor = PgConnection::connect(&fixture.owner_url())
            .await
            .expect("independent autocommit lock monitor");
        Self {
            fixture: Arc::new(fixture),
            pool,
            hot,
            other,
            holder,
            monitor,
            pid,
        }
    }

    async fn blocked(&mut self) -> i64 {
        // A census inside holder's transaction would retain old session membership.
        sqlx::query_scalar(BLOCKED)
            .bind(self.pid)
            .fetch_one(&mut self.monitor)
            .await
            .expect("current database waiters")
    }

    async fn wait_for_sender(&mut self) {
        tokio::time::timeout(CONTENTION_TIMEOUT, async {
            while self.blocked().await == 0 {
                tokio::time::sleep(CONTENTION_POLL).await;
            }
        })
        .await
        .expect("first sender holds its local turn and waits on the database");
    }

    async fn release(&mut self) {
        sqlx::query("COMMIT")
            .execute(&mut self.holder)
            .await
            .expect("release only this chat's row");
    }

    async fn stored(&self) -> (i64, i64) {
        sqlx::query_as("SELECT count(*), count(r.id) FROM authoring.messages m LEFT JOIN authoring.replay_receipts r ON r.resource_id=m.id AND r.scope=$2 WHERE m.draft_id=$1::uuid")
            .bind(self.hot.id.as_str()).bind(format!("{}:draft:{}:message", self.fixture.workspace, self.hot.id))
            .fetch_one(&mut *self.fixture.connection().await).await.expect("durable accepted messages and receipts")
    }

    async fn finish(self) {
        self.holder.close().await.expect("holder released");
        self.monitor.close().await.expect("monitor released");
        self.pool.close().await;
        Arc::try_unwrap(self.fixture)
            .ok()
            .expect("all send futures released")
            .cleanup()
            .await;
    }
}

async fn send(fixture: Arc<Fixture>, id: Uuid7, operation_id: Uuid7) -> Result<Accepted> {
    // Production surfaces clone Drafts; each clone must share its local queue.
    let drafts = fixture.drafts.clone();
    drafts
        .send(
            fixture.access(),
            &id,
            SendMessage {
                save: false,
                message: "Next turn".into(),
                operation_id,
            },
            now(),
        )
        .await
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn one_hundred_waiting_sends_leave_capacity_for_an_independent_chat() {
    let mut held = Held::create().await;
    assert_eq!(held.blocked().await, 0, "no sender has connected yet");
    let first = tokio::spawn(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    held.wait_for_sender().await;
    let mut waiting = Vec::new();
    for _ in 0..super::CALLERS {
        let mut future = Box::pin(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
        assert!(poll!(future.as_mut()).is_pending());
        waiting.push(future);
    }
    assert_eq!(
        held.blocked().await,
        1,
        "same-process waiters must not acquire spare database connections"
    );
    let independent = tokio::time::timeout(
        CONTENTION_TIMEOUT,
        send(Arc::clone(&held.fixture), held.other.id.clone(), mint()),
    )
    .await
    .expect("independent chat keeps the spare pool slot")
    .expect("independent Send");
    assert_eq!(independent.sequence, "2");
    drop(waiting);
    held.release().await;
    assert_eq!(
        first
            .await
            .expect("first task")
            .expect("first Send")
            .sequence,
        "2"
    );
    assert_eq!(held.stored().await, (2, 2));
    held.finish().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn cancelling_a_queued_sender_leaves_no_message_or_receipt_and_unblocks_its_successor() {
    let mut held = Held::create().await;
    assert_eq!(held.blocked().await, 0, "no sender has connected yet");
    let first = tokio::spawn(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    held.wait_for_sender().await;
    let mut cancelled = Box::pin(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    assert!(poll!(cancelled.as_mut()).is_pending());
    let mut next = Box::pin(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    assert!(poll!(next.as_mut()).is_pending());
    drop(cancelled);
    held.release().await;
    assert_eq!(
        first
            .await
            .expect("first task")
            .expect("first Send")
            .sequence,
        "2"
    );
    let accepted = tokio::time::timeout(CONTENTION_TIMEOUT, next)
        .await
        .expect("queue progresses after cancellation")
        .expect("next Send");
    assert_eq!(accepted.sequence, "3");
    assert_eq!(held.stored().await, (3, 3));
    held.finish().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn cancelling_the_current_sender_releases_its_local_turn_without_committing() {
    let mut held = Held::create().await;
    assert_eq!(held.blocked().await, 0, "no sender has connected yet");
    let first = tokio::spawn(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    held.wait_for_sender().await;
    let mut next = Box::pin(send(Arc::clone(&held.fixture), held.hot.id.clone(), mint()));
    assert!(poll!(next.as_mut()).is_pending());
    first.abort();
    assert!(
        first
            .await
            .expect_err("cancelled first task")
            .is_cancelled()
    );
    held.release().await;
    let accepted = tokio::time::timeout(CONTENTION_TIMEOUT, next)
        .await
        .expect("queue progresses after holder cancellation")
        .expect("next Send");
    assert_eq!(accepted.sequence, "2");
    assert_eq!(held.stored().await, (2, 2));
    held.finish().await;
}
