// Constant regression pins for the approval gate substrate.
//
// These constants are consumed by `approval_gate.zig` and `event_loop_gate.zig`
// for `GETDEL` atomicity and human-window sizing. Drift in either is silent at
// runtime: a TTL drop strands approvals, a key-prefix rename breaks gate
// routing.

const std = @import("std");
const ec = @import("../errors/error_registry.zig");

test "GATE_PENDING_TTL_SECONDS is at least 3600 (1-hour approval window)" {
    // Below 3600 a human reviewer can no longer act in time and approvals are
    // silently dropped on expiry. Current value is 7200; the floor is 3600.
    try std.testing.expect(ec.GATE_PENDING_TTL_SECONDS >= 3600);
}

test "GATE_PENDING_KEY_PREFIX contains \"gate\" (routing invariant)" {
    // approval_gate and event_loop_gate use this prefix for GETDEL-based
    // atomicity. A silent rename breaks gate routing across both modules.
    try std.testing.expect(std.mem.indexOf(u8, ec.GATE_PENDING_KEY_PREFIX, "gate") != null);
}
