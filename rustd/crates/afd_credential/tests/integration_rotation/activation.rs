//! The activation ladder against real rows: what it refuses, and what it holds
//! under a concurrent delete.
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

use afd_core::clock::UnixMillis;
use afd_credential::provider::{Activation, Providers};
use afd_crypto::entropy::Entropy;

use super::Fixture;

/// The instant every activation here is stamped with.
const NOW: UnixMillis = UnixMillis::from_millis(1_760_000_000_000);

/// A model nothing catalogues — and, for the refusals that fire before the
/// catalogue is consulted, a model whose name can never matter.
const UNCATALOGUED: &str = "gpt-imaginary";

/// The provider the catalogued row is published under.
const NAMED_PROVIDER: &str = "openai";

/// The store under test, over the fixture's pool and key.
fn providers(fixture: &Fixture) -> Providers {
    Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

/// A model id this test alone catalogues.
///
/// `core.model_library` is keyed `(provider, model_id)` with no tenant column,
/// and the whole lane shares ONE live database — so a model id shared across
/// tests is a ROW shared across tests, and [`catalogue`]'s `DO NOTHING` makes
/// whichever test seeds first the author of every other test's ceiling. The
/// first full run of this lane proved it: the `-1` seed below landed first and
/// two sibling tests read their ceiling as the clamp's 0.
fn unique_model() -> String {
    format!("gpt-fixture-{}", afd_db::test_util::mint_id())
}

/// Publishes one catalogue row, which is what the named arm's gate requires.
async fn catalogue(fixture: &Fixture, provider: &str, model: &str, cap: i32) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.model_library \
           (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
            cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
         VALUES ($1::uuid, $2, $3, $4, 0, 0, 0, 1, 1) \
         ON CONFLICT (provider, model_id) DO NOTHING",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(model)
    .bind(provider)
    .bind(cap)
    .execute(&mut *connection)
    .await
    .expect("the catalogue row seeds");
}

/// How many rows each side of the activation wrote.
async fn written(fixture: &Fixture) -> (i64, i64) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    let selections: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.tenant_model_selection WHERE tenant_id = $1::uuid",
    )
    .bind(fixture.tenant.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("the selection count reads");
    let entries: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.tenant_model_entries WHERE tenant_id = $1::uuid",
    )
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
        .activate(&fixture.tenant, "nobody-stored-this", Some(UNCATALOGUED), NOW)
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
async fn a_catalogued_activation_stores_the_catalogues_ceiling() {
    let fixture = Fixture::create().await;
    let model = unique_model();
    catalogue(&fixture, NAMED_PROVIDER, &model, 200_000).await;
    fixture
        .seed_with_shape(
            "tenant-key",
            br#"{"provider":"openai","api_key":"sk-live"}"#,
            Some(NAMED_PROVIDER),
            Some(true),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "tenant-key", Some(&model), NOW)
        .await
        .expect("a catalogued activation applies");

    let Activation::Applied(stored) = outcome else {
        panic!("a catalogued model activates: {outcome:?}");
    };
    // The ceiling comes from the CATALOGUE, never from the request — a client
    // cannot widen its own context window by asking.
    assert_eq!(stored.context_cap_tokens, 200_000);
    // The provider comes from the decrypted credential, never from the caller.
    assert_eq!(&*stored.provider, NAMED_PROVIDER);
    assert_eq!(stored.secret_ref.as_deref(), Some("tenant-key"));
    assert_eq!(written(&fixture).await, (1, 1));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_negative_catalogue_ceiling_is_clamped_the_way_the_zig_clamps_it() {
    // core.model_library.context_cap_tokens is INTEGER NOT NULL with no
    // nonnegative CHECK — bounds live in the application (RULE STS) — so a
    // negative ceiling is a row the schema permits. model_rate_cache.zig
    // clamps it with @max(cap, 0) at every read; without the same clamp in the
    // activation statement this daemon would STORE -1 where the Zig stores 0,
    // and the two implementations' rows would differ over one database.
    let fixture = Fixture::create().await;
    let model = unique_model();
    catalogue(&fixture, NAMED_PROVIDER, &model, -1).await;
    fixture
        .seed_with_shape(
            "tenant-key",
            br#"{"provider":"openai","api_key":"sk-live"}"#,
            Some(NAMED_PROVIDER),
            Some(true),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "tenant-key", Some(&model), NOW)
        .await
        .expect("a catalogued model activates whatever its ceiling reads");

    let Activation::Applied(stored) = outcome else {
        panic!("the row is catalogued, so the gate passes: {outcome:?}");
    };
    assert_eq!(
        stored.context_cap_tokens, 0,
        "a negative catalogue ceiling stores as the sentinel, not as itself"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_compatible_endpoint_activates_at_the_borrowed_ceiling() {
    // The one asymmetry: a user-hosted endpoint's model is absent from the
    // platform catalogue by design, so the gate passes and the ceiling is the
    // smallest any provider publishes for that model.
    let fixture = Fixture::create().await;
    let model = unique_model();
    catalogue(&fixture, NAMED_PROVIDER, &model, 200_000).await;
    catalogue(&fixture, "anthropic", &model, 120_000).await;
    fixture
        .seed_with_shape(
            "my-gateway",
            br#"{"provider":"openai-compatible","base_url":"https://llm.example.com"}"#,
            Some("openai-compatible"),
            Some(false),
        )
        .await;

    let outcome = providers(&fixture)
        .activate(&fixture.tenant, "my-gateway", Some(&model), NOW)
        .await
        .expect("a keyless compatible endpoint activates");

    let Activation::Applied(stored) = outcome else {
        panic!("a compatible endpoint needs no catalogue row of its own: {outcome:?}");
    };
    assert_eq!(
        stored.context_cap_tokens, 120_000,
        "a context window is a property of the model, and the smallest published wins"
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
