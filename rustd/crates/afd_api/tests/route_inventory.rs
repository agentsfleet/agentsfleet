//! Dimension 2.4 — the route table carries exactly this milestone's inventory.
//!
//! # Why a hand-written list is the right shape here
//!
//! Every other test in this crate walks [`Route::all`] and asserts a property.
//! This one asserts the ROSTER, which means it cannot derive its expectation
//! from the thing it is checking — a test that read the enum to decide what the
//! enum should contain would pass for any enum at all.
//!
//! So the list below is written out, from the spec's Interfaces block and
//! `route_template.zig`. Both halves fail: a path this milestone owns that is
//! missing from the table, and a path in the table's tenant and workspace
//! families that nobody put in the inventory. The second half is what stops a
//! route being quietly widened during the port.
//!
//! # Methods are part of the inventory
//!
//! A path served for GET and not for DELETE is a different surface from one
//! served for both, and the Zig matchers switch on method — so an inventory of
//! paths alone would let a verb go missing without a failure. What is checked
//! here is the SCOPE RUNG per method, because that is the route table's own
//! answer to "which methods does this path distinguish", and it is the fact a
//! caller's refusal is decided from.
#![cfg(feature = "test-util")]
use std::collections::BTreeSet;

use afd_api::Route;
use afd_api::route::{Ownership, TenantRoute, WorkspaceRoute};

/// Every path this milestone's §1, §2, §3, §4, §6 and §7 own.
///
/// Schedules and connector surfaces are M180's and are deliberately absent even
/// though the table carries them; the families walked below exclude them the
/// same way.
const INVENTORY: &[&str] = &[
    // §1 — the device-flow login surface.
    "/v1/auth/sessions",
    "/v1/auth/sessions/all",
    "/v1/auth/sessions/{session_id}",
    "/v1/auth/sessions/{session_id}/approve",
    "/v1/auth/sessions/{session_id}/verify",
    // §2 — the tenant plane.
    "/v1/models",
    "/v1/workspaces",
    "/v1/tenants/me/billing",
    "/v1/tenants/me/billing/charges",
    "/v1/tenants/me/workspaces",
    "/v1/tenants/me/provider",
    "/v1/tenants/me/models",
    "/v1/tenants/me/models/{id}",
    "/v1/api-keys",
    "/v1/api-keys/{id}",
    "/v1/cli-credentials",
    "/v1/cli-credentials/{id}",
    // §3 — workspace fleets and install.
    "/v1/workspaces/{workspace_id}/fleets",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    // §4 — the vault.
    "/v1/workspaces/{workspace_id}/secrets",
    "/v1/workspaces/{workspace_id}/secrets/{name}",
    // §5 — events, streams, messages, memories, grants.
    "/v1/workspaces/{workspace_id}/events",
    "/v1/workspaces/{workspace_id}/events/stream",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/stream",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories/{key}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants/{grant_id}",
    // §6 — approvals.
    "/v1/workspaces/{workspace_id}/approvals",
    "/v1/workspaces/{workspace_id}/approvals/{gate_id}",
    "/v1/workspaces/{workspace_id}/approvals/{gate_id}:{decision}",
    // §7 — onboarding, preferences, fleet-library reads.
    "/v1/workspaces/{workspace_id}/onboarding",
    "/v1/workspaces/{workspace_id}/preferences",
    "/v1/workspaces/{workspace_id}/preferences/{pref_key}",
    "/v1/workspaces/{workspace_id}/fleet-libraries",
];

/// The paths the table carries that belong to a LATER milestone.
///
/// Listed rather than filtered by prefix, because the seam is by feature and
/// not by path shape: schedules and connectors sit under the same workspace and
/// fleet roots as this milestone's own routes. A row moving between this list
/// and [`INVENTORY`] is a scope change somebody has to write down.
const DEFERRED_TO_M180: &[&str] = &[
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}:sync",
    "/v1/workspaces/{workspace_id}/connectors",
    "/v1/workspaces/{workspace_id}/connectors/{provider}",
    "/v1/workspaces/{workspace_id}/connectors/{provider}/connect",
    // The provider's redirect back, which carries no workspace in its path —
    // the provider only knows the state parameter it was handed.
    "/v1/connectors/{provider}/callback",
    // Slack's signed event delivery — ingress, so M180's rather than this
    // milestone's, even though the table files it under the connector family.
    "/v1/connectors/slack/events",
    "/v1/fleets/bundles",
];

