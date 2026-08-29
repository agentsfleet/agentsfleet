//! Dimension 7.1 — preference and onboarding round-trips against live Postgres.
//!
//! The half `workspace_preferences.rs` cannot prove: a bag is rows, and the
//! checklist is five `EXISTS` subqueries plus a platform-default read. What is
//! pinned here is the round trip (a value comes back byte for byte), the unset
//! answer (an empty bag, never an absence), last-write-wins, and the checklist
//! turning over as the workspace fills up.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_tenant::preference::{PrefKey, bag_is_true};

use crate::preference_lane::Lane;

/// A value with formatting a re-serializing implementation would normalise.
///
/// Spaces inside the object and an unpadded number: `serde_json::Value` would
/// hand back `{"a":1,"seen":true}` for this, and the column would then hold
/// something the client never wrote.
const FORMATTED: &str = r#"{ "seen": true,  "a": 1 }"#;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_subject_with_no_user_row_resolves_to_nothing() {
    let lane = Lane::create().await;

    // The fail-closed half of the preference surface: a subject authenticated
    // against no `core.users` row gets `None`, not an invented user. Inventing
    // one here would fork identity ownership away from the signup bootstrap.
    let absent = lane
        .preferences
        .resolve_user("fixture|nobody-has-this-subject")
        .await
        .expect("the read must succeed");
    assert!(absent.is_none(), "an unknown subject resolves to no user");

    let present = lane
        .preferences
        .resolve_user(&lane.subject)
        .await
        .expect("the read must succeed");
    assert_eq!(
        present.as_deref(),
        Some(lane.user.as_str()),
        "a seeded subject resolves to its own user row"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unset_bag_is_empty_rather_than_absent() {
    let lane = Lane::create().await;

    // The whole reason this endpoint never 404s: the dashboard fails open
    // toward SHOWING onboarding, so "you have set nothing" and "I could not
    // read your preferences" must not be distinguishable by shape.
    let bag = lane
        .preferences
        .bag(&lane.user, &lane.workspace)
        .await
        .expect("an unset bag is not an error");
    assert!(bag.is_empty(), "a person who set nothing has an empty bag");

    // And an empty bag reads as every step unticked, rather than as unknown.
    for key in [
        PrefKey::GettingStartedDismissed,
        PrefKey::GettingStartedCollapsed,
        PrefKey::GettingStartedCliTicked,
    ] {
        assert!(!bag_is_true(&bag, key), "{key:?} is not ticked");
    }

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_written_value_comes_back_byte_for_byte() {
    let lane = Lane::create().await;

    lane.preferences
        .upsert(
            &lane.user,
            &lane.workspace,
            PrefKey::GettingStartedCollapsed,
            FORMATTED,
            Lane::now(),
        )
        .await
        .expect("the write must land");

    let bag = lane
        .preferences
        .bag(&lane.user, &lane.workspace)
        .await
        .expect("the read must succeed");
    let stored = bag
        .iter()
        .find(|pref| pref.key == PrefKey::GettingStartedCollapsed.as_str())
        .expect("the written key is in the bag");

    // Byte for byte, spacing included. The server stores the client's own text
    // and never interprets it, so a value that came back re-formatted would be
    // a value the server had rewritten.
    assert_eq!(
        stored.value, FORMATTED,
        "a preference round-trips verbatim, whitespace and all"
    );

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_second_write_replaces_the_first_and_adds_no_row() {
    let lane = Lane::create().await;
    let key = PrefKey::GettingStartedDismissed;

    for value in ["true", "false"] {
        lane.preferences
            .upsert(&lane.user, &lane.workspace, key, value, Lane::now())
            .await
            .expect("the write must land");
    }

    let bag = lane
        .preferences
        .bag(&lane.user, &lane.workspace)
        .await
        .expect("the read must succeed");

    // Last-write-wins by design, and ONE row: the unique constraint the upsert
    // arbitrates on is what makes a repeated toggle idempotent in the table
    // rather than merely in the answer.
    assert_eq!(bag.len(), 1, "a re-written key does not add a second row");
    let only = bag
        .first()
        .expect("the one row the assertion above counted");
    assert_eq!(
        only.value, "false",
        "the later write is the one that stands"
    );
    assert!(!bag_is_true(&bag, key), "and it reads as unticked");

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn one_persons_bag_is_invisible_to_another() {
    let mine = Lane::create().await;
    let theirs = Lane::create().await;

    mine.preferences
        .upsert(
            &mine.user,
            &mine.workspace,
            PrefKey::GettingStartedDismissed,
            "true",
            Lane::now(),
        )
        .await
        .expect("the write must land");

    // Scoped by (user, workspace), so neither axis alone leaks: a second person
    // in a second workspace sees nothing of the first one's bag.
    let other = theirs
        .preferences
        .bag(&theirs.user, &theirs.workspace)
        .await
        .expect("the read must succeed");
    assert!(other.is_empty(), "another person's bag is their own");

    // And the same person reading a workspace they have set nothing in.
    let elsewhere = mine
        .preferences
        .bag(&mine.user, &theirs.workspace)
        .await
        .expect("the read must succeed");
    assert!(
        elsewhere.is_empty(),
        "onboarding progress is per workspace, so a second one starts fresh"
    );

    mine.cleanup().await;
    theirs.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_checklist_turns_over_as_the_workspace_fills_up() {
    let lane = Lane::create().await;

    let empty = lane
        .preferences
        .signals(&lane.workspace, &lane.tenant)
        .await
        .expect("the signal read must succeed");
    assert!(!empty.has_fleet, "a fresh workspace holds no fleet");
    assert!(!empty.has_secret, "nor a secret");
    assert!(!empty.has_processed_event, "nor an event");
    assert!(!empty.has_steer_event, "nor a steer");

    lane.seed_fleet().await;
    lane.seed_tenant_model("claude-fixture").await;

    let filled = lane
        .preferences
        .signals(&lane.workspace, &lane.tenant)
        .await
        .expect("the signal read must succeed");
    assert!(filled.has_fleet, "the seeded fleet is seen");
    assert!(
        filled.model_configured,
        "the tenant's own selection configures a model"
    );
    // Untouched by the two writes above: each signal reads its own table, so a
    // fleet appearing must not make a secret appear with it.
    assert!(!filled.has_secret, "seeding a fleet seeds no secret");
    assert!(!filled.has_steer_event, "and no steer event");

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_signals_are_scoped_to_their_own_workspace_and_tenant() {
    let mine = Lane::create().await;
    let theirs = Lane::create().await;

    mine.seed_fleet().await;
    mine.seed_tenant_model("claude-fixture").await;

    let other = theirs
        .preferences
        .signals(&theirs.workspace, &theirs.tenant)
        .await
        .expect("the signal read must succeed");

    // The failure this catches is a signal query that dropped its scope: with
    // one shared database, an unscoped `EXISTS` answers true for every
    // workspace the moment ANY workspace holds a fleet.
    assert!(
        !other.has_fleet,
        "another workspace's fleet is not this one's"
    );

    mine.cleanup().await;
    theirs.cleanup().await;
}
