//! Per-event execution telemetry store.
//!
//! Writes to `zombie_execution_telemetry`. All queries use PgQuery (RULE FLS).
//! Two rows per event under the credit-pool billing model:
//! `charge_type='receive'` is INSERTed at gate-pass; `charge_type='stage'` is
//! INSERTed before startStage and UPDATEd post-execution with token counts and
//! wall_ms. Idempotent on (event_id, charge_type) via ON CONFLICT DO NOTHING.
//! Cursor encode/decode lives in zombie_telemetry_cursor.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const tenant_provider = @import("tenant_provider.zig");
const cursor_mod = @import("zombie_telemetry_cursor.zig");

pub const ChargeType = enum {
    receive,
    stage,

    pub fn label(self: ChargeType) []const u8 {
        return switch (self) {
            .receive => "receive",
            .stage => "stage",
        };
    }
};

// Shared SELECT columns reused across all query branches.
// Trailing newline matters — concatenated suffix begins with WHERE/ORDER BY, so
// without it we'd get "zombie_execution_telemetryWHERE" (PG syntax error 42601).
const TELEMETRY_SELECT =
    \\SELECT id, tenant_id, workspace_id, zombie_id, event_id,
    \\       charge_type, posture, model,
    \\       credit_deducted_cents,
    \\       token_count_input, token_count_output, wall_ms,
    \\       recorded_at
    \\FROM zombie_execution_telemetry
    \\
;

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const TelemetryRow = struct {
    id: []u8,
    tenant_id: []u8,
    workspace_id: []u8,
    zombie_id: []u8,
    event_id: []u8,
    charge_type: []u8,
    posture: []u8,
    model: []u8,
    credit_deducted_cents: i64,
    token_count_input: ?i64,
    token_count_output: ?i64,
    wall_ms: ?i64,
    recorded_at: i64,

    pub fn deinit(self: *TelemetryRow, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.tenant_id);
        alloc.free(self.workspace_id);
        alloc.free(self.zombie_id);
        alloc.free(self.event_id);
        alloc.free(self.charge_type);
        alloc.free(self.posture);
        alloc.free(self.model);
    }
};

pub const InsertTelemetryParams = struct {
    tenant_id: []const u8,
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    charge_type: ChargeType,
    posture: tenant_provider.Mode,
    model: []const u8,
    credit_deducted_cents: i64,
    /// NULL on receive rows; set on stage rows post-execution via updateStageTokens.
    token_count_input: ?i64 = null,
    /// NULL on receive rows; set on stage rows post-execution via updateStageTokens.
    token_count_output: ?i64 = null,
    /// NULL on receive rows; set on stage rows post-execution via updateStageTokens.
    wall_ms: ?i64 = null,
    recorded_at: i64,
};

