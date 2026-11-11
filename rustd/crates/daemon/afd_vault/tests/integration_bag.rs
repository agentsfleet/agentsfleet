//! A platform bag read out of a workspace's vault: absent, stored, replaced,
//! refused by its schema, and read inside a caller's own transaction.
//!
//! The pure half — what a document must hold to parse — is `bag`'s unit suite.
//! This one proves the part that needs a database: the lookup by name, that a
//! replaced bag is read on the next call with no restart, and that `bag_on`
//! sees what only its transaction can.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code;
use af_crypto::secret::SecretString;
use afd_vault::StoredEnvelope;
use afd_vault::bag::{Bag, BagFault, filled, secret};
use serde::Deserialize;
use sqlx::Acquire as _;

use crate::support::{Lane, MINTED_AT, body, named};

/// A two-field bag: the shape every platform bag has, a plain field and a secret.
#[derive(Debug)]
struct Probe {
    host: String,
    token: SecretString,
}

#[derive(Deserialize)]
struct StoredProbe {
    host: Option<String>,
    token: Option<SecretString>,
}

impl Bag for Probe {
    type Stored = StoredProbe;

    fn from_stored(stored: StoredProbe) -> Result<Self, BagFault> {
        Ok(Self {
            host: filled(stored.host, "host")?,
            token: secret(stored.token, "token")?,
        })
    }
}

const COMPLETE: &str = r#"{"host":"relay.example.test","token":"t-1"}"#;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stored_bag_reads_as_its_typed_value() {
    let lane = Lane::create().await;
    lane.store("probe", COMPLETE).await;

    let probe = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "probe")
        .await
        .expect("the vault answers")
        .expect("a complete bag parses");

    assert_eq!(probe.host, "relay.example.test");
    assert_eq!(probe.token.expose(), "t-1");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn no_workspace_no_row_and_a_refused_name_are_all_absent() {
    let lane = Lane::create().await;
    lane.store("probe", COMPLETE).await;
    let other = lane.another_workspace().await;

    for (case, workspace, key) in [
        ("no workspace configured", None, "probe"),
        ("no row of that name", Some(&lane.workspace), "never-stored"),
        ("a name the vault refuses", Some(&lane.workspace), ""),
        ("the row is another workspace's", Some(&other), "probe"),
    ] {
        let read = lane
            .vault
            .bag::<Probe>(workspace, key)
            .await
            .expect("an absent bag is not a vault failure");
        assert_eq!(read.err(), Some(BagFault::Absent), "{case}");
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_row_its_schema_refuses_is_a_fault_naming_the_field_and_not_an_error() {
    let lane = Lane::create().await;
    lane.store("probe", r#"{"host":"relay.example.test"}"#)
        .await;

    let read = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "probe")
        .await
        .expect("a bag that fails its schema is not a vault failure");

    assert_eq!(read.err(), Some(BagFault::Missing("token")));
}

/// The property a rotation depends on: nothing is read once and kept.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_replaced_bag_is_read_on_the_next_call_with_no_restart() {
    let lane = Lane::create().await;
    lane.store("probe", COMPLETE).await;
    let before = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "probe")
        .await
        .expect("the vault answers")
        .expect("parses");
    assert_eq!(before.token.expose(), "t-1");

    lane.vault
        .replace(
            &lane.workspace,
            &named("probe"),
            &body(r#"{"host":"relay.example.test","token":"t-2"}"#),
        )
        .await
        .expect("the bag is replaced");

    let after = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "probe")
        .await
        .expect("the vault answers")
        .expect("parses");
    assert_eq!(after.token.expose(), "t-2");
}

/// `bag_on` reads on the caller's connection, so it sees a secret that only the
/// caller's open transaction has written, and a read elsewhere does not.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn bag_on_reads_inside_the_callers_transaction() {
    let lane = Lane::create().await;
    let mut connection = lane.pool.acquire().await.expect("a connection");
    let mut transaction = connection.begin().await.expect("a transaction");
    let _announcement = lane
        .vault
        .create_in(
            &mut transaction,
            &lane.workspace,
            &named("inflight"),
            &body(COMPLETE),
            MINTED_AT,
        )
        .await
        .expect("the secret is written in the open transaction");

    let inside = lane
        .vault
        .bag_on::<Probe>(&mut transaction, &lane.workspace, "inflight")
        .await
        .expect("the vault answers")
        .expect("the transaction's own write is visible to it");
    assert_eq!(inside.host, "relay.example.test");

    let elsewhere = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "inflight")
        .await
        .expect("the vault answers");
    assert_eq!(
        elsewhere.err(),
        Some(BagFault::Absent),
        "an uncommitted write is invisible to every other connection"
    );

    transaction.commit().await.expect("the transaction commits");
    let committed = lane
        .vault
        .bag::<Probe>(Some(&lane.workspace), "inflight")
        .await
        .expect("the vault answers");
    let committed = committed.expect("once committed it is everyone's");
    assert_eq!(committed.host, "relay.example.test");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn find_treats_a_refused_name_as_no_row() {
    let lane = Lane::create().await;
    lane.store("probe", COMPLETE).await;

    let found = lane
        .vault
        .find(&lane.workspace, "probe")
        .await
        .expect("the vault answers")
        .expect("the row exists");
    assert!(found.expose().starts_with(b"{"), "the plaintext comes back");

    for (case, key) in [("an unknown name", "nope"), ("a refused name", "")] {
        let none = lane
            .vault
            .find(&lane.workspace, key)
            .await
            .expect("an absent row is not a vault failure");
        assert!(none.is_none(), "{case}");
    }
}

/// `open_stored` opens an envelope the caller read itself, bound to the
/// workspace and name it was sealed under, and to nothing else.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn open_stored_opens_only_under_the_name_the_row_was_sealed_under() {
    let lane = Lane::create().await;
    lane.store("probe", COMPLETE).await;
    let mut connection = lane.pool.acquire().await.expect("a connection");
    let envelope = sqlx::query_as::<_, StoredEnvelope>(
        "SELECT dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, \
                secret_tag, kek_version \
           FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2",
    )
    .bind(&lane.workspace)
    .bind("probe")
    .fetch_one(&mut *connection)
    .await
    .expect("the row the caller reads");

    let again = sqlx::query_as::<_, StoredEnvelope>(
        "SELECT dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, \
                secret_tag, kek_version \
           FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2",
    )
    .bind(&lane.workspace)
    .bind("probe")
    .fetch_one(&mut *connection)
    .await
    .expect("the same row again");

    let opened = lane
        .vault
        .open_stored(envelope, &lane.workspace, "probe")
        .expect("the envelope opens under its own name");
    assert!(opened.expose().windows(3).any(|window| window == b"t-1"));

    let moved = lane
        .vault
        .open_stored(again, &lane.workspace, "another-name");
    let refused =
        moved.expect_err("a ciphertext lifted under another name fails its authentication tag");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(!refused.is_datastore_unavailable());
}
