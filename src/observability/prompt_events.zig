const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const db = @import("../db/pool.zig");
const obs_log = @import("logging.zig");
const id_format = @import("../types/id_format.zig");

pub const EventType = enum {
    prompt_birth,
    prompt_accepted,
    prompt_applied,
    prompt_eval,
    prompt_performance,

    pub fn label(self: EventType) []const u8 {
        return switch (self) {
            .prompt_birth => "prompt_birth",
            .prompt_accepted => "prompt_accepted",
            .prompt_applied => "prompt_applied",
            .prompt_eval => "prompt_eval",
            .prompt_performance => "prompt_performance",
        };
    }
};

const PromptEvent = struct {
    event_type: EventType,
    workspace_id: []const u8,
    tenant_id: []const u8,
    agent_id: ?[]const u8 = null,
    config_version_id: ?[]const u8 = null,
    metadata_json: []const u8 = "{}",
    ts_ms: i64,
};

const Emitter = struct {
    ctx: *anyopaque,
    emit_fn: *const fn (ctx: *anyopaque, event: PromptEvent) anyerror!void,

    pub fn emit(self: Emitter, event: PromptEvent) !void {
        return self.emit_fn(self.ctx, event);
    }
};

const DbEmitterCtx = struct {
    conn: *pg.Conn,
};

fn dbEmitter(conn: *pg.Conn) Emitter {
    return .{
        .ctx = @ptrCast(conn),
        .emit_fn = emitToDb,
    };
}

fn emitToDb(ctx: *anyopaque, event: PromptEvent) anyerror!void {
    const conn: *pg.Conn = @ptrCast(@alignCast(ctx));
    const row_id = try id_format.generatePromptLifecycleEventId(conn._allocator);
    defer conn._allocator.free(row_id);
    const event_id = randomEventId();
    _ = try conn.exec(
        \\INSERT INTO core.prompt_lifecycle_events
        \\  (id, event_id, event_type, workspace_id, tenant_id, agent_id, config_version_id, metadata_json, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
    , .{
        row_id,
        event_id,
        event.event_type.label(),
        event.workspace_id,
        event.tenant_id,
        event.agent_id,
        event.config_version_id,
        event.metadata_json,
        event.ts_ms,
    });
}

fn emitBestEffort(conn: *pg.Conn, event: PromptEvent) void {
    dbEmitter(conn).emit(event) catch |err| {
        obs_log.logWarnErr(.worker, err, "prompt event emission failed type={s} workspace_id={s}", .{
            event.event_type.label(),
            event.workspace_id,
        });
    };
}

fn randomEventId() [24]u8 {
    var raw: [12]u8 = undefined;
    std.crypto.random.bytes(&raw);
    return std.fmt.bytesToHex(raw, .lower);
}

test "event type labels are stable" {
    try std.testing.expectEqualStrings("prompt_birth", EventType.prompt_birth.label());
    try std.testing.expectEqualStrings("prompt_performance", EventType.prompt_performance.label());
}

fn openPromptEventTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    // pg.Pool.init does NOT copy connect/auth strings — use alloc directly so
    // they remain valid for the pool's lifetime.
    const opts = try db.parseUrl(alloc, url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

test "integration: prompt lifecycle events are append-only and auditable" {
    const fixture = @import("../db/test_fixtures_prompt_events.zig");

    const db_ctx = (try openPromptEventTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    fixture.cleanup(db_ctx.conn);
    defer fixture.cleanup(db_ctx.conn);
    try fixture.seed(db_ctx.conn);

    try dbEmitter(db_ctx.conn).emit(.{
        .event_type = .prompt_birth,
        .workspace_id = fixture.WORKSPACE_ID,
        .tenant_id = fixture.TENANT_ID,
        .agent_id = fixture.AGENT_ID,
        .config_version_id = fixture.CONFIG_VERSION_ID,
        .metadata_json = "{}",
        .ts_ms = std.time.milliTimestamp(),
    });
    try dbEmitter(db_ctx.conn).emit(.{
        .event_type = .prompt_applied,
        .workspace_id = fixture.WORKSPACE_ID,
        .tenant_id = fixture.TENANT_ID,
        .agent_id = fixture.AGENT_ID,
        .config_version_id = fixture.CONFIG_VERSION_ID,
        .metadata_json = "{}",
        .ts_ms = std.time.milliTimestamp(),
    });

    // Scope the SELECT to this suite's workspace so pre-existing rows from
    // other suites don't leak into the assertion.
    var events_q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT event_type FROM core.prompt_lifecycle_events
        \\WHERE workspace_id = $1::uuid
        \\ORDER BY created_at ASC, id ASC
    , .{fixture.WORKSPACE_ID}));
    defer events_q.deinit();
    const first = (try events_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("prompt_birth", try first.get([]const u8, 0));
    const second = (try events_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("prompt_applied", try second.get([]const u8, 0));

    // Append-only invariant: the BEFORE UPDATE trigger must raise so the
    // exec returns an error. Scope to our workspace so we don't depend on
    // the table being empty.
    _ = db_ctx.conn.exec(
        \\UPDATE core.prompt_lifecycle_events
        \\SET metadata_json = '{"mutated":true}'
        \\WHERE workspace_id = $1::uuid
    , .{fixture.WORKSPACE_ID}) catch return;
    return error.TestUnexpectedResult;
}
