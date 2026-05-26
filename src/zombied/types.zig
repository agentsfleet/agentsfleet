//! Cross-cutting enums for policy decisions and reliability classification.

const std = @import("std");

/// Reason code attached to a reliability classification result.
/// Stable wire identifier — values are emitted in audit logs and metrics.
pub const ReasonCode = enum {
    AGENT_CRASH,
    AGENT_TIMEOUT,
    AUTH_FAILED,
    RATE_LIMITED,
    SPEC_MISMATCH,

    pub fn label(self: ReasonCode) []const u8 {
        return @tagName(self);
    }
};

pub const PolicyDecision = enum { allow, deny, require_confirmation };

pub const ActionClass = enum { safe, sensitive, critical };

test "ReasonCode label round-trips through @tagName" {
    try std.testing.expectEqualStrings("RATE_LIMITED", ReasonCode.RATE_LIMITED.label());
    try std.testing.expectEqualStrings("AUTH_FAILED", ReasonCode.AUTH_FAILED.label());
    try std.testing.expectEqualStrings("AGENT_TIMEOUT", ReasonCode.AGENT_TIMEOUT.label());
    try std.testing.expectEqualStrings("AGENT_CRASH", ReasonCode.AGENT_CRASH.label());
    try std.testing.expectEqualStrings("SPEC_MISMATCH", ReasonCode.SPEC_MISMATCH.label());
}

test "PolicyDecision and ActionClass enums have stable variants" {
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(PolicyDecision).@"enum".fields.len);
    try std.testing.expectEqual(@as(usize, 3), @typeInfo(ActionClass).@"enum".fields.len);
}
