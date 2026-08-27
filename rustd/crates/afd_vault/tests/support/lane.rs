//! The lane's database, a resolved key, and the rows a secret needs to exist.
//!
//! Built on [`afd_db::test_util::TestDatabase`], which hands back the database
//! the lane already migrated. Nothing here creates or migrates one — see that
//! module on why a database per test was six thousand seven hundred migration
//! applications buying isolation the minted identifiers already gave.
//!
//! # No Redis, deliberately
//!
//! Nothing on this surface touches the queue. The fleet lifecycle's lane
//! connects one because its install guarantee is about a stream; a secret is a
//! row and an envelope, so a fixture that opened Redis would be making this
//! suite depend on a service its subject does not use.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Envelope;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_vault::{Directory, SecretBody, SecretName, Vault};
use serde_json::value::RawValue;
use sqlx::Row as _;

/// The instant every fixture row is stamped with.
pub(crate) const NOW_MS: i64 = 1_760_000_000_000;

/// The process key this suite seals under.
///
/// A literal, and the same one the daemon's own boot fixtures use: the key is
/// not what any test here is about, and drawing a fresh one per run would make
/// a failing assertion depend on which bytes it drew.
const KEK_HEX: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// A migrated database, the store over it, and the seeded tenant it lives in.
pub(crate) struct Lane {
    database: TestDatabase,
    pub(crate) pool: Db,
    pub(crate) vault: Vault,
    pub(crate) workspace: Uuid7,
    pub(crate) tenant: Uuid7,
    kek: Arc<Kek>,
}

impl Lane {
    /// Seeds a tenant and one workspace inside the lane's shared database.
    ///
    /// No `CREATE DATABASE` and no migration — the lane brought both. Isolation
    /// is the minted `tenant` and `workspace` below: no other test can name
    /// them, and every statement this suite exercises carries one of them in
    /// its predicate.
    pub(crate) async fn create() -> Self {
        let database = TestDatabase::shared();
        let pool = database.open(DbRole::Api, &[]).await;
        let kek = Arc::new(Kek::from_hex(KEK_HEX).expect("the fixture key is well formed"));
        let lane = Self {
            tenant: mint(),
            workspace: mint(),
            vault: Vault::new(pool.clone(), Arc::clone(&kek), Entropy::new()),
            database,
            pool,
            kek,
        };
        lane.seed_tenant_and_workspace().await;
        lane
    }

    /// A directory built from the pool alone, holding NO key.
    ///
    /// The never-decrypt proof's instrument. `Vault::directory()` would answer
    /// identically and is what production calls; this constructs one
    /// independently so the assertion does not rest on a value that has ever
    /// been near a key.
    pub(crate) fn keyless_directory(&self) -> Directory {
        Directory::new(self.pool.clone())
    }

