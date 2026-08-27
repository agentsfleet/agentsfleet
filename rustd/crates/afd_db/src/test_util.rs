//! The lane's database, shared by every test in the run — and the one exception.
//!
//! # One database, migrated once
//!
//! [`TestDatabase::shared`] hands back the lane's own database. Nothing is
//! created, nothing is migrated and nothing is dropped: `make
//! test-integration-rustd` drops the schemas once, applies the migrations once,
//! and every test in the run then works inside that one database.
//!
//! This is the Zig harness's contract, restored. That harness said it in one
//! line — "Runs against the LIVE test database. Never creates temp tables." —
//! and a hundred and forty-five integration files honoured it.
//!
//! **What the port had been doing instead, and what it cost.** Every test
//! created a database of its own and applied all forty-seven `schema/*.sql`
//! files into it. At a hundred and forty-three tests that is roughly six
//! thousand seven hundred migration applications per lane run, to produce one
//! schema a hundred and forty-three times, and it was the whole of the run's
//! hundred and thirty-five seconds.
//!
//! # What replaces the isolation, since something must
//!
//! Not nothing. Every fixture MINTS its own identifiers — a tenant, a
//! workspace, a fleet — so two tests cannot name each other's rows, and every
//! statement in this workspace carries that identifier in its predicate. That
//! is the isolation that does the work; a database per test was belt over
//! braces, and the braces are the ones holding.
//!
//! It is the same isolation the Redis side has always relied on, for the reason
//! it has always relied on it: Redis has no per-test database, so suites
//! namespace their keys by the identifiers they mint. That worked. This is the
//! same argument applied to Postgres.
//!
//! **The obligation it puts on a test:** assert on rows you MINTED, never on a
//! table's whole contents. A count must carry a workspace or a tenant in its
//! predicate. `SELECT count(*) FROM …` with no `WHERE` is a test that will pass
//! alone and fail beside its neighbours.
//!
//! # The exception, and why it is only one
//!
//! [`TestDatabase::create`] still makes a database of its own. The suites that
//! need it are the ones ABOUT schema state — the migrator's own ledger, lock
//! and failure paths — which cannot run inside an already-migrated database
//! without testing something else. Zig had exactly this exception too, and
//! exactly one file used it (`pool_migration_state_test.zig`, and its
//! `SCRATCH_DB`).

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
    /// The database this handle CREATED, when it created one.
    ///
    /// `None` for the shared lane database, which this fixture does not own and
    /// must not drop. Carrying the distinction as an `Option` rather than a
    /// `bool` means [`TestDatabase::cleanup`] has nothing to drop rather than a
    /// flag to consult, and [`TestDatabase::url`] has nothing to append.
    owned: Option<String>,
}

impl TestDatabase {
    /// Creates an empty database, so "fresh" means fresh.
    ///
    /// # Panics
    /// When `TEST_DATABASE_URL` is unset — which means the caller is running
    /// outside the integration lane, and every assertion after this point would
    /// be about a database that does not exist.
    /// The lane's own database, already migrated, shared with every other test.
    ///
    /// The default, and what almost every fixture wants. Costs one environment
    /// read: no `CREATE DATABASE`, no migration, no drop.
    ///
    /// # Panics
    /// When `TEST_DATABASE_URL` is unset — which means the caller is running
    /// outside the integration lane, and every assertion after this point would
    /// be about a database that does not exist.
    #[must_use]
    pub fn shared() -> Self {
        install_subscriber();
        Self {
            base_url: lane_url(),
            owned: None,
        }
    }

    /// A database of this test's own, created empty and NOT migrated.
    ///
    /// For the suites that are about schema state — the migrator's ledger, its
    /// lock, its failure paths — which cannot run inside an already-migrated
    /// database without testing something else. Everything else takes
    /// [`TestDatabase::shared`]; see the module note on why there is only one
    /// exception.
    ///
    /// # Panics
    /// When `TEST_DATABASE_URL` is unset.
    #[must_use = "the database is dropped by `cleanup`, not by going out of scope"]
    pub async fn create() -> Self {
        install_subscriber();
        let base_url = lane_url();
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
        Self {
            base_url,
            owned: Some(name),
        }
    }

    /// The connection URL this handle's tests run against.
    ///
    /// The lane's URL unchanged for a shared handle. For an owned one, the same
    /// URL with the database swapped — keeping whatever query string the lane
    /// carried, `sslmode` among them, which a lane against a TLS Postgres needs
    /// and a rebuilt URL would silently drop.
    #[must_use]
    pub fn url(&self) -> String {
        let Some(name) = self.owned.as_deref() else {
            return self.base_url.clone();
        };
        let (prefix, tail) = self
            .base_url
            .rsplit_once('/')
            .unwrap_or((self.base_url.as_str(), ""));
        let query = tail.split_once('?').map_or("", |(_, query)| query);
        if query.is_empty() {
            format!("{prefix}/{name}")
        } else {
            format!("{prefix}/{name}?{query}")
        }
    }

    /// The database this handle created, when it created one.
    ///
    /// `None` for the shared lane database — which is the point: a caller that
    /// wants to DROP a database has to get a name out of this, and the shared
    /// handle has none to give.
    #[must_use]
    pub fn database_name(&self) -> Option<&str> {
        self.owned.as_deref()
    }

    /// The lane's admin URL, for a caller that has to reach the server itself.
    #[must_use]
    pub fn lane_url(&self) -> &str {
        &self.base_url
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

    /// Drops the database this handle created, if it created one.
    ///
    /// A NO-OP for a shared handle, which does not own the lane's database and
    /// must not drop it. Fixtures still call it unconditionally — the call is
    /// how a test says it is finished, and making the shared case silent here
    /// is cheaper than making every fixture ask which kind it holds.
    ///
    /// Best-effort otherwise: a leaked test database is noise in a disposable
    /// environment, and panicking here would replace whatever the test actually
    /// found with a cleanup failure.
    pub async fn cleanup(self) {
        let Some(name) = self.owned.as_deref() else {
            return;
        };
        let statement = AssertSqlSafe(format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"));
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

/// A version-7 identifier no other test in this lane can name.
///
/// Fixtures used to spell their rows with readable constants — `FLEET`,
/// `WORKSPACE`, `GRANT_IDS` — which reads well and is fatal the moment every
/// test shares one database: a constant primary key is a unique-violation
/// waiting for a second test to run. Three of them were, and each cost a
/// five-minute lane run to find.
///
/// The process id keeps two test BINARIES apart and the counter keeps two tests
/// in one binary apart, so a caller gets a fresh identifier per call and can
/// hold it in a `let` where it used to read a `const`. That is the same thing
/// the suites' own `mint()` helpers do; this is it in one place, for the
/// fixtures that had no lane of their own to put it in.
///
/// The version nibble is `7` and the variant nibble is `8`, so the result
/// passes both the schema's `ck_*_id_uuidv7` checks and `afd_core::id::Uuid7`.
#[must_use]
pub fn mint_id() -> String {
    format!(
        "01900000-0000-7000-8000-{:06x}{:06x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// The lane's connection URL, or a panic naming the knob that is unset.
fn lane_url() -> String {
    std::env::var(LANE_KNOB).unwrap_or_else(|_unset| {
        panic!("{LANE_KNOB} is unset — run these through `make test-integration-rustd`")
    })
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
