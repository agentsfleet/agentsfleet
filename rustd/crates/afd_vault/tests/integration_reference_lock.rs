//! §4 Dimension 4.3 — what refuses a write, and the lock a delete is taken under.
//!
//! `#[ignore]`d so `make test-unit-all` compiles and lints these without a
//! datastore; `make test-integration-rustd` runs them against compose Postgres.
//!
//! # What only this lane can prove
//!
//! The shape refusals — a body that is not a non-empty object, a name outside
//! its bounds — are decided by a constructor and are proven where they are
//! decided, in `afd_vault`'s unit suite and in the router's. What they cannot
//! say is the second half of the claim: that NOTHING WAS STORED. That is a
//! statement about rows.
//!
//! The same goes for the two properties this surface leans on Postgres for. A
//! create claims a name through the database's own uniqueness decision, so two
//! callers racing one name cannot both win; and a delete counts registry
//! references under a row lock, so an entry cannot appear between the count and
//! the removal. Neither is provable without a database, and neither is a
//! property of any code path a stub could stand in for.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_vault::{Deleted, SecretBody};
use serde_json::value::RawValue;

use crate::support::{Lane, body, named};

/// A provider key, as an operator would store one.
const PROVIDER_KEY: &str = r#"{"provider":"anthropic","api_key":"sk-live"}"#;

/// A different body, so an overwrite would be visible.
const REPLACEMENT: &str = r#"{"provider":"openai","api_key":"sk-other"}"#;

