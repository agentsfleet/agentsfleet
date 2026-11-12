//! The platform key cache answers only for the witnesses a claim read, ends
//! with the price window it matched and with its time limit, and a tenant's
//! own model reference is read every time.
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
use std::time::Duration;

use af_core::clock::FixedClock;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_credential::provider::cache::Observed;
use afd_credential::provider::{Added, KeyRef, PLATFORM_MODEL_REF, Providers};
use afd_credential::secrets::rotate_refresh_token;
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
/// The base URL the seeded model's saved settings carry: the one endpoint a
/// credential of this provider may be pinned to.
const SAVED_BASE_URL: &str = "https://api.anthropic.com/v1";
/// The refresh token the key's body holds before a re-seal rotates it.
const REFRESH_OLD: &str = "refresh-old";
const REFRESH_NEW: &str = "refresh-new";

struct Lane {
    database: TestDatabase,
    owner: Db,
    api: Db,
    providers: Providers,
    vault: afd_vault::Vault,
    /// The clock the cache tells time by, which a test moves.
    clock: FixedClock,
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
        let clock = FixedClock::at(NOW);
        Self {
            providers: Providers::new(api.clone(), Arc::clone(&kek), Entropy::new())
                .with_platform_clock(Arc::new(clock.clone())),
            vault: afd_vault::Vault::new(api.clone(), Arc::clone(&kek), Entropy::new()),
            database,
            owner,
            api,
            clock,
            tenant,
            workspace,
        }
    }

    /// Writes the provider's key: a create the first time, a replace after.
    async fn key(&self, api_key: &str, replace: bool) {
        let name = SecretName::parse(PROVIDER).expect("the provider names a secret");
        let raw = RawValue::from_string(format!(
            r#"{{"provider":"{PROVIDER}","api_key":"{api_key}","refresh_token":"{REFRESH_OLD}"}}"#
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

    /// What the platform read observes now: the witnesses a claim reads beside
    /// the slot are the same expression (proved against the real claim in the
    /// fleet crate's suite).
    async fn observed(&self) -> Observed {
        self.providers
            .observe_platform(&self.tenant)
            .await
            .expect("the platform read answers")
            .expect("an active default is observed")
    }

    /// A write that changes neither the sealed bytes nor the endpoint.
    async fn touch(&self) {
        sqlx::query(
            "UPDATE vault.secrets SET updated_at = now() WHERE workspace_id = $1 AND key_name = $2",
        )
        .bind(&self.workspace)
        .bind(PROVIDER)
        .execute(&mut *self.api.acquire().await.expect("an API connection"))
        .await
        .expect("the stored credential is touched");
    }

    /// Pins the stored credential to `url`, as a metadata-only write would.
    async fn pin_endpoint(&self, url: &str) {
        sqlx::query(
            "UPDATE vault.secrets SET meta_base_url = $3 WHERE workspace_id = $1 AND key_name = $2",
        )
        .bind(&self.workspace)
        .bind(PROVIDER)
        .bind(url)
        .execute(&mut *self.api.acquire().await.expect("an API connection"))
        .await
        .expect("the stored credential is pinned");
    }

    /// Re-seals the stored envelope under a rotated refresh token.
    async fn reseal(&self) {
        rotate_refresh_token(
            &self.vault,
            &self.workspace,
            PROVIDER,
            REFRESH_OLD,
            REFRESH_NEW,
        )
        .await
        .expect("the matching handle rotates");
    }

    /// Closes the default's price window `span` from the database's clock.
    async fn close_price_window_in(&self, span: Duration) {
        sqlx::query(
            "UPDATE model.platform_model_prices SET effective_until = clock_timestamp() + $3 \
              WHERE platform_model_id = (SELECT id FROM model.platform_models \
                                          WHERE provider = $1 AND model_name = $2)",
        )
        .bind(PROVIDER)
        .bind(MODEL)
        .bind(span)
        .execute(&mut *self.owner.acquire().await.expect("an owner connection"))
        .await
        .expect("the price window closes");
    }

    /// A lease's resolution of `reference`, as the claim path asks for it.
    async fn lease(&self, reference: &str) -> String {
        let observed = self.observed().await;
        self.providers
            .resolve_observed(&self.tenant, reference, Some(&observed.key))
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
    // The vault's one read of a row's envelope: it names the sealed key and
    // the ciphertext from `vault.secrets` by workspace and name, and no other
    // statement does both without taking a lock or writing.
    ran.statements()
        .iter()
        .filter(|(query, _)| {
            query.contains("secret_ciphertext, secret_tag, kek_version")
                && query.contains("FROM vault.secrets")
                && !query.contains("FOR UPDATE")
                && !query.contains("INSERT")
                && !query.contains("UPDATE vault.secrets")
        })
        .map(|(_, calls)| calls)
        .sum()
}

#[path = "platform_key_cache/cases.rs"]
mod cases;

#[path = "integration_platform_key_cache/window.rs"]
mod window;
