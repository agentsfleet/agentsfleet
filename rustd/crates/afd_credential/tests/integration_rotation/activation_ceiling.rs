//! What an applied activation reports: the catalogue's ceiling, never the
//! request's, clamped where the catalogue is negative and borrowed where the
//! endpoint is the tenant's own.

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
async fn a_negative_catalogue_ceiling_is_stored_as_zero() {
    // model.platform_models.context_cap_tokens is INTEGER NOT NULL with no
    // nonnegative CHECK — bounds live in the application (RULE STS) — so a
    // negative ceiling is a row the schema permits. The activation statement
    // clamps it with GREATEST(…, 0), so a -1 catalogue row reads as 0, the
    // unknown/auto sentinel, rather than as a negative ceiling.
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
