//! GET /v1/connectors?workspace_id={ws} — workspace-authed connector catalog.
//!
//! Renders the comptime connector registry as data for the dashboard: every
//! entry with its archetype, display name, whether the platform side is
//! `configured` (oauth2/app_install need a `<provider>-app` bag; api_key needs
//! nothing), and whether THIS workspace is `connected` (a `fleet:<provider>`
//! handle exists). The dashboard renders its cards from this — never a
//! hard-coded provider list (RULE CFG). Read-only; `connector:read`-scoped.

const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const oauth2 = @import("oauth2.zig");
const registry = @import("registry.zig");

const Q_WORKSPACE_ID = "workspace_id";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_MISSING_WORKSPACE = "Missing workspace_id";

/// One catalog row (wire shape). `configured` is platform-global; `connected` is
/// scoped to the requested workspace. No secret material — only status flags.
const CatalogEntry = struct {
    id: []const u8,
    archetype: []const u8,
    display_name: []const u8,
    configured: bool,
    connected: bool,
};

/// RESOURCE BUDGET: innerCatalog
///   Heap allocations: ≤1 per registry entry (the `fleet:<provider>` key + the
///     loaded handle, freed each iteration) — bounded by the comptime registry
///     length, not by request input.
///   Max loop cardinality: registry.REGISTRY.len (comptime-fixed).
///   Concurrency: single request, one pooled conn held for the whole scan.
pub fn innerCatalog(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return;
    };
    const workspace_id = qs.get(Q_WORKSPACE_ID) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MISSING_WORKSPACE);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    var entries: [registry.REGISTRY.len]CatalogEntry = undefined;
    for (registry.REGISTRY, 0..) |spec, i| {
        entries[i] = .{
            .id = spec.provider,
            .archetype = @tagName(spec.archetype),
            .display_name = spec.display_name,
            .configured = isConfigured(hx, conn, spec),
            .connected = isConnected(hx, conn, workspace_id, spec.provider),
        };
    }
    hx.ok(.ok, entries[0..]);
}

/// Platform-side readiness: api_key connectors self-provision (the operator
/// pastes their own key at connect), so they are always `configured`; oauth2 and
/// app_install need the admin-workspace `<provider>-app` platform bag present.
fn isConfigured(hx: hx_mod.Hx, conn: *pg.Conn, spec: registry.ConnectorSpec) bool {
    return switch (spec.archetype) {
        .api_key => true,
        .oauth2, .app_install => {
            var parsed = oauth2.loadAppVaultJson(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.provider) orelse return false;
            parsed.deinit();
            return true;
        },
    };
}

/// Whether this workspace has a `fleet:<provider>` handle vaulted (connected).
/// Any load failure reads as not connected — never fabricates a connected state.
fn isConnected(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8, provider: []const u8) bool {
    const key = credential_key.allocKeyName(hx.alloc, provider) catch return false;
    defer hx.alloc.free(key);
    var parsed = vault.loadJson(hx.alloc, conn, workspace_id, key) catch return false;
    parsed.deinit();
    return true;
}
