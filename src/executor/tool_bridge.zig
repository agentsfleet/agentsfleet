//! Tool bridge — table-driven NullClaw built-in tool resolver for the executor.
//!
//! Replaces the hardcoded if/else chain in runner.buildToolsFromSpec().
//! The bridge owns a static registry of {name, builderFn} entries for
//! every NullClaw built-in tool.
//!
//! To add a new executor-side NullClaw tool:
//!   1. Write a builder function in tool_builders.zig.
//!   2. Add one ToolEntry to BRIDGE_REGISTRY below.
//!   Zero other changes required.
//!
//! This file is NOT about skill tools (Slack, GitHub, AgentMail). Skills are
//! dynamic — the agent uses NullClaw's shell/HTTP tools to interact with
//! skill APIs using injected credentials. No compiled Zig per skill.
//!
//! Binary boundary: the executor imports only `nullclaw`. This file must
//! NOT import anything from src/zombie/, src/pipeline/, or src/main.zig.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;
const builders = @import("tool_builders.zig");

const log = std.log.scoped(.tool_bridge);

// Duplicated from src/errors/codes.zig — executor cannot cross the binary
// boundary to import from src/errors/.
const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";

// ── Types ──────────────────────────────────────────────────────────────────

/// Context passed to every builder function.
pub const BuildCtx = struct {
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
};

/// Factory function type — receives context, returns a NullClaw Tool.
pub const BuildFn = *const fn (ctx: BuildCtx) anyerror!tools_mod.Tool;

/// One entry in the bridge registry.
pub const ToolEntry = struct {
    /// Canonical tool name (matches RPC "name" field).
    name: []const u8,
    /// Factory — instantiates the NullClaw Tool.
    buildFn: BuildFn,
};

// ── Static registry ────────────────────────────────────────────────────────
// Every NullClaw built-in tool. Skills are dynamic — no entries here.
//
// When tools: null → allTools() gives the agent everything (default).
// When tools: ["shell", "file_read"] → the bridge resolves only those.

const BRIDGE_REGISTRY = [_]ToolEntry{
    // Core file tools
    .{ .name = "shell", .buildFn = builders.buildShell },
    .{ .name = "file_read", .buildFn = builders.buildFileRead },
    .{ .name = "file_write", .buildFn = builders.buildFileWrite },
    .{ .name = "file_edit", .buildFn = builders.buildFileEdit },
    .{ .name = "file_append", .buildFn = builders.buildFileAppend },
    .{ .name = "file_delete", .buildFn = builders.buildFileDelete },
    .{ .name = "file_read_hashed", .buildFn = builders.buildFileReadHashed },
    .{ .name = "file_edit_hashed", .buildFn = builders.buildFileEditHashed },
    // Git
    .{ .name = "git", .buildFn = builders.buildGit },
    // Stateless
    .{ .name = "image", .buildFn = builders.buildImage },
    .{ .name = "calculator", .buildFn = builders.buildCalculator },
    // Memory
    .{ .name = "memory_store", .buildFn = builders.buildMemoryStore },
    .{ .name = "memory_recall", .buildFn = builders.buildMemoryRecall },
    .{ .name = "memory_list", .buildFn = builders.buildMemoryList },
    .{ .name = "memory_forget", .buildFn = builders.buildMemoryForget },
    // Agent orchestration
    .{ .name = "delegate", .buildFn = builders.buildDelegate },
    .{ .name = "schedule", .buildFn = builders.buildSchedule },
    .{ .name = "spawn", .buildFn = builders.buildSpawn },
    // Network (HTTP/search/fetch)
    .{ .name = "http_request", .buildFn = builders.buildHttpRequest },
    .{ .name = "web_search", .buildFn = builders.buildWebSearch },
    .{ .name = "web_fetch", .buildFn = builders.buildWebFetch },
    .{ .name = "pushover", .buildFn = builders.buildPushover },
    // Browser
    .{ .name = "browser", .buildFn = builders.buildBrowser },
    .{ .name = "screenshot", .buildFn = builders.buildScreenshot },
    .{ .name = "browser_open", .buildFn = builders.buildBrowserOpen },
    // Cron
    .{ .name = "cron_add", .buildFn = builders.buildCronAdd },
    .{ .name = "cron_list", .buildFn = builders.buildCronList },
    .{ .name = "cron_remove", .buildFn = builders.buildCronRemove },
    .{ .name = "cron_run", .buildFn = builders.buildCronRun },
    .{ .name = "cron_runs", .buildFn = builders.buildCronRuns },
    .{ .name = "cron_update", .buildFn = builders.buildCronUpdate },
    // Misc
    .{ .name = "message", .buildFn = builders.buildMessage },
};

// ── Public API ─────────────────────────────────────────────────────────────

