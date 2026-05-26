const std = @import("std");

pub const AuthRole = enum(u8) {
    user = 0,
    operator = 1,
    admin = 2,

    pub fn label(self: AuthRole) []const u8 {
        return @tagName(self);
    }

    pub fn allows(self: AuthRole, required: AuthRole) bool {
        return @intFromEnum(self) >= @intFromEnum(required);
    }
};

pub fn parseAuthRole(raw: []const u8) ?AuthRole {
    if (std.ascii.eqlIgnoreCase(raw, "user")) return .user;
    if (std.ascii.eqlIgnoreCase(raw, "operator")) return .operator;
    if (std.ascii.eqlIgnoreCase(raw, "admin")) return .admin;
    return null;
}

test "parseAuthRole accepts supported RBAC roles" {
    try std.testing.expectEqual(AuthRole.user, parseAuthRole("user").?);
    try std.testing.expectEqual(AuthRole.operator, parseAuthRole("OPERATOR").?);
    try std.testing.expectEqual(AuthRole.admin, parseAuthRole("Admin").?);
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("owner"));
}

test "AuthRole honors hierarchy ordering" {
    try std.testing.expect(AuthRole.admin.allows(.operator));
    try std.testing.expect(AuthRole.operator.allows(.user));
    try std.testing.expect(!AuthRole.user.allows(.operator));
}

test "parseAuthRole rejects whitespace-padded and unknown roles" {
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole(" operator "));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole(""));
}

test "AuthRole label matches canonical CLI and API strings" {
    try std.testing.expectEqualStrings("user", AuthRole.user.label());
    try std.testing.expectEqualStrings("operator", AuthRole.operator.label());
    try std.testing.expectEqualStrings("admin", AuthRole.admin.label());
}

test "parseAuthRole rejects unicode and numeric input" {
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("\u{00e9}"));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("\u{00fc}ser")); // "user" with umlaut-u
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("0"));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("1"));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("2"));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("123"));
    try std.testing.expectEqual(@as(?AuthRole, null), parseAuthRole("\u{0000}"));
}

test "AuthRole.allows is reflexive — each role allows itself" {
    try std.testing.expect(AuthRole.user.allows(.user));
    try std.testing.expect(AuthRole.operator.allows(.operator));
    try std.testing.expect(AuthRole.admin.allows(.admin));
}

test "AuthRole enum values are contiguous with no gaps" {
    const values = [_]u8{
        @intFromEnum(AuthRole.user),
        @intFromEnum(AuthRole.operator),
        @intFromEnum(AuthRole.admin),
    };
    // Verify the enum starts at 0 and increments by 1 (no exploitable gaps).
    try std.testing.expectEqual(@as(u8, 0), values[0]);
    for (values[1..], 1..) |v, idx| {
        try std.testing.expectEqual(@as(u8, @intCast(idx)), v);
    }
}
