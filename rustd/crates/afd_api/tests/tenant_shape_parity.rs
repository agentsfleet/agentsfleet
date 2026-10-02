//! Dimension 2.1 — the tenant plane's response shapes, pinned field for field.
//!
//! # What the oracle is, and what it is not
//!
//! Every shape here was ported from a Zig handler that serialises through
//! `res.json(value, .{})`, so the emitted key set is the Zig struct's field set
//! and the ORDER is its declaration order. This suite pins that key set: a
//! field added, removed or renamed on any tenant response fails here, and the
//! author has to change the pin deliberately.
//!
//! The paged and workspace shapes are `tenant_shape_parity_pages.rs` and the
//! team shapes `tenant_shape_parity_team.rs`; both pin through the helpers
//! here.
//!
//! It does NOT prove the values are right — `tenant_billing.rs`,
//! `tenant_workspaces.rs`, `tenant_models.rs` and `tenant_cli_credential.rs`
//! do that per route, against seeded rows. What only a whole-surface suite can
//! see is the property those four cannot: that the shapes agree with each other
//! about how an absent value and a page boundary are spelled.
//!
//! # Nulls stay on the wire here, and that is the divergence worth pinning
//!
//! std.json emits null optionals by default, so a tenant row always carries the
//! same keys whether or not it has been revoked — a dashboard's
//! `"revoked_at" in row` check can feel the difference. The secret list is the
//! one surface that opts out (`emit_null_optional_fields = false`), which is
//! why the assertion lives here rather than being assumed everywhere.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::tenant::{
    ApiKeySummary, BillingResponse, MintedApiKeyResponse, MintedCliCredentialResponse,
    RevokedApiKeyResponse,
};
use serde::Serialize;
use serde_json::Value;

/// A fixture string, for the shapes that carry text.
pub(crate) const TEXT: &str = "fixture";

/// A fixture instant, for the shapes that carry one.
pub(crate) const WHEN: i64 = 1_760_000_000_000;

/// The keys `value` serialises to, in the order it emits them.
///
/// Order as well as membership: `res.json` emits declaration order, and a
/// dashboard that reads a response as an ordered list of columns would feel a
/// reordering that a set comparison would call identical.
pub(crate) fn keys_of<T: Serialize>(value: &T) -> Vec<String> {
    serde_json::to_value(value)
        .expect("a wire shape serialises")
        .as_object()
        .expect("every response shape is a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// Asserts `value` emits exactly `expected`, in order.
pub(crate) fn assert_shape<T: Serialize>(value: &T, shape: &str, expected: &[&str]) {
    assert_eq!(
        keys_of(value),
        expected,
        "{shape}: the emitted key set is what the wire promises"
    );
}

#[test]
fn a_minted_api_key_emits_the_key_exactly_once_and_nothing_else() {
    // `key` is the raw secret, revealed on the mint and never again — which is
    // why it is on THIS shape and absent from `ApiKeySummary` below. A summary
    // that grew a `key` would be a credential leak the pin catches.
    assert_shape(
        &MintedApiKeyResponse {
            id: Cow::Borrowed(TEXT),
            key_name: Cow::Borrowed(TEXT),
            key: Cow::Borrowed(TEXT),
            created_at: WHEN,
        },
        "MintedApiKeyResponse",
        &["id", "key_name", "key", "created_at"],
    );
}

#[test]
fn an_api_key_summary_never_carries_the_key() {
    let summary = ApiKeySummary {
        id: Cow::Borrowed(TEXT),
        key_name: Cow::Borrowed(TEXT),
        active: true,
        created_at: WHEN,
        last_used_at: None,
        revoked_at: None,
    };
    assert_shape(
        &summary,
        "ApiKeySummary",
        &[
            "id",
            "key_name",
            "active",
            "created_at",
            "last_used_at",
            "revoked_at",
        ],
    );

    // The listing shape must not be able to grow the secret back.
    assert!(
        !keys_of(&summary).iter().any(|key| key == "key"),
        "a listed key is metadata; the secret is revealed once, at the mint"
    );
}

#[test]
fn absent_optionals_stay_on_the_wire_as_null() {
    let never_used = ApiKeySummary {
        id: Cow::Borrowed(TEXT),
        key_name: Cow::Borrowed(TEXT),
        active: true,
        created_at: WHEN,
        last_used_at: None,
        revoked_at: None,
    };
    let document = serde_json::to_value(&never_used).expect("a wire shape serialises");

    // Present AND null, not omitted. A dashboard branching on
    // `"revoked_at" in row` reads the two differently, and std.json's default
    // is what the Zig side emits.
    assert_eq!(document.get("last_used_at"), Some(&Value::Null));
    assert_eq!(document.get("revoked_at"), Some(&Value::Null));
}

#[test]
fn a_revoked_api_key_answers_the_three_fields_that_changed() {
    // Deliberately NOT the whole summary: the revoke answers what it did, so a
    // client that re-renders the row from this response would be rendering a
    // key with no name. It re-reads the list instead.
    assert_shape(
        &RevokedApiKeyResponse {
            id: Cow::Borrowed(TEXT),
            active: false,
            revoked_at: WHEN,
        },
        "RevokedApiKeyResponse",
        &["id", "active", "revoked_at"],
    );
}

#[test]
fn a_minted_command_line_credential_names_its_deployment() {
    assert_shape(
        &MintedCliCredentialResponse {
            id: Cow::Borrowed(TEXT),
            credential: Cow::Borrowed(TEXT),
            machine_name: Cow::Borrowed(TEXT),
            deployment: Cow::Borrowed(TEXT),
        },
        "MintedCliCredentialResponse",
        &["id", "credential", "machine_name", "deployment"],
    );
}

#[test]
fn the_billing_snapshot_carries_both_spellings_of_exhaustion() {
    // `is_exhausted` restates `exhausted_at` as a boolean and BOTH travel:
    // `tenant_billing.zig` emits the pair so a dashboard can branch without a
    // null check. The redundancy is the contract, so the pin protects it from
    // a tidy-up that would drop one.
    assert_shape(
        &BillingResponse {
            balance_nanos: 0,
            updated_at: WHEN,
            is_exhausted: false,
            exhausted_at: None,
        },
        "BillingResponse",
        &[
            "balance_nanos",
            "updated_at",
            "is_exhausted",
            "exhausted_at",
        ],
    );
}
