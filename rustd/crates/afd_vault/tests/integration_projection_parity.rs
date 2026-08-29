//! §4 Dimension 4.2 — the projection cannot drift from the ciphertext beside it.
//!
//! `#[ignore]`d so `make test-unit-all` compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them against compose Postgres.
//!
//! # Two halves, and they are different claims
//!
//! **Same-statement.** The `meta_*` columns describe the bytes actually sealed
//! in the row beside them. Asserted end to end: the suite opens the stored
//! envelope with its own key, projects that plaintext, and compares against the
//! columns. A unit test can prove `SecretBody` produces both from one parse; only
//! this can prove the statement wrote both of what it produced.
//!
//! **Cross-daemon.** A row another daemon wrote lists identically from here.
//! There is no Zig process in this lane, so the fixture writes the exact column
//! values `metadata.zig::project` produces for a given body, and the assertion
//! is that this list reads them back verbatim. That is the honest form of the
//! claim: what is under test is the READER's agreement with a column set, and a
//! subprocess would add a build dependency without adding a fact.
//!
//! A row from BEFORE the projection columns existed is the third case, and it
//! is not a failure — it lists as an opaque credential, because that is what
//! "we cannot describe this" looks like on the wire.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_vault::Kind;

use crate::support::{Lane, StoredProjection, body, named};

/// A provider key carrying every descriptor a `provider_key` can have.
const PROVIDER_KEY: &str =
    r#"{"provider":"anthropic","model":"claude-opus-5","api_key":"sk-live"}"#;

/// An endpoint whose URL hides a password in its userinfo.
const ENDPOINT_WITH_USERINFO: &str = r#"{"provider":"openai-compatible","base_url":"https://user:pw@gw.example.com/v1","api_key":"sk"}"#;

