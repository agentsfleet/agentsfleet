//! Per-event execution telemetry store.
//!
//! Writes to `billing.usage_ledger`. All queries use PgQuery (RULE FLS).
//! Two rows per event under the credit-pool billing model:
//! `charge_type='receive'` is INSERTed at gate-pass; `charge_type='stage'` is
//! INSERTed before startStage and UPDATEd post-execution with token counts and
//! wall_ms. Idempotent on (event_id, charge_type) via ON CONFLICT DO NOTHING.
//! Cursor encode/decode lives in fleet_telemetry_cursor.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const pool_elevation = @import("../db/pool_elevation.zig");
const id_format = @import("../types/id_format.zig");
const tenant_provider = @import("tenant_provider.zig");
const cursor_mod = @import("fleet_telemetry_cursor.zig");

pub const ChargeType = enum {
    const Self = @This();

    receive,
    stage,

    pub fn label(self: Self) []const u8 {
        return switch (self) {
            .receive => "receive",
            .stage => "stage",
        };
    }
};

// Shared SELECT columns reused across all query branches.
// Trailing newline matters — concatenated suffix begins with WHERE/ORDER BY, so
// without it we'd get "usage_ledgerWHERE" (PG syntax error 42601).
// The identity columns are UUID, not TEXT. The `::text` casts are
// load-bearing: `row.get([]const u8, …)` on a UUID column hands back the raw
// 16 bytes with no error at compile time and none at runtime — just binary in
// the charges endpoint's JSON. `workspace_id` and `fleet_id` are also nullable
// now (ON DELETE SET NULL, schema/710), so their readers take optionals.
// The column list is positional: `queryRows` reads by index, so the ORDER here
// and the indices there must move together. `token_count_cached_input` is NOT
// selected — the charges response does not carry it (see the schema in
// `public/openapi/paths/billing.yaml`), and selecting a column the mapper does
// not read shifts every index after it by one.
const TELEMETRY_SELECT =
    \\SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, event_id,
    \\       charge_type, posture, model,
    \\       credit_deducted_nanos,
    \\       token_count_input, token_count_output, wall_ms,
    \\       created_at
    \\FROM billing.usage_ledger
    \\
;

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const TelemetryRow = struct {
    const Self = @This();

    id: []u8,
    tenant_id: []u8,
    /// Null once the referenced row is deleted — both foreign keys are ON DELETE
    /// SET NULL, so a charge outlives the fleet and workspace it was incurred on
    /// (`schema/710_usage_ledger.sql`; the cascade table in
    /// `http/handlers/fleets/delete.zig` states the intent). Serialized straight
    /// to JSON, so the charges response carries null here.
    workspace_id: ?[]u8,
    fleet_id: ?[]u8,
    event_id: []u8,
    charge_type: []u8,
    posture: []u8,
    model: []u8,
    credit_deducted_nanos: i64,
    token_count_input: ?i64,
    token_count_output: ?i64,
    wall_ms: ?i64,
    recorded_at: i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.tenant_id);
        if (self.workspace_id) |v| alloc.free(v);
        if (self.fleet_id) |v| alloc.free(v);
        alloc.free(self.event_id);
        alloc.free(self.charge_type);
        alloc.free(self.posture);
        alloc.free(self.model);
    }
};

pub const InsertTelemetryParams = struct {
    tenant_id: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    charge_type: ChargeType,
    posture: tenant_provider.Mode,
    model: []const u8,
    credit_deducted_nanos: i64,
    /// NULL on receive rows; accumulated on the stage row per slice by the
    /// renewal/settle CTE's upsert (`+= Δ` per renewal, ms-precision).
    token_count_input: ?i64 = null,
    token_count_cached_input: ?i64 = null,
    token_count_output: ?i64 = null,
    wall_ms: ?i64 = null,
    /// The originating EVENT's creation instant, not this row's. Every row for
    /// one event must carry the same value (schema/710), so it comes from the
    /// event envelope — never from a local clock read, which would differ per
    /// row by however long the paths took.
    event_created_at: i64,
    created_at: i64,
    /// The run's last charge instant. `null` means "same as `created_at`" — a
    /// one-shot charge, whose span is a point. Only a caller seeding a row that
    /// accumulated over time needs to set it; the production receive path never
    /// does, and the renewal/settle paths write their own statement.
    last_charged_at: ?i64 = null,
};

