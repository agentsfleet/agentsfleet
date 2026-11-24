//! What a person approves for a fleet, and the network they open it to,
//! written as the approval and the fleet patch leave them.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_api_wire::grant::{kind, status};
use afd_db::test_util::GrantSeed;
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::Value;

use crate::support::Fixtures;

/// The service a GitHub grant is stored under.
pub(crate) const GITHUB: &str = "github";

impl Fixtures {
    /// Approves `fleet`'s GitHub grant at `scope`, the JSON an access request
    /// carries.
    pub(crate) async fn approve_github(&self, fleet: &Uuid7, scope: &Value) {
        let scope = scope.to_string();
        self.approve(fleet, GITHUB, kind::INTEGRATION, Some(&scope))
            .await;
    }

    /// Approves `fleet`'s grant of the workspace secret `name`.
    pub(crate) async fn approve_secret(&self, fleet: &Uuid7, name: &str) {
        self.approve(fleet, name, kind::SECRET, None).await;
    }

    async fn approve(&self, fleet: &Uuid7, service: &str, of: &str, scope: Option<&str>) {
        let mut connection = self.database.acquire().await.expect("grant connection");
        let mut seed =
            GrantSeed::new(fleet, service, of, status::APPROVED).approved(af_core::clock::now());
        if let Some(scope) = scope {
            seed = seed.scope(scope);
        }
        seed.insert(&mut *connection)
            .await
            .expect("the approved grant inserts");
    }

    /// Opens or closes `fleet`'s network, as a person's patch does.
    pub(crate) async fn set_network(
        &self,
        fleet: &Uuid7,
        http_egress: &str,
        package_domains: bool,
    ) {
        let mut connection = self.database.acquire().await.expect("network connection");
        sqlx::query(
            "UPDATE fleet.fleets SET http_egress = $2, package_domains = $3, \
             resource_version = resource_version + 1, updated_at = now() WHERE id = $1",
        )
        .bind(fleet)
        .bind(http_egress)
        .bind(package_domains)
        .execute(&mut *connection)
        .await
        .expect("the fleet's network updates");
    }
}

impl Fixtures {
    /// Moves `fleet` onto `model_ref`, as a person's model patch does.
    pub(crate) async fn set_model(&self, fleet: &Uuid7, model_ref: &str) {
        let mut connection = self.database.acquire().await.expect("model connection");
        sqlx::query(
            "UPDATE fleet.fleets SET model_ref = $2, resource_version = resource_version + 1, \
             updated_at = now() WHERE id = $1",
        )
        .bind(fleet)
        .bind(model_ref)
        .execute(&mut *connection)
        .await
        .expect("the fleet's model updates");
    }
}

impl Fixtures {
    /// Seals `body` under `name` in `workspace`'s vault, as the run plane
    /// reads it.
    pub(crate) async fn store_secret(&self, workspace: &Uuid7, name: &str, body: &Value) {
        let raw = serde_json::value::RawValue::from_string(body.to_string()).expect("a JSON body");
        Vault::new(
            self.database.clone(),
            Arc::new(Kek::from_hex(crate::report_seed::FIXTURE_KEK_HEX).expect("the lane key")),
            Entropy::new(),
        )
        .create(
            workspace,
            &SecretName::parse(name).expect("a vault name"),
            &SecretBody::parse(&raw).expect("a secret object"),
            self.enrolled_at(),
        )
        .await
        .expect("the secret is stored");
    }
}
