//! The tenant plane's charge, workspace, catalogue and paged envelopes, pinned
//! field for field through `tenant_shape_parity.rs`'s helpers — and the one
//! property only a whole-surface suite can see: every page spells its
//! continuation, and an absent value, the same way.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_api_wire::models::{CatalogueModel, CatalogueResponse};
use afd_api_wire::tenant::{ApiKeySummary, ChargeSummary, ChargesResponse, PageResponse};
use afd_api_wire::workspace::{
    CreatedWorkspaceResponse, WorkspaceAccount, WorkspaceSummary, WorkspacesResponse,
};
use serde_json::Value;

use crate::tenant_shape_parity::{TEXT, WHEN, assert_shape, keys_of};

/// A charge names its fleet as a string, and the fields that stayed optional
/// still spell absence as null.
///
/// `assert_shape` pins the key SET, which cannot see this: re-widening
/// `fleet_id` to `Option` emits the very same keys, and a fixture passing
/// `Some(...)` would keep compiling. What changes is the VALUE — `"fixture"`
/// becomes `null` the moment a row lacks one — so the type is pinned where the
/// difference shows.
///
/// Slot 916 made `billing.usage_ledger.fleet_id` `NOT NULL` as part of the
/// accumulate arbiter, so a charge that cannot say which fleet paid is a row
/// the ledger will not hold. The two neighbours are the control: they are
/// seeded absent and MUST still emit null, which is this suite's stated
/// divergence and not something the narrowing was allowed to take with it.
#[test]
fn a_charge_names_its_fleet_and_spells_only_the_others_null() {
    let emitted = serde_json::to_value(ChargeSummary {
        id: Cow::Borrowed(TEXT),
        tenant_id: Cow::Borrowed(TEXT),
        workspace_id: None,
        fleet_id: Cow::Borrowed(TEXT),
        fleet_name: None,
        event_id: Cow::Borrowed(TEXT),
        charge_type: Cow::Borrowed(TEXT),
        posture: Cow::Borrowed(TEXT),
        model: Cow::Borrowed(TEXT),
        credit_deducted_nanos: 0,
        token_count_input: None,
        token_count_output: None,
        wall_ms: None,
        recorded_at: WHEN,
    })
    .expect("a wire shape serialises");

    assert_eq!(
        emitted.get("fleet_id"),
        Some(&Value::String(TEXT.to_owned())),
        "a charge always names the fleet that paid for it"
    );
    for absent in ["workspace_id", "fleet_name"] {
        assert_eq!(
            emitted.get(absent),
            Some(&Value::Null),
            "{absent} is still optional, and absence is still spelled null"
        );
    }
}

#[test]
fn a_charge_row_carries_its_whole_provenance() {
    assert_shape(
        &ChargeSummary {
            id: Cow::Borrowed(TEXT),
            tenant_id: Cow::Borrowed(TEXT),
            workspace_id: Some(Cow::Borrowed(TEXT)),
            fleet_id: Cow::Borrowed(TEXT),
            fleet_name: Some(Cow::Borrowed(TEXT)),
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
            "fleet_name",
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

/// Team accounts added two keys: which account a workspace is in, and the
/// caller's role there. The dashboard groups its switcher by the first.
#[test]
fn a_workspace_summary_names_its_account_and_the_callers_role() {
    assert_shape(
        &WorkspaceSummary {
            id: Cow::Borrowed(TEXT),
            name: Some(Cow::Borrowed(TEXT)),
            created_at: WHEN,
            account: WorkspaceAccount {
                tenant_id: Cow::Borrowed(TEXT),
                owner_name: Cow::Borrowed(TEXT),
            },
            role: Cow::Borrowed(TEXT),
        },
        "WorkspaceSummary",
        &["id", "name", "created_at", "account", "role"],
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
/// `models`. That inconsistency is kept deliberately — renaming one would break
/// a shipped client — so it is pinned rather than quietly harmonised.
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
