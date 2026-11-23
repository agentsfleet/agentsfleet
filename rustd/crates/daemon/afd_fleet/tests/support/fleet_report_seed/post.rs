//! A tool-call post of a held run, statement by statement, for a test that
//! puts another verb between them.

use std::time::Duration;

use af_core::id::Uuid7;
use af_wire::tool_detail::{ItemKind, ItemStatus};
use afd_fleet::run::sql::tool_detail::{SELECT_LIVE_RUN, UPSERT_RECORDS};
use sqlx::PgConnection;

use super::Held;

/// How long a test waits for a verb to queue behind the post.
const QUEUE_LIMIT: Duration = Duration::from_secs(5);

impl Held {
    /// The post's first statement: the run, held as the record verb holds it
    /// until `post` ends. Answers the post's backend.
    pub(crate) async fn post_holds_run(&self, post: &mut PgConnection) -> i32 {
        sqlx::query(SELECT_LIVE_RUN)
            .bind(&self.issued.run_id)
            .bind(&self.runner)
            .bind(afd_state::sql::RUN_STATUS_ACTIVE)
            .fetch_one(&mut *post)
            .await
            .expect("the post holds its live run");
        sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(post)
            .await
            .expect("the post's backend")
    }

    /// The post's second statement: one finished call's item, whose foreign
    /// keys reach the run's fleet, turn and run. Answers the item.
    pub(crate) async fn post_writes_item(&self, post: &mut PgConnection) -> Uuid7 {
        let item = afd_db::test_util::mint_id();
        let written = sqlx::query(UPSERT_RECORDS)
            .bind(&self.acquired.workspace_id)
            .bind(&self.fleet)
            .bind(&self.turn_id)
            .bind(self.fence.as_i64())
            .bind(vec![item.clone()])
            .bind(vec![1_i64])
            .bind(vec!["{}"])
            .bind(vec![false])
            .bind(vec!["ok"])
            .bind(vec![1_i64])
            .bind(vec![false])
            .bind(&self.issued.run_id)
            .bind(vec![self.turn_id.clone()])
            .bind(vec![ItemKind::ToolCall.as_str()])
            .bind(vec!["shell"])
            .bind(vec![ItemStatus::Succeeded.as_str()])
            .bind(vec![None::<String>])
            .execute(post)
            .await
            .expect("the post's item is written");
        assert_eq!(written.rows_affected(), 1, "the item names a served turn");
        item
    }

    /// Whether `item` is stored, read on a connection of its own.
    pub(crate) async fn item_kept(&self, item: &Uuid7) -> bool {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM thread.items WHERE id = $1)")
            .bind(item)
            .fetch_one(&mut *self.fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("the item reads")
    }

    /// Returns once some backend waits on a lock `pid` holds, polled on
    /// `observer`, an autocommit connection of the caller's.
    pub(crate) async fn queued_behind(observer: &mut PgConnection, pid: i32) {
        tokio::time::timeout(QUEUE_LIMIT, async {
            while !Self::is_queued_behind(observer, pid).await {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a verb queues behind the post");
    }

    /// Whether some backend waits on a lock `pid` holds, read on `observer`.
    pub(crate) async fn is_queued_behind(observer: &mut PgConnection, pid: i32) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
             WHERE $1 = ANY(pg_blocking_pids(pid)))",
        )
        .bind(pid)
        .fetch_one(observer)
        .await
        .expect("the lock graph reads")
    }
}
