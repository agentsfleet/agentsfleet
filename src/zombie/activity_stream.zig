// Activity stream — append-only event log for Zombie actions.
//
// Write path: logEvent — called by the event loop after every action.
//   Never errors to the caller. If Postgres is unavailable, logs to stderr
//   and returns. The Zombie keeps running regardless of log write failure.
//
// Read path: queryByZombie, queryByWorkspace — called by the API for
//   `zombiectl logs`. Cursor-based pagination using created_at DESC.
//   Cursor is the created_at (BIGINT ms) of the last seen event, encoded
//   as a decimal string. Null cursor = first page.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const activity_cursor = @import("activity_cursor.zig");

const log = std.log.scoped(.activity_stream);

pub const MAX_ACTIVITY_PAGE_LIMIT: u32 = 100;
pub const DEFAULT_ACTIVITY_PAGE_LIMIT: u32 = 20;

// Event types written to core.activity_events.
// Used by the event loop (write path) and zombiectl logs (filter path).
// All strings that appear in event_type column must be declared here.
pub const EVT_ZOMBIE_STARTED = "zombie_started";
pub const EVT_ZOMBIE_STOPPED = "zombie_stopped";
pub const EVT_EVENT_RECEIVED = "event_received";
pub const EVT_EVENT_ERROR = "event_error";
pub const EVT_AGENT_RESPONSE = "agent_response";
pub const EVT_AGENT_ERROR = "agent_error";
// Balance-gate events (M11_006 §3/§5).
pub const EVT_BALANCE_EXHAUSTED_FIRST_DEBIT = "balance_exhausted_first_debit";
pub const EVT_BALANCE_EXHAUSTED = "balance_exhausted";
pub const EVT_BALANCE_GATE_BLOCKED = "balance_gate_blocked";

pub const ActivityEvent = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    event_type: []const u8,
    detail: []const u8,
};

pub const ActivityEventRow = struct {
    id: []const u8,
    zombie_id: []const u8,
    workspace_id: []const u8,
    event_type: []const u8,
    detail: []const u8,
    created_at: i64,

    pub fn deinit(self: *const ActivityEventRow, alloc: Allocator) void {
        alloc.free(self.id);
        alloc.free(self.zombie_id);
        alloc.free(self.workspace_id);
        alloc.free(self.event_type);
        alloc.free(self.detail);
    }
};

pub const ActivityPage = struct {
    events: []ActivityEventRow,
    // Decimal string of created_at of the last event in this page.
    // Pass as cursor to the next call. Null means no more pages.
    next_cursor: ?[]const u8,

    pub fn deinit(self: *const ActivityPage, alloc: Allocator) void {
        for (self.events) |*e| e.deinit(alloc);
        alloc.free(self.events);
        if (self.next_cursor) |c| alloc.free(c);
    }
};

// Fire-and-forget write via a pool connection.
pub fn logEvent(pool: *pg.Pool, alloc: Allocator, event: ActivityEvent) void {
    writeActivityEvent(pool, alloc, event) catch |err| {
        log.err("activity_stream.write_failed err={s} zombie_id={s} event_type={s}", .{
            @errorName(err), event.zombie_id, event.event_type,
        });
    };
}

// Fire-and-forget write on a caller-owned conn (RULE CNX).
pub fn logEventOnConn(conn: *pg.Conn, alloc: Allocator, event: ActivityEvent) void {
    writeActivityEventOnConn(conn, alloc, event) catch |err| {
        log.err("activity_stream.write_failed err={s} zombie_id={s} event_type={s}", .{
            @errorName(err), event.zombie_id, event.event_type,
        });
    };
}

// Rate-limit probe: true iff an event of `event_type` was written for
// `workspace_id` after `since_ms`. Fail-open on DB error (caller emits).
pub fn hasRecentActivityEventOnConn(
    conn: *pg.Conn,
    workspace_id: []const u8,
    event_type: []const u8,
    since_ms: i64,
) bool {
    var q = PgQuery.from(conn.query(
        \\SELECT 1 FROM core.activity_events
        \\WHERE workspace_id = $1 AND event_type = $2 AND created_at >= $3
        \\LIMIT 1
    , .{ workspace_id, event_type, since_ms }) catch return false);
    defer q.deinit();
    const row = q.next() catch return false;
    return row != null;
}

