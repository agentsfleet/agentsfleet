// Approval gate DB persistence — audit table reads and writes.
//
// Writes to core.zombie_approval_gates (append-only, UPDATE only on pending rows).
// One row per gate action: INSERT pending, UPDATE to approved/denied/timed_out.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const id_format = @import("../types/id_format.zig");

const GateStatus = @import("approval_gate.zig").GateStatus;
const log = std.log.scoped(.approval_gate_db);

const PENDING_STATUS = GateStatus.pending.toSlice();

/// Insert a pending gate row into the audit table.
/// Called once when the gate fires. Resolution updates this row via resolveGateDecision.
pub fn recordGatePending(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
) void {
    insertPendingRow(pool, alloc, zombie_id, workspace_id, action_id, tool_name, action_name) catch |err| {
        log.err("approval_gate.record_pending_fail err={s} action_id={s}", .{ @errorName(err), action_id });
    };
}

/// Update a pending gate row with the final decision (approve/deny/timeout).
/// Sets status, detail, and updated_at. The schema trigger permits UPDATE only on pending rows.
pub fn resolveGateDecision(
    pool: *pg.Pool,
    action_id: []const u8,
    status: @import("approval_gate.zig").GateStatus,
    detail: []const u8,
) void {
    updateGateRow(pool, action_id, status.toSlice(), detail) catch |err| {
        log.err("approval_gate.resolve_fail err={s} action_id={s}", .{ @errorName(err), action_id });
    };
}

fn insertPendingRow(
    pool: *pg.Pool,
    alloc: Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
) !void {
    const gate_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(gate_id);

    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name,
        \\   status, detail, requested_at, created_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, '', $8, $8)
    , .{ gate_id, zombie_id, workspace_id, action_id, tool_name, action_name, PENDING_STATUS, now_ms });
}

fn updateGateRow(
    pool: *pg.Pool,
    action_id: []const u8,
    status: []const u8,
    detail: []const u8,
) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    _ = try conn.exec(
        \\UPDATE core.zombie_approval_gates
        \\SET status = $1, detail = $2, updated_at = $3
        \\WHERE action_id = $4 AND status = $5
    , .{ status, detail, now_ms, action_id, PENDING_STATUS });
}
