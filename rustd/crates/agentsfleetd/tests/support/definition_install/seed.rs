//! The rows and sealed credentials a fleet-definition scenario stands on: the tenant and
//! workspace, the GitHub installation's routing row, and vault secrets.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::test_util::{InstallSeed, TenantSeed, WorkspaceSeed};
use afd_vault::{SecretBody, SecretName, Vault};
use jiff::Timestamp;

use crate::e2e::GOOD_KEK;

/// The GitHub connection handle a workspace holds: the App installation the
/// broker mints for. Its ids are never dialled; [`MintedToken`] answers.
pub(super) fn github_handle(installation: &str) -> String {
    format!(r#"{{"integration":"github","app_id":"7","installation_id":"{installation}"}}"#)
}

/// A fresh installation id per scenario.
///
/// The mint spends an installation only when the workspace's routing row says
/// it connected it, and `(provider, external_account_id)` is unique across the
/// table, so scenarios running at once cannot share one id.
pub(super) fn fresh_installation() -> String {
    let mut bytes = [0u8; 4];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    u32::from_be_bytes(bytes).to_string()
}

/// The tenant and workspace rows the installed fleet belongs to.
pub(super) async fn seed_workspace(
    booted: &agentsfleetd::serve::Booted,
    workspace: &str,
    tenant: &str,
    now: Timestamp,
) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let seeded: bool = sqlx::query_scalar(crate::e2e_seed::WORKSPACE_EXISTS)
        .bind(workspace)
        .fetch_one(&mut *connection)
        .await
        .expect("the workspace probe must run");
    if seeded {
        return;
    }
    TenantSeed::new(tenant)
        .name("definition-tenant")
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("the tenant row must insert");
    WorkspaceSeed::new(workspace, tenant)
        .name("definition-workspace")
        .at(now)
        .insert(&mut *connection)
        .await
        .expect("the workspace row must insert");
}

/// The routing row the GitHub connect flow lands beside its handle.
pub(super) async fn seed_install(
    booted: &agentsfleetd::serve::Booted,
    workspace: &str,
    installation: &str,
    now: Timestamp,
) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    InstallSeed::new(
        afd_connector::Provider::GitHub.id(),
        installation,
        workspace,
    )
    .at(now)
    .insert(&mut *connection)
    .await
    .expect("the routing row must insert");
}

/// Seals `body` into the workspace's vault under `name`, as the tenant plane's
/// secret write does.
pub(crate) async fn seal(
    booted: &agentsfleetd::serve::Booted,
    workspace: &Uuid7,
    name: &str,
    body: &str,
    now: Timestamp,
) {
    let kek = Arc::new(Kek::from_hex(GOOD_KEK).expect("the lane key is well formed"));
    let raw = serde_json::value::RawValue::from_string(body.to_owned()).expect("a JSON body");
    Vault::new(booted.database.clone(), kek, Entropy::new())
        .create(
            workspace,
            &SecretName::parse(name).expect("a valid secret name"),
            &SecretBody::parse(&raw).expect("a secret body is a JSON object"),
            now,
        )
        .await
        .expect("the vault takes the secret");
}
