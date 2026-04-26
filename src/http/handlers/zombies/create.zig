//! POST /v1/workspaces/{ws}/zombies — create a zombie.
//!
//! Atomic install path: INSERT into core.zombies → XGROUP CREATE MKSTREAM
//! zombie:{id}:events zombie_workers 0 → XADD zombie:control zombie_created.
//! All three happen synchronously before the 201 returns. Invariant 1 from
//! the worker substrate spec: by the time the 201 reaches the caller, both
//! the per-zombie data stream + group AND the control-stream signal exist.
//! A webhook arriving 1ms after the 201 finds the group already.
//!
//! Failure semantics: if XGROUP CREATE or XADD fails after the INSERT
//! committed, the row is in core.zombies but the worker will never claim
//! it. Return 500 with a rollback hint; a reconcile job (out of scope here)
//! cleans up orphan rows.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const zombie_config = @import("../../../zombie/config.zig");
const queue_redis = @import("../../../queue/redis_client.zig");
const control_stream = @import("../../../zombie/control_stream.zig");

const log = std.log.scoped(.zombie_api);

const Hx = hx_mod.Hx;

const MAX_NAME_LEN: usize = 64;
const MAX_SOURCE_LEN: usize = 64 * 1024; // 64KB

const CreateBody = struct {
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
};

fn parseCreateBody(hx: Hx, req: *httpz.Request) ?CreateBody {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(CreateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn validateCreateFields(hx: Hx, b: CreateBody) bool {
    if (b.name.len == 0 or b.name.len > MAX_NAME_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_NAME_REQUIRED);
        return false;
    }
    if (b.source_markdown.len == 0 or b.source_markdown.len > MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_SOURCE_REQUIRED);
        return false;
    }
    if (b.config_json.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_ZOMBIE_CONFIG_REQUIRED);
        return false;
    }
    return true;
}

pub fn innerCreateZombie(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = parseCreateBody(hx, req) orelse return;
    if (!validateCreateFields(hx, body)) return;

    _ = zombie_config.parseZombieConfig(hx.alloc, body.config_json) catch {
        hx.fail(ec.ERR_ZOMBIE_INVALID_CONFIG, ec.MSG_ZOMBIE_INVALID_CONFIG);
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const zombie_id = id_format.generateZombieId(hx.alloc) catch {
        common.internalOperationError(hx.res, "ID generation failed", hx.req_id);
        return;
    };
    const now_ms = std.time.milliTimestamp();

    insertZombieOnConn(conn, workspace_id, body, zombie_id, now_ms) catch |err| {
        if (isUniqueViolation(err)) {
            hx.fail(ec.ERR_ZOMBIE_NAME_EXISTS, ec.MSG_ZOMBIE_NAME_EXISTS);
            return;
        }
        log.err("zombie.create_failed err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    publishInstallSignals(hx.ctx.queue, zombie_id, workspace_id) catch |err| {
        log.err(
            "zombie.create_publish_failed err={s} zombie_id={s} req_id={s} hint=row_committed_worker_blind reconcile_required=true",
            .{ @errorName(err), zombie_id, hx.req_id },
        );
        common.internalOperationError(hx.res, "rollback_required: control-stream publish failed after INSERT", hx.req_id);
        return;
    };

    log.info("zombie.created id={s} name={s} workspace={s}", .{ zombie_id, body.name, workspace_id });
    hx.ok(.created, .{ .zombie_id = zombie_id, .status = zombie_config.ZombieStatus.active.toSlice() });
}

fn insertZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, body: CreateBody, zombie_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5::jsonb, $6, $7, $8)
    , .{ zombie_id, workspace_id, body.name, body.source_markdown, body.config_json, zombie_config.ZombieStatus.active.toSlice(), now_ms, now_ms });
}

/// Invariant 1: by the time this returns, both the per-zombie events stream
/// + consumer group AND the control-stream `zombie_created` signal exist.
/// Either step failing leaves the DB row committed and the worker blind —
/// caller surfaces a 500 with a rollback hint.
fn publishInstallSignals(redis: *queue_redis.Client, zombie_id: []const u8, workspace_id: []const u8) !void {
    try ensureZombieEventsGroup(redis, zombie_id);
    try control_stream.publish(redis, .{
        .zombie_created = .{ .zombie_id = zombie_id, .workspace_id = workspace_id },
    });
}

/// `XGROUP CREATE MKSTREAM zombie:{id}:events zombie_workers 0` with
/// BUSYGROUP-as-success. Mirror of `control_stream.ensureControlGroup` but
/// for the per-zombie data stream.
fn ensureZombieEventsGroup(redis: *queue_redis.Client, zombie_id: []const u8) !void {
    var stream_key_buf: [128]u8 = undefined;
    const stream_key = try std.fmt.bufPrint(&stream_key_buf, "zombie:{s}:events", .{zombie_id});

    var resp = try redis.commandAllowError(&.{
        "XGROUP", "CREATE", stream_key, "zombie_workers", "0", "MKSTREAM",
    });
    defer resp.deinit(redis.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.ZombieEventsGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
            return error.ZombieEventsGroupCreateFailed;
        },
        else => return error.ZombieEventsGroupCreateFailed,
    }
}

fn isUniqueViolation(_: anyerror) bool {
    // pg.Pool returns error.PGError for all Postgres errors (connection, constraint, cast).
    // We cannot distinguish unique_violation (SQLSTATE 23505) from other PGErrors
    // because pg.Pool does not expose structured SQLSTATE codes.
    // Return false to let the caller surface a 500 instead of a misleading 409.
    return false;
}

test "isUniqueViolation always returns false (no SQLSTATE introspection)" {
    try std.testing.expect(!isUniqueViolation(error.PGError));
    try std.testing.expect(!isUniqueViolation(error.OutOfMemory));
}