/// An endpoint whose URL is displayable.
const ENDPOINT: &str = r#"{"provider":"openai-compatible","base_url":"https://gw.example.com/v1"}"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn the_projection_columns_describe_the_bytes_sealed_beside_them() {
    let lane = Lane::create().await;

    for (name, stored, expected) in [
        (
            "anthropic-prod",
            PROVIDER_KEY,
            StoredProjection {
                kind: Some("provider_key".to_owned()),
                provider: Some("anthropic".to_owned()),
                base_url: None,
                has_key: Some(true),
            },
        ),
        (
            "gateway",
            ENDPOINT,
            StoredProjection {
                kind: Some("custom_endpoint".to_owned()),
                provider: Some("openai-compatible".to_owned()),
                base_url: Some("https://gw.example.com/v1".to_owned()),
                // No `api_key` in the body, so no key is claimed.
                has_key: Some(false),
            },
        ),
        (
            "stripe",
            r#"{"host":"db.internal","api_token":"t"}"#,
            StoredProjection {
                kind: Some("custom_secret".to_owned()),
                provider: None,
                base_url: None,
                has_key: Some(false),
            },
        ),
    ] {
        lane.store(name, stored).await;

        // The plaintext really is what the caller sent, byte for byte.
        assert_eq!(lane.opened(name).await, stored, "{name}");
        // And the columns really do describe it.
        assert_eq!(
            lane.meta_columns(name).await.expect("the row exists"),
            expected,
            "{name}"
        );
    }

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_password_inside_a_base_url_is_sealed_but_never_promoted_to_a_column() {
    // The promotion's whole argument is that a projected field is one any
    // authorized caller already sees. A password in the URL is not, so the
    // column holds nothing — while the sealed body keeps the URL intact,
    // because that is what the runner has to dial.
    let lane = Lane::create().await;
    lane.store("gateway", ENDPOINT_WITH_USERINFO).await;

    assert_eq!(lane.opened("gateway").await, ENDPOINT_WITH_USERINFO);

    let columns = lane.meta_columns("gateway").await.expect("the row exists");
    assert_eq!(columns.kind.as_deref(), Some("custom_endpoint"));
    assert_eq!(
        columns.base_url, None,
        "a credential-bearing URL must not become a column any reader can SELECT"
    );

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("the list answers");
    assert_eq!(
        listed.first().expect("the row lists").base_url,
        None,
        "the wire omits what the column refused to hold"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_replace_rewrites_both_halves_or_neither() {
    // The drift this design exists to make impossible: a write that changed the
    // ciphertext and left a `meta_*` column describing the previous body would
    // still return 200, and the list would then lie about a live credential.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;

    lane.vault
        .replace(
            &lane.workspace,
            &named("anthropic-prod"),
            &body(ENDPOINT),
            Lane::now(),
        )
        .await
        .expect("a held name replaces");

    assert_eq!(lane.opened("anthropic-prod").await, ENDPOINT);
    assert_eq!(
        lane.meta_columns("anthropic-prod")
            .await
            .expect("the row exists"),
        StoredProjection {
            kind: Some("custom_endpoint".to_owned()),
            provider: Some("openai-compatible".to_owned()),
            base_url: Some("https://gw.example.com/v1".to_owned()),
            has_key: Some(false),
        },
        "every column moved with the body, including the one that had to clear"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_row_another_daemon_projected_lists_identically_from_here() {
    // The cross-daemon half. These are the exact column values
    // `metadata.zig::project` writes for each body, supplied verbatim; the
    // assertion is that this reader agrees with them.
    let lane = Lane::create().await;
    lane.store("seed", PROVIDER_KEY).await;

    lane.seed_projected_row(
        "written-elsewhere",
        "seed",
        Some("custom_endpoint"),
        Some("openai-compatible"),
        Some("https://elsewhere.example.com/v1"),
        Some(true),
    )
    .await;

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("the list answers");
    let foreign = listed
        .iter()
        .find(|row| row.name == "written-elsewhere")
        .expect("the seeded row lists");

    assert_eq!(foreign.kind, Kind::CustomEndpoint);
    assert_eq!(foreign.provider.as_deref(), Some("openai-compatible"));
    assert_eq!(
        foreign.base_url.as_deref(),
        Some("https://elsewhere.example.com/v1")
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_row_from_before_the_projection_columns_lists_as_an_opaque_credential() {
    // Not healed by decrypting. A heal-on-read path would put an envelope open
    // back on this path and make "reads never decrypt" true only after warm-up,
    // which is not a guarantee. `agentsfleetd backfill` is what fills these.
    let lane = Lane::create().await;
    lane.store("seed", PROVIDER_KEY).await;
    lane.seed_projected_row("un-backfilled", "seed", None, None, None, None)
        .await;

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("a page must not fail over one row it cannot label");
    let legacy = listed
        .iter()
        .find(|row| row.name == "un-backfilled")
        .expect("the row still lists");

    assert_eq!(legacy.kind, Kind::CustomSecret);
    assert_eq!(legacy.provider, None);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_kind_this_build_does_not_know_sheds_its_descriptors_with_it() {
    // A newer daemon's vocabulary. Reporting the row as an opaque credential
    // while still presenting a provider label would contradict the union a
    // client narrows on, where that kind carries no such field — and a label
    // this build cannot place is one it should not be presenting.
    let lane = Lane::create().await;
    lane.store("seed", PROVIDER_KEY).await;
    lane.seed_projected_row(
        "from-the-future",
        "seed",
        Some("managed_identity"),
        Some("azure"),
        None,
        Some(true),
    )
    .await;

    let listed = lane
        .keyless_directory()
        .list(&lane.workspace)
        .await
        .expect("the list answers");
    let future = listed
        .iter()
        .find(|row| row.name == "from-the-future")
        .expect("the row still lists");

    assert_eq!(future.kind, Kind::CustomSecret);
    assert_eq!(future.provider, None);

    lane.cleanup().await;
}
