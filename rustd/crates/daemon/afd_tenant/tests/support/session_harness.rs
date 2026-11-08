//! The lane's Postgres, where device-login sessions live since M244.

use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;

/// A runtime-role connection to the shared, migrated lane database.
pub(crate) struct SessionHarness {
    pub(crate) database: Db,
}

impl SessionHarness {
    /// Opens the shared lane as the runtime role, as the daemon does.
    pub(crate) async fn connect() -> Self {
        af_core::test_util::trace::install_subscriber();
        Self {
            database: TestDatabase::shared().open(DbRole::Api, &[]).await,
        }
    }
}
