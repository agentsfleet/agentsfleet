//! A GitHub handle mints only the installation its own workspace connected.
//!
//! The repro these close: workspace A stores a handle through the generic
//! secrets route naming workspace B's installation id, and A's runner asks for
//! a token. The id is a short integer, not a secret, and the App's key signs
//! for any installation it is on — so the routing row the connect flow wrote is
//! the only fact A could not have typed.
//!
//! The fixture deployment holds no platform App, so a mint that clears every
//! check reaches the broker and is answered `CRED_BROKER_NOT_CONFIGURED`. That
//! is the positive here: it proves the request got PAST the ownership read,
//! where the negatives prove it stopped AT it.

use super::*;
use crate::requests;

/// A fresh installation id, so cases running at once never contend for one.
///
/// `(provider, external_account_id)` is unique across the table, and every
/// case in this file routes an installation: a shared constant made whichever
/// case inserted second fail at the seed rather than at what it asserts.
fn installation() -> String {
    let mut bytes = [0u8; 4];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    u32::from_be_bytes(bytes).to_string()
}

/// A GitHub handle naming `installation`, as the connect flow writes one.
fn github_handle(installation: &str) -> String {
    format!(r#"{{"integration":"github","installation_id":"{installation}"}}"#)
}

/// Writes the routing row the GitHub connect flow lands beside its handle.
async fn seed_install(fixtures: &Fixtures, workspace: &str, installation: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.connector_installs
           (id, provider, external_account_id, workspace_id, installed_by, scopes,
            created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $4::uuid, '', '{}', $5, $5)",
    )
    .bind(new_id())
    .bind(CONNECTOR_GITHUB)
    .bind(installation)
    .bind(workspace)
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the routing row must insert");
}

/// A granted fleet whose workspace holds `handle` under `name`.
async fn granted_with(fixtures: &Fixtures, name: &str, handle: &str) -> Bound {
    let owner = bound(fixtures, "{}").await;
    seed_handle(fixtures, &owner.workspace, name, handle).await;
    seed_grant(fixtures, &owner.fleet, CONNECTOR_GITHUB, "approved").await;
    owner
}

/// The code a GitHub mint for `who` is refused with.
async fn refusal(
    fixtures: &Fixtures,
    who: &Bound,
    integration: &str,
) -> afd_core::error_code::ErrorCode {
    fixtures
        .plane()
        .mint(
            &who.runner,
            &requests::mint(&who.lease, integration),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect_err("no fixture deployment can complete a GitHub exchange")
        .code()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_handle_naming_another_workspaces_installation_mints_nothing() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let installation = installation();
    let victim = bound(&fixtures, "{}").await;
    seed_install(&fixtures, &victim.workspace, &installation).await;
    let forger = granted_with(&fixtures, CONNECTOR_GITHUB, &github_handle(&installation)).await;

    assert_eq!(
        refusal(&fixtures, &forger, CONNECTOR_GITHUB).await,
        error_code::GH_RECONNECT_REQUIRED,
        "a forged handle reached the exchange for an installation its workspace never connected"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_handle_with_no_routing_row_mints_nothing() {
    // The same refusal as the forged case, on purpose: told apart, the answer
    // would say whether some other workspace holds the installation.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let unrouted = granted_with(&fixtures, CONNECTOR_GITHUB, &github_handle(&installation())).await;

    assert_eq!(
        refusal(&fixtures, &unrouted, CONNECTOR_GITHUB).await,
        error_code::GH_RECONNECT_REQUIRED
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_the_workspaces_own_installation_reaches_the_exchange() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let installation = installation();
    let owner = granted_with(&fixtures, CONNECTOR_GITHUB, &github_handle(&installation)).await;
    seed_install(&fixtures, &owner.workspace, &installation).await;

    assert_eq!(
        refusal(&fixtures, &owner, CONNECTOR_GITHUB).await,
        error_code::CRED_BROKER_NOT_CONFIGURED,
        "the owner's own installation was stopped before the broker"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_github_handle_filed_under_another_name_is_not_connected() {
    // Asked for by a name no connector carries, the grant gate has nothing to
    // check; the handle inside still says GitHub. Even with its workspace
    // holding the installation, it is not minted as a connector nobody granted.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let renamed = "github-copy";
    let installation = installation();
    let owner = granted_with(&fixtures, renamed, &github_handle(&installation)).await;
    seed_install(&fixtures, &owner.workspace, &installation).await;

    assert_eq!(
        refusal(&fixtures, &owner, renamed).await,
        error_code::CRED_INTEGRATION_NOT_CONNECTED
    );

    fixtures.cleanup().await;
}