/// The model the registry entries in these tests name.
const MODEL_ID: &str = "claude-opus-5";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_body_that_is_not_a_non_empty_object_never_becomes_a_row() {
    // The half the unit suite cannot state. `SecretBody::parse` refuses before
    // a connection is drawn, so the proof is that the table is untouched.
    let lane = Lane::create().await;

    for refused in ["{}", r#""a string""#, "[]", "42", "null"] {
        let raw = RawValue::from_string(refused.to_owned()).expect("valid JSON");
        let error = SecretBody::parse(&raw).expect_err("not a storable body");
        assert_eq!(error.code(), error_code::VAULT_DATA_INVALID, "{refused}");
    }
    assert_eq!(lane.secret_count(&lane.workspace).await, 0);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_create_on_a_taken_name_refuses_and_leaves_the_held_body_alone() {
    // `ON CONFLICT DO NOTHING`, and the assertion that matters is not the
    // refusal — it is that the credential the operator already had is still the
    // one stored. An upsert here would have buried it and answered 201.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;

    let refused = lane
        .vault
        .create(
            &lane.workspace,
            &named("anthropic-prod"),
            &body(REPLACEMENT),
            Lane::now(),
        )
        .await
        .expect_err("a held name is not free");

    assert_eq!(refused.code(), error_code::SECRET_NAME_TAKEN);
    assert_eq!(lane.opened("anthropic-prod").await, PROVIDER_KEY);
    assert_eq!(lane.secret_count(&lane.workspace).await, 1);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn two_creates_racing_one_name_resolve_to_one_winner() {
    // The window a read-then-write leaves open, closed by letting Postgres
    // decide. Both tasks find the name free at the same instant; exactly one
    // row exists afterwards and exactly one caller is told the name was taken.
    let lane = Lane::create().await;

    // Bound rather than passed inline: both futures borrow their name and body
    // for as long as the join runs, so a temporary would be freed first.
    let name = named("contended");
    let (mine, theirs) = (body(PROVIDER_KEY), body(REPLACEMENT));
    let (left, right) = tokio::join!(
        lane.vault
            .create(&lane.workspace, &name, &mine, Lane::now()),
        lane.vault
            .create(&lane.workspace, &name, &theirs, Lane::now()),
    );

    let refusals = [&left, &right].into_iter().filter(|r| r.is_err()).count();
    assert_eq!(refusals, 1, "exactly one caller loses the name");
    for outcome in [left, right] {
        if let Err(refused) = outcome {
            assert_eq!(refused.code(), error_code::SECRET_NAME_TAKEN);
        }
    }
    assert_eq!(lane.secret_count(&lane.workspace).await, 1);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_replace_creates_nothing_when_the_name_is_not_held() {
    // An UPDATE and not an upsert, which is a safety property: a replace racing
    // a delete must not resurrect the credential the operator just removed.
    let lane = Lane::create().await;

    let refused = lane
        .vault
        .replace(
            &lane.workspace,
            &named("never-stored"),
            &body(PROVIDER_KEY),
            Lane::now(),
        )
        .await
        .expect_err("claiming a name is create's job");

    assert_eq!(refused.code(), error_code::SECRET_NOT_FOUND);
    assert_eq!(lane.secret_count(&lane.workspace).await, 0);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn a_delete_is_refused_while_the_model_registry_still_names_the_secret() {
    // The orphan this lock exists to prevent: an entry naming a credential that
    // is gone survives every later read and fails at the point of use, when a
    // fleet tries to run and cannot resolve a key.
    let lane = Lane::create().await;
    lane.store("anthropic-prod", PROVIDER_KEY).await;
    lane.seed_model_entry(MODEL_ID, "anthropic-prod").await;
    lane.seed_model_entry("claude-sonnet-5", "anthropic-prod")
        .await;

    let refused = lane
        .keyless_directory()
        .delete(&lane.workspace, &named("anthropic-prod"))
        .await
        .expect_err("two entries still name it");

    assert_eq!(
        refused.code(),
        error_code::SECRET_REFERENCED_BY_MODEL_ENTRIES
    );
    assert_eq!(
        refused.referenced_by(),
        Some(2),
        "the count comes from the statement that took the locks"
    );
    assert_eq!(
        lane.secret_count(&lane.workspace).await,
        1,
        "the credential the entries name is still there"
    );

    // Remove the entries and the same delete goes through — the refusal was
    // about the references, not about the credential.
    lane.clear_model_entries("anthropic-prod").await;
    assert_eq!(
        lane.keyless_directory()
            .delete(&lane.workspace, &named("anthropic-prod"))
            .await
            .expect("nothing references it now"),
        Deleted::Removed
    );
    assert_eq!(lane.secret_count(&lane.workspace).await, 0);

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn references_are_counted_against_the_workspaces_owner_not_the_caller() {
    // The bug this signature shape exists to prevent. Whose entries are at
    // stake is a property of the CREDENTIAL — derived from the workspace, whose
    // `tenant_id` is NOT NULL — never of whoever is asking. When the two were
    // allowed to differ, an operator with cross-workspace authority counted
    // references against its OWN tenant, matched none, and deleted a credential
    // the victim's registry still named.
    //
    // Here the delete is addressed at a workspace whose tenant holds the entry,
    // and no caller identity is passed at all: there is no parameter through
    // which the wrong tenant could arrive.
    let lane = Lane::create().await;
    lane.store("shared-key", PROVIDER_KEY).await;
    lane.seed_model_entry(MODEL_ID, "shared-key").await;

    let refused = lane
        .keyless_directory()
        .delete(&lane.workspace, &named("shared-key"))
        .await
        .expect_err("the owning tenant's entry counts");

    assert_eq!(refused.referenced_by(), Some(1));

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn two_deletes_racing_one_secret_both_answer_success() {
    // Serialized by the row lock at step 1. Whichever transaction reaches it
    // first removes the row; the second blocks, then finds nothing and reports
    // absence — which is what its caller wanted. Neither is an error, and the
    // route answers 204 for both.
    let lane = Lane::create().await;
    lane.store("contended", PROVIDER_KEY).await;

    let directory = lane.keyless_directory();
    let name = named("contended");
    let (left, right) = tokio::join!(
        directory.delete(&lane.workspace, &name),
        directory.delete(&lane.workspace, &name),
    );

    let outcomes = [
        left.expect("a racing delete is not a failure"),
        right.expect("a racing delete is not a failure"),
    ];
    assert_eq!(
        outcomes.iter().filter(|o| **o == Deleted::Removed).count(),
        1,
        "exactly one transaction removed the row"
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == Deleted::AlreadyAbsent)
            .count(),
        1,
        "the other found it already gone"
    );
    assert_eq!(lane.secret_count(&lane.workspace).await, 0);

    lane.cleanup().await;
}
