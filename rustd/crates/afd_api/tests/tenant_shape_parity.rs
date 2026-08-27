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

use afd_wire::models::{CatalogueModel, CatalogueResponse};
use afd_wire::tenant::{
    ApiKeySummary, BillingResponse, ChargeSummary, ChargesResponse, MintedApiKeyResponse,
    MintedCliCredentialResponse, PageResponse, RevokedApiKeyResponse,
};
use afd_wire::workspace::{CreatedWorkspaceResponse, WorkspaceSummary, WorkspacesResponse};
use serde::Serialize;
use serde_json::Value;

/// A fixture string, for the shapes that carry text.
const TEXT: &str = "fixture";

/// A fixture instant, for the shapes that carry one.
const WHEN: i64 = 1_760_000_000_000;

/// The keys `value` serialises to, in the order it emits them.
///
/// Order as well as membership: `res.json` emits declaration order, and a
/// dashboard that reads a response as an ordered list of columns would feel a
/// reordering that a set comparison would call identical.
fn keys_of<T: Serialize>(value: &T) -> Vec<String> {
    serde_json::to_value(value)
        .expect("a wire shape serialises")
        .as_object()
        .expect("every response shape is a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// Asserts `value` emits exactly `expected`, in order.
fn assert_shape<T: Serialize>(value: &T, shape: &str, expected: &[&str]) {
    assert_eq!(
        keys_of(value),
        expected,
        "{shape}: the emitted key set is the wire contract"
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

#[test]
fn a_charge_row_carries_its_whole_provenance() {
    assert_shape(
        &ChargeSummary {
            id: Cow::Borrowed(TEXT),
            tenant_id: Cow::Borrowed(TEXT),
            workspace_id: Some(Cow::Borrowed(TEXT)),
            fleet_id: Some(Cow::Borrowed(TEXT)),
            event_id: Cow::Borrowed(TEXT),
            charge_type: Cow::Borrowed(TEXT),
            posture: Cow::Borrowed(TEXT),
            model: Cow::Borrowed(TEXT),
            credit_deducted_nanos: 0,
            token_count_input: Some(0),
            token_count_output: Some(0),
            wall_ms: Some(0),
            recorded_at: WHEN,
        },
        "ChargeSummary",
        &[
            "id",
            "tenant_id",
            "workspace_id",
            "fleet_id",
            "event_id",
            "charge_type",
            "posture",
            "model",
            "credit_deducted_nanos",
            "token_count_input",
            "token_count_output",
            "wall_ms",
            "recorded_at",
        ],
    );
}

#[test]
fn a_created_workspace_answers_its_own_request_id() {
    assert_shape(
        &CreatedWorkspaceResponse {
            workspace_id: Cow::Borrowed(TEXT),
            name: Cow::Borrowed(TEXT),
            request_id: Cow::Borrowed(TEXT),
            tenant_id: Cow::Borrowed(TEXT),
        },
        "CreatedWorkspaceResponse",
        &["workspace_id", "name", "request_id", "tenant_id"],
    );
}

#[test]
fn a_workspace_summary_is_three_fields_and_stays_three() {
    assert_shape(
        &WorkspaceSummary {
            id: Cow::Borrowed(TEXT),
            name: Some(Cow::Borrowed(TEXT)),
            created_at: WHEN,
        },
        "WorkspaceSummary",
        &["id", "name", "created_at"],
    );
}

#[test]
fn a_catalogue_model_carries_all_three_rates() {
    // Three rates, not one: cached input is priced at a fraction of fresh, and
    // a client computing a cost estimate needs each separately. A shape that
    // lost one would silently price cached tokens as fresh.
    assert_shape(
        &CatalogueModel {
            id: Cow::Borrowed(TEXT),
            provider: Cow::Borrowed(TEXT),
            context_cap_tokens: 0,
            input_nanos_per_mtok: 0,
            cached_input_nanos_per_mtok: 0,
            output_nanos_per_mtok: 0,
        },
        "CatalogueModel",
        &[
            "id",
            "provider",
            "context_cap_tokens",
            "input_nanos_per_mtok",
            "cached_input_nanos_per_mtok",
            "output_nanos_per_mtok",
        ],
    );
}

/// Every paged shape on this surface spells its cursor the same way.
///
/// The property no single-route suite can see: four envelopes, three of them
/// carrying a total and all four carrying `next_cursor`. A page that named its
/// continuation `cursor` or `after` would pass its own route's tests and break
/// a client that walks every collection with one helper.
#[test]
fn every_paged_envelope_spells_its_continuation_the_same_way() {
    let charges = ChargesResponse {
        items: Vec::new(),
        next_cursor: None,
    };
    let workspaces = WorkspacesResponse {
        items: Vec::new(),
        tenant_id: Cow::Borrowed(TEXT),
        total: Some(0),
        next_cursor: None,
    };
    let catalogue = CatalogueResponse {
        version: Cow::Borrowed(TEXT),
        models: Vec::new(),
        total: Some(0),
        next_cursor: None,
    };
    let page: PageResponse<'_, ApiKeySummary<'_>> = PageResponse {
        items: Vec::new(),
        total: 0,
        next_cursor: None,
    };

    for (shape, keys) in [
        ("ChargesResponse", keys_of(&charges)),
        ("WorkspacesResponse", keys_of(&workspaces)),
        ("CatalogueResponse", keys_of(&catalogue)),
        ("PageResponse", keys_of(&page)),
    ] {
        assert!(
            keys.iter().any(|key| key == "next_cursor"),
            "{shape}: every paged envelope continues through `next_cursor`"
        );
    }

    // And the exhausted page says so with an explicit null rather than by
    // dropping the field — the same rule as every other optional here.
    let document = serde_json::to_value(&workspaces).expect("a wire shape serialises");
    assert_eq!(
        document.get("next_cursor"),
        Some(&Value::Null),
        "a last page carries a null cursor, not an absent one"
    );
}

/// The collection envelopes name their rows for what the route returns.
///
/// `ChargesResponse` and `PageResponse` say `items`; the catalogue says
/// `models`. That inconsistency is the Zig surface's and is kept deliberately —
/// renaming one would break a shipped client — so it is pinned rather than
/// quietly harmonised.
#[test]
fn the_row_field_keeps_each_envelopes_own_spelling() {
    let catalogue = CatalogueResponse {
        version: Cow::Borrowed(TEXT),
        models: Vec::new(),
        total: Some(0),
        next_cursor: None,
    };
    assert_shape(
        &catalogue,
        "CatalogueResponse",
        &["version", "models", "total", "next_cursor"],
    );

    let charges = ChargesResponse {
        items: Vec::new(),
        next_cursor: None,
    };
    assert_shape(&charges, "ChargesResponse", &["items", "next_cursor"]);

    let workspaces = WorkspacesResponse {
        items: Vec::new(),
        tenant_id: Cow::Borrowed(TEXT),
        total: Some(0),
        next_cursor: None,
    };
    assert_shape(
        &workspaces,
        "WorkspacesResponse",
        &["items", "tenant_id", "total", "next_cursor"],
    );
}
