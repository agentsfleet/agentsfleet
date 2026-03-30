const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const workspace_guards = @import("../workspace_guards.zig");
const crypto_store = @import("../../secrets/crypto_store.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";
const KEK_VERSION: u32 = 1;

pub const Context = common.Context;

// ── PUT /v1/workspaces/{workspace_id}/credentials/llm ───────────────────────
// Store workspace BYOK LLM key. Caller owns the provider cost.
// Body: {"provider": "anthropic", "api_key": "sk-ant-..."}
// Response: 204 No Content.

const PutInput = struct {
    provider: []const u8,
    api_key: []const u8,
};

pub fn handlePutWorkspaceLlmCredential(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(PutInput, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;

    if (input.provider.len == 0 or input.provider.len > 32) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "provider must be 1–32 chars", req_id);
        return;
    }
    if (input.api_key.len == 0 or input.api_key.len > 256) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "api_key must be 1–256 chars", req_id);
        return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    _ = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    const key_name = std.fmt.allocPrint(alloc, "{s}_api_key", .{input.provider}) catch {
        common.internalOperationError(res, "Allocation failed", req_id);
        return;
    };

    crypto_store.store(alloc, conn, workspace_id, key_name, input.api_key, KEK_VERSION) catch {
        common.internalOperationError(res, "Failed to store LLM API key", req_id);
        return;
    };
    crypto_store.store(alloc, conn, workspace_id, "llm_provider_preference", input.provider, KEK_VERSION) catch {
        common.internalOperationError(res, "Failed to store provider preference", req_id);
        return;
    };

    log.info("workspace.llm_credential_set workspace_id={s} provider={s}", .{ workspace_id, input.provider });
    res.status = 204;
}

// ── DELETE /v1/workspaces/{workspace_id}/credentials/llm ────────────────────
// Remove workspace BYOK key. Subsequent runs fall back to platform default.

pub fn handleDeleteWorkspaceLlmCredential(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    _ = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    // Begin transaction: lock the preference row with FOR UPDATE, then read + delete
    // atomically. The lock prevents a concurrent PUT from swapping llm_provider_preference
    // between our SELECT and our DELETE statements (READ COMMITTED alone does not protect
    // against this because crypto_store.load issues a plain SELECT without a row lock).
    _ = conn.exec("BEGIN", .{}) catch {
        common.internalOperationError(res, "Failed to begin transaction", req_id);
        return;
    };

    // Lock the preference row so concurrent PUTs block until we commit.
    // If no preference row exists the lock is a no-op; we fall back to "anthropic".
    var pref_lock = conn.query(
        "SELECT 1 FROM vault.secrets WHERE workspace_id = $1 AND key_name = 'llm_provider_preference' FOR UPDATE",
        .{workspace_id},
    ) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Failed to lock provider preference", req_id);
        return;
    };
    pref_lock.drain() catch {};
    pref_lock.deinit();

    const provider: []const u8 = crypto_store.load(alloc, conn, workspace_id, "llm_provider_preference") catch |e| p: {
        if (e != error.NotFound) {
            _ = conn.exec("ROLLBACK", .{}) catch {};
            common.internalOperationError(res, "Failed to read provider preference", req_id);
            return;
        }
        break :p "anthropic";
    };
    const key_name = std.fmt.allocPrint(alloc, "{s}_api_key", .{provider}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Allocation failed", req_id);
        return;
    };

    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, key_name }) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Failed to delete LLM API key", req_id);
        return;
    };
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = 'llm_provider_preference'", .{workspace_id}) catch |e|
        log.err("workspace.llm_pref_delete_failed workspace_id={s} err={s}", .{ workspace_id, @errorName(e) });

    _ = conn.exec("COMMIT", .{}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(res, "Transaction commit failed", req_id);
        return;
    };

    log.info("workspace.llm_credential_deleted workspace_id={s} provider={s}", .{ workspace_id, provider });
    res.status = 204;
}

// ── GET /v1/workspaces/{workspace_id}/credentials/llm ───────────────────────
// Returns {"provider": "anthropic", "has_key": true} — never the key value.

pub fn handleGetWorkspaceLlmCredential(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    _ = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    const provider: []const u8 = crypto_store.load(alloc, conn, workspace_id, "llm_provider_preference") catch |e| p: {
        if (e != error.NotFound) {
            common.internalOperationError(res, "Failed to read provider preference", req_id);
            return;
        }
        break :p "anthropic";
    };
    const key_name = std.fmt.allocPrint(alloc, "{s}_api_key", .{provider}) catch {
        common.internalOperationError(res, "Allocation failed", req_id);
        return;
    };

    var has_key = false;
    var ck = conn.query(
        "SELECT 1 FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2 LIMIT 1",
        .{ workspace_id, key_name },
    ) catch null;
    if (ck) |*q| {
        defer q.*.deinit();
        has_key = (q.*.next() catch null) != null;
        q.*.drain() catch {};
    }

    common.writeJson(res, .ok, .{
        .provider = provider,
        .has_key = has_key,
        .request_id = req_id,
    });
}
