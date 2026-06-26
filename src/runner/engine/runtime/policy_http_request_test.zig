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
const BLOCKED_LOCAL: []const u8 = "Blocked local/private host";

/// A globally-routable IP LITERAL. `resolveConnectHost` classifies it as global
/// and returns it without any DNS lookup, so "reaches inner tool" assertions stay
/// hermetic + offline-safe now that the inner allowlist is empty and every host
/// flows through the SSRF resolve/pin path (M100). A hostname here would
/// trigger a real getAddressList — flaky in CI, slow, network-dependent.
const GLOBAL_HOST: []const u8 = "8.8.8.8";

const k_url: []const u8 = "url";
const k_headers: []const u8 = "headers";

fn buildSecretsMap(arena: std.mem.Allocator) !std.json.Value {
    var fly: std.json.ObjectMap = .empty;
    try fly.put(arena, "api_token", .{ .string = "FlyTokenXyz" });
    try fly.put(arena, "host", .{ .string = "api.fly.dev" });
    // A global IP literal as a secret host — substitutes to a host that reaches
    // the inner tool deterministically (no DNS) under the new SSRF wiring.
    try fly.put(arena, "global", .{ .string = GLOBAL_HOST });
    var top: std.json.ObjectMap = .empty;
    try top.put(arena, "fly", .{ .object = fly });
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
    // Mirror the production wiring (`tool_builders.buildHttpRequest`, M100 §2):
    // the inner allowlist is EMPTY, so every host the outer exact-match gate
    // admits flows through NullClaw's `resolveConnectHost` (private-IP reject +
    // DNS-rebind pin). The outer `hostInAllowlist` stays authoritative.
    return .{
        .policy = policy,
        .inner = .{ .allowed_domains = &.{} },
    };
}

test "host not in allowlist returns host_not_allowed" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://evil.com/path" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(r.error_msg != null);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
}

test "host in allowlist (global IP) passes through to inner tool" {
    const alloc = std.testing.allocator;

    // Allowlisted global host → outer gate admits → inner SSRF resolve passes
    // (global IP) → reaches NullClaw's is_test short-circuit (NETWORK_DISABLED).
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://8.8.8.8/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(NETWORK_DISABLED, r.error_msg.?);
}

test "test_bridge_static_unchanged" {
    // Dimension 4.3: a STATIC credential resolves at the tool boundary with no
    // mint. The tool has no `cred_channel` and the policy lists no `mintable`, so
    // `${secrets.fly.global}` takes the static lookup path. Proof it never touched
    // the mint path: a mint attempt with a null channel fails closed (SubstFailed);
    // reaching the inner tool's NETWORK_DISABLED marker means no mint was tried.
    const alloc = std.testing.allocator;
    var arena_state = std.heap.ArenaAllocator.init(alloc);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const sm = try buildSecretsMap(arena);
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(&allow, sm); // no .mintable → every name is static
    var t = newTool(&policy); // no cred_channel

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://${secrets.fly.global}/v1/apps" });

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
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://${secrets.fly.global}/v1/apps" });

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

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://${secrets.fly.host}/v1/apps" });

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

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://${secrets.unknown.host}/v1/apps" });

    try std.testing.expectError(error.SubstFailed, t.execute(alloc, args));
}

test "missing url returns descriptive failure" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{"api.fly.dev"};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);

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
    const allow = [_][]const u8{GLOBAL_HOST};
    const policy = newPolicy(&allow, sm);
    var t = newTool(&policy);

    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "Authorization", .{ .string = "Bearer ${secrets.fly.api_token}" });

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://8.8.8.8/v1/apps" });
    try args.put(alloc, k_headers, .{ .object = headers });

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

    var headers: JsonObjectMap = .empty;
    defer headers.deinit(alloc);
    try headers.put(alloc, "Authorization", .{ .string = "Bearer ${secrets.missing.token}" });

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://api.fly.dev/v1/apps" });
    try args.put(alloc, k_headers, .{ .object = headers });

    try std.testing.expectError(error.SubstFailed, t.execute(alloc, args));
}

test "empty allowlist denies every host" {
    const alloc = std.testing.allocator;

    const allow = [_][]const u8{};
    const policy = newPolicy(&allow, null);
    var t = newTool(&policy);

    var args: JsonObjectMap = .empty;
    defer args.deinit(alloc);
    try args.put(alloc, k_url, .{ .string = "https://api.fly.dev/v1/apps" });

    const r = try t.execute(alloc, args);
    defer freeResult(alloc, r);
    try std.testing.expect(!r.success);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
}

test "tenant host resolving to a private/link-local IP is rejected (M100)" {
    // The exfil hole §2 closes: a TENANT-declared allowlist entry that points at
    // a private/loopback/metadata IP used to skip SSRF and get dialed. With the
    // inner allowlist empty, every admitted host flows through resolveConnectHost,
    // so these are now blocked. IP literals → deterministic, no DNS.
    const alloc = std.testing.allocator;
    const hostile = [_][]const u8{
        "169.254.169.254", // cloud metadata (link-local)
        "127.0.0.1", // loopback
        "10.0.0.1", // RFC1918 private
    };
    for (hostile) |h| {
        const allow = [_][]const u8{h}; // tenant explicitly listed it
        const policy = newPolicy(&allow, null);
        var t = newTool(&policy);

        const url = try std.fmt.allocPrint(alloc, "https://{s}/latest/meta-data", .{h});
        defer alloc.free(url);
        var args: JsonObjectMap = .empty;
        defer args.deinit(alloc);
        try args.put(alloc, k_url, .{ .string = url });

        const r = try t.execute(alloc, args);
        defer freeResult(alloc, r);
        try std.testing.expect(!r.success);
        // Reached the inner tool (passed the outer gate) and was blocked at the
        // SSRF resolve — NOT a host_not_allowed and NOT NETWORK_DISABLED.
        try std.testing.expectEqualStrings(BLOCKED_LOCAL, r.error_msg.?);
    }
}

test "wildcard allowlist entry cannot widen the exact-match gate (M100)" {
    // The outer gate is exact, case-insensitive. A `*`/`*.x` entry is matched
    // literally — it can never admit a real host, so the inner subdomain matcher
    // (the old split-brain) can't widen egress. Host is rejected at the outer
    // gate, before any inner-tool / network path.
    const alloc = std.testing.allocator;
    const wildcards = [_][]const u8{ "*", "*.fly.dev", "8.8.8.*" };
    for (wildcards) |w| {
        const allow = [_][]const u8{w};
        const policy = newPolicy(&allow, null);
        var t = newTool(&policy);

        var args: JsonObjectMap = .empty;
        defer args.deinit(alloc);
        try args.put(alloc, k_url, .{ .string = "https://8.8.8.8/v1/apps" });

        const r = try t.execute(alloc, args);
        defer freeResult(alloc, r);
        try std.testing.expect(!r.success);
        try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
    }
}

test "tool_name and tool_params match NullClaw http_request" {
    try std.testing.expectEqualStrings(HttpRequestTool.tool_name, PolicyHttpRequestTool.tool_name);
    try std.testing.expectEqualStrings(HttpRequestTool.tool_params, PolicyHttpRequestTool.tool_params);
}
