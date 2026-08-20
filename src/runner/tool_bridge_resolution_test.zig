//! Resolution + policy-routing tests for engine/tool_bridge.zig.
//!
//! buildTools resolves a JSON tools-spec array against BRIDGE_REGISTRY: known
//! names build a NullClaw Tool, unknown names are collected in `result.skipped`
//! (logged with UZ-TOOL-005), disabled names are dropped silently. http_request
//! builds a policy-aware variant when a non-null ExecutionPolicy is supplied,
//! and the plain NullClaw variant otherwise. A default nullclaw.config.Config
//! (defaultCfg) supplies the tools/autonomy sub-configs the builders dereference.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tool_bridge = @import("engine/tool_bridge.zig");
const runner_helpers = @import("engine/runner_helpers.zig");
const child_exec_input = @import("child_exec_input.zig");
const fixtures = @import("child_exec_test_fixtures.zig");
const client_errors = @import("engine/client_errors.zig");
const context_budget = @import("engine/context_budget.zig");

const Config = nullclaw.config.Config;
const WORKSPACE = "/tmp/fleet-ws-bridge";
const TOOL_SCHEDULE = "schedule";
/// Names a lease must never receive. The scheduler entries were the whole of
/// the list this replaced; `shell` and its siblings are the ones that made a
/// no-tools Fleet dangerous, and each must fail the lease when declared.
const REFUSED_TOOLS = [_][]const u8{
    "shell",       "spawn",       "git",         "browser",
    "web_fetch",   "delegate",    TOOL_SCHEDULE, "cron_add",
    "cron_list",   "cron_remove", "cron_run",    "cron_runs",
    "cron_update",
};

/// nullclaw.config.Config requires `workspace_dir` + `config_path` (no defaults);
/// every other field (tools, autonomy, …) defaults. Builders read only the
/// defaulted sub-configs, so static paths here suffice.
fn defaultCfg() Config {
    return .{ .allocator = std.testing.allocator, .workspace_dir = WORKSPACE, .config_path = "/tmp/fleet-cfg.toml" };
}

/// Build a tools-spec JSON array of `{ "name": <n> }` objects on `alloc`.
/// Caller owns the returned Value; deinit via the helper below.
fn specOf(alloc: std.mem.Allocator, names: []const []const u8) !std.json.Value {
    var arr = std.json.Array.init(alloc);
    for (names) |n| {
        var obj: std.json.ObjectMap = .empty;
        try obj.put(alloc, "name", .{ .string = n });
        try arr.append(.{ .object = obj });
    }
    return .{ .array = arr };
}

/// Free a spec built by `specOf` — each entry's ObjectMap plus the outer array.
fn freeSpec(alloc: std.mem.Allocator, spec: std.json.Value) void {
    for (spec.array.items) |item| {
        var o = item.object;
        o.deinit(alloc);
    }
    var a = spec.array;
    a.deinit();
}

test "should resolve every canonical tool name to its own registry entry" {
    // Each registry name resolves to an entry whose .name echoes the lookup —
    // proves no cross-wired builder. Mirrors the registry's own coverage list.
    const names = [_][]const u8{
        "shell",         "file_read",    "file_write",       "file_edit",
        "file_append",   "file_delete",  "file_read_hashed", "file_edit_hashed",
        "git",           "image",        "calculator",       "memory_store",
        "memory_recall", "memory_list",  "memory_forget",    "delegate",
        "spawn",         "http_request", "web_search",       "web_fetch",
        "pushover",      "browser",      "screenshot",       "browser_open",
        "message",
    };
    for (names) |n| {
        const entry = tool_bridge.resolve(n) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(n, entry.name);
    }
}

test "a declared shell is refused, not granted" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();

    for (REFUSED_TOOLS) |tool_name| {
        const spec = try specOf(alloc, &.{tool_name});
        defer freeSpec(alloc, spec);

        try std.testing.expectError(
            error.UnsupportedHostedTool,
            tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null),
        );
    }
}

test "an absent tools spec is not a licence for every tool" {
    // This test used to assert the fallback merely FILTERED the registry — it
    // accepted a Fleet receiving every unfiltered tool as correct. That default
    // is what handed `shell` to a bundle declaring nothing. An absent spec now
    // grants nothing at all.
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const tools = try runner_helpers.buildToolsFromSpec(alloc, WORKSPACE, null, &cfg, null, null);
    defer {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    }
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "an empty tools array grants nothing" {
    // The operator's own words: `tools: []`. The empty array reaches the
    // resolver intact now, and resolves to exactly what it says.
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{});
    defer freeSpec(alloc, spec);

    const tools = try runner_helpers.buildToolsFromSpec(alloc, WORKSPACE, spec, &cfg, null, null);
    defer {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    }
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "a declared allowlisted tool still resolves" {
    // The narrowing must remove nothing a Fleet legitimately asked for — every
    // shipped bundle declares http_request.
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{"http_request"});
    defer freeSpec(alloc, spec);

    const tools = try runner_helpers.buildToolsFromSpec(alloc, WORKSPACE, spec, &cfg, null, null);
    defer {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    }
    try std.testing.expectEqual(@as(usize, 1), tools.len);
    try std.testing.expectEqualStrings("http_request", tools[0].name());
}

