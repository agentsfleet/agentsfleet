// Approval gate DB persistence — audit table writes and atomic resolution.
//
// Writes to core.fleet_approval_gates. The schema-level append-only trigger
// permits UPDATE only when OLD.status='pending', which IS the dedup
// precondition for resolution: concurrent resolvers (Slack callback,
// dashboard handler, sweeper) race against the same WHERE clause and exactly
// one wins. Losers observe RETURNING 0 rows and surface 409 to their caller.
//
// Inbox reads (listPending, getByGateId) live in approval_gate_db_reads.zig
// and are re-exported here for callers that want a single import.

const std = @import("std");
const sql = @import("sql.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// Transaction-scoped opt-out of the gates append-only trigger (schema/026).
/// ONLY the two hard-purge paths (account teardown, fleet hard-delete) may
/// execute this, immediately after BEGIN — SET LOCAL dies with the
/// transaction, so the bypass can never leak to a pooled connection's next
/// acquirer. Every other DELETE on the gates table still raises.
pub const SET_GATE_PURGE_BYPASS_SQL = "SET LOCAL fleet.allow_gate_purge = 'on'";
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");

const log = logging.scoped(.approval_gate_db);

const reads = @import("approval_gate_db_reads.zig");
const approval_gate = @import("approval_gate.zig");
const GateStatus = approval_gate.GateStatus;
const ActionDetail = approval_gate.ActionDetail;

const PENDING_STATUS = GateStatus.pending.toSlice();

// ── Public types ────────────────────────────────────────────────────────

pub const ResolvedRow = struct {
    const Self = @This();

    gate_id: []const u8,
    action_id: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    outcome: GateStatus,
    resolved_at: i64,
    resolved_by: []const u8,
    detail: []const u8,

    pub fn deinit(self: *Self, alloc: Allocator) void {
        alloc.free(self.gate_id);
        alloc.free(self.action_id);
        alloc.free(self.workspace_id);
        alloc.free(self.fleet_id);
        alloc.free(self.resolved_by);
        alloc.free(self.detail);
    }
};

pub const ResolveDbOutcome = union(enum) {
    const Self = @This();

    resolved: ResolvedRow,
    already_resolved: ResolvedRow,
    not_found,

    pub fn deinit(self: *Self, alloc: Allocator) void {
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
pub const listPending = reads.listPending;
pub const getByGateId = reads.getByGateId;

// ── Writes ──────────────────────────────────────────────────────────────

/// Insert a pending gate row. Best-effort — logs on failure, does not propagate.
/// Resolution updates this row via `ResolveArgs.atomic` / `resolveGateDecision`.
pub fn recordGatePending(
    pool: *pg.Pool,
    alloc: Allocator,
    fleet_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    detail: ActionDetail,
) void {
    insertPendingRow(pool, alloc, fleet_id, workspace_id, action_id, detail) catch |err| {
        log.err("record_pending_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .action_id = action_id });
    };
}

/// DB-only resolve: thin wrapper that discards the rich outcome.
/// Retained for the worker timeout path and other call sites that don't need
/// dedup attribution. New code should call `approval_gate.resolve()` or
/// `ResolveArgs.atomic()` directly.
pub fn resolveGateDecision(
    pool: *pg.Pool,
    action_id: []const u8,
    status: GateStatus,
    by: []const u8,
    detail: []const u8,
    sink_alloc: Allocator,
) void {
    // sink_alloc owns the transient ResolvedRow the outcome duplicates and frees
    // below. Production passes `page_allocator` (the row is freed before return,
    // so nothing is leaked to the process); a leak test passes
    // `testing.allocator` to audit that free.
    const args: ResolveArgs = .{
        .action_id = action_id,
        .outcome = status,
        .by = by,
        .reason = detail,
    };
    var outcome = args.atomic(pool, sink_alloc) catch |err| {
        log.err("resolve_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .action_id = action_id });
        return;
    };
    outcome.deinit(sink_alloc);
}

/// Inputs + operation for atomic gate resolution. Bundled into a struct so
/// callers don't grow a long positional arg list as the resolution surface
/// gains fields, and the SQL lives next to the data that drives it.
///
/// `fleet_id_filter` binds the resolution to a specific fleet when the
/// caller knows it from a trusted source (URL path, worker context). An
/// empty string disables the filter — used by the sweeper and worker timeout
/// paths that already operate on a row they read. Channels that derive both
/// `action_id` and `fleet_id` from the same untrusted payload (Slack
/// callback URL, webhook URL) MUST set this; otherwise an actor with HMAC
/// access for fleet A could resolve fleet B's gate by guessing its
/// action_id.
pub const ResolveArgs = struct {
    const Self = @This();

    action_id: []const u8,
    outcome: GateStatus,
    by: []const u8,
    reason: []const u8 = "",
    fleet_id_filter: []const u8 = "",

    /// Atomic resolution. Returns the canonical resolver attribution either
    /// way: .resolved means this caller won the race; .already_resolved
    /// means an earlier writer (different channel or concurrent retry)
    /// already terminated the row.
    pub fn atomic(self: Self, pool: *pg.Pool, alloc: Allocator) !ResolveDbOutcome {
        if (self.outcome == .pending) return error.InvalidGateStatus;

        const conn = try pool.acquire();
        defer pool.release(conn);

        const now_ms = clock.nowMillis();
        var update_q = PgQuery.from(try conn.query(sql.RESOLVE_GATE, .{ self.outcome.toSlice(), self.reason, self.by, now_ms, self.action_id, PENDING_STATUS, self.fleet_id_filter }));
        defer update_q.deinit();

        if (try update_q.next()) |row| {
            return .{ .resolved = try readResolvedRow(alloc, row) };
        }

        var select_q = PgQuery.from(try conn.query(sql.SELECT_GATE_BY_ACTION, .{ self.action_id, self.fleet_id_filter }));
        defer select_q.deinit();

        if (try select_q.next()) |row| {
            return .{ .already_resolved = try readResolvedRow(alloc, row) };
        }
        return .not_found;
    }
};

/// Read the current terminal decision for an action, if the DB row has reached
/// one. The lease-poll DB fallback (approval_gate_async.evaluateRef) calls this
/// when the Redis decision mirror is absent — the DB row is the durable source
/// of truth, so a committed resolve is observable even if its best-effort Redis
/// mirror write failed. Returns null when the row is missing or still pending.
/// Allocation-free: reads a single status enum, no row materialization.
pub fn readTerminalDecision(pool: *pg.Pool, action_id: []const u8) !?GateStatus {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = PgQuery.from(try conn.query(sql.SELECT_GATE_STATUS, .{action_id}));
    defer q.deinit();

    if (try q.next()) |row| {
        const status = parseStatus(try row.get([]const u8, 0));
        return if (status.isTerminal()) status else null;
    }
    return null;
}

// ── Internals ───────────────────────────────────────────────────────────

fn insertPendingRow(
    pool: *pg.Pool,
    alloc: Allocator,
    fleet_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    detail: ActionDetail,
) !void {
    const gate_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(gate_id);

    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = clock.nowMillis();
    const timeout_at = now_ms +| detail.timeout_ms;
    _ = try conn.exec(sql.INSERT_GATE, .{
        gate_id,          fleet_id,               workspace_id,         action_id,           detail.tool, detail.action,
        detail.gate_kind, detail.proposed_action, detail.evidence_json, detail.blast_radius, timeout_at,  PENDING_STATUS,
        now_ms,
    });
}

fn readResolvedRow(alloc: Allocator, row: pg.Row) !ResolvedRow {
    const gate_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(gate_id);
    const action_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(action_id);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(workspace_id);
    const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(fleet_id);
    const status_str = try row.get([]const u8, 4);
    const resolved_at = try row.get(i64, 5);
    const resolved_by = try alloc.dupe(u8, try row.get([]const u8, 6));
    errdefer alloc.free(resolved_by);
    const detail = try alloc.dupe(u8, try row.get([]const u8, 7));

    return .{
        .gate_id = gate_id,
        .action_id = action_id,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .outcome = parseStatus(status_str),
        .resolved_at = resolved_at,
        .resolved_by = resolved_by,
        .detail = detail,
    };
}

fn parseStatus(s: []const u8) GateStatus {
    if (std.mem.eql(u8, s, "approved")) return .approved;
    if (std.mem.eql(u8, s, "denied")) return .denied;
    if (std.mem.eql(u8, s, "timed_out")) return .timed_out;
    if (std.mem.eql(u8, s, "auto_killed")) return .auto_killed;
    return .pending;
}

test "ResolvedRow.deinit frees every owned field (leak-free under testing.allocator)" {
    // The decision sink (`resolveGateDecision`) allocates a ResolvedRow on the
    // injected `sink_alloc` and frees it via this deinit before returning. On
    // testing.allocator, a missed field surfaces as a leak and a double-free
    // panics — so this pins the exact ownership the injected allocator relies on.
    const a = std.testing.allocator;
    var row = ResolvedRow{
        .gate_id = try a.dupe(u8, "gate-1"),
        .action_id = try a.dupe(u8, "act-1"),
        .workspace_id = try a.dupe(u8, "ws-1"),
        .fleet_id = try a.dupe(u8, "fleet-1"),
        .outcome = .approved,
        .resolved_at = 0,
        .resolved_by = try a.dupe(u8, "system"),
        .detail = try a.dupe(u8, "reason"),
    };
    row.deinit(a);
}
