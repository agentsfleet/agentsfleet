//! One connect, start to landed grant, over a loopback vendor.
//!
//! # Why the whole round trip and not each step
//!
//! Every step here is proven alone somewhere: `state`'s suite signs and
//! verifies, `connect_verify` checks the starter, `vendor_contract` asks the
//! real endpoints what they answer, and `integration_platform_app` reads the
//! app bag. What none of them reaches is the HANDOFF — a nonce minted by
//! `start` and consumed by `spend`, a code redeemed at the endpoint
//! `token_endpoint` chose, and an answer parsed into the handle the vault then
//! seals under the provider's own key. Each of those is one function passing a
//! value to the next, and a value passed wrongly type-checks.
//!
//! Jira is the provider worth driving, because its completion is the only one
//! that makes a SECOND vendor call: the token answer alone does not say which
//! site the grant is scoped to, so `read_jira` resolves it and folds the cloud
//! id into the handle. A connect that skipped that lands a grant naming no
//! site, which fails later at the first API call rather than here.
//!
//! # The loopback vendor, and why pinning is what makes this safe
//!
//! `Exchange::pointed_at` moves every host one connect dials, so a single
//! listener answers both the token POST and the site listing, routed by path.
//! Nothing in this file can reach Atlassian: the pin is the whole reason a
//! round-trip test can redeem a code at all.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints this without
//! datastores; `make test-integration-rustd` is the only lane that runs it.
//!
//! Split at the file-length cap three ways, along the seams the round trip
//! already has: this half stands the deployment up, [`vendor`] answers as
//! Atlassian, and [`cases`] drives the verbs.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "integration_connect_roundtrip/cases.rs"]
mod cases;
#[path = "integration_connect_roundtrip/vendor.rs"]
mod vendor;

use std::sync::Arc;

use afd_connector::app::PlatformApp;
use afd_connector::connect::{Connectors, Started, Starting};
use afd_connector::exchange::Exchange;
use afd_connector::grant::Grants;
use afd_connector::provider::Provider;
use afd_connector::{Finishing, Landed};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};
use afd_vault::{SecretName, Vault};

use self::vendor::FakeAtlassian;

/// The key every fixture in this lane seals and opens under.
const FIXTURE_KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The instant the round trip is driven at.
const NOW_MS: i64 = 1_700_000_000_000;

/// The Jira app an operator registered for this deployment.
const JIRA_APP_BAG: &str =
    r#"{"client_id":"jira-roundtrip-client","client_secret":"jira-roundtrip-secret"}"#;

/// Who pressed Connect, as the identity provider names them.
const SUBJECT: &str = "user_roundtrip";

/// Where the provider sends the browser back.
const REDIRECT_URI: &str = "https://fixture.invalid/connect/jira/callback";

/// The authorization code the callback carried.
const CODE: &str = "a-fixture-authorization-code";

/// What this deployment signs install states with.
pub(crate) fn signing_secret() -> SecretBytes {
    SecretBytes::new(b"a-fixture-state-signing-secret".to_vec())
}

/// One live deployment: its admin workspace, the workspace being connected,
/// and the stores a connect acts through.
pub(crate) struct Round {
    pub(crate) connectors: Connectors,
    pub(crate) admin: Uuid7,
    pub(crate) workspace: Uuid7,
    vault: Vault,
    database: Db,
    _lane: TestDatabase,
}

