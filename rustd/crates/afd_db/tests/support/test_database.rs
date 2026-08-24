//! A database per test, created and dropped with it.
//!
//! Shared by both integration targets. Not politeness: 2.1 is a claim about a
//! FRESH database, 2.2 runs two migrators at once, and 2.3 deliberately fails a
//! migration. Sharing one database between them would make each test's
//! precondition depend on the last one's cleanup, which is how a suite starts
//! passing in one order and failing in another.

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::env::MapEnv;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use sqlx::AssertSqlSafe;

const LANE_KNOB: &str = "AFD_TEST_DATABASE_URL";

/// Distinguishes databases created by one process. Combined with the process
/// id so two lanes on one host cannot collide either.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A database created for one test and dropped with it.
pub(crate) struct TestDatabase {
    base_url: String,
    name: String,
}

impl TestDatabase {
    /// Creates an empty database, so "fresh" means fresh.
    pub(crate) async fn create() -> Self {
        install_subscriber();
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_| {
            panic!("{LANE_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_t_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );

        // The name is built from a process id and a counter, never from input,
        // which is what makes interpolating it safe — Postgres does not bind
        // identifiers, so a name from anywhere else would have to be rejected
        // rather than escaped.
        // `AssertSqlSafe` is sqlx 0.9 asking, at the type level, whether SQL
        // built at runtime is safe to run — `sqlx::query` otherwise takes only
        // `&'static str`. It is, and only here: the name is a process id and a
        // counter, and every other statement in this crate is a literal.
        let create = AssertSqlSafe(format!("CREATE DATABASE {name}"));
        let admin = sqlx::PgPool::connect(&base_url)
            .await
            .expect("the lane's database must be reachable");
        let mut connection = admin.acquire().await.expect("an admin connection");
        sqlx::query(create)
            .execute(&mut *connection)
            .await
            .expect("creating a test database");
        drop(connection);
        admin.close().await;

        Self { base_url, name }
    }

    /// The connection URL for this test's own database.
    pub(crate) fn url(&self) -> String {
        let (prefix, tail) = self
            .base_url
            .rsplit_once('/')
            .expect("a Postgres URL carries a database path");
        let query = tail.split_once('?').map_or("", |(_, query)| query);
        if query.is_empty() {
            format!("{prefix}/{}", self.name)
        } else {
            format!("{prefix}/{}?{query}", self.name)
        }
    }

    /// An environment pointing all three roles at this database.
    pub(crate) fn env(&self, extra: &[(&str, &str)]) -> MapEnv {
        let url = self.url();
        let mut pairs: Vec<(String, String)> = DbRole::ALL
            .iter()
            .map(|role| (role.url_knob().to_owned(), url.clone()))
            .collect();
        pairs.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        );
        MapEnv::from_pairs(
            pairs
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
    }

    /// Opens a pool for one role against this database.
    pub(crate) async fn open(&self, role: DbRole, extra: &[(&str, &str)]) -> Db {
        Db::connect(&PoolConfig::resolve(&self.env(extra), role).unwrap())
            .await
            .expect("the test database must accept a connection")
    }

    /// Drops the database. Best-effort: a leaked test database is noise in a
    /// disposable environment, not a failure worth masking the real one with.
    pub(crate) async fn cleanup(self) {
        let drop_it = AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        ));
        let Ok(admin) = sqlx::PgPool::connect(&self.base_url).await else {
            return;
        };
        if let Ok(mut connection) = admin.acquire().await {
            let _ = sqlx::query(drop_it).execute(&mut *connection).await;
        }
        admin.close().await;
    }
}

/// Installs a subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE it evaluates
/// the fields inside it, so with no subscriber every field expression in every
/// diagnostic is skipped — the failure path runs and the line reporting it does
/// not. Output goes to a sink; the point is evaluation, not reading.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}
