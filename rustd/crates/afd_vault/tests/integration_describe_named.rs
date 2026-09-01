//! Describing a NAMED SET of credentials — the read the model registry page
//! makes, and the one the workspace list cannot answer for it.
//!
//! `crate::describe`'s own header states the contract this grades: a name with
//! no row is ABSENT from the map rather than an error or a blank descriptor;
//! empty input answers without a round trip; one credential legitimately backs
//! several model rows, so the answer is a map the caller looks up per row rather
//! than a slot per entry.
//!
//! # Why it is not the list with a filter
//!
//! `integration_list_no_decrypt.rs` next door grades the workspace list, and
//! the two reads project DIFFERENT column sets on purpose: `meta_has_key` is on
//! this one and not on that one, because key presence is the registry page's
//! question and the secrets list never asked it. So the `has_key` cases below
//! are this read's alone.
//!
//! What both share is `labelled`, and sharing it is the point — one function
//! decides the degrade for both readers, rather than two written to agree. The
//! un-labelled case here is what proves this reader inherited that decision
//! rather than reimplementing it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_vault::Kind;

use crate::support::Lane;

/// A provider key, fully labelled by the write path.
const PROVIDER_KEY: &str = r#"{"provider":"anthropic","api_key":"sk-live"}"#;

/// An OpenAI-compatible endpoint, which carries a base URL.
const CUSTOM_ENDPOINT: &str =
    r#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1","api_key":"sk"}"#;

/// A credential holding no key at all.
const KEYLESS: &str = r#"{"note":"not a provider credential"}"#;

/// Each named credential comes back with the projection the write path stored,
/// and `has_key` reports key PRESENCE without the key.
///
/// The three shapes together are the discrimination: a provider key, an
/// endpoint that also carries a base URL, and a body holding no key at all. A
/// reader that defaulted `has_key` either way would pass on two of them.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_describe_each_named_credential_from_its_projection() {
    let lane = Lane::create().await;
    lane.store("anthropic-key", PROVIDER_KEY).await;
    lane.store("gateway", CUSTOM_ENDPOINT).await;
    lane.store("a-note", KEYLESS).await;

    let described = lane
        .keyless_directory()
        .describe(
            &lane.workspace,
            &["anthropic-key", "gateway", "a-note"],
        )
        .await
        .expect("the describe answers");

    assert_eq!(described.len(), 3, "every named row is described");

    let key = described.get("anthropic-key").expect("the provider key is described");
    assert_eq!(key.kind, Kind::ProviderKey);
    assert_eq!(key.provider.as_deref(), Some("anthropic"));
    assert!(key.has_key, "a provider key holds one");

    let gateway = described.get("gateway").expect("the endpoint is described");
    assert_eq!(gateway.kind, Kind::CustomEndpoint);
    assert_eq!(
        gateway.base_url.as_deref(),
        Some("https://gw.example.com/v1"),
        "the endpoint is the field this kind exists to display"
    );

    let note = described.get("a-note").expect("the opaque secret is described");
    assert_eq!(note.kind, Kind::CustomSecret);
    assert!(
        !note.has_key,
        "a body with no key must not report one — this is the column the page \
         gates its rendering on"
    );

    lane.cleanup().await;
}

/// A name with no row is ABSENT from the map — not an error, and not a blank
/// descriptor.
///
/// This is what lets an entry naming a credential deleted out of band still
/// list, degraded, instead of failing the page it appears on. The decision is
/// the caller's, and it can only make it if this read distinguishes "no row"
/// from "a row saying nothing".
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_omit_a_name_with_no_row_rather_than_failing_or_blanking_it() {
    let lane = Lane::create().await;
    lane.store("present", PROVIDER_KEY).await;

    let described = lane
        .keyless_directory()
        .describe(&lane.workspace, &["present", "never-stored"])
        .await
        .expect("an unknown name is not a failure");

    assert!(described.contains_key("present"));
    assert!(
        !described.contains_key("never-stored"),
        "absent means absent — a blank descriptor would be indistinguishable \
         from a real row that happened to say nothing"
    );

    lane.cleanup().await;
}

/// Empty input answers an empty map without a round trip.
///
/// A page with no rows has nothing to describe, and the statement count a
/// registry read costs is pinned — a degenerate page must not spend one.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_answer_empty_input_without_a_round_trip() {
    let lane = Lane::create().await;

    let described = lane
        .keyless_directory()
        .describe(&lane.workspace, &[])
        .await
        .expect("no names is not a failure");

    assert!(described.is_empty());

    lane.cleanup().await;
}

/// A row carrying no `meta_kind` degrades to an opaque secret and sheds the
/// descriptors its other columns hold.
///
/// The same decision the workspace list makes, because both readers call
/// `labelled`. The provider and base URL are seeded NON-NULL on purpose: a
/// reader that returned them beside a degraded kind would be describing a row
/// as something it could not classify.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_degrade_a_row_this_build_cannot_label_and_shed_its_descriptors() {
    let lane = Lane::create().await;
    lane.store("labelled", PROVIDER_KEY).await;
    lane.seed_projected_row(
        "unlabelled",
        "labelled",
        None,
        Some("anthropic"),
        Some("https://gw.example.com/v1"),
        Some(true),
    )
    .await;

    let described = lane
        .keyless_directory()
        .describe(&lane.workspace, &["unlabelled"])
        .await
        .expect("an un-labelled row lists rather than failing");

    let row = described.get("unlabelled").expect("the row is described");
    assert_eq!(row.kind, Kind::CustomSecret);
    assert_eq!(
        row.provider, None,
        "a row this build cannot classify is not described as a provider's"
    );
    assert_eq!(row.base_url, None);
    assert!(
        row.has_key,
        "key PRESENCE survives the degrade — it is a column, not a classification"
    );

    lane.cleanup().await;
}

/// The read is scoped to its workspace: a name stored in another one is not
/// described here, even when spelled identically.
///
/// The statement filters on the workspace, and this is the assertion that says
/// so — the registry page passes a tenant's primary workspace, and a describe
/// that ignored it would let one tenant read another's credential labels.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_describe_only_rows_in_the_workspace_it_was_asked_about() {
    let lane = Lane::create().await;
    lane.store("shared-name", PROVIDER_KEY).await;
    let elsewhere = lane.another_workspace().await;

    let described = lane
        .keyless_directory()
        .describe(&elsewhere, &["shared-name"])
        .await
        .expect("the read answers");

    assert!(
        described.is_empty(),
        "the name exists, but not in the workspace this read named"
    );

    lane.cleanup().await;
}
