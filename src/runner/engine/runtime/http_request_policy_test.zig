//! Provider-neutral request-rule tests for the HTTP runtime boundary.

const std = @import("std");
const nullclaw = @import("nullclaw");
const execution_policy = @import("contract").execution_policy;
const subject = @import("http_request_policy.zig");
const PolicyHttpRequestTool = @import("policy_http_request.zig");

const JsonFieldRule = execution_policy.HttpJsonFieldRule;
const RequestRule = execution_policy.HttpRequestRule;
const OriginPolicy = execution_policy.HttpOriginPolicy;

const REF_FIELDS = [_]JsonFieldRule{.{
    .name = "ref",
    .string_value = "refs/heads/repair/run-123",
}};
const PULL_FIELDS = [_]JsonFieldRule{
    .{ .name = "head", .string_value = "repair/run-123" },
    .{ .name = "base", .string_value = "main" },
    .{ .name = "draft", .boolean_value = true },
};
const REQUESTS = [_]RequestRule{
    .{ .method = .get, .path = "/projects/acme/payments/", .path_match = .prefix },
    .{ .method = .head, .path = "/projects/acme/payments/", .path_match = .prefix },
    .{ .method = .post, .path = "/projects/acme/payments/objects" },
    .{ .method = .post, .path = "/projects/acme/payments/refs", .json_fields = &REF_FIELDS },
    .{ .method = .post, .path = "/projects/acme/payments/reviews", .json_fields = &PULL_FIELDS },
};
const ORIGINS = [_]OriginPolicy{.{
    .host = "api.code.example",
    .credential_names = &.{"source_control"},
    .requests = &REQUESTS,
}};

fn verdict(url: []const u8, method: []const u8, body: ?std.json.Value) subject.Verdict {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    return subject.validate(arena_state.allocator(), &ORIGINS, url, method, body);
}

test "generic rules distinguish unscoped, allowed, and denied requests" {
    try std.testing.expectEqual(subject.Verdict.unscoped, verdict("https://telemetry.example/query", "GET", null));
    try std.testing.expectEqual(subject.Verdict.allowed, verdict("https://api.code.example/projects/acme/payments/files/a", "GET", null));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("http://api.code.example/projects/acme/payments/files/a", "GET", null));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/other/files/a", "GET", null));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments-evil/files/a", "GET", null));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments/objects", "DELETE", null));
}

test "generic rules lock required top-level JSON fields for object and string bodies" {
    const valid_ref = std.json.Value{ .string = "{\"ref\":\"refs/heads/repair/run-123\",\"sha\":\"abc\"}" };
    const wrong_ref = std.json.Value{ .string = "{\"ref\":\"refs/heads/other\",\"sha\":\"abc\"}" };
    const valid_pull = std.json.Value{ .string = "{\"head\":\"repair/run-123\",\"base\":\"main\",\"draft\":true,\"title\":\"fix\"}" };
    const wrong_base = std.json.Value{ .string = "{\"head\":\"repair/run-123\",\"base\":\"develop\",\"draft\":true}" };
    const wrong_type = std.json.Value{ .string = "{\"head\":\"repair/run-123\",\"base\":\"main\",\"draft\":\"true\"}" };

    try std.testing.expectEqual(subject.Verdict.allowed, verdict("https://api.code.example/projects/acme/payments/refs", "POST", valid_ref));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments/refs", "POST", wrong_ref));
    try std.testing.expectEqual(subject.Verdict.allowed, verdict("https://api.code.example/projects/acme/payments/reviews", "POST", valid_pull));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments/reviews", "POST", wrong_base));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments/reviews", "POST", wrong_type));
    try std.testing.expectEqual(subject.Verdict.denied, verdict("https://api.code.example/projects/acme/payments/reviews", "POST", null));
}

test "credential names are bound to their declared origin" {
    try std.testing.expect(subject.credentialAllowed(&ORIGINS, "source_control", "api.code.example"));
    try std.testing.expect(!subject.credentialAllowed(&ORIGINS, "source_control", "telemetry.example"));
    try std.testing.expect(!subject.credentialAllowed(&ORIGINS, "other", "api.code.example"));
}

