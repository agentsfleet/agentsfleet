//! Which reads Postgres says return the bodies, asked of the prepared text.
//!
//! A listing page carries up to two hundred rows, so the trigger payload, the
//! agent's answer and its tool calls stay off it; the thread is the expanded
//! read and pays for them. The listing's decoder reads by index and ignores
//! trailing columns, so a body column added to a listing would decode clean and
//! ship megabytes nobody renders. Preparing each text and reading back the
//! columns Postgres will return is what catches that.

use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_events::READ_TEXTS;
use sqlx::{Executor as _, SqlSafeStr as _, Statement as _};

/// The columns only a thread read returns.
const BODY_COLUMNS: [&str; 3] = ["request_json", "response_text", "tool_calls"];

/// How `READ_TEXTS` names the reads that page a thread.
const THREAD_READ: &str = "thread";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn only_the_thread_reads_return_the_bodies() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let mut connection = database.acquire().await.expect("an API connection");

    for (name, text) in READ_TEXTS {
        let prepared = (&mut *connection)
            .prepare(text.into_sql_str())
            .await
            .unwrap_or_else(|failure| panic!("{name} prepares: {failure}"));
        let returned: Vec<&str> = prepared.columns().iter().map(sqlx::Column::name).collect();
        let bodies = BODY_COLUMNS
            .iter()
            .filter(|body| returned.contains(body))
            .count();
        let expected = if name.starts_with(THREAD_READ) {
            BODY_COLUMNS.len()
        } else {
            0
        };
        assert_eq!(bodies, expected, "{name} returns {returned:?}");
    }

    drop(connection);
    lane.cleanup().await;
}
