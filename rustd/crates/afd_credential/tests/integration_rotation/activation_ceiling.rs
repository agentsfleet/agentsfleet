//! What an applied activation reports: the catalogue's ceiling, never the
//! request's, with invalid contexts refused by the schema and the smallest
//! published context borrowed for a custom endpoint.

use afd_credential::provider::Activation;

use super::activation::{NAMED_PROVIDER, catalogue, providers, unique_model, written};
use super::{Fixture, NOW};

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
    assert_eq!(stored.provider.as_deref(), Some(NAMED_PROVIDER));
    assert_eq!(&*stored.secret_ref, "tenant-key");
    assert_eq!(written(&fixture).await, (1, 1));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_nonpositive_catalogue_ceiling_is_refused_without_an_identity() {
    use std::error::Error as _;
    let fixture = Fixture::create().await;
    let model = unique_model();
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    let refused = afd_db::test_util::ModelSeed::new(
        &afd_db::test_util::mint_id(),
        NAMED_PROVIDER,
        &model,
        -1,
    )
    .insert(&mut *connection)
    .await
    .expect_err("contexts must be positive");
    let database = refused
        .source()
        .and_then(|source| source.downcast_ref::<sqlx::Error>())
        .and_then(sqlx::Error::as_database_error)
        .expect("the check failure retains its SQL source");
    assert_eq!(database.code().as_deref(), Some("23514"));
    assert_eq!(
        database.constraint(),
        Some("ck_platform_models_context_positive")
    );
    drop(connection);
    assert_eq!(written(&fixture).await, (0, 0));
    fixture.cleanup().await;
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
