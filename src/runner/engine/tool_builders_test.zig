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

// ── Every builder in the registry ──────────────────────────────────────────
//
// The tests above drive one builder deeply. This one drives ALL of them
// shallowly, and the two answer different questions. A builder is a wiring
// step: it allocates a tool struct, fills it from config, and returns the
// vtable-erased `Tool`. Once erased, a field left at `undefined` or a vtable
// wired to the wrong struct is no longer a compile error — it is a crash or a
// wrong answer the first time an agent invokes that tool in production.
//
// Twenty-four of the twenty-five builders had no test at all, so adding a
// twenty-sixth to the registry and forgetting to wire it was a silent change.
// Calling each one and reading its identity back through the vtable is the
// cheapest proof that the erasure landed on the right struct.

const BuilderCase = struct {
    label: []const u8,
    build: *const fn (ctx: BuildCtx) anyerror!tools_mod.Tool,
};

const ALL_BUILDERS = [_]BuilderCase{
    .{ .label = "shell", .build = tool_builders.buildShell },
    .{ .label = "file_read", .build = tool_builders.buildFileRead },
    .{ .label = "file_write", .build = tool_builders.buildFileWrite },
    .{ .label = "file_edit", .build = tool_builders.buildFileEdit },
    .{ .label = "file_append", .build = tool_builders.buildFileAppend },
    .{ .label = "file_delete", .build = tool_builders.buildFileDelete },
    .{ .label = "file_read_hashed", .build = tool_builders.buildFileReadHashed },
    .{ .label = "file_edit_hashed", .build = tool_builders.buildFileEditHashed },
    .{ .label = "git", .build = tool_builders.buildGit },
    .{ .label = "image", .build = tool_builders.buildImage },
    .{ .label = "calculator", .build = tool_builders.buildCalculator },
    .{ .label = "memory_store", .build = tool_builders.buildMemoryStore },
    .{ .label = "memory_recall", .build = tool_builders.buildMemoryRecall },
    .{ .label = "memory_list", .build = tool_builders.buildMemoryList },
    .{ .label = "memory_forget", .build = tool_builders.buildMemoryForget },
    .{ .label = "delegate", .build = tool_builders.buildDelegate },
    .{ .label = "spawn", .build = tool_builders.buildSpawn },
    .{ .label = "web_search", .build = tool_builders.buildWebSearch },
    .{ .label = "web_fetch", .build = tool_builders.buildWebFetch },
    .{ .label = "pushover", .build = tool_builders.buildPushover },
    .{ .label = "browser", .build = tool_builders.buildBrowser },
    .{ .label = "screenshot", .build = tool_builders.buildScreenshot },
    .{ .label = "browser_open", .build = tool_builders.buildBrowserOpen },
    .{ .label = "message", .build = tool_builders.buildMessage },
};

test "every tool builder returns a wired tool that can describe itself" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const cfg = testConfig();
    const policy = newPolicy(&.{});
    const ctx = BuildCtx{
        .alloc = alloc,
        .workspace_path = WORKSPACE,
        .cfg = &cfg,
        .policy = &policy,
    };

    for (ALL_BUILDERS) |case| {
        const t = case.build(ctx) catch |err| {
            std.debug.print("\nbuilder {s} failed: {s}\n", .{ case.label, @errorName(err) });
            return err;
        };
        // Reading identity back through the vtable is what proves the erasure
        // landed on the struct the builder allocated: a mismatched vtable
        // returns another tool's name, or reads uninitialised memory.
        const name = t.vtable.name(t.ptr);
        if (name.len == 0) {
            std.debug.print("\nbuilder {s} produced a tool with no name\n", .{case.label});
            return error.ToolHasNoName;
        }
        // A tool with no parameter schema cannot be offered to a model at all.
        const params = t.vtable.parameters_json(t.ptr);
        if (params.len == 0) {
            std.debug.print("\nbuilder {s} produced tool '{s}' with no parameter schema\n", .{ case.label, name });
            return error.ToolHasNoParameters;
        }
    }
}
