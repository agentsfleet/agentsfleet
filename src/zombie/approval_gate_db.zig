// Approval gate DB persistence — audit table writes and atomic resolution.
//
// Writes to core.zombie_approval_gates. The schema-level append-only trigger
// permits UPDATE only when OLD.status='pending', which IS the dedup
// precondition for resolution: concurrent resolvers (Slack callback,
// dashboard handler, sweeper) race against the same WHERE clause and exactly
// one wins. Losers observe RETURNING 0 rows and surface 409 to their caller.
//
// Inbox reads (listPending, getByGateId) live in approval_gate_db_reads.zig
// and are re-exported here for callers that want a single import.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const log = std.log.scoped(.approval_gate_db);

const reads = @import("approval_gate_db_reads.zig");
const GateStatus = @import("approval_gate.zig").GateStatus;
const ActionDetail = @import("approval_gate.zig").ActionDetail;

const PENDING_STATUS = GateStatus.pending.toSlice();

// ── Public types ────────────────────────────────────────────────────────

pub const ResolvedRow = struct {
    gate_id: []const u8,
    action_id: []const u8,
    workspace_id: []const u8,
    zombie_id: []const u8,
    outcome: GateStatus,
    resolved_at: i64,
    resolved_by: []const u8,
    detail: []const u8,

    pub fn deinit(self: *ResolvedRow, alloc: Allocator) void {
        alloc.free(self.gate_id);
        alloc.free(self.action_id);
        alloc.free(self.workspace_id);
        alloc.free(self.zombie_id);
        alloc.free(self.resolved_by);
        alloc.free(self.detail);
    }
};

pub const ResolveDbOutcome = union(enum) {
    resolved: ResolvedRow,
    already_resolved: ResolvedRow,
    not_found,

    pub fn deinit(self: *ResolveDbOutcome, alloc: Allocator) void {
        switch (self.*) {
            .resolved => |*r| r.deinit(alloc),
            .already_resolved => |*r| r.deinit(alloc),
            .not_found => {},
        }
    }
};

// Re-exports from the reads sibling so callers get a single import surface.
pub const PendingRow = reads.PendingRow;
pub const ListFilter = reads.ListFilter;
pub const ListResult = reads.ListResult;
pub const listPending = reads.listPending;
pub const getByGateId = reads.getByGateId;

// ── Writes ──────────────────────────────────────────────────────────────

/// Insert a pending gate row. Best-effort — logs on failure, does not propagate.
/// Resolution updates this row via resolveAtomic / resolveGateDecision.
pub fn recordGatePending(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    detail: ActionDetail,
) void {
    insertPendingRow(pool, alloc, zombie_id, workspace_id, action_id, detail) catch |err| {
        log.err("approval_gate.record_pending_fail err={s} action_id={s}", .{ @errorName(err), action_id });
    };
}

/// DB-only resolve: thin wrapper over resolveAtomic that discards the rich outcome.
/// Retained for the worker timeout path and other call sites that don't need
/// dedup attribution. New code calling this should consider the channel-agnostic
/// `approval_gate.resolve()` instead.
pub fn resolveGateDecision(
    pool: *pg.Pool,
    action_id: []const u8,
    status: GateStatus,
    detail: []const u8,
) void {
    const sink_alloc = std.heap.page_allocator;
    var outcome = resolveAtomic(pool, sink_alloc, action_id, status, "", detail) catch |err| {
        log.err("approval_gate.resolve_fail err={s} action_id={s}", .{ @errorName(err), action_id });
        return;
    };
    outcome.deinit(sink_alloc);
}

/// Atomic resolution. Returns the canonical resolver attribution either way:
/// .resolved means this caller won the race; .already_resolved means an
/// earlier writer (different channel or concurrent retry) already terminated
/// the row, and the returned attribution is what should surface to the user.
pub fn resolveAtomic(
    pool: *pg.Pool,
    alloc: Allocator,
    action_id: []const u8,
    outcome: GateStatus,
    by: []const u8,
    reason: []const u8,
) !ResolveDbOutcome {
    if (outcome == .pending) return error.InvalidGateStatus;

    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    var update_q = PgQuery.from(try conn.query(
        \\UPDATE core.zombie_approval_gates
        \\SET status = $1, detail = $2, resolved_by = $3, updated_at = $4
        \\WHERE action_id = $5 AND status = $6
        \\RETURNING id::text, action_id, workspace_id::text, zombie_id::text,
        \\          status, COALESCE(updated_at, $4::bigint), resolved_by, detail
    , .{ outcome.toSlice(), reason, by, now_ms, action_id, PENDING_STATUS }));
    defer update_q.deinit();

    if (try update_q.next()) |row| {
        return .{ .resolved = try readResolvedRow(alloc, row) };
    }

    var select_q = PgQuery.from(try conn.query(
        \\SELECT id::text, action_id, workspace_id::text, zombie_id::text,
        \\       status, COALESCE(updated_at, requested_at), resolved_by, detail
        \\FROM core.zombie_approval_gates
        \\WHERE action_id = $1
        \\ORDER BY requested_at DESC LIMIT 1
    , .{action_id}));
    defer select_q.deinit();

    if (try select_q.next()) |row| {
        return .{ .already_resolved = try readResolvedRow(alloc, row) };
    }
    return .not_found;
}

// ── Internals ───────────────────────────────────────────────────────────

fn insertPendingRow(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    detail: ActionDetail,
) !void {
    const gate_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(gate_id);

    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    const timeout_at = now_ms +| detail.timeout_ms;
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   status, detail, requested_at, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10, $11, $12, '', $13, $13)
    , .{
        gate_id, zombie_id, workspace_id, action_id, detail.tool, detail.action,
        detail.gate_kind, detail.proposed_action, detail.evidence_json,
        detail.blast_radius, timeout_at, PENDING_STATUS, now_ms,
    });
}

fn readResolvedRow(alloc: Allocator, row: pg.Row) !ResolvedRow {
    const gate_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(gate_id);
    const action_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(action_id);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(workspace_id);
    const zombie_id = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(zombie_id);
    const status_str = try row.get([]const u8, 4);
    const resolved_at = try row.get(i64, 5);
    const resolved_by = try alloc.dupe(u8, try row.get([]const u8, 6));
    errdefer alloc.free(resolved_by);
    const detail = try alloc.dupe(u8, try row.get([]const u8, 7));

    return .{
        .gate_id = gate_id, .action_id = action_id,
        .workspace_id = workspace_id, .zombie_id = zombie_id,
        .outcome = parseStatus(status_str), .resolved_at = resolved_at,
        .resolved_by = resolved_by, .detail = detail,
    };
}

fn parseStatus(s: []const u8) GateStatus {
    if (std.mem.eql(u8, s, "approved")) return .approved;
    if (std.mem.eql(u8, s, "denied")) return .denied;
    if (std.mem.eql(u8, s, "timed_out")) return .timed_out;
    if (std.mem.eql(u8, s, "auto_killed")) return .auto_killed;
    return .pending;
}
