//! Tool bridge — table-driven NullClaw built-in tool resolver for the executor.
//!
//! Replaces the hardcoded if/else chain in runner.buildToolsFromSpec().
//! The bridge owns a static registry of {name, builderFn} entries for
//! NullClaw's built-in tools (file_read, memory, etc.).
//!
//! To add a new executor-side NullClaw tool:
//!   1. Write a builder function below.
//!   2. Add one ToolEntry to BRIDGE_REGISTRY.
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

// ── Builder functions ──────────────────────────────────────────────────────

fn buildFileRead(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.file_read.FileReadTool);
    ptr.* = .{
        .workspace_dir = ctx.workspace_path,
        .allowed_paths = ctx.cfg.autonomy.allowed_paths,
        .max_file_size = ctx.cfg.tools.max_file_size_bytes,
    };
    return ptr.tool();
}

fn buildMemoryRecall(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_recall.MemoryRecallTool);
    ptr.* = .{};
    return ptr.tool();
}

fn buildMemoryList(ctx: BuildCtx) anyerror!tools_mod.Tool {
    const ptr = try ctx.alloc.create(tools_mod.memory_list.MemoryListTool);
    ptr.* = .{};
    return ptr.tool();
}

// ── Static registry ────────────────────────────────────────────────────────
// NullClaw built-in tools only. Skills are dynamic — no entries here.

const BRIDGE_REGISTRY = [_]ToolEntry{
    .{ .name = "file_read", .buildFn = buildFileRead },
    .{ .name = "memory_recall", .buildFn = buildMemoryRecall },
    .{ .name = "memory_list", .buildFn = buildMemoryList },
};

// ── Public API ─────────────────────────────────────────────────────────────

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

test "resolve: unknown name returns null" {
    try std.testing.expect(resolve("linear") == null);
    try std.testing.expect(resolve("slack") == null);
    try std.testing.expect(resolve("memory_read") == null);
    try std.testing.expect(resolve("memory_write") == null);
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
