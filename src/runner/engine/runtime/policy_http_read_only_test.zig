//! Failure proofs for the verifier's read-only HTTP boundary.

const std = @import("std");
const nullclaw = @import("nullclaw");
const JsonObjectMap = nullclaw.tools.JsonObjectMap;

const PolicyHttpRequestTool = @import("policy_http_request.zig");
const context_budget = @import("../context_budget.zig");

const GLOBAL_HOST = "8.8.8.8";
const NETWORK_DISABLED = "Network disabled in tests";
const METHOD_NOT_ALLOWED = "method_not_allowed";
const CREDENTIAL_HOST_NOT_ALLOWED = "credential_host_not_allowed";
const CREDENTIAL_PLACEMENT_NOT_ALLOWED = "credential_placement_not_allowed";

fn newTool(policy: *const context_budget.ExecutionPolicy) PolicyHttpRequestTool {
    return .{ .policy = policy, .inner = .{ .allowed_domains = &.{} } };
}

fn newPolicy(read_only: bool, allow: []const []const u8, read_post_paths: []const []const u8, secrets: ?std.json.Value) context_budget.ExecutionPolicy {
    return .{
        .network_policy = .{
            .allow = allow,
            .read_only = read_only,
            .read_post_paths = read_post_paths,
        },
        .secrets_map = secrets,
        .context = .{},
    };
}

fn staticSecrets(arena: std.mem.Allocator, elastic_host: []const u8) !std.json.Value {
    var elastic: std.json.ObjectMap = .empty;
    try elastic.put(arena, "host", .{ .string = elastic_host });
    try elastic.put(arena, "api_key", .{ .string = "elastic-test-key" });
    var root: std.json.ObjectMap = .empty;
    try root.put(arena, "elastic", .{ .object = elastic });
    return .{ .object = root };
}

fn expectToolError(result: nullclaw.tools.ToolResult, expected: []const u8) !void {
    try std.testing.expect(!result.success);
    try std.testing.expectEqualStrings(expected, result.error_msg.?);
}

test "read-only policy permits the default GET" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(true, &allow, &.{}, null);
    var tool = newTool(&policy);
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://8.8.8.8/health" });
    try expectToolError(try tool.execute(alloc, args), NETWORK_DISABLED);
}

test "read-only policy refuses mutating and ordinary POST methods" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{GLOBAL_HOST};
    const methods = [_][]const u8{ "DELETE", "PUT", "PATCH", "POST" };
    for (methods) |method| {
        const policy = newPolicy(true, &allow, &.{}, null);
        var tool = newTool(&policy);
        var args: JsonObjectMap = .empty;
        defer args.deinit(alloc);
        try args.put(alloc, "url", .{ .string = "https://8.8.8.8/write" });
        try args.put(alloc, "method", .{ .string = method });
        try expectToolError(try tool.execute(alloc, args), METHOD_NOT_ALLOWED);
    }
}

test "read-only policy permits only exact Elasticsearch query POST" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const allow = [_][]const u8{GLOBAL_HOST};
    const read_post_paths = [_][]const u8{"https://8.8.8.8/_query"};
    const policy = newPolicy(true, &allow, &read_post_paths, try staticSecrets(arena_state.allocator(), GLOBAL_HOST));
    var tool = newTool(&policy);
    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "Authorization", .{ .string = "ApiKey ${secrets.elastic.api_key}" });
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://8.8.8.8/_query" });
    try args.put(alloc, "method", .{ .string = "POST" });
    try args.put(alloc, "headers", .{ .object = headers });
    try expectToolError(try tool.execute(alloc, args), NETWORK_DISABLED);
}

test "read-only policy refuses a POST prefix-confusion path" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{GLOBAL_HOST};
    const read_post_paths = [_][]const u8{"https://8.8.8.8/_query"};
    const policy = newPolicy(true, &allow, &read_post_paths, null);
    var tool = newTool(&policy);
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://8.8.8.8/_query/delete" });
    try args.put(alloc, "method", .{ .string = "POST" });
    try expectToolError(try tool.execute(alloc, args), METHOD_NOT_ALLOWED);
}

test "read-only policy refuses an Elastic credential on Grafana" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const allow = [_][]const u8{ "elastic.example.com", "grafana.example.com" };
    const policy = newPolicy(true, &allow, &.{}, try staticSecrets(arena_state.allocator(), "elastic.example.com"));
    var tool = newTool(&policy);
    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "Authorization", .{ .string = "ApiKey ${secrets.elastic.api_key}" });
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://grafana.example.com/api/annotations" });
    try args.put(alloc, "headers", .{ .object = headers });
    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_HOST_NOT_ALLOWED);
}

test "read-only policy refuses a GitHub token on another host" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{GLOBAL_HOST};
    var policy = newPolicy(true, &allow, &.{}, null);
    policy.mintable = &.{.{ .name = "github", .integration = "github" }};
    var tool = newTool(&policy);
    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "Authorization", .{ .string = "Bearer ${secrets.github.token}" });
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://8.8.8.8/anything" });
    try args.put(alloc, "headers", .{ .object = headers });
    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_HOST_NOT_ALLOWED);
}

test "mintable credentials are refused in URLs before minting" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{"api.github.com"};
    var policy = newPolicy(true, &allow, &.{}, null);
    policy.mintable = &.{.{ .name = "github", .integration = "github" }};
    var tool = newTool(&policy);
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://${secrets.github.token}@api.github.com/repos/acme/payments" });
    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_HOST_NOT_ALLOWED);
}

test "read-only policy refuses a GitHub token in a same-host body" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{"api.github.com"};
    var policy = newPolicy(true, &allow, &.{}, null);
    policy.mintable = &.{.{ .name = "github", .integration = "github" }};
    var tool = newTool(&policy);
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://api.github.com/repos/acme/payments" });
    try args.put(alloc, "body", .{ .string = "token=${secrets.github.token}" });
    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_PLACEMENT_NOT_ALLOWED);
}

test "read-only policy refuses a GitHub token nested in a JSON body" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();
    const allow = [_][]const u8{"api.github.com"};
    var policy = newPolicy(true, &allow, &.{}, null);
    policy.mintable = &.{.{ .name = "github", .integration = "github" }};
    var tool = newTool(&policy);

    var values = std.json.Array.init(arena);
    try values.append(.{ .string = "${secrets.github.token}" });
    var body: JsonObjectMap = .empty;
    try body.put(arena, "values", .{ .array = values });
    var args: JsonObjectMap = .empty;
    try args.put(arena, "url", .{ .string = "https://api.github.com/repos/acme/payments" });
    try args.put(arena, "body", .{ .object = body });

    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_PLACEMENT_NOT_ALLOWED);
}

test "read-only policy refuses a GitHub token outside the authorization header" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{"api.github.com"};
    var policy = newPolicy(true, &allow, &.{}, null);
    policy.mintable = &.{.{ .name = "github", .integration = "github" }};
    var tool = newTool(&policy);
    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "X-Debug", .{ .string = "${secrets.github.token}" });
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://api.github.com/repos/acme/payments" });
    try args.put(alloc, "headers", .{ .object = headers });
    try expectToolError(try tool.execute(alloc, args), CREDENTIAL_PLACEMENT_NOT_ALLOWED);
}

test "non-read-only policies retain their permitted HTTP methods" {
    const alloc = std.testing.allocator;
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(false, &allow, &.{}, null);
    var tool = newTool(&policy);
    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, "url", .{ .string = "https://8.8.8.8/write" });
    try args.put(alloc, "method", .{ .string = "PUT" });
    try expectToolError(try tool.execute(alloc, args), NETWORK_DISABLED);
}
