//! `PlatformApp` against a live vault — what this deployment has configured.
//!
//! # Why this crate's first live test is here
//!
//! Everything else in `afd_connector` decides without a datastore, which is a
//! property of those functions and is tested as one. `PlatformApp` is the
//! exception: every answer it gives is a vault read, and the module's whole
//! claim — that a missing bag, an unreadable body and an absent field are ONE
//! answer rather than three errors — is a claim about rows. A fake vault would
//! prove the match arms and not the claim.
//!
//! # The failure this exists to catch
//!
//! `Ok(None)` is what one layer up renders as "this deployment has not
//! configured this connector". The dangerous direction is the other one: a
//! half-configured bag answering `Some` would start a connect with an empty
//! client secret and fail at the vendor, where the operator reads a 401 rather
//! than the truth. So the bag missing its OAuth pair is a case of its own.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without
//! datastores; `make test-integration-rustd` is the only lane that runs them.
//!
//! Split at the file-length cap along the seam the sibling suites use: this
//! half puts the rows in the database, [`cases`] drives the reader over them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "integration_platform_app/cases.rs"]
mod cases;

use std::sync::Arc;

use afd_connector::app::PlatformApp;
use afd_connector::provider::Provider;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_vault::Vault;

/// The key every fixture in this lane seals and opens under.
///
/// The same value the sibling crates' fixtures use. A row written under any
/// other key refuses at the open and never reaches the arm under test.
const FIXTURE_KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The instant every fixture row is stamped with.
const SEEDED_AT: i64 = 1_700_000_000_000;

/// A Slack app bag carrying all three fields an operator registers at once.
const SLACK_BAG: &str = r#"{"client_id":"slack-client-1","client_secret":"slack-secret-1","signing_secret":"slack-signing-1"}"#;

/// A Jira bag carrying the OAuth pair and, correctly, no signing secret.
///
/// A second provider with DIFFERENT values, which is what makes the read a
/// read: one bag cannot tell a vault lookup from a hardcoded answer.
const JIRA_BAG: &str = r#"{"client_id":"jira-client-2","client_secret":"jira-secret-2"}"#;

/// A bag an operator saved half-finished: connected, and unusable.
const SIGNING_ONLY_BAG: &str = r#"{"signing_secret":"linear-signing-3"}"#;

/// One live deployment holding its own admin workspace.
pub(crate) struct Deployment {
    /// The vault the app bags are sealed in.
    vault: Vault,
    /// The workspace those bags belong to.
    pub(crate) admin: Uuid7,
    /// The pool, kept for the seeding statements.
    database: Db,
    /// The lane, kept so the fixture can hand it back.
    lane: TestDatabase,
}

impl Deployment {
    /// Opens the shared lane and seeds the tenant and workspace rows the
    /// vault's foreign key requires.
    pub(crate) async fn create() -> Self {
        afd_db::test_util::install_subscriber();
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let tenant = mint_id();
        let admin = mint_id();

        let mut connection = database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO core.tenants (id, name, created_at, updated_at) \
             VALUES ($1::uuid, 'a connector fixture', $2, $2)",
        )
        .bind(&tenant)
        .bind(SEEDED_AT)
        .execute(&mut *connection)
        .await
        .expect("the tenant row must insert");
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, created_at) \
             VALUES ($1::uuid, $2::uuid, $3)",
        )
        .bind(&admin)
        .bind(&tenant)
        .bind(SEEDED_AT)
        .execute(&mut *connection)
        .await
        .expect("the admin workspace row must insert");
        drop(connection);

        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        Self {
            vault: Vault::new(database.clone(), kek, Entropy::new()),
            admin: Uuid7::parse(&admin).expect("a minted workspace id is a v7 spelling"),
            database,
            lane,
        }
    }

    /// The reader under test, over this deployment's vault.
    pub(crate) fn apps(&self) -> PlatformApp {
        PlatformApp::new(self.vault.clone())
    }

    /// Seals `bag` under `provider`'s app key, the way an operator's save does.
    pub(crate) async fn configure(&self, provider: Provider, bag: &str) {
        let name = provider.app_key();
        let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed");
        let sealed = Sealer::new()
            .seal(&kek, &Aad::new(self.admin.as_str(), &name), bag.as_bytes())
            .expect("the fixture bag seals");

        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query(
            "INSERT INTO vault.secrets \
               (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, \
                nonce, ciphertext, tag, kek_version, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
        )
        .bind(mint_id())
        .bind(self.admin.as_str())
        .bind(&name)
        .bind(sealed.wrapped_dek())
        .bind(sealed.dek_nonce().as_slice())
        .bind(sealed.dek_tag().as_slice())
        .bind(sealed.payload_nonce().as_slice())
        .bind(sealed.payload_ciphertext())
        .bind(sealed.payload_tag().as_slice())
        .bind(sealed.kek_version())
        .bind(SEEDED_AT)
        .execute(&mut *connection)
        .await
        .expect("the app bag must insert");
    }

    /// Drops this fixture's rows; the workspace cascade takes the bags with it.
    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        sqlx::query("DELETE FROM core.workspaces WHERE id = $1::uuid")
            .bind(self.admin.as_str())
            .execute(&mut *connection)
            .await
            .expect("the fixture workspace must delete");
        drop(connection);
        drop(self.lane);
    }
}
