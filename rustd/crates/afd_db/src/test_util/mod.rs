//! The lane's database, shared by every test in the run — and the one exception.
//!
//! # One database, migrated once
//!
//! [`TestDatabase::shared`] hands back the lane's own database. Nothing is
//! created, nothing is migrated and nothing is dropped: `make
//! test-integration-rustd` drops the schemas once, applies the migrations once,
//! and every test in the run then works inside that one database.
//!
//! The rule in one line: runs against the LIVE test database, never creates
//! temp tables.
//!
//! **What a database per test costs.** Every test would create a database of
//! its own and apply all forty-seven `schema/*.sql` files into it. At a
//! hundred and forty-three tests that is roughly six thousand seven hundred
//! migration applications per lane run, to produce one schema a hundred and
//! forty-three times: over two minutes of wall clock spent on nothing a test
//! asserts.
//!
//! # What replaces the isolation, since something must
//!
//! Not nothing. Every fixture MINTS its own identifiers — a tenant, a
//! workspace, a fleet — so two tests cannot name each other's rows, and every
//! statement in this workspace carries that identifier in its predicate. That
//! is the isolation that does the work; a database per test was belt over
//! braces, and the braces are the ones holding.
//!
//! It is the same isolation the Dragonfly side has always relied on, for the reason
//! it has always relied on it: Dragonfly has no per-test database, so suites
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
//! without testing something else.

#![expect(
    clippy::panic,
    reason = "a feature-gated fixture: it compiles into the crate only because a sibling integration test needs it, and every panic here reports a LANE fault — an unset knob, an unreachable Postgres — where a Result would hand the test a value it could only unwrap (dispatch/write_rust.md, test-util carve-out)"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::env::{EnvSource, MapEnv, ProcessEnv};
use sqlx::AssertSqlSafe;

use crate::config::{DbRole, PoolConfig};
use crate::pool::Db;

mod billing;
mod connector;
mod fleet;
mod fleet_event;
mod identity;
mod platform_default;
mod platform_model;
mod rows;
mod runner;
mod unreachable;

pub use self::billing::WalletSeed;
pub use self::connector::{GrantSeed, InstallSeed};
pub use self::fleet::{
    FIXTURE_SETUP_DEFINITION, FIXTURE_SETUP_DIGEST, FIXTURE_SETUP_MODEL_REF,
    FIXTURE_SETUP_SEMANTICS, FleetSeed, SetupRevision,
};
pub use self::fleet_event::{EventKind, FleetEventSeed};
pub use self::identity::{ApiKeySeed, TenantSeed, UserSeed, WorkspaceSeed};
pub use self::platform_default::{DefaultSeed, PlatformDefault};
pub use self::platform_model::ModelSeed;
pub use self::rows::OwnedRows;
pub use self::runner::{RunnerPolicy, RunnerSeed};
pub use self::unreachable::unreachable_db;

/// The fixed instant fixtures stamp on the rows they seed: one millisecond past
/// the epoch, so it survives `TIMESTAMPTZ(3)` unchanged.
pub const SEEDED_AT: jiff::Timestamp = jiff::Timestamp::constant(0, 1_000_000);

/// The environment knob naming the lane's admin connection.
const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// The environment knob naming the lane's API login: a member of
/// `api_runtime` and nothing else, as production's `DATABASE_URL_API` is
/// (`playbooks/operations/database_region_move/001_playbook.md`).
const API_KNOB: &str = "TEST_DATABASE_URL_API";

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
    /// The same server through the API login, which is what [`DbRole::Api`]
    /// connects as, so a missing grant fails here as it fails in production.
    api_base_url: String,
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
            api_base_url: api_lane_url(),
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
            api_base_url: api_lane_url(),
            owned: Some(name),
        }
    }

    /// The connection URL this handle's tests run against.
    ///
    /// The lane's URL unchanged for a shared handle. For an owned one, the same
    /// URL with the database swapped — keeping whatever query string the lane
    /// carried, `sslmode` among them, which a lane against a TLS Postgres needs
    /// and a rebuilt URL would silently drop.
    ///
    /// Swapped through a parse rather than at the last `/`: a query carrying a
    /// path of its own (`sslrootcert=/etc/ca.pem`) has a later `/` than the
    /// database's, and a lane URL naming no database has none at all.
    ///
    /// # Panics
    /// When the lane's URL is not a URL, which is a lane fault.
    #[must_use]
    pub fn url(&self) -> String {
        self.on_this_database(&self.base_url, LANE_KNOB)
    }

    /// [`TestDatabase::url`] through the API login: what [`DbRole::Api`]
    /// connects as.
    ///
    /// # Panics
    /// When the API lane URL is not a URL, which is a lane fault.
    #[must_use]
    pub fn api_url(&self) -> String {
        self.on_this_database(&self.api_base_url, API_KNOB)
    }

    /// `lane` with this handle's database swapped in, when it owns one.
    fn on_this_database(&self, lane: &str, knob: &str) -> String {
        let Some(name) = self.owned.as_deref() else {
            return lane.to_owned();
        };
        let mut url = url::Url::parse(lane)
            .unwrap_or_else(|failure| panic!("{knob} must be a URL: {failure}"));
        url.set_path(&format!("/{name}"));
        url.into()
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

    /// An environment pointing every role at this database: the API role
    /// through the API login, every other role through the lane's owner.
    #[must_use]
    pub fn env(&self, extra: &[(&str, &str)]) -> MapEnv {
        let (url, api_url) = (self.url(), self.api_url());
        let mut pairs: Vec<(String, String)> = DbRole::ALL
            .iter()
            .map(|role| {
                let target = if *role == DbRole::Api { &api_url } else { &url };
                (role.url_knob().to_owned(), target.clone())
            })
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

/// [`mint_id`] as the typed identifier, for the suites that hold a `Uuid7`.
///
/// # Panics
/// Never in practice: [`mint_id`] spells a canonical `UUIDv7` by construction.
#[must_use]
pub fn mint_uuid7() -> afd_core::id::Uuid7 {
    afd_core::id::Uuid7::parse(&mint_id())
        .unwrap_or_else(|failure| panic!("mint_id spells a canonical UUIDv7: {failure}"))
}

/// The lane's connection URL, or a panic naming the knob that is unset.
fn lane_url() -> String {
    knob(LANE_KNOB)
}

/// The lane's API login URL, or a panic naming the knob that is unset.
fn api_lane_url() -> String {
    knob(API_KNOB)
}

fn knob(name: &str) -> String {
    ProcessEnv.get(name).unwrap_or_else(|| {
        panic!("{name} is unset — run these through `make test-integration-rustd`")
    })
}

/// Installs a subscriber so event macros actually run.
///
/// Re-exported for the live suites that already reach it through this seed
/// home; the one implementation is `afd_core`'s.
pub use afd_core::test_util::trace::install_subscriber;

#[cfg(test)]
mod tests;