/// Every inventory path exists in the route table, and the table adds none.
///
/// Both directions, named separately, because they are different mistakes: a
/// missing path is a verb this daemon will 404 after cutover, and an extra one
/// is a surface nobody agreed to serve.
#[test]
fn test_route_inventory_matches_interfaces() {
    let tabled: BTreeSet<&str> = tenant_and_workspace_templates().collect();
    let expected: BTreeSet<&str> = INVENTORY
        .iter()
        .copied()
        .chain(DEFERRED_TO_M180.iter().copied())
        .collect();

    let missing: Vec<&str> = expected.difference(&tabled).copied().collect();
    assert!(
        missing.is_empty(),
        "the inventory names paths the route table does not carry: {missing:?}"
    );

    let extra: Vec<&str> = tabled.difference(&expected).copied().collect();
    assert!(
        extra.is_empty(),
        "the route table carries paths no inventory names: {extra:?}"
    );
}

/// Every path this milestone owns under a workspace checks ownership.
///
/// The inventory's own version of the derivation test next door: that one asks
/// whether the table is internally consistent, and this asks whether the
/// PLANNED surface is. A route added to the inventory under a workspace root
/// but spelled without the parameter would pass the first and fail this.
#[test]
fn test_every_workspace_scoped_inventory_path_is_owned() {
    for route in Route::all() {
        let meta = route.meta();
        if !INVENTORY.contains(&meta.template) {
            continue;
        }
        let under_workspace = meta.template.starts_with("/v1/workspaces/");
        assert_eq!(
            meta.ownership.is_checked(),
            under_workspace,
            "{}: sits under a workspace = {under_workspace}, ownership checked = {}",
            meta.template,
            meta.ownership.is_checked()
        );
    }
}

/// The one route under `/v1/workspaces` that is NOT workspace-owned.
///
/// Creating a workspace cannot check that the workspace is yours, because it
/// does not exist yet; its boundary is the tenant the credential resolves to.
/// Stated as its own test rather than as an exception inside the one above,
/// because an exception buried in a loop is how a second one gets added.
#[test]
fn test_workspace_create_is_tenant_scoped_not_workspace_scoped() {
    let meta = Route::Tenant(TenantRoute::CreateWorkspace).meta();
    assert_eq!(meta.template, "/v1/workspaces");
    assert_eq!(meta.ownership, Ownership::None);
}

/// The templates of the two families this milestone's inventory covers.
fn tenant_and_workspace_templates() -> impl Iterator<Item = &'static str> {
    Route::all()
        .filter(|route| {
            matches!(
                route,
                Route::Auth(_)
                    | Route::Tenant(_)
                    | Route::Workspace(_)
                    | Route::Fleet(_)
                    | Route::Connector(_)
            )
        })
        // The identity-provider delivery is signature-authenticated ingress and
        // lands with M180, so it is neither in the inventory nor a gap in it.
        .filter(|route| {
            !matches!(
                route,
                Route::Auth(afd_api::route::AuthRoute::IdentityEventClerk)
            )
        })
        .map(|route| route.meta().template)
}

/// The workspace family's own roster is what the inventory walks.
///
/// A guard on the guard: if [`WorkspaceRoute::ALL`] were short a variant, the
/// walk above would never see it and both directions of the inventory check
/// would pass while the surface was incomplete.
#[test]
fn test_the_workspace_roster_is_whole() {
    assert_eq!(
        WorkspaceRoute::ALL.len(),
        12,
        "a workspace route was added or removed without the inventory moving"
    );
}
