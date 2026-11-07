//! The activation ladder against real rows: what it refuses, and what it holds
//! under a concurrent delete. The ceiling an applied activation reports is
//! graded in `activation_ceiling.rs`, over the fixtures defined here.
//!
//! Every refusal below is an [`Activation`] VALUE rather than an error, so the
//! assertions read as a truth table over one verb rather than as a matrix of
//! error kinds.
//!
//! # What these do NOT cover
//!
//! Every case here drives ONE connection, so none of them exercises the lock
//! treaty. The vault-row serialization that orders an activation against a
//! concurrent credential delete needs two connections held open against each
//! other, and proving it is a separate piece of work — the last test below
//! covers only the sequential half and is named for it.

use std::sync::Arc;

use afd_credential::provider::{Activation, OPENAI_COMPATIBLE, Providers, Rejection};
use afd_crypto::entropy::Entropy;

use super::{Fixture, NOW};

/// A model nothing catalogues — and, for the refusals that fire before the
/// catalogue is consulted, a model whose name can never matter.
const UNCATALOGUED: &str = "gpt-imaginary";

/// The provider the catalogued row is published under.
pub(super) const NAMED_PROVIDER: &str = "openai";

/// The store under test, over the fixture's pool and key.
pub(super) fn providers(fixture: &Fixture) -> Providers {
    Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

/// A model id this test alone catalogues.
///
/// `model.platform_models` is keyed `(provider, model_name)` with no tenant
/// column, and the whole lane shares ONE live database — so a model shared across
/// tests is a ROW shared across tests, and [`catalogue`]'s `DO NOTHING` makes
/// whichever test seeds first the author of every other test's ceiling. The
/// first full run of this lane proved it: the `-1` seed below landed first and
/// two sibling tests read their ceiling as the clamp's 0.
pub(super) fn unique_model() -> String {
    format!("gpt-fixture-{}", afd_db::test_util::mint_id())
}

/// Publishes one catalogue row, which is what the named arm's gate requires.
pub(super) async fn catalogue(fixture: &Fixture, provider: &str, model: &str, cap: i32) {
    fixture.seed_catalogue(provider, model, cap).await;
}

/// How many rows each side of the activation wrote.
pub(super) async fn written(fixture: &Fixture) -> (i64, i64) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    let selections: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM model.tenant_selections WHERE tenant_id = $1::uuid",
    )
    .bind(fixture.tenant.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the selection count reads");
    let entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM model.tenant_models WHERE tenant_id = $1::uuid")
            .bind(fixture.tenant.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("the entry count reads");
    (selections, entries)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_name_the_vault_does_not_hold_is_refused_without_writing() {
    let fixture = Fixture::create().await;
    let outcome = providers(&fixture)
        .activate(
            &fixture.tenant,
            "nobody-stored-this",
            Some(UNCATALOGUED),
            NOW,
        )
        .await
        .expect("a missing credential is an outcome, not a failure");

    assert_eq!(outcome, Activation::CredentialMissing);
    assert_eq!(written(&fixture).await, (0, 0));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_row_whose_metadata_is_not_a_provider_key_is_refused_before_any_decrypt() {
    let fixture = Fixture::create().await;
    // A perfectly readable credential body — the refusal is decided from the
    // metadata columns alone, so the envelope is never opened.
    fixture
        .seed_with_shape(
            "a-webhook-secret",
            br#"{"provider":"openai","api_key":"sk-live"}"#,
            None,
            None,
        )
        .await;

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "a-webhook-secret", Some(UNCATALOGUED), NOW)
        .await
        .expect("a non-provider credential is an outcome");

    assert_eq!(outcome, Activation::NotAProviderKey);
    assert_eq!(written(&fixture).await, (0, 0));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_uncatalogued_model_writes_neither_the_entry_nor_the_selection() {
    // The property the gate-and-write statement exists for: the registry entry
    // is inserted BEFORE the gated write, so a refusal that left it behind
    // would be an orphaned entry naming a model nothing catalogues.
    let fixture = Fixture::create().await;
    fixture
        .seed_with_shape(
            "tenant-key",
            br#"{"provider":"openai","api_key":"sk-live"}"#,
            Some(NAMED_PROVIDER),
            Some(true),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "tenant-key", Some(UNCATALOGUED), NOW)
        .await
        .expect("an uncatalogued model is an outcome");

    assert_eq!(outcome, Activation::ModelUnknown);
    assert_eq!(
        written(&fixture).await,
        (0, 0),
        "the entry insert must roll back with the refused write"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_activation_after_a_committed_delete_writes_nothing() {
    // NOT a race test, and named so it cannot be mistaken for one: the delete
    // COMMITS before the activation begins, so this proves the sequential
    // half — a credential that is already gone produces no selection row
    // naming it. The concurrent half, where the two overlap and the vault-row
    // lock is what orders them, needs two connections driven against each
    // other and is not covered here.
    let fixture = Fixture::create().await;
    let model = unique_model();
    catalogue(&fixture, NAMED_PROVIDER, &model, 200_000).await;
    fixture
        .seed_with_shape(
            "doomed-key",
            br#"{"provider":"openai","api_key":"sk-live"}"#,
            Some(NAMED_PROVIDER),
            Some(true),
        )
        .await;

    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid AND key_name = $2")
        .bind(fixture.workspace.as_str())
        .bind("doomed-key")
        .execute(&mut *connection)
        .await
        .expect("the credential deletes");
    drop(connection);

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "doomed-key", Some(&model), NOW)
        .await
        .expect("a deleted credential is an outcome");

    assert_eq!(outcome, Activation::CredentialMissing);
    assert_eq!(written(&fixture).await, (0, 0));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_body_that_will_not_read_as_a_credential_is_refused_after_the_metadata_passes() {
    let fixture = Fixture::create().await;
    // The metadata says provider key, so the two cheap rungs pass and the
    // envelope IS opened — which is what makes this a different refusal from
    // the one above rather than a second spelling of it.
    fixture
        .seed_with_shape(
            "a-truncated-credential",
            b"[]",
            Some(NAMED_PROVIDER),
            Some(true),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(
            &fixture.tenant,
            "a-truncated-credential",
            Some(UNCATALOGUED),
            NOW,
        )
        .await
        .expect("an unreadable body is an outcome, not a failure");

    assert_eq!(outcome, Activation::Malformed);
    assert_eq!(written(&fixture).await, (0, 0));
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_compatible_endpoint_in_a_blocked_range_is_refused_by_the_guard() {
    let fixture = Fixture::create().await;
    // The cloud metadata service, which is the whole reason the guard exists:
    // a tenant that can point an endpoint anywhere can point it at the host's
    // own credentials. `https` on purpose, so what refuses this is the address
    // range and not the scheme.
    fixture
        .seed_with_shape(
            "a-blocked-endpoint",
            br#"{"provider":"openai-compatible","base_url":"https://169.254.169.254/v1"}"#,
            Some(OPENAI_COMPATIBLE),
            Some(false),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(
            &fixture.tenant,
            "a-blocked-endpoint",
            Some(UNCATALOGUED),
            NOW,
        )
        .await
        .expect("a refused endpoint is an outcome");

    assert_eq!(outcome, Activation::EndpointRefused(Rejection::BlockedHost));
    assert_eq!(written(&fixture).await, (0, 0));
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_with_no_workspace_is_refused_before_any_credential_is_looked_for() {
    let fixture = Fixture::create().await;
    let orphan = fixture.seed_tenant_without_workspace().await;

    // The credential name is one nothing stored, and it does not matter: the
    // workspace is what a credential is looked up UNDER, so there is nowhere
    // to look and the refusal names that rather than the missing credential.
    let outcome = providers(&fixture)
        .activate(&orphan, "anything-at-all", Some(UNCATALOGUED), NOW)
        .await
        .expect("a bootstrap invariant violation is an outcome");

    assert_eq!(outcome, Activation::NoWorkspace);
    fixture.cleanup().await;
}
