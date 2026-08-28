//! The lane's database, and the credential rows a lookup is meant to find.
//!
//! # One database, and what keeps the tests apart in it
//!
//! Every assertion here is about what a DIGEST resolves to, and a digest is
//! unique per test because the row it is computed from is: each fixture mints
//! its own credential material and looks up only what it filed. That is the
//! isolation; a database per test was belt over braces, and it cost the lane
//! forty-seven migrations per test to provide.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which is where the four
//! near-identical copies of this — `afd_db`'s own, `afd_fleet`'s,
//! `agentsfleetd`'s and the one that used to live here — were always headed.
//! This is one of the deletions that module predicted.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_auth::credential::Presented;
use afd_auth::directory::Digest;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, Migrator};
use sqlx::AssertSqlSafe;

/// A handle on the lane's schema-loaded database, and the api-role pool over it.
pub(crate) struct Fixtures {
    lane: TestDatabase,
    database: Db,
}

impl Fixtures {
    /// Opens the api-role pool against the database the lane already migrated.
    ///
    /// What almost every test here wants. The two that DESTROY something take
    /// [`Fixtures::create_disposable`] instead.
    pub(crate) async fn create() -> Self {
        let lane = TestDatabase::shared();
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            lane,
        }
    }

    /// A database of this test's own, created and migrated for it alone.
    ///
    /// For the two failure paths that break the datastore on purpose —
    /// [`Fixtures::drop_table`] and [`Fixtures::destroy_database`]. Those
    /// cannot run against the shared lane database for the obvious reason: the
    /// next test would find the table, or the database, gone.
    ///
    /// This is the exception the Zig harness carried too, and it stayed one
    /// file wide there for the same reason it is two tests wide here — a
    /// database of your own costs forty-seven migrations, so it is worth having
    /// only when the test is ABOUT the datastore failing.
    pub(crate) async fn create_disposable() -> Self {
        let lane = TestDatabase::create().await;
        let migrator = lane.open(DbRole::Migrator, &[]).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema must apply to a fresh database");
        drop(migrator);
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            lane,
        }
    }

    /// The pool a directory reads through.
    pub(crate) const fn database(&self) -> &Db {
        &self.database
    }

    /// Releases the handle. A no-op against the shared database, and called
    /// anyway: it is how a test says it is finished.
    pub(crate) async fn cleanup(self) {
        // Our own pool goes first, so nothing is still borrowing a connection
        // when the handle is released.
        drop(self.database);
        self.lane.cleanup().await;
    }
}

/// The digest a presented credential is stored under.
pub(crate) fn digest_of(presented: &str) -> Digest {
    Digest::of(&Presented::new(presented).expect("a fixture credential is not blank"))
}

// ── Row seeding ─────────────────────────────────────────────────────────────

/// A canonical `UUIDv7` built from `seed`, for a fixture needing an identifier.
///
/// The process id rides in the low field beside the seed. Every test in this
/// binary already picks a distinct seed, so the seed alone keeps them apart —
/// but the lane is now ONE database shared by every binary in the run, and a
/// deterministic identifier is exactly how two suites come to name the same
/// row. The process id costs nothing and removes the class.
pub(crate) fn identifier(seed: u32) -> String {
    format!(
        "01900000-0000-7000-8000-{:06x}{seed:06x}",
        std::process::id()
    )
}

/// A seed no other call in this binary will use.
///
/// For the rows a caller never names: `api_key` and `cli_credential` invent
/// their own primary key, and each used ONE fixed seed — an id per HELPER
/// rather than per CALL. Two tests seeding an api key therefore wrote the same
/// id, which a database apiece hid completely and one shared database reports
/// as `duplicate key value violates unique constraint "api_keys_pkey"`.
///
/// Starts above every seed the suites spell by hand (1..42), so a minted row
/// can never land on one a test is also naming.
fn minted_seed() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(9_000);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// An identifier the schema accepts and [`afd_core::id::Uuid7`] refuses.
///
/// The version nibble is `7`, which is all `ck_*_id_uuidv7` checks — but the
/// variant nibble is `0`, and RFC 4122 spells that `8`, `9`, `a` or `b`. So
/// this is a value Postgres will store and the port will not read, which is
/// exactly the corrupt row the malformed-identifier path exists for.
pub(crate) fn identifier_with_bad_variant(seed: u32) -> String {
    format!(
        "01900000-0000-7000-0000-{:06x}{seed:06x}",
        std::process::id()
    )
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
            &[tenant, digest.as_str(), subject, &identifier(minted_seed())],
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
            &[user, tenant, digest.as_str(), &identifier(minted_seed())],
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
    ///
    /// Only ever a DISPOSABLE fixture's table — see [`Fixtures::destroy_database`].
    pub(crate) async fn drop_table(&self, table: &str) {
        self.run(&format!("DROP TABLE {table} CASCADE"), &[]).await;
    }

    /// Destroys the database while this pool still holds connections to it.
    ///
    /// Only ever a DISPOSABLE fixture's database — [`Fixtures::create`] holds
    /// the lane's, and dropping that would take every other test with it.
    ///
    /// Every pooled connection dies with it, so the next acquire fails — the
    /// datastore-unreachable half of the two failure paths, where
    /// [`Self::drop_table`] is the statement-refused half.
    pub(crate) async fn destroy_database(&self) {
        let statement = AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS {} WITH (FORCE)",
            self.lane.database_name().expect(
                "destroy_database is only reachable from a disposable fixture — see create_disposable"
            )
        ));
        let pool = sqlx::PgPool::connect(self.lane.lane_url())
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
