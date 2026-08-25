//! A schema-loaded database, and the credential rows a lookup is meant to find.
//!
//! # Why a database per test
//!
//! Every assertion here is about what a digest resolves to, and a digest is
//! unique per test only if the rows are. Sharing one database would make each
//! test's precondition depend on the previous test's cleanup, which is how a
//! suite starts passing in one order and failing in another.
//!
//! # A note on where this belongs
//!
//! `afd_db`'s own suites carry a near-identical `TestDatabase`, included by
//! `#[path]` into three test binaries, and `install_subscriber` is already
//! written out twice over there. The right home for all of it is
//! `afd_db::test_util`, behind the feature that already exists for exactly this
//! — moving it is a follow-up rather than part of this change, because it would
//! edit a green integration suite that cannot be re-run until the pre-PR lane.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use sqlx::AssertSqlSafe;

/// The lane's admin connection URL.
const LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Distinguishes databases created by one process, combined with the process
/// id so two lanes on one host cannot collide either.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A schema-loaded database created for one test and dropped with it.
pub(crate) struct Fixtures {
    base_url: String,
    name: String,
    database: Db,
}

impl Fixtures {
    /// Creates a database, migrates it, and opens the api-role pool.
    pub(crate) async fn create() -> Self {
        install_subscriber();
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_state_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        // The name is a process id and a counter, never input — which is what
        // makes interpolating it safe, since Postgres does not bind
        // identifiers. `AssertSqlSafe` is sqlx 0.9 asking that at the type
        // level; every other statement in this crate is a literal.
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;

        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema must apply to a fresh database");
        drop(migrator);

        Self {
            database: open(&url, DbRole::Api).await,
            base_url,
            name,
        }
    }

    /// The pool a directory reads through.
    pub(crate) const fn database(&self) -> &Db {
        &self.database
    }

    /// Drops the database. Best-effort: a leaked test database is noise in a
    /// disposable environment, not a failure worth masking the real one with.
    pub(crate) async fn cleanup(self) {
        let Self {
            base_url,
            name,
            database,
        } = self;
        // Our own pool goes first, so the drop is not racing connections this
        // test still holds. `WITH (FORCE)` would evict them anyway; closing is
        // the difference between a clean teardown and one that relies on it.
        drop(database);

        let statement = AssertSqlSafe(format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"));
        let Ok(pool) = sqlx::PgPool::connect(&base_url).await else {
            return;
        };
        if let Ok(mut connection) = pool.acquire().await {
            let _dropped = sqlx::query(statement).execute(&mut *connection).await;
        }
        pool.close().await;
    }
}

/// The connection URL for one named database on the lane's server.
fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL carries a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

/// Opens a pool for one role against `url`.
async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the test URL resolves"))
        .await
        .expect("the test database must accept a connection")
}

/// Runs one statement on the lane's admin database.
async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane's database must be reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement must run");
    drop(connection);
    pool.close().await;
}

/// The digest a presented credential is stored under.
pub(crate) fn digest_of(presented: &str) -> Digest {
    Digest::of(&Presented::new(presented).expect("a fixture credential is not blank"))
}

/// Installs a subscriber so event macros actually run.
///
/// `tracing::error!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it, so with no subscriber a diagnostic's fields never run —
/// the failure path executes and the line reporting it does not. Output goes to
/// a sink; the point is evaluation, not reading.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _previous = tracing::subscriber::set_global_default(subscriber);
    });
}

// ── Row seeding ─────────────────────────────────────────────────────────────

/// A canonical `UUIDv7` built from `seed`, for a fixture needing an identifier.
pub(crate) fn identifier(seed: u32) -> String {
    format!("01900000-0000-7000-8000-{seed:012x}")
}

/// An identifier the schema accepts and [`afd_core::id::Uuid7`] refuses.
///
/// The version nibble is `7`, which is all `ck_*_id_uuidv7` checks — but the
/// variant nibble is `0`, and RFC 4122 spells that `8`, `9`, `a` or `b`. So
/// this is a value Postgres will store and the port will not read, which is
/// exactly the corrupt row the malformed-identifier path exists for.
pub(crate) fn identifier_with_bad_variant(seed: u32) -> String {
    format!("01900000-0000-7000-0000-{seed:012x}")
}

