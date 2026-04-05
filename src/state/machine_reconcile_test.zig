//! State machine reconciliation tests — extracted from machine.zig (500-line limit).

const std = @import("std");
const types = @import("../types.zig");
const machine = @import("machine.zig");

test "reconciliation trigger states include blocked terminal and requeue edges" {
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.SPEC_QUEUED));
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.BLOCKED));
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.NOTIFIED_BLOCKED));
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.DONE));
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.CANCELLED));
    try std.testing.expect(machine.shouldReconcileSideEffectsForState(.ABORTED));
    try std.testing.expect(!machine.shouldReconcileSideEffectsForState(.PATCH_IN_PROGRESS));
    try std.testing.expect(!machine.shouldReconcileSideEffectsForState(.PR_OPENED));
}

test "integration: dead-letter reconciliation reasons are stable" {
    try std.testing.expectEqualStrings("reconciled_on_requeue", machine.deadLetterReasonForState(.SPEC_QUEUED));
    try std.testing.expectEqualStrings("reconciled_on_blocked", machine.deadLetterReasonForState(.BLOCKED));
    try std.testing.expectEqualStrings("reconciled_on_notified_blocked", machine.deadLetterReasonForState(.NOTIFIED_BLOCKED));
    try std.testing.expectEqualStrings("reconciled_on_done", machine.deadLetterReasonForState(.DONE));
    try std.testing.expectEqualStrings("reconciled_on_cancelled", machine.deadLetterReasonForState(.CANCELLED));
    try std.testing.expectEqualStrings("reconciled_on_aborted", machine.deadLetterReasonForState(.ABORTED));
}

test "dead-letter reconciliation reason falls back for non-reconcile target states" {
    try std.testing.expectEqualStrings("reconciled", machine.deadLetterReasonForState(.PR_OPENED));
    try std.testing.expectEqualStrings("reconciled", machine.deadLetterReasonForState(.RUN_PLANNED));
}

test "outbox status labels are stable" {
    try std.testing.expectEqualStrings("pending", machine.outboxStatusLabel(.pending));
    try std.testing.expectEqualStrings("delivered", machine.outboxStatusLabel(.delivered));
    try std.testing.expectEqualStrings("dead_letter", machine.outboxStatusLabel(.dead_letter));
}
