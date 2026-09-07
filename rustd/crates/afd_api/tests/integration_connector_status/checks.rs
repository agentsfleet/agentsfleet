//! The five things the status walk asserts, one function each.
//!
//! The walk itself is one test — it shares a database lane and a seeded
//! workspace, so splitting it into five tests would mean seeding five times.
//! What CAN be split is the assertions, and they read better named than
//! inlined. A child module rather than a sibling because each one reaches
//! into [`super::Fixture`], whose fields stay private to the parent.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use super::*;

/// The sealed handle opens, carries its marker, and names itself.
pub(super) async fn a_held_handle_reads_as_connected(router: &axum::Router, fixture: &Fixture) {
    let read = send(
        router,
        Method::GET,
        &fixture.one(HELD),
        Some(&fixture.token),
        "",
    )
    .await;
    let status = read.status();
    let document = json_body(read).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some(STATUS_CONNECTED),
        "{document}"
    );
    assert_eq!(
        document.get("label").and_then(Value::as_str),
        Some(LABEL),
        "the label is what a person recognises the connection by: {document}"
    );
}

/// A provider whose key the vault holds nothing under is absent, not an error.
pub(super) async fn a_provider_with_no_handle_reads_as_not_connected(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let read = send(
        router,
        Method::GET,
        &fixture.one(UNHELD),
        Some(&fixture.token),
        "",
    )
    .await;
    let status = read.status();
    let document = json_body(read).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some(STATUS_NOT_CONNECTED),
        "{document}"
    );
}

/// The catalogue's `connected` column comes from the vault listing.
///
/// One listing and no decryption — the grant key IS the provider id — so this
/// is the assertion that the listing is filtered by the registry rather than
/// the other way round: an ordinary workspace secret must not add a row.
pub(super) async fn the_catalogue_marks_only_what_is_held(
    router: &axum::Router,
    fixture: &Fixture,
) {
    let listed = send(
        router,
        Method::GET,
        &fixture.all(),
        Some(&fixture.token),
        "",
    )
    .await;
    let status = listed.status();
    let document = json_body(listed).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    let rows = document.as_array().expect("the catalogue is a bare array");
    assert_eq!(
        rows.len(),
        Provider::ALL.len(),
        "every shipped connector gets a card, held or not: {document}"
    );
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .expect("a row names its provider");
        let connected = row.get("connected").and_then(Value::as_bool);
        assert_eq!(
            connected,
            Some(id == HELD.id()),
            "`{id}` is connected exactly when its handle is held: {document}"
        );
    }
}

/// A disconnect removes the handle AND the routing row, and a second press is
/// still 204.
///
/// The row is the half a vault read cannot see: an ingress resolves the
/// provider account to a workspace through `core.connector_installs`, so a
/// disconnect that removed only the handle would leave deliveries routing to a
/// workspace that can no longer answer them. Nothing here reaches a vendor —
/// the disconnect path holds no HTTP client — which is what "leaves the
/// external app alone" means in code rather than in prose.
pub(super) async fn a_disconnect_removes_the_handle_and_repeats_harmlessly(
    router: &axum::Router,
    fixture: &Fixture,
) {
    assert_eq!(
        fixture.routed(HELD, ROUTED_ACCOUNT).await,
        1,
        "the fixture routes the held account before the disconnect"
    );
    let gone = send(
        router,
        Method::DELETE,
        &fixture.one(HELD),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        fixture.routed(HELD, ROUTED_ACCOUNT).await,
        0,
        "the reverse-routing row went with the handle"
    );

    let read = send(
        router,
        Method::GET,
        &fixture.one(HELD),
        Some(&fixture.token),
        "",
    )
    .await;
    let document = json_body(read).await;
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some(STATUS_NOT_CONNECTED),
        "the handle the disconnect removed must not still read: {document}"
    );

    // Idempotent in the way a delete is asked to be. A 404 for the second press
    // would make a person believe their first one had failed.
    let again = send(
        router,
        Method::DELETE,
        &fixture.one(HELD),
        Some(&fixture.token),
        "",
    )
    .await;
    assert_eq!(again.status(), StatusCode::NO_CONTENT);
}

/// A workspace secret sharing a provider's name is not a connection.
pub(super) async fn a_secret_that_is_not_a_connector_handle_is_not_a_connection(
    router: &axum::Router,
    fixture: &Fixture,
) {
    fixture
        .seal_handle(HELD, r#"{"note":"an ordinary workspace secret"}"#)
        .await;

    let read = send(
        router,
        Method::GET,
        &fixture.one(HELD),
        Some(&fixture.token),
        "",
    )
    .await;
    let document = json_body(read).await;
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some(STATUS_NOT_CONNECTED),
        "an envelope with no `integration` marker is somebody's own secret, and \
         offering a disconnect for it would delete something they stored: \
         {document}"
    );
}

/// A stored connector handle, as `land` writes one.
pub(super) fn handle(label: &str) -> String {
    format!(r#"{{"integration":"{}","label":"{label}"}}"#, HELD.id())
}
