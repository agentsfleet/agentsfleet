// Workspace secret API handlers.
//
// POST   /v1/workspaces/{ws}/secrets             → innerStoreSecret
// GET    /v1/workspaces/{ws}/secrets             → innerListSecrets
// PUT    /v1/workspaces/{ws}/secrets/{name}      → innerReplaceSecret
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
const secret_reference_txn = @import("../../../state/secret_reference_txn.zig");

const log = logging.scoped(.fleet_secrets_api);

pub const Context = common.Context;

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
        // Creation claims a free name; replacing a body is PUT on the named secret.
        error.SecretNameTaken => {
            hx.fail(ec.ERR_SECRET_NAME_TAKEN, ec.MSG_SECRET_NAME_TAKEN);
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

    try vault.createJsonPlaintext(alloc, conn, workspace_id, cred.name, plaintext);
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

    deleteReferencedSecret(hx, conn, workspace_id, secret_name) catch return;
    hx.res.status = 204;
}

/// Delete the credential under the shared reference lock protocol.
///
/// The referenced-entry check and the DELETE now happen inside ONE transaction
/// holding the vault row lock. They used to be two unsynchronized statements,
/// which let a concurrent `POST /tenants/me/models` slip an entry in between
/// them — the entry survived, naming a credential that no longer existed, and
/// nothing noticed until a fleet tried to run (see state/secret_reference_txn.zig).
///
/// `SecretGone` is success here, not failure: another transaction already
/// removed the row, which is exactly what this request wanted. DELETE stays
/// idempotent 204, matching the behaviour before the lock existed.
fn deleteReferencedSecret(
    hx: hx_mod.Hx,
    conn: *pg.Conn,
    workspace_id: []const u8,
    secret_name: []const u8,
) !void {
    // The tenant whose entries matter is the WORKSPACE's owner, not the
    // caller's. Passing `hx.principal.tenant_id` here meant a `workspace:any`
    // operator deleting inside another tenant's workspace counted references
    // against its own tenant, matched none, and deleted a credential that the
    // victim's registry entries still named.
    var txn = secret_reference_txn.begin(conn, workspace_id, secret_name) catch |err| switch (err) {
        secret_reference_txn.Error.SecretGone => return,
        else => {
            log.err("delete_lock_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return err;
        },
    };
    errdefer txn.abort();

    // The count came from the same statement that took the entry locks, so no
    // entry can appear or vanish between deciding and deleting.
    if (txn.reference_count > 0) {
        const n = txn.reference_count;
        const detail = std.fmt.allocPrint(hx.alloc, "Secret is referenced by {d} model registry entr{s}", .{ n, if (n == 1) "y" else "ies" }) catch "Secret is referenced by model registry entries";
        hx.fail(ec.ERR_SECRET_REFERENCED_BY_MODEL_ENTRIES, detail);
        txn.abort();
        return error.StillReferenced;
    }

    const removed = vault.deleteCredential(conn, workspace_id, secret_name) catch |err| {
        log.err("delete_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return err;
    };
    try txn.commit();
    log.info("deleted", .{ .name = secret_name, .workspace = workspace_id, .removed = removed });
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

// ── Replace Secret (PUT) ───────────────────────────────────────────────

// Replace body: the same `data` object `create` takes. There is no merge and no
// privileged field name, so every stored shape — `api_key`, `token`,
// `api_token`, anything — is equally replaceable. A field absent here is absent
// from the stored secret afterwards.
//
// This replaced a `PATCH {api_key}` that merged one hardcoded field. Merging
// cannot express intent on a resource the caller can never read back: on a
// secret keyed anything but `api_key` it added an unused field, left the live
// credential stale, and answered 200.
const ReplaceBody = struct {
    data: std.json.Value,
};

pub fn innerReplaceSecret(
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

    const parsed = std.json.parseFromSlice(ReplaceBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    // Same shape gate as create — a replace that accepted a shape create
    // rejects would let the two verbs disagree about what a secret is.
    vault.validateObject(parsed.value.data) catch {
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

    replaceSecretOnConn(conn, hx.alloc, workspace_id, secret_name, parsed.value.data) catch |err| switch (err) {
        // Zero affected rows: this workspace holds no such name. Nothing was
        // written and nothing was created — the statement is an UPDATE.
        error.NotFound => {
            hx.fail(ec.ERR_SECRET_NOT_FOUND, ec.MSG_SECRET_NOT_FOUND);
            return;
        },
        error.DataTooLarge => {
            hx.fail(ec.ERR_VAULT_DATA_TOO_LARGE, ec.MSG_SECRET_DATA_TOO_LARGE);
            return;
        },
        else => {
            log.err("replace_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .name = secret_name, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };

    log.debug("replaced", .{ .name = secret_name, .workspace = workspace_id });
    hx.ok(.ok, .{ .name = secret_name });
}

fn replaceSecretOnConn(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    secret_name: []const u8,
    data: std.json.Value,
) !void {
    // No read precedes this write, which is the whole design. The former
    // rotate loaded, merged one field, and re-stored through an upsert — two
    // autocommit statements with nothing held between them, so a delete
    // committing in the gap left the upsert with no row to conflict against
    // and it re-INSERTED the credential that had just been removed. A single
    // UPDATE has no such gap, and creates nothing when it matches nothing.
    const plaintext = try std.json.Stringify.valueAlloc(alloc, data, .{});
    defer secure_memory.freeBytes(alloc, plaintext);
    if (plaintext.len > MAX_SECRET_DATA_LEN) return error.DataTooLarge;

    try vault.replaceJsonPlaintext(alloc, conn, workspace_id, secret_name, plaintext);
}
