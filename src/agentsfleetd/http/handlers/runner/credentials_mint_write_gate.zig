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
const hx_mod = @import("../hx.zig");
const pg_query = @import("../../../db/pg_query.zig");
const approval_gate = @import("../../../fleet_runtime/approval_gate.zig");
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");
const binding_json = @import("../../../fleet_runtime/repository_binding_json.zig");
const integration = @import("../../../credentials/integration.zig");
const logging = @import("log");

const Hx = hx_mod.Hx;
const PgQuery = pg_query.PgQuery;
const log = logging.scoped(.credential_mint);

pub const WriteApproval = enum {
    /// Approved repository-write gate for this event, binding unchanged.
    approved,
    /// No repository-write gate row for the event, a non-approved status, or
    /// an answer stamped after the card's own deadline.
    unapproved,
    /// The gate is approved but the fleet's binding changed since the card was
    /// answered — the human approved a reach this mint would not honour.
    binding_drift,
};

/// Decide whether a write-scoped mint may proceed for `event_id`'s gate.
/// Read-only; the caller maps the verdict to its typed refusal. Absent
/// `stated_binding` on an otherwise-approved row is drift, not a pass — a
/// write approval that recorded no reach cannot vouch for one.
pub fn verifyWriteApproval(
    hx: Hx,
    conn: *pg.Conn,
    fleet_id: []const u8,
    event_id: []const u8,
    binding: integration.RepositoryBinding,
) !WriteApproval {
    var q = PgQuery.from(try conn.query(sql.SELECT_WRITE_GATE_FOR_MINT, .{
        fleet_id, event_id, gate_constants.GATE_KIND_REPOSITORY_WRITE,
    }));
    defer q.deinit();
    const row = try q.next() orelse {
        log.warn("write_mint_no_gate", .{ .fleet_id = fleet_id, .event_id = event_id });
        return .unapproved;
    };

    const status = try row.get([]const u8, 0);
    const approved = approval_gate.GateStatus.approved.toSlice();
    if (!std.mem.eql(u8, status, approved)) {
        log.warn("write_mint_unapproved", .{ .fleet_id = fleet_id, .event_id = event_id, .status = status });
        return .unapproved;
    }

    // The answer must have landed inside the card's own deadline. The sweeper
    // that times a pending card out runs on an interval, so a click arriving
    // in that window still flips an already-expired row to approved — and an
    // approval stamped after the question stopped being asked is not one this
    // mint may spend. A row with no stamp predates the resolve path that
    // writes one; there is no answer time to judge, so the deadline is not
    // enforced against it rather than guessed at.
    const timeout_at = try row.get(i64, 2);
    if (try row.get(?i64, 3)) |answered_at| {
        if (answered_at > timeout_at) {
            log.warn("write_mint_gate_expired", .{ .fleet_id = fleet_id, .event_id = event_id, .answered_at = answered_at, .timeout_at = timeout_at });
            return .unapproved;
        }
    }

    const stated = try row.get(?[]const u8, 1) orelse {
        log.warn("write_mint_binding_unrecorded", .{ .fleet_id = fleet_id, .event_id = event_id });
        return .binding_drift;
    };
    if (!binding_json.matches(hx.alloc, stated, binding)) {
        log.warn("write_mint_binding_drift", .{ .fleet_id = fleet_id, .event_id = event_id });
        return .binding_drift;
    }
    return .approved;
}