/// Insert one telemetry row. ON CONFLICT (event_id, charge_type) DO NOTHING —
/// safe to call on replay.
pub fn insertTelemetry(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: InsertTelemetryParams,
) !void {
    // A ledger row's own identifier, not a fleet's — `allocUuidV7` says that
    // where `generateFleetId` did not. The retired table carried BOTH a generated
    // UUID and a TEXT `id` holding the same value; one column replaces them.
    const row_id = try id_format.allocUuidV7(alloc);
    defer alloc.free(row_id);

    // Ledger writes belong to `billing_runtime` (schema/710); api_runtime
    // keeps SELECT only. The callback brackets this one INSERT's transaction.
    var scope = try pool_elevation.begin(conn, .billing);
    defer scope.deinit();
    // `last_charged_at` equals `created_at` here: a receive fee is charged once,
    // so its span is a point and the budget drain's apportionment degenerates to
    // all-or-nothing, which is what it always was for this row (schema/710).
    _ = try scope.conn.exec(
        \\INSERT INTO billing.usage_ledger
        \\  (id, tenant_id, workspace_id, fleet_id, event_id,
        \\   charge_type, posture, model,
        \\   credit_deducted_nanos,
        \\   token_count_input, token_count_cached_input, token_count_output, wall_ms,
        \\   event_created_at, created_at, last_charged_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, $10,
        \\        $11, $12, $13, $14, $15, $16)
        \\ON CONFLICT (event_id, charge_type) DO NOTHING
    , .{
        row_id,
        params.tenant_id,
        params.workspace_id,
        params.fleet_id,
        params.event_id,
        params.charge_type.label(),
        params.posture.label(),
        params.model,
        params.credit_deducted_nanos,
        params.token_count_input,
        params.token_count_cached_input,
        params.token_count_output,
        params.wall_ms,
        params.event_created_at,
        params.created_at,
        params.last_charged_at orelse params.created_at,
    });
    try scope.commit();
}

/// Build an opaque base64url cursor token from the last row of a page.
pub fn makeCursor(alloc: std.mem.Allocator, row: TelemetryRow) ![]u8 {
    return cursor_mod.makeCursor(alloc, row.recorded_at, row.id);
}

// `usage_ledger.id` is TABLE-QUALIFIED in both ORDER BYs, and that qualification
// is load-bearing. `TELEMETRY_SELECT` emits `id::text`, which names an OUTPUT
// column `id`; a bare `ORDER BY … id DESC` resolves against the output list
// before the table, so it would sort by the TEXT cast. The index orders the raw
// uuid, so the planner could not supply that ordering and added an Incremental
// Sort to every page — the exact per-page sort `schema/720_usage_ledger_indexes`
// reshaped the index to remove. The WHERE clause never had the problem: output
// aliases are not visible there, so the seek always bound to the real column.
// Public so the index-fitness suite plans against this exact text rather than a
// copy that could drift from it.
pub const SELECT_TENANT_CHARGES_PAGE_AFTER = TELEMETRY_SELECT ++
    \\WHERE tenant_id = $1
    \\  AND (created_at, id) < ($2, $3)
    \\ORDER BY created_at DESC, usage_ledger.id DESC
    \\LIMIT $4
;

pub const SELECT_TENANT_CHARGES_PAGE_FIRST = TELEMETRY_SELECT ++
    \\WHERE tenant_id = $1
    \\ORDER BY created_at DESC, usage_ledger.id DESC
    \\LIMIT $2
;

/// Tenant-scoped charges query — backs `GET /v1/tenants/me/billing/charges`
/// (read by the Settings → Billing dashboard's Usage tab and `agentsfleet
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
        return queryRows(conn, alloc, SELECT_TENANT_CHARGES_PAGE_AFTER, .{ tenant_id, parsed.recorded_at, parsed.id, @as(i32, @intCast(limit)) });
    }
    return queryRows(conn, alloc, SELECT_TENANT_CHARGES_PAGE_FIRST, .{ tenant_id, @as(i32, @intCast(limit)) });
}

// ── Internal helpers ────────────────────────────────────────────────

/// Copy a nullable text column into owned memory; caller must free when non-null.
fn dupeOptional(alloc: std.mem.Allocator, value: ?[]const u8) !?[]u8 {
    return if (value) |v| try alloc.dupe(u8, v) else null;
}

fn queryRows(conn: *pg.Conn, alloc: std.mem.Allocator, comptime sql: []const u8, params: anytype) ![]TelemetryRow {
    var q = PgQuery.from(try conn.query(sql, params));
    defer q.deinit();

    var rows: std.ArrayList(TelemetryRow) = .empty;
    errdefer {
        for (rows.items) |*r| r.deinit(alloc);
        rows.deinit(alloc);
    }

    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        const tenant_id_s = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(tenant_id_s);
        const workspace_id_s = try dupeOptional(alloc, try row.get(?[]const u8, 2));
        errdefer if (workspace_id_s) |v| alloc.free(v);
        const fleet_id_s = try dupeOptional(alloc, try row.get(?[]const u8, 3));
        errdefer if (fleet_id_s) |v| alloc.free(v);
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
            .fleet_id = fleet_id_s,
            .event_id = event_id_s,
            .charge_type = charge_type_s,
            .posture = posture_s,
            .model = model_s,
            // Indices track TELEMETRY_SELECT's column order exactly: adding a
            // column there without a read here shifts every index below it, and
            // each value then arrives under the next field's name.
            .credit_deducted_nanos = try row.get(i64, 8),
            .token_count_input = try row.get(?i64, 9),
            .token_count_output = try row.get(?i64, 10),
            .wall_ms = try row.get(?i64, 11),
            .recorded_at = try row.get(i64, 12),
        });
    }

    return rows.toOwnedSlice(alloc);
}

test {
    _ = cursor_mod;
    _ = @import("fleet_telemetry_store_test.zig");
}
