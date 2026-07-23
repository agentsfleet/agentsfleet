//! POST /v1/runners — register a runner.
//!
//! Authed by an existing operator credential (Clerk JWT or `agt_t` api_key via
//! `bearer_or_api_key` + admin role) — there is no enrollment token. Mints a
//! durable `agt_r` runner token (256-bit random, returned once), stores only its
//! SHA-256 hash in `fleet.runners`, and records the self-reported `sandbox_tier`
//! + `labels`. `tenant_id` is NULL in S0 (trusted fleet); the per-tenant-scoped
//! mode wires it later. See `docs/AUTH.md` (Runner token).

const std = @import("std");
const sql = @import("sql.zig");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const api_key = @import("../../../auth/api_key.zig");
const protocol = @import("contract").protocol;
const runner_bearer = @import("../../../auth/middleware/runner_bearer.zig");
const runner_events = @import("../../../fleet/runner_events.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_register);

// 256-bit random token body, per docs/AUTH.md (Runner token → Provisioning).
const TOKEN_RANDOM_BYTES: usize = 32;
const MAX_HOST_ID_LEN: usize = 256;

const RegisterError = error{ DbError, OperationError };

/// Mint a `agt_r<64-hex>` runner token. The prefix is single-sourced in
/// `runner_bearer` (RULE UFS) so the minter and the validator never drift.
fn mintRunnerToken(alloc: std.mem.Allocator) ![]const u8 {
    var raw: [TOKEN_RANDOM_BYTES]u8 = undefined;
    try constants.secureRandomBytes(&raw);
    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ runner_bearer.RUNNER_TOKEN_PREFIX, hex });
}

pub fn innerRegisterRunner(hx: Hx, req: *httpz.Request) void {
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(protocol.RegisterRequest, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body (host_id, sandbox_tier, labels[])");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (body.host_id.len == 0 or body.host_id.len > MAX_HOST_ID_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "host_id must be 1-256 chars");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    performRegister(hx, conn, body) catch |err| switch (err) {
        error.DbError => common.internalDbError(hx.res, hx.req_id),
        error.OperationError => common.internalOperationError(hx.res, "runner registration failed", hx.req_id),
    };
}

fn performRegister(hx: Hx, conn: *pg.Conn, body: protocol.RegisterRequest) RegisterError!void {
    const raw_token = mintRunnerToken(hx.alloc) catch return error.OperationError;
    const token_hash = api_key.sha256Hex(raw_token);
    const runner_id = id_format.generateRunnerId(hx.alloc) catch return error.OperationError;
    const event_row_id = id_format.generateRunnerEventId(hx.alloc) catch return error.OperationError;
    defer hx.alloc.free(event_row_id);
    const labels_json = std.json.Stringify.valueAlloc(hx.alloc, body.labels, .{}) catch return error.OperationError;
    const now_ms = clock.nowMillis();

    // tenant_id NULL: S0 is trusted-fleet; the per-tenant-scoped mode wires it.
    // last_seen_at = RUNNER_LAST_SEEN_NEVER: the runner is minted but has not
    // connected, so the fleet read derives `registered` (not a fake `online`)
    // until its first heartbeat moves last_seen forward. created/updated = now.
    _ = conn.exec(sql.INSERT_RUNNER_WITH_EVENT, .{
        runner_id,
        body.host_id,
        token_hash[0..],
        @tagName(body.sandbox_tier),
        protocol.ADMIN_STATE_ACTIVE,
        labels_json,
        protocol.RUNNER_LAST_SEEN_NEVER,
        now_ms,
        event_row_id,
        @tagName(protocol.RunnerEventType.runner_registered),
        runner_events.META_HOST_ID,
        runner_events.META_SANDBOX_TIER,
    }) catch return error.DbError;

    log.debug("registered", .{
        .runner_id = runner_id,
        .host_id = body.host_id,
        .sandbox_tier = @tagName(body.sandbox_tier),
    });

    hx.okSensitive(.created, protocol.RegisterResponse{
        .runner_id = runner_id,
        .runner_token = raw_token,
    });
}
