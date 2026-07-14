// Workspace secret API handlers.
//
// POST   /v1/workspaces/{ws}/secrets             → innerStoreSecret
// GET    /v1/workspaces/{ws}/secrets             → innerListSecrets
// PATCH  /v1/workspaces/{ws}/secrets/{name}      → innerRotateSecret
// DELETE /v1/workspaces/{ws}/secrets/{name}      → innerDeleteSecret

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const secret_list = @import("secret_list.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const vault = @import("../../../state/vault.zig");
const secure_memory = @import("../../../secrets/secure_memory.zig");
const workspace_guards = @import("../../workspace_guards.zig");
const tenant_model_entries = @import("../../../state/tenant_model_entries.zig");

const log = logging.scoped(.fleet_secrets_api);

pub const Context = common.Context;

const S_API_KEY = "api_key";

const MAX_SECRET_DATA_LEN: usize = 4 * 1024; // 4KB stringified JSON
const MAX_SECRET_NAME_LEN: usize = 64;

// ── Store Secret ──────────────────────────────────────────────────

// workspace_id comes from URL path; body is `{name, data: <JSON-object>}`.
const SecretBody = struct {
    name: []const u8,
    data: std.json.Value,
};

pub fn innerStoreSecret(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;

    const parsed = std.json.parseFromSlice(SecretBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const cred = parsed.value;

    if (!validateSecretName(hx, cred.name)) return;
    vault.validateObject(cred.data) catch {
        hx.fail(ec.ERR_VAULT_DATA_INVALID, ec.MSG_SECRET_DATA_REQUIRED);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Secret endpoints require operator-minimum role.
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
    defer access.deinit(hx.alloc);

    storeSecretJsonOnConn(conn, hx.alloc, workspace_id, cred) catch |err| switch (err) {
        error.DataTooLarge => {
            hx.fail(ec.ERR_VAULT_DATA_TOO_LARGE, ec.MSG_SECRET_DATA_TOO_LARGE);
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

fn validateSecretName(hx: hx_mod.Hx, name: []const u8) bool {
    if (name.len == 0 or name.len > MAX_SECRET_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_SECRET_NAME_REQUIRED);
        return false;
    }
    return true;
}

fn storeSecretJsonOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    cred: SecretBody,
) !void {
    // Stringify once: serves both the size pre-flight (so the API surfaces a
    // precise 400 rather than letting the DB layer truncate) and the bytes
    // we hand to the vault envelope. innerStoreSecret already ran
    // vault.validateObject on cred.data, so the JSON shape is known good.
    const plaintext = try std.json.Stringify.valueAlloc(alloc, cred.data, .{});
    defer secure_memory.freeBytes(alloc, plaintext);
    if (plaintext.len > MAX_SECRET_DATA_LEN) return error.DataTooLarge;

    try vault.storeJsonPlaintext(alloc, conn, workspace_id, cred.name, plaintext);
}

// ── Delete Secret ─────────────────────────────────────────────────

pub fn innerDeleteSecret(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    secret_name: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!validateSecretName(hx, secret_name)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
    defer access.deinit(hx.alloc);

    if (!checkNotReferencedByModelEntries(hx, conn, secret_name)) return;

    const removed = vault.deleteCredential(conn, workspace_id, secret_name) catch |err| {
        log.err("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    log.info("deleted", .{ .name = secret_name, .workspace = workspace_id, .removed = removed });
    hx.res.status = 204;
}

/// M121 guard: refuse the delete when a tenant model registry entry still
/// points at this secret_ref, naming the count. Bootstrap/platform principals
/// (no tenant_id) carry no registry, so the check is a no-op for them —
/// deletion proceeds as before this guard existed.
fn checkNotReferencedByModelEntries(hx: hx_mod.Hx, conn: *pg.Conn, secret_name: []const u8) bool {
    const tenant_id = hx.principal.tenant_id orelse return true;
    const count = tenant_model_entries.referencedSecretCount(conn, tenant_id, secret_name) catch |err| {
        log.err("referenced_count_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    if (count == 0) return true;
    const detail = std.fmt.allocPrint(hx.alloc, "Secret is referenced by {d} model registry entr{s}", .{ count, if (count == 1) "y" else "ies" }) catch "Secret is referenced by model registry entries";
    hx.fail(ec.ERR_SECRET_REFERENCED_BY_MODEL_ENTRIES, detail);
    return false;
}

// ── List Secrets ──────────────────────────────────────────────────

pub fn innerListSecrets(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
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

    // RULE BIL: secret endpoints require operator-minimum role.
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
    defer access.deinit(hx.alloc);

    const creds = secret_list.fetchSecretListOnConn(conn, hx.alloc, workspace_id) catch |err| {
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    respondSecretList(hx, creds);
}

/// Serialize the list with null optional fields omitted, so each row carries
/// only its kind's descriptors (the per-kind wire shape the client union and
/// the `integration` CLI consume). hx.ok would emit `provider:null` noise.
fn respondSecretList(hx: hx_mod.Hx, creds: []const secret_list.SecretListRow) void {
    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.json(.{ .secrets = creds }, .{ .emit_null_optional_fields = false }) catch {
        common.internalOperationError(hx.res, "Failed to build the secret list", hx.req_id);
    };
}

// ── Rotate Secret Key (PATCH) ──────────────────────────────────────────

// Replace-key body: only the secret rotates; provider/model/base_url are
// preserved by loading the stored object and swapping a single field.
const RotateBody = struct {
    api_key: []const u8,
};

pub fn innerRotateSecret(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    secret_name: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!validateSecretName(hx, secret_name)) return;

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
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_SECRET_KEY_REQUIRED);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Secret endpoints require operator-minimum role.
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
    defer access.deinit(hx.alloc);

    rotateSecretKeyOnConn(conn, hx.alloc, workspace_id, secret_name, parsed.value.api_key) catch |err| switch (err) {
        error.NotFound => {
            hx.fail(ec.ERR_SECRET_NOT_FOUND, ec.MSG_SECRET_NOT_FOUND);
            return;
        },
        error.DataTooLarge => {
            hx.fail(ec.ERR_VAULT_DATA_TOO_LARGE, ec.MSG_SECRET_DATA_TOO_LARGE);
            return;
        },
        else => {
            log.err("rotate_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };

    log.debug("rotated", .{ .name = secret_name, .workspace = workspace_id });
    hx.ok(.ok, .{ .name = secret_name });
}

fn rotateSecretKeyOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    secret_name: []const u8,
    new_key: []const u8,
) !void {
    // Load the existing object, swap ONLY api_key, re-store. A missing row
    // surfaces error.NotFound (mapped to 404 by the caller).
    var parsed = vault.loadJson(alloc, conn, workspace_id, secret_name) catch |err| switch (err) {
        error.NotFound => return error.NotFound,
        else => return err,
    };
    defer parsed.deinit();

    // Own a mutable copy of the key so it can be erased immediately after the
    // re-store. The dispatcher erases the request body and parse arena later.
    const key_copy = try alloc.dupe(u8, new_key);
    defer secure_memory.freeBytes(alloc, key_copy);
    // The object map is backed by the parse arena — mutate it with that same
    // allocator so its storage stays single-owner (freed by parsed.deinit()).
    try parsed.value.object.put(parsed.arena.allocator(), S_API_KEY, .{ .string = key_copy });

    const plaintext = try std.json.Stringify.valueAlloc(alloc, parsed.value, .{});
    defer secure_memory.freeBytes(alloc, plaintext);
    if (plaintext.len > MAX_SECRET_DATA_LEN) return error.DataTooLarge;

    try vault.storeJsonPlaintext(alloc, conn, workspace_id, secret_name, plaintext);
}
