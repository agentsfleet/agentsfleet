const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const error_codes = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const hx_mod = @import("hx.zig");

const log = std.log.scoped(.http);

pub const Context = common.Context;

// Row shape for GET /v1/admin/platform-keys response.
// Defined at module level so std.ArrayList(PlatformKeyRow) compiles in all build modes.
const PlatformKeyRow = struct {
    provider: []const u8,
    source_workspace_id: []const u8,
    active: bool,
    updated_at: i64,
};

// ── PUT /v1/admin/platform-keys ─────────────────────────────────────────────
// Upsert the platform default LLM key source for a provider.
// Body: {"provider": "kimi", "source_workspace_id": "..."}

const PutInput = struct {
    provider: []const u8,
    source_workspace_id: []const u8,
};

fn innerPutAdminPlatformKey(hx: hx_mod.Hx, req: *httpz.Request) void {
    if (!common.requireRole(hx.res, hx.req_id, hx.principal, .admin)) return;

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
    if (!common.requireUuidV7Id(hx.res, hx.req_id, input.source_workspace_id, "source_workspace_id")) return;

    const key_id = id_format.generatePlatformLlmKeyId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to generate platform key id", hx.req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Validate source_workspace_id references an existing workspace.
    const ws_exists = blk: {
        var ws_q = PgQuery.from(conn.query(
            "SELECT 1 FROM core.workspaces WHERE workspace_id = $1 LIMIT 1",
            .{input.source_workspace_id},
        ) catch {
            common.internalOperationError(hx.res, "Failed to check workspace existence", hx.req_id);
            return;
        });
        defer ws_q.deinit();
        break :blk (ws_q.next() catch null) != null;
    };
    if (!ws_exists) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "source_workspace_id does not reference an existing workspace");
        return;
    }

    _ = conn.exec(
        \\INSERT INTO core.platform_llm_keys (id, provider, source_workspace_id, active, created_at, updated_at)
        \\VALUES ($1, $2, $3, true, $4, $4)
        \\ON CONFLICT (provider) DO UPDATE
        \\SET source_workspace_id = EXCLUDED.source_workspace_id,
        \\    active = true,
        \\    updated_at = EXCLUDED.updated_at
    , .{ key_id, input.provider, input.source_workspace_id, now_ms }) catch {
        common.internalOperationError(hx.res, "Failed to upsert platform key", hx.req_id);
        return;
    };

    log.info("admin.platform_key_upserted provider={s} source_workspace_id={s}", .{ input.provider, input.source_workspace_id });

    hx.ok(.ok, .{
        .provider = input.provider,
        .source_workspace_id = input.source_workspace_id,
        .active = true,
        .request_id = hx.req_id,
    });
}

pub const handlePutAdminPlatformKey = hx_mod.authenticated(innerPutAdminPlatformKey);

// ── DELETE /v1/admin/platform-keys/{provider} ────────────────────────────────
// Deactivate the platform default for a provider (sets active = false).

fn innerDeleteAdminPlatformKey(hx: hx_mod.Hx, req: *httpz.Request, provider: []const u8) void {
    _ = req;
    if (!common.requireRole(hx.res, hx.req_id, hx.principal, .admin)) return;

    if (provider.len == 0 or provider.len > 32) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "provider must be 1–32 chars");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = conn.exec(
        "UPDATE core.platform_llm_keys SET active = false, updated_at = $1 WHERE provider = $2",
        .{ std.time.milliTimestamp(), provider },
    ) catch {
        common.internalOperationError(hx.res, "Failed to deactivate platform key", hx.req_id);
        return;
    };

    log.info("admin.platform_key_deactivated provider={s}", .{provider});

    hx.ok(.ok, .{
        .provider = provider,
        .active = false,
        .request_id = hx.req_id,
    });
}

pub const handleDeleteAdminPlatformKey = hx_mod.authenticatedWithParam(innerDeleteAdminPlatformKey);

// ── GET /v1/admin/platform-keys ──────────────────────────────────────────────
// List all platform key rows (active and inactive). Never returns key material.

fn innerGetAdminPlatformKeys(hx: hx_mod.Hx, req: *httpz.Request) void {
    _ = req;
    if (!common.requireRole(hx.res, hx.req_id, hx.principal, .admin)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var q = PgQuery.from(conn.query(
        "SELECT provider, source_workspace_id, active, updated_at FROM core.platform_llm_keys ORDER BY provider",
        .{},
    ) catch {
        common.internalOperationError(hx.res, "Failed to query platform keys", hx.req_id);
        return;
    });
    defer q.deinit();

    var rows: std.ArrayList(PlatformKeyRow) = .{};

    while (true) {
        const maybe_row = q.next() catch |e| {
            log.err("admin.platform_keys_row_error err={s}", .{@errorName(e)});
            break;
        };
        const row = maybe_row orelse break;
        const prov = hx.alloc.dupe(u8, row.get([]u8, 0) catch continue) catch continue;
        const src_ws = hx.alloc.dupe(u8, row.get([]u8, 1) catch continue) catch continue;
        const active = row.get(bool, 2) catch continue;
        const updated_at = row.get(i64, 3) catch continue;
        rows.append(hx.alloc, .{
            .provider = prov,
            .source_workspace_id = src_ws,
            .active = active,
            .updated_at = updated_at,
        }) catch continue;
    }

    hx.ok(.ok, .{
        .keys = rows.items,
        .request_id = hx.req_id,
    });
}

pub const handleGetAdminPlatformKeys = hx_mod.authenticated(innerGetAdminPlatformKeys);