/// Insert one telemetry row. ON CONFLICT (event_id, charge_type) DO NOTHING —
/// safe to call on replay.
pub fn insertTelemetry(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: InsertTelemetryParams,
) !void {
    const row_id = try id_format.generateZombieId(alloc);
    defer alloc.free(row_id);

    _ = try conn.exec(
        \\INSERT INTO zombie_execution_telemetry
        \\  (id, tenant_id, workspace_id, zombie_id, event_id,
        \\   charge_type, posture, model,
        \\   credit_deducted_cents,
        \\   token_count_input, token_count_output, wall_ms,
        \\   recorded_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        \\ON CONFLICT (event_id, charge_type) DO NOTHING
    , .{
        row_id,
        params.tenant_id,
        params.workspace_id,
        params.zombie_id,
        params.event_id,
        params.charge_type.label(),
        params.posture.label(),
        params.model,
        params.credit_deducted_cents,
        params.token_count_input,
        params.token_count_output,
        params.wall_ms,
        params.recorded_at,
    });
}

/// Update the stage row for an event with the executor's reported token counts
/// and wall_ms once startStage returns. Idempotent on (event_id, charge_type=stage).
pub fn updateStageTokens(
    conn: *pg.Conn,
    event_id: []const u8,
    token_count_input: i64,
    token_count_output: i64,
    wall_ms: i64,
) !void {
    _ = try conn.exec(
        \\UPDATE zombie_execution_telemetry
        \\   SET token_count_input  = $2,
        \\       token_count_output = $3,
        \\       wall_ms            = $4
        \\ WHERE event_id    = $1
        \\   AND charge_type = 'stage'
    , .{ event_id, token_count_input, token_count_output, wall_ms });
}

/// Build an opaque base64url cursor token from the last row of a page.
pub fn makeCursor(alloc: std.mem.Allocator, row: TelemetryRow) ![]u8 {
    return cursor_mod.makeCursor(alloc, row.recorded_at, row.id);
}

/// Tenant-scoped charges query — backs `GET /v1/tenants/me/billing/charges`
/// (read by the Settings → Billing dashboard's Usage tab and `zombiectl
/// billing show`). Newest-first with cursor pagination over `(recorded_at,
/// id)`; cursor is opaque to callers and produced by `makeCursor`.
pub fn listTelemetryForTenant(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    tenant_id: []const u8,
    limit: u32,
    cursor: ?[]const u8,
) ![]TelemetryRow {
    if (cursor) |c| {
        const parsed = try cursor_mod.parseCursor(alloc, c);
        defer alloc.free(parsed.id);
        return queryRows(conn, alloc, TELEMETRY_SELECT ++
            \\WHERE tenant_id = $1
            \\  AND (recorded_at, id) < ($2, $3)
            \\ORDER BY recorded_at DESC, id DESC
            \\LIMIT $4
        , .{ tenant_id, parsed.recorded_at, parsed.id, @as(i32, @intCast(limit)) });
    }
    return queryRows(conn, alloc, TELEMETRY_SELECT ++
        \\WHERE tenant_id = $1
        \\ORDER BY recorded_at DESC, id DESC
        \\LIMIT $2
    , .{ tenant_id, @as(i32, @intCast(limit)) });
}

// ── Internal helpers ────────────────────────────────────────────────

fn queryRows(conn: *pg.Conn, alloc: std.mem.Allocator, comptime sql: []const u8, params: anytype) ![]TelemetryRow {
    var q = PgQuery.from(try conn.query(sql, params));
    defer q.deinit();

    var rows: std.ArrayList(TelemetryRow) = .{};
    errdefer {
        for (rows.items) |*r| r.deinit(alloc);
        rows.deinit(alloc);
    }

    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const tenant_id_s = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(tenant_id_s);
        const workspace_id_s = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(workspace_id_s);
        const zombie_id_s = try alloc.dupe(u8, try row.get([]const u8, 3));
        errdefer alloc.free(zombie_id_s);
        const event_id_s = try alloc.dupe(u8, try row.get([]const u8, 4));
        errdefer alloc.free(event_id_s);
        const charge_type_s = try alloc.dupe(u8, try row.get([]const u8, 5));
        errdefer alloc.free(charge_type_s);
        const posture_s = try alloc.dupe(u8, try row.get([]const u8, 6));
        errdefer alloc.free(posture_s);
        const model_s = try alloc.dupe(u8, try row.get([]const u8, 7));
        errdefer alloc.free(model_s);

        try rows.append(alloc, .{
            .id = id,
            .tenant_id = tenant_id_s,
            .workspace_id = workspace_id_s,
            .zombie_id = zombie_id_s,
            .event_id = event_id_s,
            .charge_type = charge_type_s,
            .posture = posture_s,
            .model = model_s,
            .credit_deducted_cents = try row.get(i64, 8),
            .token_count_input = try row.get(?i64, 9),
            .token_count_output = try row.get(?i64, 10),
            .wall_ms = try row.get(?i64, 11),
            .recorded_at = try row.get(i64, 12),
        });
    }

    return rows.toOwnedSlice(alloc);
}

test {
    _ = @import("zombie_telemetry_cursor.zig");
    _ = @import("zombie_telemetry_store_test.zig");
}
