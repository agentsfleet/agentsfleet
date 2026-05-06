//! PATCH /v1/workspaces/{ws}/zombies/{id} — partial update of a zombie.
//!
//! Body fields are optional and presence-based:
//!   - `config_json`?: string                         — replace zombie config (XADD zombie_config_changed).
//!   - `status`?: "active" | "stopped" | "killed"     — drive the FSM (XADD zombie_status_changed).
//!
//! Status FSM (paused is gate-only, never set via API):
//!     active   ── stop    ──▶ stopped   ─┐
//!     active   ── kill    ──▶ killed     │ (terminal — 404 on further PATCH)
//!     paused   ── stop    ──▶ stopped    │
//!     paused   ── resume  ──▶ active     │
//!     paused   ── kill    ──▶ killed     │
//!     stopped  ── resume  ──▶ active     │
//!     stopped  ── kill    ──▶ killed    ─┘
//!
//! Both fields can ride together; the SQL UPDATE is one statement and the
//! control-stream publishes (one per dirty surface) follow. Empty body is a
//! 200 no-op — matches REST PATCH semantics.
//!
//! `updated_at` (BIGINT, ms epoch) doubles as the config_revision — strictly
//! monotonic per zombie, no schema migration needed.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const control_stream = @import("../../../zombie/control_stream.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.zombie_api);
const API_ACTOR = "api";

const Hx = hx_mod.Hx;

const PatchBody = struct {
    config_json: ?[]const u8 = null,
    status: ?[]const u8 = null,
};

const TransitionResult = enum {
    transitioned,
    not_found,
    invalid_transition,
};

