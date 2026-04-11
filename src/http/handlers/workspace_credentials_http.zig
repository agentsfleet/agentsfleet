const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const workspace_guards = @import("../workspace_guards.zig");
const crypto_store = @import("../../secrets/crypto_store.zig");
const hx_mod = @import("hx.zig");

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

fn innerPutWorkspaceLlmCredential(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(PutInput, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;

    if (input.provider.len == 0 or input.provider.len > 32) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "provider must be 1–32 chars");
        return;
    }
    if (input.api_key.len == 0 or input.api_key.len > 256) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "api_key must be 1–256 chars");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    const key_name = std.fmt.allocPrint(hx.alloc, "{s}_api_key", .{input.provider}) catch {
        common.internalOperationError(hx.res, "Allocation failed", hx.req_id);
        return;
    };

    crypto_store.store(hx.alloc, conn, workspace_id, key_name, input.api_key, KEK_VERSION) catch {
        common.internalOperationError(hx.res, "Failed to store LLM API key", hx.req_id);
        return;
    };
    crypto_store.store(hx.alloc, conn, workspace_id, "llm_provider_preference", input.provider, KEK_VERSION) catch {
        common.internalOperationError(hx.res, "Failed to store provider preference", hx.req_id);
        return;
    };

    log.info("workspace.llm_credential_set workspace_id={s} provider={s}", .{ workspace_id, input.provider });
    hx.res.status = 204;
}

pub const handlePutWorkspaceLlmCredential = hx_mod.authenticatedWithParam(innerPutWorkspaceLlmCredential);

// ── DELETE /v1/workspaces/{workspace_id}/credentials/llm ────────────────────
// Remove workspace BYOK key. Subsequent runs fall back to platform default.

fn innerDeleteWorkspaceLlmCredential(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    _ = req;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    // Begin transaction: lock the preference row with FOR UPDATE, then read + delete
    // atomically. The lock prevents a concurrent PUT from swapping llm_provider_preference
    // between our SELECT and our DELETE statements.
    _ = conn.exec("BEGIN", .{}) catch {
        common.internalOperationError(hx.res, "Failed to begin transaction", hx.req_id);
        return;
    };

    var pref_lock = conn.query(
        "SELECT 1 FROM vault.secrets WHERE workspace_id = $1 AND key_name = 'llm_provider_preference' FOR UPDATE",
        .{workspace_id},
    ) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(hx.res, "Failed to lock provider preference", hx.req_id);
        return;
    };
    pref_lock.drain() catch {};
    pref_lock.deinit();

    const provider: []const u8 = crypto_store.load(hx.alloc, conn, workspace_id, "llm_provider_preference") catch |e| p: {
        if (e != error.NotFound) {
            _ = conn.exec("ROLLBACK", .{}) catch {};
            common.internalOperationError(hx.res, "Failed to read provider preference", hx.req_id);
            return;
        }
        break :p "anthropic";
    };
    const key_name = std.fmt.allocPrint(hx.alloc, "{s}_api_key", .{provider}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(hx.res, "Allocation failed", hx.req_id);
        return;
    };

    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2", .{ workspace_id, key_name }) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(hx.res, "Failed to delete LLM API key", hx.req_id);
        return;
    };
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = 'llm_provider_preference'", .{workspace_id}) catch |e| {
        log.err("workspace.llm_pref_delete_failed workspace_id={s} err={s}", .{ workspace_id, @errorName(e) });
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(hx.res, "Failed to delete provider preference", hx.req_id);
        return;
    };

    _ = conn.exec("COMMIT", .{}) catch {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        common.internalOperationError(hx.res, "Transaction commit failed", hx.req_id);
        return;
    };

    log.info("workspace.llm_credential_deleted workspace_id={s} provider={s}", .{ workspace_id, provider });
    hx.res.status = 204;
}

pub const handleDeleteWorkspaceLlmCredential = hx_mod.authenticatedWithParam(innerDeleteWorkspaceLlmCredential);

// ── GET /v1/workspaces/{workspace_id}/credentials/llm ───────────────────────
// Returns {"provider": "anthropic", "has_key": true} — never the key value.

fn innerGetWorkspaceLlmCredential(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    _ = req;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;

    const provider: []const u8 = crypto_store.load(hx.alloc, conn, workspace_id, "llm_provider_preference") catch |e| p: {
        if (e != error.NotFound) {
            common.internalOperationError(hx.res, "Failed to read provider preference", hx.req_id);
            return;
        }
        break :p "anthropic";
    };
    const key_name = std.fmt.allocPrint(hx.alloc, "{s}_api_key", .{provider}) catch {
        common.internalOperationError(hx.res, "Allocation failed", hx.req_id);
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

    hx.ok(.ok, .{
        .provider = provider,
        .has_key = has_key,
        .request_id = hx.req_id,
    });
}

pub const handleGetWorkspaceLlmCredential = hx_mod.authenticatedWithParam(innerGetWorkspaceLlmCredential);