impl Fixtures {
    /// Inserts a tenant, returning its identifier.
    pub(crate) async fn tenant(&self, id: &str) -> String {
        self.run(
            "INSERT INTO core.tenants (id, name, created_at, updated_at) \
             VALUES ($1::uuid, 'fixture', 0, 0)",
            &[id],
        )
        .await;
        id.to_owned()
    }

    /// Inserts a user under `tenant`, returning its identifier.
    pub(crate) async fn user(&self, id: &str, tenant: &str, subject: &str) -> String {
        self.run(
            "INSERT INTO core.users \
             (id, tenant_id, oidc_subject, email, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $1 || '@example.test', 0, 0)",
            &[id, tenant, subject],
        )
        .await;
        id.to_owned()
    }

    /// Inserts a tenant api-key. `active` and `revoked_at` move together, which
    /// the schema insists on: `ck_api_keys_revoked_iff_inactive` refuses a row
    /// that is inactive without a revocation instant, or carries one while
    /// active.
    pub(crate) async fn api_key(&self, tenant: &str, digest: &Digest, subject: &str, active: bool) {
        let revoked_at = if active { "NULL" } else { "1" };
        self.run(
            &format!(
                "INSERT INTO core.api_keys \
                 (id, tenant_id, key_name, description, key_hash, created_by, \
                  active, revoked_at, created_at, updated_at) \
                 VALUES ($4::uuid, $1::uuid, $2, '', $2, $3, {active}, {revoked_at}, 0, 0)"
            ),
            &[tenant, digest.as_str(), subject, &identifier(9_001)],
        )
        .await;
    }

    /// Inserts a command-line credential for `user`.
    pub(crate) async fn cli_credential(
        &self,
        user: &str,
        tenant: &str,
        digest: &Digest,
        revoked: bool,
    ) {
        let revoked_at = if revoked { "1" } else { "NULL" };
        self.run(
            &format!(
                "INSERT INTO core.cli_credentials \
                 (id, user_id, tenant_id, machine_name, credential_hash, \
                  credential_prefix, deployment, created_from_address, \
                  created_at, revoked_at) \
                 VALUES ($4::uuid, $1::uuid, $2::uuid, 'fixture', $3, 'afc_', \
                         'test', '127.0.0.1', 0, {revoked_at})"
            ),
            &[user, tenant, digest.as_str(), &identifier(9_002)],
        )
        .await;
    }

    /// Inserts a runner in `admin_state`.
    pub(crate) async fn runner(
        &self,
        id: &str,
        digest: &Digest,
        admin_state: &str,
        degraded: bool,
    ) {
        self.run(
            &format!(
                "INSERT INTO fleet.runners \
                 (id, host_id, token_hash, sandbox_tier, admin_state, labels, \
                  degraded, last_seen_at, created_at, updated_at) \
                 VALUES ($1::uuid, 'fixture-host', $2, 'standard', $3, '{{}}'::jsonb, \
                         {degraded}, 0, 0, 0)"
            ),
            &[id, digest.as_str(), admin_state],
        )
        .await;
    }

    /// Removes a table out from under the directory.
    ///
    /// The pool still answers; the statement no longer can. That is the shape
    /// of a migration mid-flight, and it is the cheapest deterministic way to
    /// make a query fail without touching the connection.
    pub(crate) async fn drop_table(&self, table: &str) {
        self.run(&format!("DROP TABLE {table} CASCADE"), &[]).await;
    }

    /// Destroys the database while this pool still holds connections to it.
    ///
    /// Every pooled connection dies with it, so the next acquire fails — the
    /// datastore-unreachable half of the two failure paths, where
    /// [`Self::drop_table`] is the statement-refused half.
    pub(crate) async fn destroy_database(&self) {
        let statement = AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.name
        ));
        let pool = sqlx::PgPool::connect(&self.base_url)
            .await
            .expect("the lane's database must be reachable");
        let mut connection = pool.acquire().await.expect("an admin connection");
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the database must drop");
        drop(connection);
        pool.close().await;
    }

    /// Runs one seeding statement with text binds.
    async fn run(&self, statement: &str, binds: &[&str]) {
        let mut connection = self
            .database
            .acquire()
            .await
            .expect("a connection for seeding");
        let mut query = sqlx::query(AssertSqlSafe(statement.to_owned()));
        for bind in binds {
            query = query.bind((*bind).to_owned());
        }
        query
            .execute(&mut *connection)
            .await
            .expect("the fixture row must insert");
    }
}
