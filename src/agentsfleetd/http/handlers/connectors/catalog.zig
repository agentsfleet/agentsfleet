//! GET /v1/workspaces/{ws}/connectors — workspace-authed connector catalog.
//!
//! Renders the comptime connector registry as data for the dashboard: every
//! entry with its archetype, display name, whether the platform side is
//! `configured` (oauth2/app_install need a `<provider>-app` bag; api_key needs
//! nothing), and whether THIS workspace is `connected` (a `fleet:<provider>`
//! handle exists). The dashboard renders its cards from this — never a
//! hard-coded provider list (RULE CFG). Read-only; `connector:read`-scoped.

const std = @import("std");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const oauth2 = @import("oauth2.zig");
const api_key = @import("api_key.zig");
const registry = @import("registry.zig");

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_KEY_ALLOC_FAILED = "catalog key alloc failed";

/// One catalog row (wire shape). `configured` is platform-global; `connected` is
/// scoped to the requested workspace. No secret material — only status flags and
/// (for api_key connectors) the input-field schema the dashboard's connect form
/// renders from. `api_key.Field` is exactly the `{name, secret}` wire shape, so
/// it serializes verbatim — keep it wire-clean if it ever gains a field.
const CatalogEntry = struct {
    id: []const u8,
    archetype: []const u8,
    display_name: []const u8,
    configured: bool,
    connected: bool,
    /// Declared input fields for the api_key connect form (which secrets/plain
    /// values the operator submits). Empty for oauth2/app_install — those connect
    /// by redirect, not a form. Registry-sourced so the dashboard never hard-codes
    /// a provider's field list.
    fields: []const api_key.Field,
};

const N = registry.REGISTRY.len;

/// RESOURCE BUDGET: innerCatalog
///   Heap allocations: 2·N short key strings (the `<provider>-app` + the
///     `fleet:<provider>` candidates), freed before return; N comptime-fixed.
///   DB round-trips: exactly 2 (one batch existence query per set) — never the
///     ~2·N sequential decrypting `loadJson` reads the naive shape would do.
///   Concurrency: single request, one pooled conn held for the two queries.
pub fn innerCatalog(hx: hx_mod.Hx, workspace_id: []const u8) void {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    // Build the two candidate key sets (index-aligned with the registry).
    var app_keys: [N][]const u8 = undefined; // `<provider>-app` (platform bag)
    var fleet_keys: [N][]const u8 = undefined; // `fleet:<provider>` (workspace handle)
    var made: usize = 0;
    defer for (0..made) |j| {
        hx.alloc.free(app_keys[j]);
        hx.alloc.free(fleet_keys[j]);
    };
    for (registry.REGISTRY, 0..) |spec, i| {
        app_keys[i] = std.fmt.allocPrint(hx.alloc, "{s}" ++ oauth2.APP_VAULT_KEY_SUFFIX, .{spec.provider}) catch {
            common.internalOperationError(hx.res, S_KEY_ALLOC_FAILED, hx.req_id);
            return;
        };
        fleet_keys[i] = credential_key.allocKeyName(hx.alloc, spec.provider) catch {
            hx.alloc.free(app_keys[i]); // this pair is not yet counted in `made`
            common.internalOperationError(hx.res, S_KEY_ALLOC_FAILED, hx.req_id);
            return;
        };
        made = i + 1;
    }

    // Two batch existence checks (no decryption): platform bags in the admin
    // workspace, connected handles in the requested workspace.
    var app_present: [N]bool = undefined;
    var fleet_present: [N]bool = undefined;
    vault.markExisting(conn, hx.ctx.platform_admin_workspace_id, app_keys[0..], app_present[0..]) catch {
        common.internalOperationError(hx.res, "catalog configured lookup failed", hx.req_id);
        return;
    };
    vault.markExisting(conn, workspace_id, fleet_keys[0..], fleet_present[0..]) catch {
        common.internalOperationError(hx.res, "catalog connected lookup failed", hx.req_id);
        return;
    };

    var entries: [N]CatalogEntry = undefined;
    for (registry.REGISTRY, 0..) |spec, i| {
        entries[i] = .{
            .id = spec.provider,
            .archetype = @tagName(spec.archetype),
            .display_name = spec.display_name,
            // api_key connectors self-provision (operator pastes their own key),
            // so they are always configured; oauth2/app_install need the bag.
            .configured = switch (spec.archetype) {
                .api_key => true,
                .oauth2, .app_install => app_present[i],
            },
            .connected = fleet_present[i],
            // Exhaustive by archetype (a new archetype can't land half-wired):
            // only api_key carries a submit form, so only it exposes fields.
            .fields = switch (spec.archetype) {
                .api_key => |d| d.fields,
                .oauth2, .app_install => &.{},
            },
        };
    }
    hx.ok(.ok, entries[0..]);
}
