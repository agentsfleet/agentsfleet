// M4_001 + M1_001 webhook router tests — extracted to keep router.zig under 500 lines.

const std = @import("std");
const router = @import("router.zig");

test "M4_001: approval webhook route resolves correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    // match() integration
    const route = router.match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11:approval") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .approval_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M4_001: approval route does not interfere with regular webhook" {
    const route = router.match("/v1/webhooks/z1") orelse return error.TestExpectedMatch;
    switch (route) {
        .receive_webhook => {},
        else => return error.TestExpectedEqual,
    }
}

test "M4_001: approval route rejects empty and multi-segment zombie_id" {
    try std.testing.expect(router.match("/v1/webhooks/:approval") == null);
    try std.testing.expect(router.match("/v1/webhooks/a/b:approval") == null);
}

test "M1_001: webhook routes resolve and reject correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(zombie_id, switch (router.match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?) {
        .receive_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
    try std.testing.expect(router.match("/v1/webhooks/") == null);
    try std.testing.expect(router.match("/v1/webhooks/a/b") == null);
    try std.testing.expect(router.match("/v1/webhooks") == null);
}
