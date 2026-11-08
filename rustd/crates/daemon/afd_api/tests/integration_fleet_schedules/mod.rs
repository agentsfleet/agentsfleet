//! The schedules CRUD surface, over the migrated schema.
//!
//! # Two behaviours here read as bugs until you know why
//!
//! **A create answers 201 even when the external scheduler refused.** The row
//! is saved either way and the answer carries the sync state, so a caller can
//! see which happened. Answering 502 would tell a person their schedule was not
//! created when it was — and the next reconcile would then repair a schedule
//! they believe does not exist.
//!
//! **A delete does not delete.** It sets `desired_status = deleting` and pushes;
//! the row goes only once the scheduler confirms. A row removed first would
//! leave a schedule firing at a callback this daemon can no longer resolve to a
//! fleet.
//!
//! This suite is where both get asserted, because both are exactly the kind of
//! thing a later reader "fixes".
//!
//! # The scheduler is unreachable here, and that is the interesting state
//!
//! `Fleet::live` gives a real Postgres and a scheduler nothing resolves — which
//! is precisely the create-saved-but-not-registered case above. What this suite
//! cannot reach is a create that registered upstream, and that needs a vendor.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use http::{Method, StatusCode};
use serde_json::Value;

use self::fixture::Fixture;
use crate::harness::{json_body, send};

mod ceiling;
mod fixture;
mod lifecycle;
mod refusals;

/// An expression this daemon registers.
const NIGHTLY: &str = "0 3 * * *";

/// One create attempt, as a status and its document.
async fn create_with(router: &axum::Router, fixture: &Fixture, body: Value) -> (StatusCode, Value) {
    let response = send(
        router,
        Method::POST,
        &fixture.collection(),
        Some(&fixture.token),
        &body.to_string(),
    )
    .await;
    let status = response.status();
    (status, json_body(response).await)
}

/// One string field of a document, or the empty string where it is absent.
fn field<'d>(document: &'d Value, name: &str) -> &'d str {
    document
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The rows a page carries.
fn schedules(page: &Value) -> Vec<Value> {
    page.get("schedules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}
