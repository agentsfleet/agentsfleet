// Constant regression pins for the approval gate substrate.
//
// These constants are consumed by `approval_gate.zig` and `event_loop_gate.zig`
// for `GETDEL` atomicity and human-window sizing. Drift in either is silent at
// runtime: a TTL drop strands approvals, a key-prefix rename breaks gate
// routing.

const std = @import("std");
const gate_constants = @import("approval_gate_constants.zig");
const gate_sql = @import("sql.zig");
const schema = @import("schema");

/// The two slots whose triggers read GATE_PURGE_SETTING.
const SLOT_APPROVAL_GATES = 810;
const SLOT_REPAIR_PR_LINKS = 830;
const SECONDS_PER_HOUR = 3600;

test "GATE_PENDING_TTL_SECONDS is at least SECONDS_PER_HOUR (1-hour approval window)" {
    // Below 3600 a human reviewer can no longer act in time and approvals are
    // silently dropped on expiry. Current value is 7200; the floor is 3600.
    try std.testing.expect(gate_constants.GATE_PENDING_TTL_SECONDS >= SECONDS_PER_HOUR);
}

test "GATE_PENDING_KEY_PREFIX contains \"gate\" (routing invariant)" {
    // approval_gate and event_loop_gate use this prefix for GETDEL-based
    // atomicity. A silent rename breaks gate routing across both modules.
    try std.testing.expect(std.mem.indexOf(u8, gate_constants.GATE_PENDING_KEY_PREFIX, "gate") != null);
}

// ── The gate-purge bypass setting ───────────────────────────────────────────
//
// `GATE_PURGE_SETTING` is the one name Zig and SQL must agree on by hand: two
// triggers spell it themselves (`schema/810`'s append-only gates, `schema/830`'s
// repair links) and no compiler crosses that boundary. Losing the agreement is
// silent in the worst direction — `current_setting(name, true)` yields NULL for
// an unset setting rather than raising, so the trigger simply stops recognising
// the bypass and the next account purge raises "append-only" instead of erasing.

test "the gate-purge setting name appears verbatim in every slot whose trigger reads it" {
    var in_gates_slot = false;
    var in_repair_slot = false;
    for (schema.migrations) |slot| {
        const present = std.mem.indexOf(u8, slot.sql, gate_sql.GATE_PURGE_SETTING) != null;
        if (slot.version == SLOT_APPROVAL_GATES and present) in_gates_slot = true;
        if (slot.version == SLOT_REPAIR_PR_LINKS and present) in_repair_slot = true;
    }
    try std.testing.expect(in_gates_slot);
    try std.testing.expect(in_repair_slot);
}

test "the bypass statement sets exactly the pinned setting to the pinned value" {
    // Composed, not retyped: this fails only if the composition itself is broken.
    try std.testing.expectEqualStrings(
        "SET LOCAL fleet.allow_gate_purge = 'on'",
        gate_sql.SET_GATE_PURGE_BYPASS_SQL,
    );
    // And the value the triggers compare against is the one we send.
    for (schema.migrations) |slot| {
        if (slot.version != SLOT_APPROVAL_GATES) continue;
        const needle = "= '" ++ gate_sql.GATE_PURGE_ON ++ "'";
        try std.testing.expect(std.mem.indexOf(u8, slot.sql, needle) != null);
    }
}
