//! Refresh-token rotation against a locked, encrypted vault row.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_credential::credential::platform::Platform;
use afd_credential::vault::rotate::Rotated;
use afd_credential::vault::{KeyRef, Vault};
use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use serde_json::Value;

#[path = "integration_rotation/activation.rs"]
mod activation;
#[path = "integration_rotation/provider_resolution.rs"]
mod provider_resolution;
#[path = "integration_rotation/registry.rs"]
mod registry;

const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);
const KEY_NAME: &str = "provider-handle";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn refresh_rotation_persists_only_the_handle_that_was_redeemed() {
    let fixture = Fixture::create().await;
    fixture
        .seed(KEY_NAME, br#"{"refresh_token":"old","account":"kept"}"#)
        .await;
    let key = fixture.key(KEY_NAME);

    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(key, "old", "replacement", NOW)
            .await
            .expect("the matching handle rotates"),
        Rotated::Persisted
    );
    let opened = fixture
        .vault
        .open(key)
        .await
        .expect("the envelope opens")
        .expect("the row exists");
    let body: Value = serde_json::from_slice(opened.expose()).expect("the handle is JSON");
    assert_eq!(
        body.get("refresh_token").and_then(Value::as_str),
        Some("replacement")
    );
    assert_eq!(body.get("account").and_then(Value::as_str), Some("kept"));

    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(key, "old", "must-not-win", NOW)
            .await
            .expect("a stale exchange is a typed outcome"),
        Rotated::SkippedStale
    );
    assert_eq!(
        fixture
            .vault
            .rotate_refresh_token(fixture.key("missing"), "old", "new", NOW)
            .await
            .expect("a deleted handle is a typed outcome"),
        Rotated::SkippedStale
    );

    fixture.seed("malformed", b"[]").await;
    let invalid = fixture
        .vault
        .rotate_refresh_token(fixture.key("malformed"), "old", "new", NOW)
        .await
        .expect_err("a stored handle must be a JSON object");
    assert_eq!(invalid.code(), error_code::VAULT_DATA_INVALID);

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_credentials_load_independently_and_degrade_bad_rows() {
    let fixture = Fixture::create().await;
    fixture
        .seed("github-app", br#"{"app_id":"42","private_key_pem":"pem"}"#)
        .await;
    fixture
        .seed(
            "zoho-app",
            br#"{"client_id":"zoho-client","client_secret":"secret"}"#,
        )
        .await;
    fixture.seed("jira-app", b"not-json").await;
    fixture.seed("linear-app", b"{}").await;

    let platform = Platform::load(&fixture.vault, &fixture.workspace).await;

    assert_eq!(platform.github().map(|app| app.app_id), Some(42));
    assert_eq!(
        platform.oauth("zoho").map(|app| app.client_id.as_str()),
        Some("zoho-client")
    );
    assert!(platform.oauth("jira").is_none());
    assert!(platform.oauth("linear").is_none());

    fixture.cleanup().await;
}

struct Fixture {
    lane: TestDatabase,
    database: Db,
    vault: Vault,
    kek: Arc<Kek>,
    tenant: Uuid7,
    workspace: Uuid7,
}

impl Fixture {
    async fn create() -> Self {
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let kek = Arc::new(Kek::from_bytes([0x42; 32]));
        let fixture = Self {
            vault: Vault::new(database.clone(), Arc::clone(&kek)),
            database,
            tenant: id(),
            workspace: id(),
            kek,
            lane,
        };
        fixture.seed_scope().await;
        fixture
    }

    const fn key<'fixture>(&'fixture self, name: &'fixture str) -> KeyRef<'fixture> {
        KeyRef {
            workspace_id: &self.workspace,
            name,
        }
    }

    /// A tenant row with NO workspace under it.
    ///
    /// The bootstrap invariant every other fixture upholds, deliberately
    /// broken: `primary_workspace` resolves the earliest-named workspace, so a
    /// tenant without one is the only way to reach the refusal that names it.
    /// Seeded as its own tenant rather than by deleting this fixture's
    /// workspace, which the vault rows reference.
    async fn seed_tenant_without_workspace(&self) -> Uuid7 {
        let orphan = id();
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at) \
             VALUES ($1::uuid, 'No workspace', 1, 1)",
        )
        .bind(orphan.as_str())
        .execute(&mut *connection)
        .await
        .expect("the workspace-less tenant seeds");
        orphan
    }

    async fn seed_scope(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "WITH tenant AS ( \
               INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ($1::uuid, 'Credential rotation', 1, 1) \
               RETURNING id \
             ) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT $2::uuid, id, $2, 'test', 1 FROM tenant",
        )
        .bind(self.tenant.as_str())
        .bind(self.workspace.as_str())
        .execute(&mut *connection)
        .await
        .expect("the vault scope seeds");
    }

    async fn seed(&self, name: &str, plaintext: &[u8]) {
        let envelope = Sealer::new()
            .seal(
                &self.kek,
                &Aad::new(self.workspace.as_str(), name),
                plaintext,
            )
            .expect("the fixture envelope seals");
        self.insert(name, &envelope).await;
    }

    /// Seeds a credential WITH the non-secret projection columns.
    ///
    /// `seed` leaves `meta_provider` and `meta_has_key` NULL, which is what a
    /// row storing something other than a provider credential looks like. The
    /// activation gate reads exactly those two, so a test about the gate has to
    /// be able to set them.
    async fn seed_with_shape(
        &self,
        name: &str,
        plaintext: &[u8],
        meta_provider: Option<&str>,
        meta_has_key: Option<bool>,
    ) {
        self.seed(name, plaintext).await;
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "UPDATE vault.secrets SET meta_provider = $3, meta_has_key = $4 \
             WHERE workspace_id = $1::uuid AND key_name = $2",
        )
        .bind(self.workspace.as_str())
        .bind(name)
        .bind(meta_provider)
        .bind(meta_has_key)
        .execute(&mut *connection)
        .await
        .expect("the projection columns seed");
    }

    /// Publishes one catalogue row, which the default below has a foreign key
    /// into: `platform_provider_defaults(provider, model)` references
    /// `model_library`, so a default can only name a model the catalogue
    /// carries — the schema's own way of saying an unpriced default is not a
    /// default.
    async fn seed_catalogue(&self, provider: &str, model: &str, cap: i32) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.model_library \
               (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
                cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
             VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, 1, 1) \
             ON CONFLICT (provider, model_id) DO NOTHING",
        )
        .bind(mint_id())
        .bind(model)
        .bind(provider)
        .bind(cap)
        .execute(&mut *connection)
        .await
        .expect("the catalogue row seeds");
    }

    /// Publishes one ACTIVE platform default, for the reset's copy path.
    ///
    /// `core.platform_provider_defaults` has no tenant column and the read is
    /// `WHERE active = true ... LIMIT 1`, so a caller seeding one is naming the
    /// deployment's default and not its own — see `registry.rs`'s header on
    /// what that costs and which half is therefore graded. Pass a provider name
    /// no other test uses: the key is the provider, so a shared one would have
    /// this seed silently rewrite a sibling's row rather than add its own.
    ///
    /// The row must be dropped by [`Self::clear_platform_default`] before
    /// cleanup — it points at this fixture's workspace, and the scope teardown
    /// cannot delete a workspace something still references.
    async fn seed_platform_default(&self, provider: &str, model: &str, cap: i32) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO core.platform_provider_defaults \
               (provider, source_workspace_id, active, model, context_cap_tokens, \
                created_at, updated_at) \
             VALUES ($1, $2::uuid, TRUE, $3, $4, $5, $5) \
             ON CONFLICT (provider) DO UPDATE SET \
               source_workspace_id = EXCLUDED.source_workspace_id, \
               active = TRUE, model = EXCLUDED.model, \
               context_cap_tokens = EXCLUDED.context_cap_tokens, \
               updated_at = EXCLUDED.updated_at",
        )
        .bind(provider)
        .bind(self.workspace.as_str())
        .bind(model)
        .bind(cap)
        .bind(NOW.as_millis())
        .execute(&mut *connection)
        .await
        .expect("the platform default seeds");
    }

    /// Drops the default this fixture published, freeing its workspace.
    async fn clear_platform_default(&self, provider: &str) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.platform_provider_defaults WHERE provider = $1")
            .bind(provider)
            .execute(&mut *connection)
            .await
            .expect("the platform default clears");
    }

    async fn insert(&self, name: &str, envelope: &Envelope) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO vault.secrets \
               (id, workspace_id, key_name, kek_version, encrypted_dek, dek_nonce, dek_tag, \
                nonce, ciphertext, tag, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, 1, 1)",
        )
        .bind(mint_id())
        .bind(self.workspace.as_str())
        .bind(name)
        .bind(envelope.kek_version())
        .bind(envelope.wrapped_dek())
        .bind(envelope.dek_nonce().as_slice())
        .bind(envelope.dek_tag().as_slice())
        .bind(envelope.payload_nonce().as_slice())
        .bind(envelope.payload_ciphertext())
        .bind(envelope.payload_tag().as_slice())
        .execute(&mut *connection)
        .await
        .expect("the sealed handle seeds");
    }

    async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query("DELETE FROM core.tenants WHERE id = $1::uuid")
            .bind(self.tenant.as_str())
            .execute(&mut *connection)
            .await
            .expect("the scoped fixture cleans up");
        drop(connection);
        drop(self.database);
        self.lane.cleanup().await;
    }
}

fn id() -> Uuid7 {
    Uuid7::parse(&mint_id()).expect("the minted fixture id is UUIDv7")
}
