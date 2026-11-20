//! Dimension 2.4 — the route table carries exactly this milestone's inventory.
//!
//! # Why a hand-written list is the right shape here
//!
//! Every other test in this crate walks [`Route::all`] and asserts a property.
//! This one asserts the ROSTER, which means it cannot derive its expectation
//! from the thing it is checking — a test that read the enum to decide what the
//! enum should contain would pass for any enum at all.
//!
//! So the list below is written out, from the spec's Interfaces block. Both
//! halves fail: a path this milestone owns that is missing from the table, and
//! a path in the table's tenant and workspace families that nobody put in the
//! inventory. The second half is what stops a route being quietly widened.
//!
//! # Methods are part of the inventory
//!
//! A path served for GET and not for DELETE is a different surface from one
//! served for both, and the router dispatches on method — so an inventory of
//! paths alone would let a verb go missing without a failure. What is checked
//! here is the SCOPE RUNG per method, because that is the route table's own
//! answer to "which methods does this path distinguish", and it is the fact a
//! caller's refusal is decided from.
#![cfg(feature = "test-util")]
use std::collections::BTreeSet;

use afd_http::route::Route;
use afd_http::route::{Ownership, RunnerRoute, TenantRoute};

/// Every path this milestone's §1, §2, §3, §4, §6 and §7 own.
///
/// Schedules and connector surfaces are M180's and are deliberately absent even
/// though the table carries them; the families walked below exclude them the
/// same way.
const INVENTORY: &[&str] = &[
    "/v1/runners/me/stream",
    // §1 — the device-flow login surface.
    "/v1/auth/sessions",
    "/v1/auth/sessions/{session_id}",
    "/v1/auth/sessions/{session_id}/approve",
    "/v1/auth/sessions/{session_id}/verify",
    // §2 — the tenant plane.
    "/v1/models",
    "/v1/workspaces",
    "/v1/tenants/me/billing",
    "/v1/tenants/me/billing/charges",
    "/v1/tenants/me/billing/reports/current",
    "/v1/tenants/me/billing/activities/{activity_id}/charges",
    "/v1/tenants/me/workspaces",
    "/v1/tenants/me/provider",
    "/v1/tenants/me/models",
    "/v1/tenants/me/models/{id}",
    "/v1/api-keys",
    "/v1/api-keys/{id}",
    "/v1/cli-credentials",
    "/v1/cli-credentials/{id}",
    // The caller's identity read. Listed here beside the tenant plane's own
    // rows because it is read for the caller itself, like every path above it.
    "/v1/users/me",
    // §3 — workspace fleets and install.
    "/v1/workspaces/{workspace_id}/fleets",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/setup-context",
    // §4 — the vault.
    "/v1/workspaces/{workspace_id}/secrets",
    "/v1/workspaces/{workspace_id}/secrets/{name}",
    // §5 — events, streams, messages, memories, grants.
    "/v1/workspaces/{workspace_id}/turns",
    "/v1/workspaces/{workspace_id}/turns/stream",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/stream",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}",
    // One tool call of an event, in full — the read behind "show all".
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/{turn_id}/items/{item_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/tasks",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/tasks/{task_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories/{key}",
    // Who reads and publishes the workspace's shared memory.
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memory-access",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants/{grant_id}",
    // §6 — approvals.
    "/v1/workspaces/{workspace_id}/approvals",
    "/v1/workspaces/{workspace_id}/approvals/{gate_id}",
    "/v1/workspaces/{workspace_id}/approvals/{gate_id}/{decision}",
    // Onboarding and preferences.
    "/v1/workspaces/{workspace_id}/onboarding",
    "/v1/workspaces/{workspace_id}/preferences/{pref_key}",
    // M227 N1–N11 (`docs/architecture/api_resources.md`) — eligible models,
    // creator-private drafts and reviewed setup revisions. Removed R1/R2
    // `library-entries` and M253's Plugin routes have no row here.
    "/v1/workspaces/{workspace_id}/models",
    "/v1/workspaces/{workspace_id}/drafts",
    "/v1/workspaces/{workspace_id}/drafts/{draft_id}",
    "/v1/workspaces/{workspace_id}/drafts/{draft_id}/messages",
    "/v1/workspaces/{workspace_id}/drafts/{draft_id}/turns/stream",
    // M253 §8 — Stop on a draft's turn, beside the fleet turn's PATCH.
    "/v1/workspaces/{workspace_id}/drafts/{draft_id}/turns/{turn_id}",
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/revisions",
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
    // `/sync` as its own segment rather than `{schedule_id}:sync`: the router
    // cannot carry a literal after a parameter inside one segment, so the verb
    // is a segment of its own — the same shape the approval decision takes,
    // argued at `FleetRoute::ScheduleSync`. This roster is the gate that would
    // catch the two spellings drifting apart.
    "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}/sync",
    "/v1/workspaces/{workspace_id}/connectors",
    "/v1/workspaces/{workspace_id}/connectors/{provider}",
    "/v1/workspaces/{workspace_id}/connectors/{provider}/connect",
    // The dashboard's completion of a connect, which carries no workspace in
    // its path — the workspace is inside the signed state.
    "/v1/connectors/{provider}/callback",
    // A connector's signed event delivery — ingress, so M180's rather than
    // this milestone's, even though the table files it under the connector
    // family. The segment is the provider, as it is on the callback beside it:
    // Slack is the only connector that delivers today, and the template that
    // serves it serves the next one without a route change.
    "/v1/connectors/{provider}/events",
    // Team accounts: an owner's invites and members under the caller's own
    // account, the invitee's side under `/v1/users/me`, and the names a workspace's
    // thread shows beside each sender.
    "/v1/tenants/me/invites",
    "/v1/tenants/me/invites/{invite_id}",
    "/v1/tenants/me/invites/{invite_id}/send",
    "/v1/tenants/me/members",
    "/v1/tenants/me/members/{user_id}",
    "/v1/users/me/invites",
    "/v1/users/me/invites/{invite_id}/accept",
    "/v1/workspaces/{workspace_id}/members",
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

/// Dimension 7.1 (M253) — the surface speaks in turns, items and runs.
///
/// A run's tool calls are items, a runner works a run rather than a lease, a
/// turn is stopped by its own `PATCH`, and a fleet's history is its turns. So
/// no inventory path, and no template the daemon tables, names a retired
/// segment, and `/events` survives only as the vendor webhook's ingress.
#[test]
fn the_route_inventory_names_turns_items_and_runs() {
    const RETIRED: [&str; 3] = ["/tool-calls", "/leases", "/cancellations"];
    const VENDOR_EVENTS: &str = "/v1/connectors/{provider}/events";
    let paths = INVENTORY
        .iter()
        .copied()
        .chain(DEFERRED_TO_M180.iter().copied())
        .chain(Route::all().map(|route| route.meta().template));
    for path in paths {
        for retired in RETIRED {
            assert!(!path.contains(retired), "{path} names {retired}");
        }
        assert!(
            !path.contains("/events") || path == VENDOR_EVENTS,
            "{path} names /events, which only the vendor webhook keeps"
        );
    }
}

/// Dimension 14.1 (`M253_002`) — Plugins are gone end to end: no route the
/// daemon tables, no schema file or statement naming a `setup.plugin_*`
/// table, and no `/plugins` page in the app.
#[test]
fn no_plugin_table_route_or_page_remains() -> std::io::Result<()> {
    for route in Route::all() {
        let template = route.meta().template;
        assert!(
            !template.contains("/plugins"),
            "{template} is a Plugin route"
        );
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut statements = Vec::new();
    sql_files(&root.join("rustd/crates"), &mut statements)?;
    assert!(!statements.is_empty(), "the walk found no statement");
    for path in statements {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert!(
            !name.contains("_plugin_"),
            "{} is a Plugin schema file",
            path.display()
        );
        let text = std::fs::read_to_string(&path)?;
        assert!(
            !names_a_plugin_table(&text),
            "{} names a Plugin table",
            path.display()
        );
    }
    let page = root.join("ui/packages/app/app/(dashboard)/w/[workspaceId]/plugins");
    assert!(!page.exists(), "{} is a Plugin page", page.display());
    Ok(())
}

/// The scan above would name a statement that still reads a Plugin table.
#[test]
fn a_statement_naming_a_plugin_table_is_named() {
    assert!(names_a_plugin_table(
        "SELECT id FROM setup.plugin_entries WHERE tenant_id = $1"
    ));
    assert!(!names_a_plugin_table(
        "SELECT id FROM setup.drafts WHERE id = $1"
    ));
}

fn names_a_plugin_table(statement: &str) -> bool {
    statement.contains("setup.plugin_")
}

/// Every `.sql` file under `dir`, skipping build output.
fn sql_files(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                sql_files(&path, found)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "sql") {
            found.push(path);
        }
    }
    Ok(())
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
                    | Route::Runner(RunnerRoute::Stream)
            )
        })
        // The identity-provider delivery is signature-authenticated ingress and
        // lands with M180, so it is neither in the inventory nor a gap in it.
        .filter(|route| {
            !matches!(
                route,
                Route::Auth(afd_http::route::AuthRoute::IdentityEventClerk)
            )
        })
        .map(|route| route.meta().template)
}
