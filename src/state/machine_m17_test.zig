//! M17_001 — CANCELLED state machine tests.
//! Imported by machine.zig comptime block.

const std = @import("std");
const types = @import("../types.zig");
const machine = @import("machine.zig");

const isAllowed = machine.isAllowed;

// T1: all seven active states can transition to CANCELLED.
test "M17: CANCELLED reachable from all active non-terminal states" {
    const sources = [_]types.RunState{
        .SPEC_QUEUED,  .RUN_PLANNED,              .PATCH_IN_PROGRESS,
        .PATCH_READY,  .VERIFICATION_IN_PROGRESS, .VERIFICATION_FAILED,
        .PR_PREPARED,
    };
    for (sources) |from| {
        try std.testing.expect(isAllowed(from, .CANCELLED));
    }
}

// T2: terminal states must NOT transition to CANCELLED (no re-cancelling).
test "M17: terminal states cannot transition to CANCELLED" {
    try std.testing.expect(!isAllowed(.DONE, .CANCELLED));
    try std.testing.expect(!isAllowed(.NOTIFIED_BLOCKED, .CANCELLED));
    try std.testing.expect(!isAllowed(.CANCELLED, .CANCELLED));
}

// T2: CANCELLED must not transition to any other state (terminal = no exit).
test "M17: CANCELLED has no outbound transitions" {
    const all_states = [_]types.RunState{
        .SPEC_QUEUED,              .RUN_PLANNED,         .PATCH_IN_PROGRESS, .PATCH_READY,
        .VERIFICATION_IN_PROGRESS, .VERIFICATION_FAILED, .PR_PREPARED,       .PR_OPENED,
        .NOTIFIED,                 .DONE,                .BLOCKED,           .NOTIFIED_BLOCKED,
        .CANCELLED,
    };
    for (all_states) |to| {
        try std.testing.expect(!isAllowed(.CANCELLED, to));
    }
}

// T2: late-pipeline states (PR_OPENED, NOTIFIED) cannot cancel.
test "M17: PR_OPENED and NOTIFIED cannot transition to CANCELLED" {
    try std.testing.expect(!isAllowed(.PR_OPENED, .CANCELLED));
    try std.testing.expect(!isAllowed(.NOTIFIED, .CANCELLED));
}

// Note: reconcile and dead-letter tests remain inline in machine.zig
// because shouldReconcileSideEffectsForState / deadLetterReasonForState are private.
