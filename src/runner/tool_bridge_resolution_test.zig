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
const client_errors = @import("engine/client_errors.zig");
const context_budget = @import("engine/context_budget.zig");

const Config = nullclaw.config.Config;
const WORKSPACE = "/tmp/fleet-ws-bridge";
const TOOL_SCHEDULE = "schedule";
const UNSUPPORTED_HOSTED_TOOLS = [_][]const u8{
    TOOL_SCHEDULE,
    "cron_add",
    "cron_list",
    "cron_remove",
    "cron_run",
    "cron_runs",
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

test "should reject hosted NullClaw scheduler tools before execution" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();

    for (UNSUPPORTED_HOSTED_TOOLS) |tool_name| {
        const spec = try specOf(alloc, &.{tool_name});
        defer freeSpec(alloc, spec);

        try std.testing.expectError(
            error.UnsupportedHostedTool,
            tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null),
        );
    }
}

test "should filter local scheduler from fallback default tools" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const tools = try runner_helpers.buildToolsFromSpec(alloc, WORKSPACE, null, &cfg, null, null);
    defer {
        for (tools) |t| t.deinit(alloc);
        alloc.free(tools);
    }

    for (tools) |tool| {
        try std.testing.expect(!std.mem.eql(u8, TOOL_SCHEDULE, tool.name()));
        try std.testing.expect(!tool_bridge.isUnsupportedHostedToolName(tool.name()));
    }
}

test "should return null when resolving an unknown tool name" {
    try std.testing.expect(tool_bridge.resolve("linear") == null);
    try std.testing.expect(tool_bridge.resolve("unknown_tool") == null);
    try std.testing.expect(tool_bridge.resolve("") == null);
}

test "should skip unknown tool and report UZ-TOOL-005 in buildTools" {
    const alloc = std.testing.allocator;
    const cfg = defaultCfg();
    const spec = try specOf(alloc, &.{ "shell", "unknown_tool" });
    defer freeSpec(alloc, spec);

    const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null);
    defer result.deinit(alloc);

    // shell builds; unknown_tool lands in `skipped` (never a built tool).
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
        const spec = try specOf(alloc, &.{ "shell", "ghost_tool", "calculator", "phantom" });
        defer freeSpec(alloc, spec);
        const result = try tool_bridge.buildTools(alloc, spec, WORKSPACE, &cfg, null, null);
        defer result.deinit(alloc);
        try std.testing.expectEqual(@as(usize, 2), result.tools.len);
        try std.testing.expectEqual(@as(usize, 2), result.skipped.len);
    }
}
