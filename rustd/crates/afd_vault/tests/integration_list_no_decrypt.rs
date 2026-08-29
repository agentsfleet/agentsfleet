//! §4 Dimension 4.1 — the round trip, and a list that performs zero decrypts.
//!
//! `#[ignore]`d so `make test-unit-all` compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them against compose Postgres.
//!
//! # How zero decrypts is proven, without counting anything
//!
//! `crypto_store.zig` proves it with a `decrypt_tally` counter and a
//! `noteDecrypt` funnel every decrypt site must remember to call. A counter
//! proves what happened on the run that was measured, and only if nobody
//! forgot the funnel.
//!
//! Here it is proven twice, and neither proof is a count:
//!
//! - **By type.** `Directory` holds no key. `Envelope::open` takes a `&Kek`, so
//!   the half of the store that lists could not decrypt if it wanted to. The
//!   fixture builds one from the pool alone to make that visible at the call
//!   site — see [`Lane::keyless_directory`].
//! - **By observation.** A row whose ciphertext has been CORRUPTED still lists
//!   with its full projection. `secret_list.zig` answers that same row as an
//!   opaque `custom_secret`, because its projection comes from a body it could
//!   not open. The two implementations therefore give different answers on this
//!   row, and the difference is the assertion.
//!
//! The second is what makes this more than a restatement of the type: it would
//! fail the moment anything on this path opened an envelope, however it was
//! spelled.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_vault::{Deleted, Kind};

use crate::support::{Lane, body, named};

/// A provider key, as an operator would store one.
const PROVIDER_KEY: &str =
    r#"{"provider":"anthropic","model":"claude-opus-5","api_key":"sk-live"}"#;

/// An OpenAI-compatible endpoint.
const CUSTOM_ENDPOINT: &str =
    r#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1","api_key":"sk"}"#;

/// An opaque credential a skill reads fields out of by name.
const OPAQUE: &str = r#"{"host":"db.internal","api_token":"t"}"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_stored_secret_round_trips_from_write_to_list_to_delete() {
    let lane = Lane::create().await;

    lane.store("anthropic-prod", PROVIDER_KEY).await;
    lane.store("gateway", CUSTOM_ENDPOINT).await;
    lane.store("stripe", OPAQUE).await;

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("the list answers");

    // Ordered by name, which is what the statement orders by — a client paging
    // this surface twice sees the same sequence.
    let names: Vec<&str> = listed.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["anthropic-prod", "gateway", "stripe"]);

    let row = |index: usize| listed.get(index).expect("the page holds three rows");

    let provider_key = row(0);
    assert_eq!(provider_key.kind, Kind::ProviderKey);
    assert_eq!(provider_key.provider.as_deref(), Some("anthropic"));
    assert_eq!(provider_key.base_url, None);
    assert_eq!(provider_key.created_at_ms, crate::support::NOW_MS);

    let endpoint = row(1);
    assert_eq!(endpoint.kind, Kind::CustomEndpoint);
    assert_eq!(endpoint.provider.as_deref(), Some("openai-compatible"));
    assert_eq!(
        endpoint.base_url.as_deref(),
        Some("https://gw.example.com/v1")
    );

    let opaque = row(2);
    assert_eq!(opaque.kind, Kind::CustomSecret);
    assert_eq!(opaque.provider, None);

    // Delete is idempotent, and both outcomes are distinguishable here even
    // though the route answers 204 for either.
    let directory = lane.keyless_directory();
    assert_eq!(
        directory
            .delete(&lane.workspace, &named("gateway"))
            .await
            .expect("nothing references it"),
        Deleted::Removed
    );
    assert_eq!(
        directory
            .delete(&lane.workspace, &named("gateway"))
            .await
            .expect("a second delete is not a failure"),
        Deleted::AlreadyAbsent
    );
    assert_eq!(lane.secret_count(&lane.workspace).await, 2);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_row_whose_ciphertext_cannot_open_still_lists_with_its_full_projection() {
    // The observable half of the never-decrypt proof. Nothing can open this row
    // any more — the fixture flipped a byte of its ciphertext, so the
    // authentication tag will refuse it under any key. A list that decrypted
    // would have to degrade it to an opaque `custom_secret`, which is exactly
    // what the Zig list does with it.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;
    lane.corrupt_ciphertext("anthropic-prod").await;

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("a corrupt envelope is not a failed list");

    assert_eq!(listed.len(), 1);
    let corrupt = listed.first().expect("the row still lists");
    assert_eq!(
        corrupt.kind,
        Kind::ProviderKey,
        "the projection is read from columns, so an unopenable body changes nothing"
    );
    assert_eq!(corrupt.provider.as_deref(), Some("anthropic"));

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn one_workspaces_secrets_are_invisible_to_another() {
    // Tenancy is in the statement's predicate, not in the caller's discipline:
    // the list is workspace-scoped, so a second workspace under the SAME tenant
    // sees nothing of the first's.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;
    let neighbour = lane.another_workspace().await;

    let listed = lane
        .keyless_directory()
        .list(&neighbour)
        .await
        .expect("the list answers");

    assert!(listed.is_empty());

    // And a delete addressed at the neighbour's copy of the name removes
    // nothing — absence rather than somebody else's row.
    assert_eq!(
        lane.keyless_directory()
            .delete(&neighbour, &named("anthropic-prod"))
            .await
            .expect("a foreign name is absent, not an error"),
        Deleted::AlreadyAbsent
    );
    assert_eq!(lane.secret_count(&lane.workspace).await, 1);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_replace_rewrites_the_body_whole_and_the_projection_with_it() {
    // Replacement is total: a field absent from the new body is absent from the
    // stored secret afterwards. That is the verb's point, and it is why no
    // caller has to read a secret back in order to change it.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;

    lane.vault
        .replace(
            &lane.workspace,
            &named("anthropic-prod"),
            &body(OPAQUE),
            Lane::now(),
        )
        .await
        .expect("a held name replaces");

    assert_eq!(lane.opened("anthropic-prod").await, OPAQUE);

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("the list answers");
    let replaced = listed.first().expect("the row still lists");
    assert_eq!(
        replaced.kind,
        Kind::CustomSecret,
        "the projection followed the body into the same statement"
    );
    assert_eq!(replaced.provider, None);

    lane.cleanup().await;
}
