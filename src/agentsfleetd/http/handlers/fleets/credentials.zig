// Workspace credential API handlers.
//
// POST   /v1/workspaces/{ws}/credentials             → innerStoreCredential
// GET    /v1/workspaces/{ws}/credentials             → innerListCredentials
// PATCH  /v1/workspaces/{ws}/credentials/{name}      → innerRotateCredential
// DELETE /v1/workspaces/{ws}/credentials/{name}      → innerDeleteCredential

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const credential_list = @import("credential_list.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const vault = @import("../../../state/vault.zig");
const credential_key = @import("../../../fleet_runtime/credential_key.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.fleet_credentials_api);
const API_ACTOR = "api";

pub const Context = common.Context;

const S_API_KEY = "api_key";

const MAX_CREDENTIAL_DATA_LEN: usize = 4 * 1024; // 4KB stringified JSON
const MAX_CREDENTIAL_NAME_LEN: usize = 64;

// ── Store Credential ──────────────────────────────────────────────────

// workspace_id comes from URL path; body is `{name, data: <JSON-object>}`.
const CredentialBody = struct {
    name: []const u8,
    data: std.json.Value,
};

pub fn innerStoreCredential(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const parsed = std.json.parseFromSlice(CredentialBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const cred = parsed.value;

    if (!validateCredentialName(hx, cred.name)) return;
    vault.validateObject(cred.data) catch {
        hx.fail(ec.ERR_VAULT_DATA_INVALID, ec.MSG_CREDENTIAL_DATA_REQUIRED);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Credential endpoints require operator-minimum role.
    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    storeCredentialJsonOnConn(conn, hx.alloc, workspace_id, cred) catch |err| switch (err) {
        error.DataTooLarge => {
            hx.fail(ec.ERR_VAULT_DATA_TOO_LARGE, ec.MSG_CREDENTIAL_DATA_TOO_LARGE);
            return;
        },
        else => {
            log.err("store_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .name = cred.name, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };

    log.debug("stored", .{ .name = cred.name, .workspace = workspace_id });
    hx.ok(.created, .{ .name = cred.name });
}

fn validateCredentialName(hx: hx_mod.Hx, name: []const u8) bool {
    if (name.len == 0 or name.len > MAX_CREDENTIAL_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_NAME_REQUIRED);
        return false;
    }
    return true;
}

fn storeCredentialJsonOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cred: CredentialBody,
) !void {
    // Stringify once: serves both the size pre-flight (so the API surfaces a
    // precise 400 rather than letting the DB layer truncate) and the bytes
    // we hand to the vault envelope. innerStoreCredential already ran
    // vault.validateObject on cred.data, so the JSON shape is known good.
    const plaintext = try std.json.Stringify.valueAlloc(alloc, cred.data, .{});
    defer alloc.free(plaintext);
    if (plaintext.len > MAX_CREDENTIAL_DATA_LEN) return error.DataTooLarge;

    const key_name = try credential_key.allocKeyName(alloc, cred.name);
    defer alloc.free(key_name);
    try vault.storeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext);
}

// ── Delete Credential ─────────────────────────────────────────────────

pub fn innerDeleteCredential(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    credential_name: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!validateCredentialName(hx, credential_name)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    const key_name = credential_key.allocKeyName(hx.alloc, credential_name) catch {
        common.internalOperationError(hx.res, "Allocation failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(key_name);

    const removed = vault.deleteCredential(conn, workspace_id, key_name) catch |err| {
        log.err("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .name = credential_name, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    log.info("deleted", .{ .name = credential_name, .workspace = workspace_id, .removed = removed });
    hx.res.status = 204;
}

// ── List Credentials ──────────────────────────────────────────────────

pub fn innerListCredentials(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // RULE BIL: credential endpoints require operator-minimum role.
    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    const creds = credential_list.fetchCredentialListOnConn(conn, hx.alloc, workspace_id) catch |err| {
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    respondCredentialList(hx, creds);
}

/// Serialize the list with null optional fields omitted, so each row carries
/// only its kind's descriptors (the per-kind wire shape the client union and
/// the `integration` CLI consume). hx.ok would emit `provider:null` noise.
fn respondCredentialList(hx: hx_mod.Hx, creds: []const credential_list.CredentialListRow) void {
    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.json(.{ .credentials = creds }, .{ .emit_null_optional_fields = false }) catch {
        common.internalOperationError(hx.res, "Failed to serialize credential list", hx.req_id);
    };
}

// ── Rotate Credential Key (PATCH) ──────────────────────────────────────────

// Replace-key body: only the secret rotates; provider/model/base_url are
// preserved by loading the stored object and swapping a single field.
const RotateBody = struct {
    api_key: []const u8,
};

pub fn innerRotateCredential(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    credential_name: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!validateCredentialName(hx, credential_name)) return;

    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const parsed = std.json.parseFromSlice(RotateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    if (parsed.value.api_key.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_CREDENTIAL_KEY_REQUIRED);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Credential endpoints require operator-minimum role.
    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    rotateCredentialKeyOnConn(conn, hx.alloc, workspace_id, credential_name, parsed.value.api_key) catch |err| switch (err) {
        error.NotFound => {
            hx.fail(ec.ERR_CREDENTIAL_NOT_FOUND, ec.MSG_CREDENTIAL_NOT_FOUND);
            return;
        },
        error.DataTooLarge => {
            hx.fail(ec.ERR_VAULT_DATA_TOO_LARGE, ec.MSG_CREDENTIAL_DATA_TOO_LARGE);
            return;
        },
        else => {
            log.err("rotate_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .name = credential_name, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };

    log.debug("rotated", .{ .name = credential_name, .workspace = workspace_id });
    hx.ok(.ok, .{ .name = credential_name });
}

fn rotateCredentialKeyOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    credential_name: []const u8,
    new_key: []const u8,
) !void {
    const key_name = try credential_key.allocKeyName(alloc, credential_name);
    defer alloc.free(key_name);

    // Load the existing object, swap ONLY api_key, re-store. A missing row
    // surfaces error.NotFound (mapped to 404 by the caller).
    var parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |err| switch (err) {
        error.NotFound => return error.NotFound,
        else => return err,
    };
    defer parsed.deinit();

    // Own a mutable copy of the key so it can be securely zeroed after the
    // re-store; the parse-arena/request-body copies are not wiped.
    const key_copy = try alloc.dupe(u8, new_key);
    defer {
        std.crypto.secureZero(u8, key_copy);
        alloc.free(key_copy);
    }
    // The object map is backed by the parse arena — mutate it with that same
    // allocator so its storage stays single-owner (freed by parsed.deinit()).
    try parsed.value.object.put(parsed.arena.allocator(), S_API_KEY, .{ .string = key_copy });

    const plaintext = try std.json.Stringify.valueAlloc(alloc, parsed.value, .{});
    defer alloc.free(plaintext);
    if (plaintext.len > MAX_CREDENTIAL_DATA_LEN) return error.DataTooLarge;

    try vault.storeJsonPlaintext(alloc, conn, workspace_id, key_name, plaintext);
}
