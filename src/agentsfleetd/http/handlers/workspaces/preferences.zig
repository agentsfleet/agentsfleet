//! /v1/workspaces/{workspace_id}/preferences — per-user dashboard preferences.
//!
//! GET  returns the caller's whole bag for this workspace; an unset bag is
//!      `{"prefs":{}}`, never a 404 — the dashboard fails open toward showing
//!      onboarding, so "I could not read your prefs" must look exactly like
//!      "you have not set any".
//! PUT  …/preferences/{pref_key} upserts one key. The client supplies the key on
//!      the path, so an unknown key is refused at the path (UZ-PREFS-001) and
//!      an oversize value at the body (UZ-PREFS-002). Last-write-wins per key.
//!
//! The stored value is opaque JSON the server never interprets — it is parsed
//! only to prove it is well-formed on write, and re-emitted verbatim on read.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const prefs_store = @import("../../../state/user_preferences.zig");

const Hx = hx_mod.Hx;

const log = logging.scoped(.http_workspace_preferences);

const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_USER_CONTEXT_REQUIRED = "User context required";

pub fn innerGetPreferences(hx: Hx, workspace_id: []const u8) void {
    const ctx = open(hx, workspace_id) orelse return;
    defer hx.ctx.pool.release(ctx.conn);

    respondWithBag(hx, ctx, workspace_id);
}

pub fn innerPutPreference(hx: Hx, req: *httpz.Request, workspace_id: []const u8, pref_key: []const u8) void {
    const key = prefs_store.PrefKey.fromWire(pref_key) orelse {
        hx.fail(ec.ERR_PREF_KEY_UNKNOWN, "pref_key is not a known preference");
        return;
    };

    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    if (body.len > prefs_store.MAX_PREF_VALUE_BYTES) {
        hx.fail(ec.ERR_PREF_VALUE_TOO_LARGE, "pref value exceeds the 1 KiB limit");
        return;
    }
    // Parsed only to reject malformed input at the boundary; the text itself is
    // what we store, so a value always round-trips byte-for-byte.
    if (!isWellFormedJson(hx.alloc, body)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    }

    const ctx = open(hx, workspace_id) orelse return;
    defer hx.ctx.pool.release(ctx.conn);

    prefs_store.upsert(hx.alloc, ctx.conn, ctx.user_id, workspace_id, key, body) catch |err| {
        log.err("upsert_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .pref_key = key.wire(),
            .err = @errorName(err),
        });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    respondWithBag(hx, ctx, workspace_id);
}

/// Everything both methods need before touching prefs: a validated workspace,
/// an authorized principal, a live connection, and the internal user id the
/// Clerk subject maps to. Null means a response was already written.
const Opened = struct {
    conn: *pg.Conn,
    user_id: []const u8,
};

fn open(hx: Hx, workspace_id: []const u8) ?Opened {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return null;
    }
    const subject = hx.principal.user_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_USER_CONTEXT_REQUIRED);
        return null;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    errdefer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        hx.ctx.pool.release(conn);
        return null;
    }

    const user_id = prefs_store.resolveUserId(hx.alloc, conn, subject) catch |err| {
        log.err("user_lookup_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        hx.ctx.pool.release(conn);
        return null;
    } orelse {
        // Authenticated against a subject with no core.users row: nothing can be
        // keyed to them, and inventing a row here would fork identity ownership
        // away from the signup bootstrap that owns it.
        hx.fail(ec.ERR_FORBIDDEN, S_USER_CONTEXT_REQUIRED);
        hx.ctx.pool.release(conn);
        return null;
    };

    return .{ .conn = conn, .user_id = user_id };
}

fn respondWithBag(hx: Hx, ctx: Opened, workspace_id: []const u8) void {
    const bag = prefs_store.readBag(hx.alloc, ctx.conn, ctx.user_id, workspace_id) catch |err| {
        log.err("read_bag_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err) });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer prefs_store.deinitBag(bag, hx.alloc);

    var map: std.json.ObjectMap = .empty;
    for (bag) |pref| {
        const value = std.json.parseFromSliceLeaky(std.json.Value, hx.alloc, pref.value, .{}) catch {
            // A row only lands through the write path above, which proves the
            // value parses — so a malformed one is corruption, not client input.
            // Drop it rather than fail the read: a bag that cannot be read is a
            // bag that hides onboarding, which is the one thing it must not do.
            log.warn("pref_value_malformed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .pref_key = pref.key });
            continue;
        };
        map.put(hx.alloc, pref.key, value) catch {
            common.internalOperationError(hx.res, "prefs could not be assembled", hx.req_id);
            return;
        };
    }

    hx.ok(.ok, .{ .prefs = std.json.Value{ .object = map } });
}

fn isWellFormedJson(alloc: std.mem.Allocator, body: []const u8) bool {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return false;
    parsed.deinit();
    return true;
}