// Tenant-scoped rate-limit probe: true iff an event of `event_type`
// was written for ANY workspace owned by `tenant_id` after `since_ms`.
// Used by the metering warn-policy path so a tenant with N workspaces
// does not receive up to N `balance_exhausted` notifications on the
// same day. JOIN through `core.workspaces` keeps the owning tenant
// authoritative; the index on `activity_events.workspace_id` still
// drives the scan. Fail-open on DB error.
pub fn hasRecentActivityEventForTenantOnConn(
    conn: *pg.Conn,
    tenant_id: []const u8,
    event_type: []const u8,
    since_ms: i64,
) bool {
    var q = PgQuery.from(conn.query(
        \\SELECT 1
        \\FROM core.activity_events ae
        \\JOIN core.workspaces w ON w.workspace_id = ae.workspace_id
        \\WHERE w.tenant_id = $1::uuid
        \\  AND ae.event_type = $2
        \\  AND ae.created_at >= $3
        \\LIMIT 1
    , .{ tenant_id, event_type, since_ms }) catch return false);
    defer q.deinit();
    const row = q.next() catch return false;
    return row != null;
}

// queryByZombie returns a page of events for a single Zombie, ordered newest first.
// cursor: decimal string of created_at from the previous page's last event, or null.
// limit: capped at MAX_ACTIVITY_PAGE_LIMIT.
pub fn queryByZombie(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !ActivityPage {
    const conn = try pool.acquire();
    defer pool.release(conn);
    return queryByZombieOnConn(conn, alloc, zombie_id, cursor, limit);
}

// queryByZombieOnConn reuses a caller-owned connection. Use this from request
// handlers that already acquired a conn for authorization — it avoids holding
// two connections from the pool concurrently for a single request.
pub fn queryByZombieOnConn(
    conn: *pg.Conn,
    alloc: Allocator,
    zombie_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !ActivityPage {
    const page_limit = @min(limit, MAX_ACTIVITY_PAGE_LIMIT);
    return if (cursor) |c|
        fetchActivityPage(conn, alloc, .zombie, zombie_id, c, page_limit)
    else
        fetchActivityPageFirst(conn, alloc, .zombie, zombie_id, page_limit);
}

// queryByWorkspace returns a page of events across all Zombies in a workspace.
pub fn queryByWorkspace(
    pool: *pg.Pool,
    alloc: Allocator,
    workspace_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !ActivityPage {
    const conn = try pool.acquire();
    defer pool.release(conn);
    return queryByWorkspaceOnConn(conn, alloc, workspace_id, cursor, limit);
}

// queryByWorkspaceOnConn reuses a caller-owned connection. Use this from request
// handlers that already acquired a conn for authorization — avoids RULE CNX
// (holding two pool connections concurrently per request).
pub fn queryByWorkspaceOnConn(
    conn: *pg.Conn,
    alloc: Allocator,
    workspace_id: []const u8,
    cursor: ?[]const u8,
    limit: u32,
) !ActivityPage {
    const page_limit = @min(limit, MAX_ACTIVITY_PAGE_LIMIT);
    return if (cursor) |c|
        fetchActivityPage(conn, alloc, .workspace, workspace_id, c, page_limit)
    else
        fetchActivityPageFirst(conn, alloc, .workspace, workspace_id, page_limit);
}

// --- internal ---

const FilterKind = enum { zombie, workspace };

fn writeActivityEvent(pool: *pg.Pool, alloc: Allocator, event: ActivityEvent) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    try writeActivityEventOnConn(conn, alloc, event);
}

fn writeActivityEventOnConn(conn: *pg.Conn, alloc: Allocator, event: ActivityEvent) !void {
    const event_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(event_id);

    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.activity_events
        \\  (id, zombie_id, workspace_id, event_type, detail, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6)
    , .{ event_id, event.zombie_id, event.workspace_id, event.event_type, event.detail, now_ms });
}

fn fetchActivityPageFirst(
    conn: *pg.Conn,
    alloc: Allocator,
    kind: FilterKind,
    filter_id: []const u8,
    limit: u32,
) !ActivityPage {
    const sql_zombie =
        \\SELECT id, zombie_id, workspace_id, event_type, detail, created_at
        \\FROM core.activity_events
        \\WHERE zombie_id = $1
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $2
    ;
    const sql_workspace =
        \\SELECT id, zombie_id, workspace_id, event_type, detail, created_at
        \\FROM core.activity_events
        \\WHERE workspace_id = $1
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $2
    ;
    const sql = if (kind == .zombie) sql_zombie else sql_workspace;

    var q = PgQuery.from(try conn.query(sql, .{ filter_id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectActivityPage(alloc, &q, limit);
}

fn fetchActivityPage(
    conn: *pg.Conn,
    alloc: Allocator,
    kind: FilterKind,
    filter_id: []const u8,
    cursor: []const u8,
    limit: u32,
) !ActivityPage {
    // Composite keyset cursor prevents silent skips when multiple events share
    // a millisecond timestamp. See activity_cursor.zig.
    const parsed = activity_cursor.parse(cursor) catch return error.InvalidCursor;
    const cursor_ts = parsed.created_at_ms;
    const cursor_id = parsed.id;

    const sql_zombie =
        \\SELECT id, zombie_id, workspace_id, event_type, detail, created_at
        \\FROM core.activity_events
        \\WHERE zombie_id = $1
        \\  AND (created_at < $2 OR (created_at = $2 AND id < $3))
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $4
    ;
    const sql_workspace =
        \\SELECT id, zombie_id, workspace_id, event_type, detail, created_at
        \\FROM core.activity_events
        \\WHERE workspace_id = $1
        \\  AND (created_at < $2 OR (created_at = $2 AND id < $3))
        \\ORDER BY created_at DESC, id DESC
        \\LIMIT $4
    ;
    const sql = if (kind == .zombie) sql_zombie else sql_workspace;

    var q = PgQuery.from(try conn.query(sql, .{ filter_id, cursor_ts, cursor_id, @as(i64, @intCast(limit)) }));
    defer q.deinit();
    return collectActivityPage(alloc, &q, limit);
}

fn collectActivityPage(alloc: Allocator, q: *PgQuery, limit: u32) !ActivityPage {
    var rows: std.ArrayList(ActivityEventRow) = .{};
    errdefer {
        for (rows.items) |*r| r.deinit(alloc);
        rows.deinit(alloc);
    }

    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const zombie_id = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(zombie_id);
        const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(workspace_id);
        const event_type = try alloc.dupe(u8, try row.get([]const u8, 3));
        errdefer alloc.free(event_type);
        const detail = try alloc.dupe(u8, try row.get([]const u8, 4));
        errdefer alloc.free(detail);
        const created_at = try row.get(i64, 5);

        try rows.append(alloc, .{
            .id = id,
            .zombie_id = zombie_id,
            .workspace_id = workspace_id,
            .event_type = event_type,
            .detail = detail,
            .created_at = created_at,
        });
    }

    const events = try rows.toOwnedSlice(alloc);
    errdefer {
        for (events) |*e| e.deinit(alloc);
        alloc.free(events);
    }

    // If we got a full page there may be more — produce a composite cursor from the
    // last row: "{created_at}:{id}". The composite keyset prevents silent skips when
    // multiple events share the same millisecond timestamp.
    const next_cursor: ?[]const u8 = if (events.len == limit) blk: {
        const last = events[events.len - 1];
        break :blk try activity_cursor.format(alloc, .{ .created_at_ms = last.created_at, .id = last.id });
    } else null;

    return ActivityPage{ .events = events, .next_cursor = next_cursor };
}

test {
    _ = @import("activity_stream_test.zig");
}