test "should return null when resolving an unknown tool name" {
    try std.testing.expect(tool_bridge.resolve("linear") == null);
    try std.testing.expect(tool_bridge.resolve("unknown_tool") == null);
    try std.testing.expect(tool_bridge.resolve("") == null);
}

test "should skip unknown tool and report UZ-TOOL-005 in buildTools" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{ "file_read", "unknown_tool" });
    defer freeSpec(alloc, spec);

    const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null);
    defer result.deinit(alloc);

    // file_read builds; unknown_tool lands in `skipped` (never a built tool).
    // An allowlisted tool stands in for what used to be `shell` here: the
    // subject is the SKIP path, and shell now fails the lease outright.
    try std.testing.expectEqual(@as(usize, 1), result.tools.len);
    try std.testing.expectEqual(@as(usize, 1), result.skipped.len);
    try std.testing.expectEqualStrings("unknown_tool", result.skipped[0]);
    // The error code the bridge logs for an unknown tool is the registered one.
    try std.testing.expectEqualStrings("UZ-TOOL-005", client_errors.ERR_TOOL_UNKNOWN);
}

test "should build http_request plain variant when policy is null" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{"http_request"});
    defer freeSpec(alloc, spec);

    const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null);
    defer result.deinit(alloc);

    // No policy → plain NullClaw http_request still builds (one tool, none skipped).
    try std.testing.expectEqual(@as(usize, 1), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "should build http_request policy-aware variant when ExecutionPolicy is present" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{"http_request"});
    defer freeSpec(alloc, spec);

    // A policy carrying a network allowlist drives the policy-aware builder.
    const allow = [_][]const u8{ "api.example.com", "registry.npmjs.org" };
    const policy = context_budget.ExecutionPolicy{
        .network_policy = .{ .allow = &allow },
    };
    const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, &policy, null);
    defer result.deinit(alloc);

    // Policy-aware variant still resolves to exactly one built tool.
    try std.testing.expectEqual(@as(usize, 1), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "should drop a disabled tool without building or skipping it" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    var arr = std.json.Array.init(alloc);
    defer {
        for (arr.items) |item| {
            var o = item.object;
            o.deinit(alloc);
        }
        arr.deinit();
    }
    var obj: std.json.ObjectMap = .empty;
    try obj.put(alloc, "name", .{ .string = "file_read" });
    try obj.put(alloc, "enabled", .{ .bool = false });
    try arr.append(.{ .object = obj });

    const result = try tool_bridge.buildTools(alloc, .{ .array = arr }, WORKSPACE, &cfg, null, null);
    defer result.deinit(alloc);

    // Disabled → silent drop: neither built nor reported as skipped.
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "should return empty result when spec is not an array" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const result = try tool_bridge.buildTools(alloc, .{ .integer = 7 }, WORKSPACE, &cfg, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "should have no memory leaks when buildTools skips unknown tools repeatedly" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    // Mixed known/unknown over 50 cycles; std.testing.allocator panics on any
    // leak, so a clean run proves both the tools slice and the skipped[] (with
    // its dup'd names) are fully freed by BuildResult.deinit each time.
    for (0..50) |_| {
        const spec = try specOf(alloc, &.{ "file_read", "ghost_tool", "calculator", "phantom" });
        defer freeSpec(alloc, spec);
        const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null);
        defer result.deinit(alloc);
        try std.testing.expectEqual(@as(usize, 2), result.tools.len);
        try std.testing.expectEqual(@as(usize, 2), result.skipped.len);
    }
}

test "a tool declared on a REAL lease resolves — producer and consumer agree on one shape" {
    // The test this suite was missing, and the reason a total outage sat green.
    // Every other resolution test builds its spec with `specOf`, which emits
    // `{name: …}` OBJECTS. The lease wire carries `tools: []const []const u8`
    // and `buildCallArgs` emits bare STRINGS, so production sent a shape the
    // bridge skipped outright — every declared tool dropped before any refusal
    // arm ran. Going through buildCallArgs is what makes this assertion real.
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();

    var args = try child_exec_input.buildCallArgs(alloc, fixtures.testLease(.{
        .tools = &.{"http_request"},
    }));
    defer args.deinit(alloc);

    const tools = try runner_helpers.buildToolsFromSpec(
        alloc,
        WORKSPACE,
        args.tools_spec,
        &cfg,
        null,
        null,
    );
    defer {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    }
    try std.testing.expectEqual(@as(usize, 1), tools.len);
    try std.testing.expectEqualStrings("http_request", tools[0].name());
}

test "a refused tool declared on a REAL lease still fails the lease" {
    // The refusal arms must be reachable from the shape production actually
    // sends, not only from the object shape the other tests build. If the
    // producer/consumer seam ever drifts again, this fails instead of silently
    // granting nothing and calling it safety.
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();

    var args = try child_exec_input.buildCallArgs(alloc, fixtures.testLease(.{
        .tools = &.{"shell"},
    }));
    defer args.deinit(alloc);

    try std.testing.expectError(
        error.UnsupportedHostedTool,
        runner_helpers.buildToolsFromSpec(alloc, WORKSPACE, args.tools_spec, &cfg, null, null),
    );
}