/// Total number of registered tools.
pub const TOOL_COUNT = BRIDGE_REGISTRY.len;

/// Resolve a tool name to its registry entry.
pub fn resolve(tool_name: []const u8) ?*const ToolEntry {
    for (&BRIDGE_REGISTRY) |*entry| {
        if (std.mem.eql(u8, entry.name, tool_name)) return entry;
    }
    return null;
}

/// Build NullClaw tools from a JSON tools-spec array.
///
/// Unknown names are logged and skipped. Disabled tools are skipped.
/// Callers that need allTools() fallback (null/non-array spec) handle
/// that logic themselves — the bridge only processes arrays.
pub fn buildTools(
    alloc: std.mem.Allocator,
    spec: std.json.Value,
    workspace_path: []const u8,
    cfg: *const Config,
) ![]tools_mod.Tool {
    const ctx = BuildCtx{ .alloc = alloc, .workspace_path = workspace_path, .cfg = cfg };

    var list: std.ArrayList(tools_mod.Tool) = .{};
    errdefer {
        for (list.items) |t| t.deinit(alloc);
        list.deinit(alloc);
    }

    if (spec != .array) return list.toOwnedSlice(alloc);

    for (spec.array.items) |item| {
        if (item != .object) continue;
        const tool_name = jsonGetStr(item, "name") orelse continue;
        if (!jsonGetBoolDefault(item, "enabled", true)) continue;

        const entry = resolve(tool_name) orelse {
            log.warn("tool_bridge.unknown_tool error_code={s} name={s}", .{ ERR_TOOL_UNKNOWN, tool_name });
            continue;
        };

        const t = entry.buildFn(ctx) catch |err| {
            log.err("tool_bridge.build_failed name={s} err={s}", .{ tool_name, @errorName(err) });
            continue;
        };
        try list.append(alloc, t);
    }

    return list.toOwnedSlice(alloc);
}

// ── JSON helpers ───────────────────────────────────────────────────────────
// Duplicated — executor binary boundary prevents import.

fn jsonGetStr(val: std.json.Value, key: []const u8) ?[]const u8 {
    if (val != .object) return null;
    const v = val.object.get(key) orelse return null;
    return if (v == .string) v.string else null;
}

fn jsonGetBoolDefault(val: std.json.Value, key: []const u8, default: bool) bool {
    if (val != .object) return default;
    const v = val.object.get(key) orelse return default;
    return if (v == .bool) v.bool else default;
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "resolve: canonical name found" {
    const entry = resolve("file_read").?;
    try std.testing.expectEqualStrings("file_read", entry.name);
}

test "resolve: all core tools resolvable" {
    const core = [_][]const u8{
        "shell",     "file_read",      "file_write",    "file_edit",
        "file_append", "file_delete",  "file_read_hashed", "file_edit_hashed",
        "git",       "image",          "calculator",
        "memory_store", "memory_recall", "memory_list", "memory_forget",
        "delegate",  "schedule",       "spawn",
        "http_request", "web_search",  "web_fetch",     "pushover",
        "browser",   "screenshot",     "browser_open",
        "cron_add",  "cron_list",      "cron_remove",   "cron_run",
        "cron_runs", "cron_update",    "message",
    };
    for (core) |name| {
        try std.testing.expect(resolve(name) != null);
    }
    try std.testing.expectEqual(@as(usize, core.len), TOOL_COUNT);
}

test "resolve: unknown name returns null" {
    try std.testing.expect(resolve("linear") == null);
    try std.testing.expect(resolve("slack") == null);
    try std.testing.expect(resolve("") == null);
}

test "buildTools: empty array returns empty slice" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    const tools = try buildTools(alloc, arr, "/tmp", undefined);
    defer alloc.free(tools);
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "buildTools: non-array value returns empty slice" {
    const alloc = std.testing.allocator;
    const tools = try buildTools(alloc, .{ .integer = 42 }, "/tmp", undefined);
    defer alloc.free(tools);
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "buildTools: unknown tool name skipped without error" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj = std.json.ObjectMap.init(alloc);
    defer obj.deinit();
    try obj.put("name", .{ .string = "unknown_future_tool" });
    try arr.array.append(.{ .object = obj });
    const tools = try buildTools(alloc, arr, "/tmp", undefined);
    defer alloc.free(tools);
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}

test "buildTools: disabled tool skipped" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj = std.json.ObjectMap.init(alloc);
    defer obj.deinit();
    try obj.put("name", .{ .string = "file_read" });
    try obj.put("enabled", .{ .bool = false });
    try arr.array.append(.{ .object = obj });
    const tools = try buildTools(alloc, arr, "/tmp", undefined);
    defer alloc.free(tools);
    try std.testing.expectEqual(@as(usize, 0), tools.len);
}
