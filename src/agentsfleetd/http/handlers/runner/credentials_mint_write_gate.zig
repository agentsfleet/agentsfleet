//! The write-mint approval check: may THIS lease spend a human's answer?
//!
//! A write-scoped token issues only when the lease's event carries a gate of
//! the repository-write kind that is approved, whose answer landed inside the
//! card's own deadline, and whose recorded `stated_binding` still matches the
//! fleet's CURRENT binding. Every leg reads durable rows — no Redis on this
//! path, so a cache loss can only withhold a token, never widen one. The
//! binding comparison is what closes the approval-to-mint drift: gate
//! rules and the binding both ride `config_json`, PATCHable under the same
//! `fleet:write` scope that wakes the fleet, so "what the human approved" must
//! be read from the gate row the daemon wrote, never from anything editable.

const std = @import("std");
const pg = @import("pg");

const sql = @import("sql.zig");
const pg_query = @import("../../../db/pg_query.zig");
const approval_gate = @import("../../../fleet_runtime/approval_gate.zig");
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");
const binding_json = @import("../../../fleet_runtime/repository_binding_json.zig");
const integration = @import("../../../credentials/integration.zig");
const logging = @import("log");

const PgQuery = pg_query.PgQuery;
const log = logging.scoped(.credential_mint);
const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";

pub const WriteApproval = enum {
    /// Approved repository-write gate for this event, binding unchanged.
    approved,
    /// No repository-write gate row for the event, a non-approved status, or
    /// an answer stamped after the card's own deadline.
    unapproved,
    /// The gate is approved but the fleet's binding changed since the card was
    /// answered — the human approved a reach this mint would not honour.
    binding_drift,
    /// The fixed request allowance has already been consumed.
    exhausted,
};

/// Atomically confirm and spend one approved write request before any secret or
/// provider access. Absent recorded reach fails as binding drift.
pub fn reserveWriteApproval(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    fleet_id: []const u8,
    event_id: []const u8,
    binding: integration.RepositoryBinding,
) !WriteApproval {
    _ = try conn.exec(SQL_BEGIN, .{});
    var committed = false;
    defer if (!committed) rollback(conn);
    const gate_id = gate: {
        var query = PgQuery.from(try conn.query(sql.SELECT_WRITE_GATE_FOR_MINT, .{
            fleet_id, event_id, gate_constants.GATE_KIND_REPOSITORY_WRITE,
        }));
        defer query.deinit();
        const row = try query.next() orelse {
            log.warn("write_mint_no_gate", .{ .fleet_id = fleet_id, .event_id = event_id });
            return .unapproved;
        };
        const status = try row.get([]const u8, 1);
        const approved = approval_gate.GateStatus.approved.toSlice();
        if (!std.mem.eql(u8, status, approved)) {
            log.warn("write_mint_unapproved", .{ .fleet_id = fleet_id, .event_id = event_id, .status = status });
            return .unapproved;
        }
        const timeout_at = try row.get(i64, 3);
        if (try row.get(?i64, 4)) |answered_at| {
            if (answered_at > timeout_at) {
                log.warn("write_mint_gate_expired", .{ .fleet_id = fleet_id, .event_id = event_id, .answered_at = answered_at, .timeout_at = timeout_at });
                return .unapproved;
            }
        }
        const stated = try row.get(?[]const u8, 2) orelse {
            log.warn("write_mint_binding_unrecorded", .{ .fleet_id = fleet_id, .event_id = event_id });
            return .binding_drift;
        };
        if (!binding_json.matches(alloc, stated, binding)) {
            log.warn("write_mint_binding_drift", .{ .fleet_id = fleet_id, .event_id = event_id });
            return .binding_drift;
        }
        const spend_count = try row.get(?i64, 5) orelse return .unapproved;
        const spend_ceiling = try row.get(?i64, 6) orelse return .unapproved;
        if (spend_count >= spend_ceiling) {
            log.warn("write_mint_spend_exhausted", .{ .fleet_id = fleet_id, .event_id = event_id, .spend_count = spend_count, .spend_ceiling = spend_ceiling });
            return .exhausted;
        }
        break :gate try alloc.dupe(u8, try row.get([]const u8, 0));
    };
    defer alloc.free(gate_id);
    const affected = try conn.exec(sql.SPEND_WRITE_GATE_FOR_MINT, .{
        gate_id,
        approval_gate.GateStatus.approved.toSlice(),
    });
    if ((affected orelse 0) == 0) return .exhausted;
    _ = try conn.exec(SQL_COMMIT, .{});
    committed = true;
    return .approved;
}

fn rollback(conn: *pg.Conn) void {
    conn.rollback() catch |err| log.warn("write_mint_gate_rollback_failed", .{ .err = @errorName(err) });
}