fn fakeSuccess(
    _: *nullclaw.tools.http_request.HttpRequestTool,
    _: std.mem.Allocator,
    _: nullclaw.tools.JsonObjectMap,
) anyerror!nullclaw.tools.ToolResult {
    return nullclaw.tools.ToolResult.ok("ok");
}

var counted_calls: std.atomic.Value(u32) = .init(0);

fn fakeCounted(
    _: *nullclaw.tools.http_request.HttpRequestTool,
    _: std.mem.Allocator,
    _: nullclaw.tools.JsonObjectMap,
) anyerror!nullclaw.tools.ToolResult {
    _ = counted_calls.fetchAdd(1, .monotonic);
    return nullclaw.tools.ToolResult.ok("unexpected");
}

fn callScoped(
    tool: *PolicyHttpRequestTool,
    url: []const u8,
    body: []const u8,
) !nullclaw.tools.ToolResult {
    const allocator = std.testing.allocator;
    var args: nullclaw.tools.JsonObjectMap = .empty;
    defer args.deinit(allocator);
    try args.put(allocator, "url", .{ .string = url });
    try args.put(allocator, "method", .{ .string = "POST" });
    try args.put(allocator, "body", .{ .string = body });
    return tool.execute(allocator, args);
}

test "HTTP runtime applies generic rules without process-local progress" {
    const policy: execution_policy.ExecutionPolicy = .{
        .network_policy = .{ .allow = &.{"api.code.example"}, .read_only = true },
        .http_origin_policies = &ORIGINS,
    };
    var tool = PolicyHttpRequestTool{
        .policy = &policy,
        .inner = .{ .allowed_domains = &.{} },
        .inner_execute = fakeSuccess,
    };
    const body = "{\"ref\":\"refs/heads/repair/run-123\",\"sha\":\"abc\"}";
    try std.testing.expect((try callScoped(&tool, "https://api.code.example/projects/acme/payments/refs", body)).success);
    try std.testing.expect((try callScoped(&tool, "https://api.code.example/projects/acme/payments/refs", body)).success);

    const wrong = try callScoped(
        &tool,
        "https://api.code.example/projects/acme/payments/reviews",
        "{\"head\":\"repair/run-123\",\"base\":\"develop\",\"draft\":true}",
    );
    try std.testing.expectEqualStrings("request_policy_not_allowed", wrong.error_msg.?);

    inline for (@typeInfo(PolicyHttpRequestTool).@"struct".fields) |field| {
        if (comptime std.mem.indexOf(u8, field.name, "github") != null or
            std.mem.indexOf(u8, field.name, "repair_ref") != null or
            std.mem.indexOf(u8, field.name, "repair_pr") != null)
            @compileError("HTTP runtime gained provider-specific progress state");
    }
}

test "HTTP runtime rejects normalized traversal before calling the client" {
    counted_calls.store(0, .monotonic);
    const policy: execution_policy.ExecutionPolicy = .{
        .network_policy = .{ .allow = &.{"api.code.example"}, .read_only = true },
        .http_origin_policies = &ORIGINS,
    };
    var tool = PolicyHttpRequestTool{
        .policy = &policy,
        .inner = .{ .allowed_domains = &.{} },
        .inner_execute = fakeCounted,
    };
    const body = "{\"ref\":\"refs/heads/repair/run-123\",\"sha\":\"abc\"}";
    const urls = [_][]const u8{
        "https://api.code.example/projects/acme/payments/../../../installation/repositories",
        "https://api.code.example/projects/acme/payments/%2e%2e/%2e%2e/admin",
        "https://api.code.example/projects/acme/payments/%252e%252e/admin",
        "https://api.code.example/projects/acme/payments/..%5c..%5cadmin",
    };
    for (urls) |url| {
        const result = try callScoped(&tool, url, body);
        try std.testing.expect(!result.success);
        try std.testing.expectEqualStrings("request_policy_not_allowed", result.error_msg.?);
    }
    try std.testing.expectEqual(@as(u32, 0), counted_calls.load(.monotonic));
}