/// Partial update of a zombie. Validates inputs, persists in one SQL UPDATE
/// gated by the FSM, and publishes one control-stream signal per dirty
/// surface. Returns 404 if the zombie is missing or already-killed (a
/// killed zombie is a tombstone — no further state changes apply). Returns
/// 409 if the requested status transition is not allowed from the current
/// state (e.g. resume on an active zombie, stop on a stopped zombie).
pub fn innerPatchZombie(hx: Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const body = parsePatchBody(hx, req) orelse return;
    if (body.config_json == null and body.status == null) {
        hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = @as(?i64, null) });
        return;
    }
    if (!validateBody(hx, body)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Status transitions (stop/resume/kill) are destructive lifecycle actions
    // and require operator-minimum role per RULE BIL — same gate the retired
    // DELETE /current-run enforced. Pure config_json updates (no status field)
    // stay at workspace-member.
    if (body.status != null) {
        const actor = hx.principal.user_id orelse API_ACTOR;
        const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
            .minimum_role = .operator,
        }) orelse return;
        defer access.deinit(hx.alloc);
    } else {
        if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
            hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
            return;
        }
    }

    const revision_opt = patchZombieOnConn(conn, workspace_id, zombie_id, body) catch |err| {
        log.err("patch_db_failed", .{ .err = @errorName(err), .zombie_id = zombie_id, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    if (revision_opt == null) {
        const result = classifyMiss(conn, workspace_id, zombie_id) catch |err| {
            log.err("patch_classify_failed", .{ .err = @errorName(err), .zombie_id = zombie_id, .req_id = hx.req_id });
            common.internalDbError(hx.res, hx.req_id);
            return;
        };
        switch (result) {
            .not_found => hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND),
            .invalid_transition => hx.fail(ec.ERR_ZOMBIE_ALREADY_TERMINAL, "Status transition not allowed from current state"),
            .transitioned => unreachable,
        }
        return;
    }
    const revision = revision_opt.?;

    if (body.status) |s| publishStatusChange(hx.ctx.queue, zombie_id, s) catch |err| {
        // PG row already reflects the new status. Watcher's reconcile loop
        // (≈30s) heals control-plane drift if the publish failed.
        log.warn(
            "status_publish_failed",
            .{ .err = @errorName(err), .zombie_id = zombie_id, .status = s, .req_id = hx.req_id, .hint = "row_updated_reconcile_will_apply" },
        );
    };
    if (body.config_json != null) publishConfigChange(hx.ctx.queue, zombie_id, revision) catch |err| {
        log.warn(
            "patch_publish_failed",
            .{ .err = @errorName(err), .zombie_id = zombie_id, .revision = revision, .req_id = hx.req_id, .hint = "row_updated_worker_blind" },
        );
    };

    log.info("patched", .{ .id = zombie_id, .workspace = workspace_id, .revision = revision, .status_set = body.status });
    if (body.status) |s| {
        hx.ok(.ok, .{
            .zombie_id = zombie_id,
            .status = s,
            .config_revision = revision,
        });
    } else {
        hx.ok(.ok, .{ .zombie_id = zombie_id, .config_revision = revision });
    }
}

fn parsePatchBody(hx: Hx, req: *httpz.Request) ?PatchBody {
    const body = req.body() orelse return PatchBody{};
    if (body.len == 0) return PatchBody{};
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn validateBody(hx: Hx, body: PatchBody) bool {
    if (body.config_json) |cj| {
        if (cj.len == 0) {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED);
            return false;
        }
        _ = zombie_config.parseZombieConfig(hx.alloc, cj) catch {
            hx.fail(ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG);
            return false;
        };
    }
    if (body.status) |s| {
        // Only operator-targetable states. `paused` is reserved for the
        // platform's anomaly gate and rejected here so callers can't use
        // PATCH to forge a system-halt provenance.
        const allowed =
            std.mem.eql(u8, s, zombie_config.ZombieStatus.active.toSlice()) or
            std.mem.eql(u8, s, zombie_config.ZombieStatus.stopped.toSlice()) or
            std.mem.eql(u8, s, zombie_config.ZombieStatus.killed.toSlice());
        if (!allowed) {
            hx.fail(ec.ERR_INVALID_REQUEST, "status must be one of \"active\", \"stopped\", \"killed\"");
            return false;
        }
    }
    return true;
}

/// One UPDATE handles either or both fields. The status FSM is encoded as
/// SQL gates so the kernel of state-machine truth lives at the storage
/// boundary where it is impossible to bypass via parallel writes:
///
///   killed              — never transitions out (`status != 'killed'` guard)
///   active   → stopped  — ok
///   active   → killed   — ok
///   paused   → stopped  — ok
///   paused   → active   — ok (resume from auto-pause)
///   paused   → killed   — ok
///   stopped  → active   — ok (resume from operator stop)
///   stopped  → killed   — ok
///   active   → active / stopped → stopped — rejected (no-op transitions
///                                            return 0 rows → 409)
///
/// `paused` is intentionally unset-able via API. Returns null on miss; the
/// caller distinguishes 404 (gone or killed) from 409 (no valid transition)
/// via a presence probe (`classifyMiss`).
fn patchZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8, body: PatchBody) !?i64 {
    const now_ms = std.time.milliTimestamp();
    const active = zombie_config.ZombieStatus.active.toSlice();
    const stopped = zombie_config.ZombieStatus.stopped.toSlice();
    const killed = zombie_config.ZombieStatus.killed.toSlice();
    const paused = zombie_config.ZombieStatus.paused.toSlice();

    var q = PgQuery.from(try conn.query(
        \\UPDATE core.zombies SET
        \\    config_json = COALESCE($1::jsonb, config_json),
        \\    status = COALESCE($2, status),
        \\    updated_at = $3
        \\WHERE id = $4::uuid
        \\  AND workspace_id = $5::uuid
        \\  AND status != $6
        \\  AND (
        \\        $2::text IS NULL
        \\     OR ($2 = $6)
        \\     OR ($2 = $7 AND status = ANY($9::text[]))
        \\     OR ($2 = $8 AND status = ANY($10::text[]))
        \\  )
        \\RETURNING updated_at
    , .{
        body.config_json,
        body.status,
        now_ms,
        zombie_id,
        workspace_id,
        killed,
        stopped,
        active,
        &[_][]const u8{ active, paused },
        &[_][]const u8{ stopped, paused },
    }));
    defer q.deinit();
    if (try q.next()) |row| return try row.get(i64, 0);
    return null;
}

/// Distinguish 404 (zombie not in workspace, or already killed — both are
/// tombstones) from 409 (zombie exists in non-killed state but the
/// transition isn't allowed from there).
fn classifyMiss(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8) !TransitionResult {
    const killed = zombie_config.ZombieStatus.killed.toSlice();
    var q = PgQuery.from(try conn.query(
        \\SELECT status FROM core.zombies
        \\WHERE id = $1::uuid AND workspace_id = $2::uuid
        \\LIMIT 1
    , .{ zombie_id, workspace_id }));
    defer q.deinit();
    if (try q.next()) |row| {
        const s = try row.get([]const u8, 0);
        if (std.mem.eql(u8, s, killed)) return .not_found;
        return .invalid_transition;
    }
    return .not_found;
}

fn publishConfigChange(redis: *queue_redis.Client, zombie_id: []const u8, revision: i64) !void {
    try control_stream.publish(redis, .{
        .zombie_config_changed = .{
            .zombie_id = zombie_id,
            .config_revision = revision,
        },
    });
}

fn publishStatusChange(redis: *queue_redis.Client, zombie_id: []const u8, status_str: []const u8) !void {
    const cs_status = control_stream.ZombieStatus.fromSlice(status_str) orelse return error.UnknownStatus;
    try control_stream.publish(redis, .{
        .zombie_status_changed = .{
            .zombie_id = zombie_id,
            .status = cs_status,
        },
    });
}