    /// A second workspace under the same tenant, for the cross-workspace proofs.
    pub(crate) async fn another_workspace(&self) -> Uuid7 {
        let workspace = mint();
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, 'second', 'fixture', $3)",
        )
        .bind(workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a second workspace");
        workspace
    }

    /// Stores one secret through the production write path.
    pub(crate) async fn store(&self, name: &str, data: &str) {
        self.vault
            .create(&self.workspace, &named(name), &body(data), Self::now())
            .await
            .expect("the fixture secret must store");
    }

    /// The four projection columns of one row, as text.
    ///
    /// `None` for a row that is not there. Each entry is `None` where the column
    /// is NULL, which is how a pre-projection row reads.
    pub(crate) async fn meta_columns(&self, name: &str) -> Option<StoredProjection> {
        sqlx::query(
            "SELECT meta_kind, meta_provider, meta_base_url, meta_has_key \
               FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2",
        )
        .bind(self.workspace.as_str())
        .bind(name)
        .fetch_optional(&mut *self.connection().await)
        .await
        .expect("reading the projection columns")
        .map(|row| StoredProjection {
            kind: row.try_get(0).expect("meta_kind is text"),
            provider: row.try_get(1).expect("meta_provider is text"),
            base_url: row.try_get(2).expect("meta_base_url is text"),
            has_key: row.try_get(3).expect("meta_has_key is boolean"),
        })
    }

    /// The plaintext stored under `name`, opened with the fixture's own key.
    ///
    /// The suite decrypts; the daemon's secret surface does not. That asymmetry
    /// is the point — this is how a test checks that the projection describes
    /// the bytes actually sealed beside it.
    pub(crate) async fn opened(&self, name: &str) -> String {
        let row = sqlx::query(
            "SELECT encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version \
               FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2",
        )
        .bind(self.workspace.as_str())
        .bind(name)
        .fetch_one(&mut *self.connection().await)
        .await
        .expect("the row must exist");

        let column = |index: usize| row.try_get::<Vec<u8>, _>(index).expect("a bytea column");
        let opened = Envelope::from_parts(
            column(0),
            &column(1),
            &column(2),
            &column(3),
            column(4),
            &column(5),
            row.try_get(6).expect("kek_version is an integer"),
        )
        .expect("the stored columns are a well-formed envelope")
        .open(&self.kek, &Aad::new(self.workspace.as_str(), name))
        .expect("the envelope opens under the fixture key");

        String::from_utf8(opened.expose().to_vec()).expect("the plaintext is UTF-8 JSON")
    }

    /// Flips every bit of the first ciphertext byte, so nothing can open the row.
    ///
    /// The never-decrypt proof's teeth. `secret_list.zig` answers this row as an
    /// opaque `custom_secret`, because its projection comes from a body it could
    /// not decrypt. A list that reads the columns is unaffected — so the two
    /// implementations give observably different answers here, and that
    /// difference is the assertion rather than a comment.
    ///
    /// XOR with 255 rather than overwriting with a fixed byte: a fixed byte is a
    /// no-op one time in two hundred and fifty-six, and a test that silently
    /// stops corrupting anything is worse than no test.
    pub(crate) async fn corrupt_ciphertext(&self, name: &str) {
        sqlx::query(
            "UPDATE vault.secrets \
                SET ciphertext = set_byte(ciphertext, 0, get_byte(ciphertext, 0) # 255) \
              WHERE workspace_id = $1::uuid AND key_name = $2",
        )
        .bind(self.workspace.as_str())
        .bind(name)
        .execute(&mut *self.connection().await)
        .await
        .expect("corrupting the ciphertext");
    }

    /// Seeds a row the way another daemon would have written it.
    ///
    /// The ciphertext columns are borrowed from a row this suite sealed, so the
    /// envelope is real; the projection is supplied verbatim, which is what lets
    /// a test say "these exact `meta_*` values list like this" without a Zig
    /// process in the loop. `None` for `kind` seeds a row from before the
    /// projection columns existed.
    pub(crate) async fn seed_projected_row(
        &self,
        name: &str,
        borrow_envelope_from: &str,
        kind: Option<&str>,
        provider: Option<&str>,
        base_url: Option<&str>,
        has_key: Option<bool>,
    ) {
        sqlx::query(
            "INSERT INTO vault.secrets \
               (id, workspace_id, key_name, \
                encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, \
                created_at, updated_at, \
                meta_kind, meta_provider, meta_base_url, meta_has_key) \
             SELECT $3::uuid, workspace_id, $4, \
                    encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag, kek_version, \
                    $5, $5, $6, $7, $8, $9 \
               FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2",
        )
        .bind(self.workspace.as_str())
        .bind(borrow_envelope_from)
        .bind(mint().as_str())
        .bind(name)
        .bind(NOW_MS)
        .bind(kind)
        .bind(provider)
        .bind(base_url)
        .bind(has_key)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a projected row");
    }

    /// Adds one model registry entry naming `secret_ref`.
    pub(crate) async fn seed_model_entry(&self, model_id: &str, secret_ref: &str) {
        sqlx::query(
            "INSERT INTO core.tenant_model_entries \
               (id, tenant_id, model_id, secret_ref, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $5)",
        )
        .bind(mint().as_str())
        .bind(self.tenant.as_str())
        .bind(model_id)
        .bind(secret_ref)
        .bind(NOW_MS)
        .execute(&mut *self.connection().await)
        .await
        .expect("seeding a model registry entry");
    }

    /// Removes every registry entry naming `secret_ref`.
    pub(crate) async fn clear_model_entries(&self, secret_ref: &str) {
        sqlx::query(
            "DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid AND secret_ref = $2",
        )
        .bind(self.tenant.as_str())
        .bind(secret_ref)
        .execute(&mut *self.connection().await)
        .await
        .expect("clearing model registry entries");
    }

    /// How many secret rows a workspace holds.
    pub(crate) async fn secret_count(&self, workspace: &Uuid7) -> i64 {
        sqlx::query("SELECT count(*) FROM vault.secrets WHERE workspace_id = $1::uuid")
            .bind(workspace.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("counting secrets")
            .try_get::<i64, _>(0)
            .expect("a count is a bigint")
    }

    /// The instant this suite stamps writes with.
    pub(crate) const fn now() -> UnixMillis {
        UnixMillis::from_millis(NOW_MS)
    }

    /// Releases the lane handle. A no-op on the shared database, and called
    /// anyway: it is how a test says it is finished.
    pub(crate) async fn cleanup(self) {
        self.database.cleanup().await;
    }

    /// One pooled connection, for the fixture's own reads and writes.
    async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.pool
            .acquire()
            .await
            .expect("the fixture database must answer")
    }

    /// Seeds the tenant and the workspace every fixture secret lands in.
    async fn seed_tenant_and_workspace(&self) {
        let mut connection = self.connection().await;
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at) \
             VALUES ($1::uuid, 'fixture', $2, $2)",
        )
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a tenant");

        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             VALUES ($1::uuid, $2::uuid, 'fixture', 'fixture', $3)",
        )
        .bind(self.workspace.as_str())
        .bind(self.tenant.as_str())
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("seeding a workspace");
    }
}

/// The four projection columns, as the table holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredProjection {
    pub(crate) kind: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) has_key: Option<bool>,
}

/// A parsed secret name, for a fixture that knows its own literals are valid.
pub(crate) fn named(name: &str) -> SecretName {
    SecretName::parse(name).expect("the fixture name is within bounds")
}

/// A parsed secret body, for the same reason.
pub(crate) fn body(json: &str) -> SecretBody {
    let raw = RawValue::from_string(json.to_owned()).expect("the fixture body is valid JSON");
    SecretBody::parse(&raw).expect("the fixture body is a non-empty object within bounds")
}

/// A fresh identifier, so no two fixtures can name each other's rows.
pub(crate) fn mint() -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(Lane::now(), bytes).expect("a well-formed identifier")
}
