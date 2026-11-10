//! The platform key cache answers only for the versions a claim read, and a
//! tenant's own model reference is read every time (M245 §3).
//!
//! A private database, counted with `pg_stat_statements`: platform defaults
//! are deployment-wide, so a default another suite seeded on the shared lane
//! would be the one both reads choose.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_credential::provider::cache::PlatformKeyKey;
use afd_credential::provider::{Added, PLATFORM_MODEL_REF, Providers};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::statements::{self, Ran, Window};
use afd_db::test_util::{DefaultSeed, ModelSeed, TenantSeed, TestDatabase, WorkspaceSeed, mint_id};
use afd_vault::{SecretBody, SecretName};
use jiff::Timestamp;
use serde_json::value::RawValue;

const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);
/// The platform default's provider, which is also its vault row's name.
const PROVIDER: &str = "anthropic";
const MODEL: &str = "claude-cache-fixture";
const CAP: i32 = 200_000;

/// The versions the lease claim reads beside the slot it takes, by the same
/// choice of active default (`claim_admission_head.sql`'s `platform`).
const OBSERVE: &str = "SELECT c.resource_version, s.id, s.resource_version \
    FROM model.catalogue_version c \
    CROSS JOIN LATERAL ( \
      SELECT s.id, s.resource_version FROM model.platform_defaults p \
      JOIN vault.secrets s ON s.workspace_id = p.source_workspace_id AND s.key_name = p.provider \
      WHERE p.active AND s.meta_has_key AND s.meta_provider = p.provider \
      ORDER BY p.provider LIMIT 1) s \
    WHERE c.id = 1";

struct Lane {
    database: TestDatabase,
    owner: Db,
    api: Db,
    providers: Providers,
    vault: afd_vault::Vault,
    tenant: Uuid7,
    workspace: Uuid7,
}

impl Lane {
    async fn create() -> Self {
        let (database, owner) = statements::counted_database().await;
        let api = database.open(DbRole::Api, &[]).await;
        let (tenant, workspace) = (mint_id(), mint_id());
        let mut connection = api.acquire().await.expect("an API connection");
        TenantSeed::new(&tenant)
            .name("Key cache")
            .insert(&mut *connection)
            .await
            .expect("the tenant seeds");
        WorkspaceSeed::new(&workspace, &tenant)
            .insert(&mut *connection)
            .await
            .expect("the workspace seeds");
        ModelSeed::new(&mint_id(), PROVIDER, MODEL, CAP)
            .verified_at(NOW)
            .insert(&mut *connection)
            .await
            .expect("the catalogue row seeds");
        drop(connection);
        let kek = Arc::new(Kek::from_bytes([0x42; 32]));
        Self {
            providers: Providers::new(api.clone(), Arc::clone(&kek), Entropy::new()),
            vault: afd_vault::Vault::new(api.clone(), kek, Entropy::new()),
            database,
            owner,
            api,
            tenant,
            workspace,
        }
    }

    /// Writes the provider's key: a create the first time, a replace after.
    async fn key(&self, api_key: &str, replace: bool) {
        let name = SecretName::parse(PROVIDER).expect("the provider names a secret");
        let raw = RawValue::from_string(format!(
            r#"{{"provider":"{PROVIDER}","api_key":"{api_key}"}}"#
        ))
        .expect("the fixture body is JSON");
        let body = SecretBody::parse(&raw).expect("the fixture body is a non-empty object");
        if replace {
            self.vault
                .replace(&self.workspace, &name, &body)
                .await
                .expect("the key is replaced under the same KEK");
        } else {
            self.vault
                .create(&self.workspace, &name, &body, NOW)
                .await
                .expect("the key is stored");
        }
    }

    async fn activate_default(&self) {
        let mut connection = self.api.acquire().await.expect("an API connection");
        DefaultSeed {
            provider: PROVIDER,
            source_workspace_id: &self.workspace,
            model: MODEL,
            context_cap_tokens: CAP,
            created_at: NOW,
        }
        .claim(&mut *connection)
        .await
        .expect("the platform default seeds");
    }

    /// The versions a claim would read now.
    async fn observed(&self) -> PlatformKeyKey {
        let (catalogue, secret, version): (i64, Uuid7, i64) = sqlx::query_as(OBSERVE)
            .fetch_one(&mut *self.api.acquire().await.expect("an API connection"))
            .await
            .expect("an active default is observed");
        PlatformKeyKey::observed(catalogue, secret, version)
    }

    /// A lease's resolution of `reference`, as the claim path asks for it.
    async fn lease(&self, reference: &str) -> String {
        let observed = self.observed().await;
        self.providers
            .resolve_observed(&self.tenant, reference, Some(&observed))
            .await
            .expect("the reference resolves")
            .api_key()
            .expose()
            .to_owned()
    }

    async fn cleanup(self) {
        self.api.close().await;
        self.owner.close().await;
        self.database.cleanup().await;
    }
}

/// Calls of the statement that opens a vault row.
fn secret_reads(ran: &Ran) -> u64 {
    let read = afd_credential::vault::sql::SELECT_SECRET.trim();
    ran.statements()
        .iter()
        .filter(|(query, _)| query.trim() == read)
        .map(|(_, calls)| calls)
        .sum()
}

// Value: protects=a same-KEK key replace reaches the next lease; fails_when=the
// cache keys on the secret's id or the catalogue alone; why_new=a replace keeps
// the id and kek_version; seam=none; no_claim=a rotation during a lease.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_key_cache_misses_on_same_kek_rotation() {
    let lane = Lane::create().await;
    lane.key("sk-key-a", false).await;
    lane.activate_default().await;

    let window = Window::open(&lane.owner).await;
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-a",
        "the held version answers"
    );
    lane.key("sk-key-b", true).await;
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-b",
        "the moved version misses"
    );
    let ran = window.close().await;

    assert_eq!(
        secret_reads(&ran),
        2,
        "read for lease one and after the rotation: {:?}",
        ran.statements()
    );
    lane.cleanup().await;
}

// Value: protects=tenant references stay uncached; fails_when=a tenant entry is
// served from the platform cache; why_new=the cache sits on the resolution
// path both kinds share; seam=none; no_claim=entry versioning (blocker 2).
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn tenant_model_reference_reads_every_lease() {
    let lane = Lane::create().await;
    lane.key("sk-tenant", false).await;
    lane.activate_default().await;
    let Added::Stored(entry) = lane
        .providers
        .add_entry(&lane.tenant, MODEL, PROVIDER, NOW)
        .await
        .expect("an entry over a stored credential is an outcome")
    else {
        panic!("the credential is stored, so the entry stores");
    };
    let reference = entry.id.as_str().to_owned();

    let window = Window::open(&lane.owner).await;
    for _ in 0..2 {
        assert_eq!(lane.lease(&reference).await, "sk-tenant");
    }
    let ran = window.close().await;

    let entry_reads: u64 = ran
        .statements()
        .iter()
        .filter(|(query, _)| {
            query.contains("FROM model.tenant_models e") && query.contains("resolution_version")
        })
        .map(|(_, calls)| calls)
        .sum();
    assert_eq!(entry_reads, 2, "{:?}", ran.statements());
    assert_eq!(secret_reads(&ran), 2, "{:?}", ran.statements());
    lane.cleanup().await;
}
