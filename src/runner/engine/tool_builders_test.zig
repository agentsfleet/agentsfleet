//! End-to-end tests for `tool_builders.buildHttpRequest` (M100).
//!
//! Unlike `policy_http_request_test.zig` (which hand-builds the tool), these
//! drive the REAL production builder — the path that wires the inner NullClaw
//! allowlist — so a regression that re-feeds the tenant allowlist to the inner
//! tool (re-opening the SSRF skip + the wildcard split-brain) fails here.
//!
//! Allocation model mirrors production: the tool struct is arena-allocated
//! (BuildCtx.alloc → freed by the session arena), while `execute` takes the
//! call allocator (`std.testing.allocator`) so its result paths stay
//! leak-checked. IP literals keep `resolveConnectHost` hermetic (no DNS).

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const JsonObjectMap = tools_mod.JsonObjectMap;

const tool_builders = @import("tool_builders.zig");
const bridge = @import("tool_bridge.zig");
const BuildCtx = bridge.BuildCtx;
// BuildCtx.cfg is NullClaw's Config (not the runner daemon Config); buildHttpRequest
// reads only `cfg.tools.shell_timeout_secs`, which is defaulted.
const Config = nullclaw.config.Config;
const context_budget = @import("context_budget.zig");

const NETWORK_DISABLED: []const u8 = "Network disabled in tests";
const BLOCKED_LOCAL: []const u8 = "Blocked local/private host";
const WORKSPACE = "/tmp/agentsfleet-runner-ws";

/// Minimal NullClaw Config; only `workspace_dir` + `config_path` lack defaults.
/// `buildHttpRequest` reads only `cfg.tools.shell_timeout_secs` (defaulted).
fn testConfig() Config {
    return Config{
        .workspace_dir = WORKSPACE,
        .config_path = "",
        .allocator = std.testing.allocator,
    };
}

fn newPolicy(allow: []const []const u8) context_budget.ExecutionPolicy {
    return .{
        .network_policy = .{ .allow = allow },
        .tools = &.{},
        .secrets_map = null,
        .context = .{},
    };
}

/// Build via the real production builder, execute one url, return the result.
/// The tool lives in `arena`; the result uses `std.testing.allocator`.
fn runBuilt(
    arena: std.mem.Allocator,
    policy: *const context_budget.ExecutionPolicy,
    url: []const u8,
) !tools_mod.ToolResult {
    const cfg = testConfig();
    const ctx = BuildCtx{
        .alloc = arena,
        .workspace_path = WORKSPACE,
        .cfg = &cfg,
        .policy = policy,
    };
    const t = try tool_builders.buildHttpRequest(ctx);

    var args: JsonObjectMap = .empty;
    defer args.deinit(std.testing.allocator);
    try args.put(std.testing.allocator, "url", .{ .string = url });
    return t.execute(std.testing.allocator, args);
}

fn freeResult(r: tools_mod.ToolResult) void {
    const m = r.error_msg orelse return;
    // Only our outer `host_not_allowed: <host>` message is heap-owned; every
    // inner NullClaw message in these tests is a string literal.
    if (std.mem.startsWith(u8, m, "host_not_allowed:")) std.testing.allocator.free(m);
}

test "buildHttpRequest (policy path) rejects a tenant private-IP host end-to-end" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    // Tenant lists the cloud-metadata IP. The real builder must NOT treat it as
    // operator-trusted: the inner allowlist is empty, so the SSRF resolve blocks it.
    const allow = [_][]const u8{"169.254.169.254"};
    const policy = newPolicy(&allow);
    const r = try runBuilt(arena, &policy, "https://169.254.169.254/latest/meta-data");
    defer freeResult(r);
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(BLOCKED_LOCAL, r.error_msg.?);
}

test "buildHttpRequest (policy path) admits an allowlisted global host end-to-end" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const allow = [_][]const u8{"8.8.8.8"};
    const policy = newPolicy(&allow);
    const r = try runBuilt(arena, &policy, "https://8.8.8.8/v1/apps");
    defer freeResult(r);
    // Passed the outer gate + the inner SSRF resolve (global IP) → reached the
    // inner tool's is_test short-circuit.
    try std.testing.expect(!r.success);
    try std.testing.expectEqualStrings(NETWORK_DISABLED, r.error_msg.?);
}

test "buildHttpRequest (policy path) denies an off-allowlist host at the outer gate" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    const allow = [_][]const u8{"8.8.8.8"};
    const policy = newPolicy(&allow);
    const r = try runBuilt(arena, &policy, "https://1.1.1.1/v1/apps");
    defer freeResult(r);
    try std.testing.expect(!r.success);
    try std.testing.expect(std.mem.startsWith(u8, r.error_msg.?, "host_not_allowed:"));
}
