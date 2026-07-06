// Proves the standing guards actually FIRE on a violation, not just that
// today's registry happens to be clean. Complements the real-file sweeps in
// internal_op_error_sweep_test.zig (which only prove today's state is clean)
// and error_entries_reachability_test.zig (the real reachability sweep).
const std = @import("std");
const sweep = @import("internal_op_error_sweep_test.zig");

// ── jargon/raw-tag detail denylist ──────────────────────────────────────────

test "guard: a fixture detail with a raw @errorName tag is caught" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    common.internalOperationError(hx.res, @errorName(err), hx.req_id);
        \\}
    ;
    try std.testing.expectError(error.TestUnexpectedResult, sweep.scanCallSitesForJargon("fixture.zig", fixture));
}

test "guard: a fixture detail with OOM jargon is caught" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    common.internalOperationError(hx.res, "OOM building steer actor", hx.req_id);
        \\}
    ;
    try std.testing.expectError(error.TestUnexpectedResult, sweep.scanCallSitesForJargon("fixture.zig", fixture));
}

test "guard: a clean plain-English detail is not flagged" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    common.internalOperationError(hx.res, "Failed to create workspace", hx.req_id);
        \\}
    ;
    try sweep.scanCallSitesForJargon("fixture.zig", fixture);
}

// ── mudball-ok justification requirement ────────────────────────────────────

test "guard: an added call site without justification is unjustified" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    common.internalOperationError(hx.res, "Failed to do a new thing", hx.req_id);
        \\}
    ;
    const counted = sweep.countCallSites(fixture);
    try std.testing.expectEqual(@as(usize, 1), counted.total);
    try std.testing.expectEqual(@as(usize, 0), counted.justified);
}

test "guard: an added call site with an inline mudball-ok comment is justified" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    common.internalOperationError(hx.res, "Failed to do a new thing", hx.req_id); // mudball-ok: transient, no dedicated code needed
        \\}
    ;
    const counted = sweep.countCallSites(fixture);
    try std.testing.expectEqual(@as(usize, 1), counted.total);
    try std.testing.expectEqual(@as(usize, 1), counted.justified);
}

test "guard: a mudball-ok comment on the line above also justifies the call" {
    const fixture =
        \\fn handle(hx: Hx) void {
        \\    // mudball-ok: transient, no dedicated code needed
        \\    common.internalOperationError(hx.res, "Failed to do a new thing", hx.req_id);
        \\}
    ;
    const counted = sweep.countCallSites(fixture);
    try std.testing.expectEqual(@as(usize, 1), counted.total);
    try std.testing.expectEqual(@as(usize, 1), counted.justified);
}
