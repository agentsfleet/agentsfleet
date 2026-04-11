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

pub const PromptEvent = struct {
    event_type: EventType,
    workspace_id: []const u8,
    tenant_id: []const u8,
    agent_id: ?[]const u8 = null,
    config_version_id: ?[]const u8 = null,
    metadata_json: []const u8 = "{}",
    ts_ms: i64,
};

pub const Emitter = struct {
    ctx: *anyopaque,
    emit_fn: *const fn (ctx: *anyopaque, event: PromptEvent) anyerror!void,

    pub fn emit(self: Emitter, event: PromptEvent) !void {
        return self.emit_fn(self.ctx, event);
    }
};

const DbEmitterCtx = struct {
    conn: *pg.Conn,
};

pub fn dbEmitter(conn: *pg.Conn) Emitter {
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
        \\INSERT INTO prompt_lifecycle_events
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

pub fn emitBestEffort(conn: *pg.Conn, event: PromptEvent) void {
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
    const db_ctx = (try openPromptEventTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE prompt_lifecycle_events (
        \\  id UUID PRIMARY KEY,
        \\  event_id TEXT NOT NULL UNIQUE,
        \\  event_type TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  tenant_id TEXT NOT NULL,
        \\  agent_id TEXT,
        \\  config_version_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE OR REPLACE FUNCTION reject_prompt_lifecycle_event_mutation()
        \\RETURNS trigger LANGUAGE plpgsql AS $$
        \\BEGIN
        \\  RAISE EXCEPTION 'prompt_lifecycle_events is append-only';
        \\END;
        \\$$
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TRIGGER trg_prompt_lifecycle_events_no_update
        \\BEFORE UPDATE ON prompt_lifecycle_events
        \\FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation()
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TRIGGER trg_prompt_lifecycle_events_no_delete
        \\BEFORE DELETE ON prompt_lifecycle_events
        \\FOR EACH ROW EXECUTE FUNCTION reject_prompt_lifecycle_event_mutation()
    , .{});

    emitBestEffort(db_ctx.conn, .{
        .event_type = .prompt_birth,
        .workspace_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        .tenant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01",
        .agent_id = "agent_1",
        .config_version_id = "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98",
        .metadata_json = "{}",
        .ts_ms = std.time.milliTimestamp(),
    });
    emitBestEffort(db_ctx.conn, .{
        .event_type = .prompt_applied,
        .workspace_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        .tenant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01",
        .agent_id = "agent_1",
        .config_version_id = "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98",
        .metadata_json = "{}",
        .ts_ms = std.time.milliTimestamp(),
    });

    var events_q = PgQuery.from(try db_ctx.conn.query(
        "SELECT event_type FROM prompt_lifecycle_events ORDER BY id ASC",
        .{},
    ));
    defer events_q.deinit();
    const first = (try events_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("prompt_birth", try first.get([]const u8, 0));
    const second = (try events_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("prompt_applied", try second.get([]const u8, 0));

    _ = db_ctx.conn.exec(
        "UPDATE prompt_lifecycle_events SET metadata_json = '{\"mutated\":true}'",
        .{},
    ) catch return;
    return error.TestUnexpectedResult;
}
