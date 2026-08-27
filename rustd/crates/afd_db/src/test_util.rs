//! A database per test, created and dropped with it.
//!
//! # Why this lives here rather than in a suite
//!
//! Four suites carry a near-identical copy of it — `afd_db`'s own,
//! `afd_state`'s, `afd_fleet`'s and `agentsfleetd`'s — and every one of them
//! records in its header that the right home is this module, behind the feature
//! that exists for exactly this. This is that module. It is ADDITIVE: the four
//! copies are untouched, because migrating them is a refactor across four green
//! integration suites and a fifth copy is the thing worth not writing. What it
//! changes is the shape of the eventual consolidation — four deletions rather
//! than a rewrite.
//!
//! # Why a database per test rather than a transaction
//!
//! The suites that reach for this assert on what a WRITE left behind, and
//! several of them write from more than one connection — an install releases
//! its pool connection before touching Redis and takes a fresh one to roll
//! back. A test transaction cannot span that, and sharing one database would
//! make each test's precondition depend on the previous test's cleanup, which
//! is how a suite starts passing in one order and failing in another.
//!
//! Redis has no per-test equivalent and is deliberately not wrapped here:
//! suites namespace their keys by the identifiers they mint, which is the
//! isolation that actually works against a shared server.

#![expect(
    clippy::panic,
    reason = "a feature-gated fixture: it compiles into the crate only because a sibling integration test needs it, and every panic here reports a LANE fault — an unset knob, an unreachable Postgres — where a Result would hand the test a value it could only unwrap (dispatch/write_rust.md, test-util carve-out)"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::env::MapEnv;
use sqlx::AssertSqlSafe;

use crate::config::{DbRole, PoolConfig};
use crate::pool::Db;

/// The environment knob naming the lane's admin connection.
const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Distinguishes databases created by one process.
///
/// Combined with the process id, so two lanes on one host cannot collide
/// either — `cargo test` runs one binary per crate and several at once.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A database created for one test and dropped with it.
///
/// Dropping is [`TestDatabase::cleanup`] rather than `Drop`, because dropping a
/// database is an `await` and `Drop` cannot be one. A test that forgets leaks a
/// database into a disposable environment, which is noise rather than a fault.
#[derive(Debug)]
pub struct TestDatabase {
    base_url: String,
    name: String,
}

impl TestDatabase {
    /// Creates an empty database, so "fresh" means fresh.
    ///
    /// # Panics
    /// When `TEST_DATABASE_URL` is unset — which means the caller is running
    /// outside the integration lane, and every assertion after this point would
    /// be about a database that does not exist.
    #[must_use = "the database is dropped by `cleanup`, not by going out of scope"]
    pub async fn create() -> Self {
        install_subscriber();
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_unset| {
            panic!("{LANE_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_it_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );

        // The name is a process id and a counter, never input, which is what
        // makes interpolating it safe: Postgres does not bind identifiers, so a
        // name from anywhere else would have to be refused rather than escaped.
        // `AssertSqlSafe` is sqlx 0.9 asking that at the type level.
        Self::admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
        Self { base_url, name }
    }

    /// The connection URL for this test's own database.
    ///
    /// Keeps whatever query string the lane's URL carried — `sslmode` among
    /// them, which a lane against a TLS Postgres needs and a rebuilt URL would
    /// silently drop.
    #[must_use]
    pub fn url(&self) -> String {
        let (prefix, tail) = self
            .base_url
            .rsplit_once('/')
            .unwrap_or((self.base_url.as_str(), ""));
        let query = tail.split_once('?').map_or("", |(_, query)| query);
        if query.is_empty() {
            format!("{prefix}/{}", self.name)
        } else {
            format!("{prefix}/{}?{query}", self.name)
        }
    }

    /// An environment pointing every role at this database.
    #[must_use]
    pub fn env(&self, extra: &[(&str, &str)]) -> MapEnv {
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
    ///
    /// # Panics
    /// When the pool will not open, which at this point means the database this
    /// call just created is unreachable — a lane fault, not a test outcome.
    pub async fn open(&self, role: DbRole, extra: &[(&str, &str)]) -> Db {
        let config = PoolConfig::resolve(&self.env(extra), role).unwrap_or_else(|failure| {
            panic!("the fixture connection string is well formed: {failure}")
        });
        Db::connect(&config).await.unwrap_or_else(|failure| {
            panic!("the test database must accept a connection: {failure}")
        })
    }

    /// Drops the database.
    ///
    /// Best-effort throughout: a leaked test database is noise in a disposable
    /// environment, and panicking here would replace whatever the test actually
    /// found with a cleanup failure.
    pub async fn cleanup(self) {
        let statement = AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        ));
        let Ok(admin) = sqlx::PgPool::connect(&self.base_url).await else {
            return;
        };
        if let Ok(mut connection) = admin.acquire().await {
            let _discarded = sqlx::query(statement).execute(&mut *connection).await;
        }
        admin.close().await;
    }

    /// Runs one administrative statement against the lane, then disconnects.
    ///
    /// # Panics
    /// When the lane's Postgres will not answer — every assertion downstream
    /// would be about a database that was never created.
    async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
        let admin = sqlx::PgPool::connect(base_url)
            .await
            .unwrap_or_else(|failure| panic!("the lane's database must be reachable: {failure}"));
        let mut connection = admin
            .acquire()
            .await
            .unwrap_or_else(|failure| panic!("an admin connection: {failure}"));
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .unwrap_or_else(|failure| panic!("creating a test database: {failure}"));
        drop(connection);
        admin.close().await;
    }
}

/// Installs a subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE it evaluates the
/// fields inside it, so with no subscriber every field expression in every
/// diagnostic is skipped — the failure path runs and the line reporting it does
/// not, and llvm-cov scores those fields as never executed. Output goes to a
/// sink; the point is evaluation, not reading.
pub fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _already_set = tracing::subscriber::set_global_default(subscriber);
    });
}
