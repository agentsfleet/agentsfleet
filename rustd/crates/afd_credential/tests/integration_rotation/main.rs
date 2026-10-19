//! The credential store against live Postgres: the shared [`Fixture`] every
//! case seeds through, and one module per concern.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_credential::vault::{KeyRef, Vault};
use afd_crypto::aad::Aad;
use afd_crypto::envelope::{Envelope, Sealer};
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{
    DefaultSeed, ModelSeed, PlatformDefault, TenantSeed, TestDatabase, WorkspaceSeed, mint_id,
};
use jiff::Timestamp;

mod activation;
mod activation_ceiling;
mod provider_resolution;
mod registry;
mod registry_default;
mod registry_page;
mod registry_tenancy;
mod registry_walk;
mod rotation;
mod selection_pointer;

const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);
const KEY_NAME: &str = "provider-handle";

/// Points a tenant's selection at one entry, as a raw row.
///
/// `$1` tenant · `$2` entry.
const POINT_SELECTION: &str = "\
INSERT INTO model.tenant_selections (tenant_id, tenant_model_id, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, now(), now())
ON CONFLICT (tenant_id) DO UPDATE SET tenant_model_id = EXCLUDED.tenant_model_id,
                                      updated_at = now()";

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
        TenantSeed::new(orphan.as_str())
            .name("No workspace")
            .insert(&mut *connection)
            .await
            .expect("the workspace-less tenant seeds");
        orphan
    }

    async fn seed_scope(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        TenantSeed::new(self.tenant.as_str())
            .name("Credential rotation")
            .insert(&mut *connection)
            .await
            .expect("the vault scope's tenant seeds");
        WorkspaceSeed::new(self.workspace.as_str(), self.tenant.as_str())
            .insert(&mut *connection)
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

    /// Publishes synthetic identity evidence and a zero-priced test quote.
    async fn seed_catalogue(&self, provider: &str, model: &str, cap: i32) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        ModelSeed::new(&mint_id(), provider, model, cap)
            .verified_at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the catalogue row seeds");
    }

    /// Publishes one ACTIVE platform default, for the reset's copy path, and
    /// holds it.
    ///
    /// `model.platform_defaults` has no tenant column and the read is
    /// `WHERE active = true ... LIMIT 1`, so a caller seeding one is naming the
    /// deployment's default and not its own — see `registry.rs`'s header on
    /// what that costs and which half is therefore graded. Pass a provider name
    /// no other test uses: the key is the provider.
    ///
    /// Drop the hold before cleanup — the row points at this fixture's
    /// workspace, and the scope teardown cannot delete a workspace something
    /// still references. A test that panics first drops it while unwinding, so
    /// the row never outlives the test either way.
    async fn seed_platform_default(
        &self,
        provider: &str,
        model: &str,
        cap: i32,
    ) -> PlatformDefault {
        PlatformDefault::hold(
            &self.database,
            DefaultSeed {
                provider,
                source_workspace_id: self.workspace.as_str(),
                model,
                context_cap_tokens: cap,
                created_at: NOW,
            },
        )
        .await
    }

    /// Points `tenant`'s selection straight at one of its registry entries.
    ///
    /// Written as the row rather than through `activate`, so a test about the
    /// page or the pointer does not also depend on the catalogue gate and a
    /// decrypt. The composite foreign key still checks the entry is the
    /// tenant's own.
    async fn point_selection_at(&self, tenant: &Uuid7, entry: &Uuid7) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(POINT_SELECTION)
            .bind(tenant.as_str())
            .bind(entry.as_str())
            .execute(&mut *connection)
            .await
            .expect("the selection points at the entry");
    }

    async fn insert(&self, name: &str, envelope: &Envelope) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(
            "INSERT INTO vault.secrets \
               (id, workspace_id, key_name, kek_version, dek_ciphertext, dek_nonce, dek_tag, \
                secret_nonce, secret_ciphertext, secret_tag, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, now(), now())",
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
        sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
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
