//! A tenant's balance can owe: the billing snapshot publishes it as a signed
//! decimal string, in the document and on the wire.
//!
//! A settlement takes its whole charge, so `balance_nanos` falls below zero
//! when a tenant runs past its credit. Every other nanos field is unsigned, so
//! the balance is the one field on `SignedNanos`, and both halves are checked:
//! a field that kept the unsigned component would publish a pattern its own
//! debt breaks.
#![expect(
    clippy::expect_used,
    reason = "a document utoipa just built must serialize; a failure here is the
              generator broken, not a state under test"
)]
#![cfg(all(feature = "test-util", feature = "openapi"))]

use afd_api_wire::tenant::BillingResponse;
use jiff::Timestamp;
use serde_json::{Value, json};

use crate::openapi_contract::document;

/// What a tenant in debt owes in the wire check.
const DEBT: i64 = -50;

#[test]
fn test_the_balance_is_published_as_a_signed_amount() {
    let document = document();
    let schemas = document
        .pointer("/components/schemas")
        .expect("the document publishes its schemas");
    assert_eq!(
        schemas.pointer("/BillingResponse/properties/balance_nanos/$ref"),
        Some(&json!("#/components/schemas/SignedNanos")),
        "the balance names the signed component"
    );
    assert_eq!(
        schemas.pointer("/SignedNanos/pattern"),
        Some(&json!(afd_wire::nanos::signed::PATTERN))
    );
    assert_eq!(
        schemas.pointer("/Nanos/pattern"),
        Some(&json!(afd_wire::nanos::PATTERN)),
        "every other amount stays unsigned"
    );
}

#[test]
fn test_a_debt_is_served_as_a_negative_decimal_string() {
    let written = serde_json::to_value(BillingResponse {
        balance_nanos: DEBT,
        updated_at: Timestamp::UNIX_EPOCH,
        is_exhausted: true,
        exhausted_at: Some(Timestamp::UNIX_EPOCH),
    })
    .expect("a wire response serializes");
    assert_eq!(written.get("balance_nanos"), Some(&Value::from("-50")));
    assert_eq!(written.get("is_exhausted"), Some(&Value::Bool(true)));
}
