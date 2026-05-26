//! Tests for `policy_http_request.zig`. Split from the source file to
//! keep that file under the 350-line cap.
//!
//! Inner-tool note: NullClaw's HttpRequestTool short-circuits with
//! `ToolResult.fail("Network disabled in tests")` under `builtin.is_test`.
//! That string is the marker tests use to confirm a request reached the
//! inner tool (i.e. passed our allowlist + substitution layers).

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const ToolResult = tools_mod.ToolResult;
const JsonObjectMap = tools_mod.JsonObjectMap;
const HttpRequestTool = tools_mod.http_request.HttpRequestTool;

const PolicyHttpRequestTool = @import("policy_http_request.zig");
const context_budget = @import("../context_budget.zig");

const NETWORK_DISABLED: []const u8 = "Network disabled in tests";

const k_url: []const u8 = "url";
const k_headers: []const u8 = "headers";

fn buildSecretsMap(arena: std.mem.Allocator) !std.json.Value {
    var fly = std.json.ObjectMap.init(arena);
    try fly.put("api_token", .{ .string = "FlyTokenXyz" });
    try fly.put("host", .{ .string = "api.fly.dev" });
    var top = std.json.ObjectMap.init(arena);
    try top.put("fly", .{ .object = fly });
    return .{ .object = top };
}

fn freeResult(allocator: std.mem.Allocator, r: ToolResult) void {
    // ToolResult.error_msg is heap-owned only when the tool used
    // `allocPrint`; bare `ToolResult.fail("literal")` returns a literal
    // pointer that must NOT be freed. Our tool's only allocPrint path
    // emits `host_not_allowed: <host>` — the rest of our messages and
    // every NullClaw-side message in this test are literals.
    const m = r.error_msg orelse return;
    if (std.mem.startsWith(u8, m, "host_not_allowed:")) allocator.free(m);
}

fn newPolicy(allow: []const []const u8, secrets: ?std.json.Value) context_budget.ExecutionPolicy {
    return .{
        .network_policy = .{ .allow = allow },
        .tools = &.{},
        .secrets_map = secrets,
        .context = .{},
    };
}

fn newTool(policy: *const context_budget.ExecutionPolicy) PolicyHttpRequestTool {
    // Mirror the production wiring (`tool_builders.buildHttpRequest`):
    // pass the outer allowlist down so NullClaw treats these hosts as
    // operator-trusted and skips SSRF + builtin.is_test fires the
    // `NETWORK_DISABLED` short-circuit deterministically.
    return .{
        .policy = policy,
        .inner = .{ .allowed_domains = policy.network_policy.allow },
    };
}

test "host not in allowlist returns host_not_allowed" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://evil.com/path" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(r.error_msg != null);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
}

test "host in allowlist passes through to inner tool" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://api.fly.dev/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(NETWORK_DISABLED, r.error_msg.?);
}

test "substitution runs before allowlist check" {
    // Pre-substitution url host is the literal "${secrets.fly.host}" — never
    // matches an allowlist. Post-substitution host is "api.fly.dev" — does
    // match. If the order were reversed, the request would be blocked at
    // allowlist; since substitution-first is contract, it reaches the
    // inner tool and gets the network-disabled marker.
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://${secrets.fly.host}/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(NETWORK_DISABLED, r.error_msg.?);
}

test "substituted host is what the allowlist sees" {
    // Inverse direction: pre-substitution host is the placeholder string,
    // post-substitution resolves to a host that's NOT in the allowlist.
    // Allowlist sees the substituted bytes and rejects.
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{"api.upstash.com"};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://${secrets.fly.host}/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
    try std.testing.expect(std.mem.indexOf(u8, r.error_msg.?, "api.fly.dev") != null);
}

test "missing secret fails closed before allowlist check" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://${secrets.unknown.host}/v1/apps" });

    try std.testing.expectError(error.SubstFailed, t.execute(alloc, args));
}

test "missing url returns descriptive failure" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(std.mem.indexOf(u8, r.error_msg.?, "url") != null);
}

test "header values get substituted (success path reaches inner)" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var headers = JsonObjectMap.init(alloc);
    defer headers.deinit();
    try headers.put("Authorization", .{ .string = "Bearer ${secrets.fly.api_token}" });

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://api.fly.dev/v1/apps" });
    try args.put(k_headers, .{ .object = headers });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(NETWORK_DISABLED, r.error_msg.?);
}

test "header value with missing secret fails closed" {
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var headers = JsonObjectMap.init(alloc);
    defer headers.deinit();
    try headers.put("Authorization", .{ .string = "Bearer ${secrets.missing.token}" });

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://api.fly.dev/v1/apps" });
    try args.put(k_headers, .{ .object = headers });

    try std.testing.expectError(error.SubstFailed, t.execute(alloc, args));
}

test "empty allowlist denies every host" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args = JsonObjectMap.init(alloc);
    defer args.deinit();
    try args.put(k_url, .{ .string = "https://api.fly.dev/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
}

test "tool_name and tool_params match NullClaw http_request" {
    try std.testing.expectEqualStrings(HttpRequestTool.tool_name, PolicyHttpRequestTool.tool_name);
    try std.testing.expectEqualStrings(HttpRequestTool.tool_params, PolicyHttpRequestTool.tool_params);
}
