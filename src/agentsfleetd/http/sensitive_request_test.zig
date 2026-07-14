const std = @import("std");
const httpz = @import("httpz");
const router = @import("router.zig");
const sensitive_request = @import("sensitive_request.zig");
const metrics = @import("../observability/metrics_sensitive_memory.zig");

const SECRET_BODY = "{\"api_key\":\"erase-after-dispatch\"}";

test "sensitive request cleanup erases store rotate and mint bodies" {
    const before = metrics.snapshot();
    const cases = [_]struct { method: httpz.Method, route: router.Route }{
        .{ .method = .POST, .route = .{ .workspace_secrets = "workspace" } },
        .{ .method = .PATCH, .route = .{ .workspace_secret = .{ .workspace_id = "workspace", .secret_name = "provider" } } },
        .{ .method = .POST, .route = .runner_credentials_mint },
    };
    for (cases) |case| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        ht.body(SECRET_BODY);
        ht.req.method = case.method;
        sensitive_request.eraseAfterDispatch(ht.req, case.route);
        try std.testing.expect(allZero(ht.req.body().?));
    }
    const after = metrics.snapshot();
    try std.testing.expectEqual(before.request_erased_bytes_total + SECRET_BODY.len * cases.len, after.request_erased_bytes_total);
}

test "sensitive request cleanup leaves bodyless methods untouched" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body(SECRET_BODY);
    ht.req.method = .GET;

    sensitive_request.eraseAfterDispatch(ht.req, .{ .workspace_secrets = "workspace" });
    try std.testing.expectEqualStrings(SECRET_BODY, ht.req.body().?);
}

test "sensitive request cleanup leaves unrelated route bodies untouched" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.body(SECRET_BODY);
    ht.req.method = .POST;

    sensitive_request.eraseAfterDispatch(ht.req, .healthz);
    try std.testing.expectEqualStrings(SECRET_BODY, ht.req.body().?);
}

fn allZero(bytes: []const u8) bool {
    for (bytes) |byte| if (byte != 0) return false;
    return true;
}