impl Round {
    /// Seeds both workspaces and binds the connectors to `vendor`.
    pub(crate) async fn create(vendor: &FakeAtlassian) -> Self {
        afd_db::test_util::install_subscriber();
        let lane = TestDatabase::shared();
        let database = lane.open(DbRole::Api, &[]).await;
        let tenant = mint_id();
        let admin = mint_id();
        let workspace = mint_id();
        seed_rows(&database, &tenant, &[&admin, &workspace]).await;

        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        let vault = Vault::new(database.clone(), Arc::clone(&kek), Entropy::new());
        let connectors = Connectors::new(
            PlatformApp::new(vault.clone()),
            Grants::new(vault.clone(), database.clone(), Entropy::new()),
            Exchange::new(reqwest::Client::new()).pointed_at(vendor.base().to_owned()),
            reqwest::Client::new(),
            queue().await,
            Entropy::new(),
        );

        Self {
            connectors,
            admin: Uuid7::parse(&admin).expect("a minted id is a v7 spelling"),
            workspace: Uuid7::parse(&workspace).expect("a minted id is a v7 spelling"),
            vault,
            database,
            _lane: lane,
        }
    }

    /// Seals the Jira app bag this deployment connects with.
    pub(crate) async fn configure_jira(&self) {
        let name = Provider::Jira.app_key();
        let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed");
        let sealed = Sealer::new()
            .seal(
                &kek,
                &Aad::new(self.admin.as_str(), &name),
                JIRA_APP_BAG.as_bytes(),
            )
            .expect("the fixture bag seals");
        insert_secret(&self.database, self.admin.as_str(), &name, &sealed).await;
    }

    /// The grant this connect landed, opened back out of the vault.
    pub(crate) async fn landed_grant(&self) -> serde_json::Value {
        let name = SecretName::parse(Provider::Jira.grant_key()).expect("the grant key parses");
        let stored = self
            .vault
            .load(&self.workspace, &name)
            .await
            .expect("the vault read runs")
            .expect("a completed connect sealed a grant");
        serde_json::from_slice(stored.expose()).expect("the handle is a JSON object")
    }

    /// Drops this fixture's rows; the workspace cascade takes the secrets.
    pub(crate) async fn cleanup(self) {
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        for workspace in [self.admin.as_str(), self.workspace.as_str()] {
            sqlx::query("DELETE FROM core.workspaces WHERE id = $1::uuid")
                .bind(workspace)
                .execute(&mut *connection)
                .await
                .expect("the fixture workspace must delete");
        }
    }
}

/// The lane's Dragonfly, where a round trip's single-use nonce lives.
async fn queue() -> Dragonfly {
    let url = std::env::var("TEST_DRAGONFLY_URL")
        .expect("TEST_DRAGONFLY_URL is unset — run this through `make test-integration-rustd`");
    let config = DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(std::env::var("TEST_DRAGONFLY_CA_CERT").ok().map(Into::into));
    afd_dragonfly::test_util::connect_live(&config)
        .await
        .expect("the lane's Dragonfly must be reachable")
}

/// One tenant and its workspaces, which the vault's foreign key requires.
async fn seed_rows(database: &Db, tenant: &str, workspaces: &[&str]) {
    let mut connection = database.acquire().await.expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at) \
         VALUES ($1::uuid, 'a connect fixture', $2, $2)",
    )
    .bind(tenant)
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the tenant row must insert");
    for workspace in workspaces {
        sqlx::query(
            "INSERT INTO core.workspaces (id, tenant_id, created_at) \
             VALUES ($1::uuid, $2::uuid, $3)",
        )
        .bind(workspace)
        .bind(tenant)
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("the workspace row must insert");
    }
}

/// Writes one sealed row the way an operator's save writes it.
async fn insert_secret(
    database: &Db,
    workspace: &str,
    name: &str,
    sealed: &afd_crypto::envelope::Envelope,
) {
    let mut connection = database.acquire().await.expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets \
           (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag, \
            nonce, ciphertext, tag, kek_version, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
    )
    .bind(mint_id())
    .bind(workspace)
    .bind(name)
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(sealed.kek_version())
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the sealed row must insert");
}

/// The `state` parameter the consent URL carries.
pub(crate) fn state_of(consent: &str) -> String {
    url::Url::parse(consent)
        .expect("the consent URL parses")
        .query_pairs()
        .find(|(key, _value)| key == "state")
        .map(|(_key, value)| value.into_owned())
        .expect("a consent URL carries its state")
}
