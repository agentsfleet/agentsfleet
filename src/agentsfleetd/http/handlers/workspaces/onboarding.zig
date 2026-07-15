//! GET /v1/workspaces/{workspace_id}/onboarding — the whole checklist state in
//! one call.
//!
//! Consolidates what the dashboard used to fetch as six separate requests: the
//! five derivable signals (fleet, secret, processed event, steer event, model
//! configured) come from one query in the onboarding store, and the three UI
//! preferences (dismissed, collapsed, cli_ticked) come from the preferences bag
//! on the same connection. One HTTP call, one auth, one connection.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const onboarding = @import("../../../state/workspace_onboarding.zig");
const prefs_store = @import("../../../state/user_preferences.zig");

const Hx = hx_mod.Hx;

const log = logging.scoped(.http_workspace_onboarding);

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_USER_CONTEXT_REQUIRED = "User context required";
const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";

pub fn innerGetOnboarding(hx: Hx, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const subject = hx.principal.user_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_USER_CONTEXT_REQUIRED);
        return;
    };
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }

    const signals = onboarding.read(hx.alloc, conn, workspace_id, tenant_id) catch |err| {
        log.err("signals_read_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    const view = readPrefs(hx, conn, subject, workspace_id, signals) catch |err| {
        log.err("prefs_read_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse return;

    hx.ok(.ok, view);
}

const OnboardingView = struct {
    model_configured: bool,
    has_fleet: bool,
    has_secret: bool,
    has_processed_event: bool,
    has_steer_event: bool,
    cli_ticked: bool,
    dismissed: bool,
    collapsed: bool,
};

// Reads the preference bag and assembles the response. Returns null (after
// writing a response) when the Clerk subject has no user row — the same
// fail-closed shape the preferences handler uses.
fn readPrefs(
    hx: Hx,
    conn: *pg.Conn,
    subject: []const u8,
    workspace_id: []const u8,
    signals: onboarding.Signals,
) !?OnboardingView {
    const user_id = (try prefs_store.resolveUserId(hx.alloc, conn, subject)) orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_USER_CONTEXT_REQUIRED);
        return null;
    };
    defer hx.alloc.free(user_id);

    const bag = try prefs_store.readBag(hx.alloc, conn, user_id, workspace_id);
    defer prefs_store.deinitBag(bag, hx.alloc);

    return .{
        .model_configured = signals.model_configured,
        .has_fleet = signals.has_fleet,
        .has_secret = signals.has_secret,
        .has_processed_event = signals.has_processed_event,
        .has_steer_event = signals.has_steer_event,
        .cli_ticked = bagTrue(bag, prefs_store.PrefKey.getting_started_cli_ticked),
        .dismissed = bagTrue(bag, prefs_store.PrefKey.getting_started_dismissed),
        .collapsed = bagTrue(bag, prefs_store.PrefKey.getting_started_collapsed),
    };
}

// A preference is truthy only when its row holds the JSON literal `true`. The
// value is stored verbatim, so an exact match is the whole test.
fn bagTrue(bag: []prefs_store.Pref, key: prefs_store.PrefKey) bool {
    for (bag) |pref| {
        if (std.mem.eql(u8, pref.key, key.wire())) {
            return std.mem.eql(u8, std.mem.trim(u8, pref.value, " "), "true");
        }
    }
    return false;
}
