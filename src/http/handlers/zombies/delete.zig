//! DELETE /v1/workspaces/{ws}/zombies/{id} — hard-delete a zombie.
//!
//! Precondition: status='killed' (kill-before-purge). The two-step lifecycle
//! protects against accidental data loss — operator must mark terminal first,
//! then explicitly purge. Returns 409 if the zombie isn't killed yet.
//!
//! Cascade:
//!   - core.zombie_events           — FK ON DELETE CASCADE (auto)
//!   - core.integration_grants      — FK ON DELETE CASCADE (auto)
//!   - core.agent_keys              — FK ON DELETE CASCADE (auto)
//!   - core.zombie_sessions         — no FK cascade; explicit DELETE
//!   - core.zombie_approval_gates   — no FK cascade; explicit DELETE
//!   - memory.memory_entries        — keyed by instance_id (no FK); explicit
//!   - zombie_execution_telemetry   — keyed by zombie_id (no FK); explicit
//!   - zombie:{id}:events Redis stream — best-effort DEL after PG commit
//!
//! Auth: operator-minimum role per RULE BIL — destructive lifecycle action.

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
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.zombie_api);
const API_ACTOR = "api";

const Hx = hx_mod.Hx;

const DeleteOutcome = enum { purged, not_killed, not_found };

pub fn innerDeleteZombie(hx: Hx, _: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

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

    const outcome = purgeZombieOnConn(conn, workspace_id, zombie_id) catch |err| {
        log.err("delete_failed", .{ .err = @errorName(err), .zombie_id = zombie_id, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    switch (outcome) {
        .purged => {
            // Best-effort Redis cleanup. PG state is the source of truth; if
            // the stream DEL fails the row is already gone, so the next
            // worker XREADGROUP will see the empty stream and move on.
            cleanupRedisStream(hx.ctx.queue, zombie_id) catch |err| {
                log.warn(
                    "delete_redis_cleanup_failed",
                    .{ .err = @errorName(err), .zombie_id = zombie_id, .req_id = hx.req_id, .hint = "pg_row_purged_stream_orphaned_until_ttl" },
                );
            };
            log.info("purged", .{ .id = zombie_id, .workspace = workspace_id, .actor = actor });
            hx.res.status = 204;
        },
        .not_killed => hx.fail(ec.ERR_ZOMBIE_ALREADY_TERMINAL, "Zombie must be killed before delete (PATCH status=killed first)"),
        .not_found => hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND),
    }
}

/// Single-transaction purge. The non-cascading child tables are wiped first,
/// then the parent row; PG-level CASCADE handles the rest. The whole sequence
/// runs inside BEGIN/COMMIT so a failure mid-stream rolls back, leaving the
/// caller free to retry without partial-purge wreckage.
fn purgeZombieOnConn(conn: *pg.Conn, workspace_id: []const u8, zombie_id: []const u8) !DeleteOutcome {
    const killed = zombie_config.ZombieStatus.killed.toSlice();

    // Pre-flight: classify before any DELETE so we return distinct 404 vs 409
    // without partial mutation.
    {
        var probe = PgQuery.from(try conn.query(
            \\SELECT status FROM core.zombies
            \\WHERE id = $1::uuid AND workspace_id = $2::uuid
            \\LIMIT 1
        , .{ zombie_id, workspace_id }));
        defer probe.deinit();
        const row = (try probe.next()) orelse return .not_found;
        const cur_status = try row.get([]const u8, 0);
        if (!std.mem.eql(u8, cur_status, killed)) return .not_killed;
    }

    _ = try conn.exec("BEGIN", .{});
    errdefer _ = conn.exec("ROLLBACK", .{}) catch {};

    _ = try conn.exec(
        "DELETE FROM zombie_execution_telemetry WHERE workspace_id = $1 AND zombie_id = $2",
        .{ workspace_id, zombie_id },
    );
    _ = try conn.exec(
        "DELETE FROM memory.memory_entries WHERE instance_id = $1",
        .{zombie_id},
    );
    _ = try conn.exec(
        "DELETE FROM core.zombie_approval_gates WHERE zombie_id = $1::uuid",
        .{zombie_id},
    );
    _ = try conn.exec(
        "DELETE FROM core.zombie_sessions WHERE zombie_id = $1::uuid",
        .{zombie_id},
    );
    // Final delete; PG CASCADE handles zombie_events / integration_grants /
    // agent_keys. Status guard is belt-and-suspenders against TOCTOU between
    // the pre-flight probe and here — if a concurrent PATCH resurrected the
    // zombie, the row count is 0 and we surface 409.
    var del = PgQuery.from(try conn.query(
        \\DELETE FROM core.zombies
        \\WHERE id = $1::uuid AND workspace_id = $2::uuid AND status = $3
        \\RETURNING id
    , .{ zombie_id, workspace_id, killed }));
    defer del.deinit();
    const purged = (try del.next()) != null;
    if (!purged) {
        _ = conn.exec("ROLLBACK", .{}) catch {};
        return .not_killed;
    }
    _ = try conn.exec("COMMIT", .{});
    return .purged;
}

fn cleanupRedisStream(redis: *queue_redis.Client, zombie_id: []const u8) !void {
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var resp = try redis.commandAllowError(&.{ "DEL", key });
    resp.deinit(redis.alloc);
}
