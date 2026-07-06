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
const registry = @import("registry.zig");

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_CATALOG_KEY_BUILD_FAILED = "Failed to build the connector catalog key";

/// One catalog row (wire shape). `configured` is platform-global; `connected` is
/// scoped to the requested workspace. No secret material — only status flags.
const CatalogEntry = struct {
    id: []const u8,
    archetype: []const u8,
    display_name: []const u8,
    configured: bool,
    connected: bool,
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
            common.internalOperationError(hx.res, S_CATALOG_KEY_BUILD_FAILED, hx.req_id);
            return;
        };
        fleet_keys[i] = credential_key.allocKeyName(hx.alloc, spec.provider) catch {
            hx.alloc.free(app_keys[i]); // this pair is not yet counted in `made`
            common.internalOperationError(hx.res, S_CATALOG_KEY_BUILD_FAILED, hx.req_id);
            return;
        };
        made = i + 1;
    }

    // Two batch existence checks (no decryption): platform bags in the admin
    // workspace, connected handles in the requested workspace.
    var app_present: [N]bool = undefined;
    var fleet_present: [N]bool = undefined;
    // An unconfigured deployment leaves the platform-admin workspace unset (the
    // field defaults to "" — "empty → connectors fail closed"). Binding "" into a
    // `workspace_id UUID` predicate is a hard 22P02 cast error, which would 500
    // every catalog request; instead skip the configured lookup and report every
    // oauth2/app_install provider as not-configured (mirrors serve_broker's
    // empty-workspace guard). markExisting normally zeroes present_out, so zero it
    // here for the skipped path.
    @memset(app_present[0..], false);
    if (hx.ctx.platform_admin_workspace_id.len != 0) {
        vault.markExisting(conn, hx.ctx.platform_admin_workspace_id, app_keys[0..], app_present[0..]) catch {
            common.errorResponse(hx.res, ec.ERR_CONNECTOR_CATALOG_LOOKUP_FAILED, "Failed to check which connectors are configured", hx.req_id);
            return;
        };
    }
    vault.markExisting(conn, workspace_id, fleet_keys[0..], fleet_present[0..]) catch {
        common.errorResponse(hx.res, ec.ERR_CONNECTOR_CATALOG_LOOKUP_FAILED, "Failed to check which connectors are connected", hx.req_id);
        return;
    };

    var entries: [N]CatalogEntry = undefined;
    for (registry.REGISTRY, 0..) |spec, i| {
        entries[i] = .{
            .id = spec.provider,
            .archetype = @tagName(spec.archetype),
            .display_name = spec.display_name,
            // Both archetypes are platform-provisioned: connectable only once the
            // deployment holds the `<provider>-app` bag.
            .configured = app_present[i],
            .connected = fleet_present[i],
        };
    }
    hx.ok(.ok, entries[0..]);
}
