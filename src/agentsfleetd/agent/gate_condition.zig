// Gate-condition grammar — the single source of truth for the small
// `field == 'value'` / `field != 'value'` expression language used by agent
// approval gates.
//
// Both the config parser (parse-time validation in config_gates.zig) and the
// runtime evaluator (approval_gate.zig) consume this module, so the two can
// never disagree on what counts as a valid condition. That agreement is
// load-bearing: a condition that parsed here but not at eval time would
// silently fire the gate on every matching action (over-gate / mass-pause),
// which is exactly what UZ-APPROVAL-005 (rejected at parse time) prevents.

const std = @import("std");

// Operators. The surrounding spaces are part of the token — a condition must
// read `field == 'value'`, not `field=='value'`. `==` is matched before `!=`.
const OP_EQ = " == ";
const OP_NE = " != ";

pub const Condition = struct {
    field: []const u8,
    value: []const u8,
    /// true for `!=` (negated match), false for `==`.
    negate: bool,
};

/// Parse a condition into field / value / negate, or null if it carries
/// neither supported operator. Single quotes around the right-hand side are
/// stripped. `==` is checked before `!=` so an expression containing both
/// resolves the way the evaluator has always resolved it.
pub fn parse(condition: []const u8) ?Condition {
    if (split(condition, OP_EQ)) |s| return .{ .field = s.field, .value = s.value, .negate = false };
    if (split(condition, OP_NE)) |s| return .{ .field = s.field, .value = s.value, .negate = true };
    return null;
}

/// True when the condition is a parseable gate expression. Defined in terms of
/// parse() so the parse-time check and the eval-time match can never diverge.
pub fn isValid(condition: []const u8) bool {
    return parse(condition) != null;
}

const Split = struct { field: []const u8, value: []const u8 };

fn split(condition: []const u8, op: []const u8) ?Split {
    const idx = std.mem.indexOf(u8, condition, op) orelse return null;
    const field = std.mem.trim(u8, condition[0..idx], " ");
    const rhs = std.mem.trim(u8, condition[idx + op.len ..], " ");
    if (rhs.len >= 2 and rhs[0] == '\'' and rhs[rhs.len - 1] == '\'') {
        return .{ .field = field, .value = rhs[1 .. rhs.len - 1] };
    }
    return .{ .field = field, .value = rhs };
}

test "isValid: accepts == and != expressions, rejects operator-less input" {
    try std.testing.expect(isValid("branch == 'main'"));
    try std.testing.expect(isValid("env != 'prod'"));
    try std.testing.expect(!isValid("garbage"));
    try std.testing.expect(!isValid(""));
    // No surrounding spaces — the operator token is not present.
    try std.testing.expect(!isValid("branch=='main'"));
}

test "parse: extracts field/value/negate and strips single quotes" {
    const eq = parse("branch == 'main'").?;
    try std.testing.expectEqualStrings("branch", eq.field);
    try std.testing.expectEqualStrings("main", eq.value);
    try std.testing.expect(!eq.negate);

    const ne = parse("env != 'prod'").?;
    try std.testing.expectEqualStrings("env", ne.field);
    try std.testing.expectEqualStrings("prod", ne.value);
    try std.testing.expect(ne.negate);

    // == is matched before != when both are present.
    const both = parse("a == 'b' != 'c'").?;
    try std.testing.expect(!both.negate);

    try std.testing.expect(parse("no operator here") == null);
}
