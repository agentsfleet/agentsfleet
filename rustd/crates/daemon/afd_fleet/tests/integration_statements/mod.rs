//! The run plane's statements against a freshly migrated schema: every
//! runtime `.sql` file prepares, the task lock grants hold, and the metering
//! sweep reads its index.
//!
//! They live here, beside the statements they check, rather than in `afd_db`,
//! whose suite would otherwise dev-depend on this crate while this crate
//! depends on it.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use sqlx::PgConnection;

/// A fresh database, migrated by the canonical list.
async fn migrated() -> (TestDatabase, afd_db::Db) {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("an empty database migrates");
    (database, db)
}

mod statements;
mod task_lock_privileges;
