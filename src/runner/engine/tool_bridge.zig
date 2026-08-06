//! Tool bridge — table-driven NullClaw built-in tool resolver for the runner.
//!
//! Replaces the hardcoded if/else chain in runner.buildToolsFromSpec().
//! The bridge owns a static registry of {name, builderFn} entries for
//! every hosted NullClaw built-in tool.
//!
//! To add a new runner-side hosted NullClaw tool:
//!   1. Write a builder function in tool_builders.zig.
//!   2. Add one ToolEntry to BRIDGE_REGISTRY below.
//!   Zero other changes required.
//!
//! This file is NOT about skill tools (Slack, GitHub, AgentMail). Skills are
//! dynamic — the fleet uses NullClaw's shell/HTTP tools to interact with
//! skill APIs using injected credentials. No compiled Zig per skill.
//!
//! Binary boundary: the runner imports only `nullclaw`. This file must
//! NOT import anything from src/fleet/, src/pipeline/, or src/main.zig.

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;
const builders = @import("tool_builders.zig");
const context_budget = @import("context_budget.zig");
const client_errors = @import("client_errors.zig");
const credential_request = @import("credential_request.zig");

const log = logging.scoped(.tool_bridge);

const ERR_TOOL_UNKNOWN = client_errors.ERR_TOOL_UNKNOWN;
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const TOOL_SCHEDULE = "schedule";
const TOOL_CRON_ADD = "cron_add";
const TOOL_CRON_LIST = "cron_list";
const TOOL_CRON_REMOVE = "cron_remove";
const TOOL_CRON_RUN = "cron_run";
const TOOL_CRON_RUNS = "cron_runs";
const TOOL_CRON_UPDATE = "cron_update";
const UNSUPPORTED_HOSTED_TOOLS = [_][]const u8{
    TOOL_SCHEDULE,
    TOOL_CRON_ADD,
    TOOL_CRON_LIST,
    TOOL_CRON_REMOVE,
    TOOL_CRON_RUN,
    TOOL_CRON_RUNS,
    TOOL_CRON_UPDATE,
};

// ── Types ──────────────────────────────────────────────────────────────────

/// Context passed to every builder function.
///
/// `policy` is borrowed from the session for the lifetime of the stage.
/// When non-null, builders for tools that consult per-execution policy
/// (currently only http_request) construct the policy-aware variant
/// and capture the borrow. `null` keeps the plain NullClaw behaviour
/// for callers that don't have a session yet (e.g. unit tests, the
/// register-only fallback path before policy-aware execution lands everywhere).
pub const BuildCtx = struct {
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    cfg: *const Config,
    policy: ?*const context_budget.ExecutionPolicy = null,
    /// The child→runner on-demand mint channel (M102 §4), threaded to the
    /// policy-aware http tool. Null on the no-session path (unit tests, the
    /// register-only fallback) — a mintable placeholder then fails closed.
    cred_channel: ?credential_request.Channel = null,
};

/// Factory function type — receives context, returns a NullClaw Tool.
const BuildFn = *const fn (ctx: BuildCtx) anyerror!tools_mod.Tool;

/// One entry in the bridge registry.
const ToolEntry = struct {
    /// Canonical tool name (matches RPC "name" field).
    name: []const u8,
    /// Factory — instantiates the NullClaw Tool.
    buildFn: BuildFn,
};

// ── Static registry ────────────────────────────────────────────────────────
// Every hosted NullClaw built-in tool. Skills are dynamic — no entries here.
//
// When tools: null → runner_helpers filters NullClaw fallback tools against
// this file's unsupported hosted-tool list before exposing them.
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
    // Fleet orchestration
    .{ .name = "delegate", .buildFn = builders.buildDelegate },
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
    // Misc
    .{ .name = "message", .buildFn = builders.buildMessage },
};

// ── Public API ─────────────────────────────────────────────────────────────

/// Total number of registered tools.
const TOOL_COUNT = BRIDGE_REGISTRY.len;

/// Resolve a tool name to its registry entry.
pub fn resolve(tool_name: []const u8) ?*const ToolEntry {
    for (&BRIDGE_REGISTRY) |*entry| {
        if (std.mem.eql(u8, entry.name, tool_name)) return entry;
    }
    return null;
}

/// True when a NullClaw tool manages local scheduler state and is therefore
/// unsupported in hosted runs. Hosted scheduling goes through agentsfleetd cron.
pub fn isUnsupportedHostedToolName(tool_name: []const u8) bool {
    for (UNSUPPORTED_HOSTED_TOOLS) |unsupported| {
        if (std.mem.eql(u8, unsupported, tool_name)) return true;
    }
    return false;
}

/// Result of buildTools — tools plus any names that could not be resolved.
pub const BuildResult = struct {
    tools: []tools_mod.Tool,
    /// Tool names from the spec that were not in BRIDGE_REGISTRY.
    /// Caller should log these to the activity stream for observability.
    skipped: []const []const u8,

    pub fn deinit(self: *const BuildResult, alloc: std.mem.Allocator) void {
        for (self.tools) |t| t.deinit(alloc);
        alloc.free(self.tools);
        for (self.skipped) |s| alloc.free(s);
        alloc.free(self.skipped);
    }
};

/// Build NullClaw tools from a JSON tools-spec array.
///
/// Unknown names are logged and collected in `result.skipped`.
/// Disabled tools are skipped silently. Callers that need allTools()
/// fallback (null/non-array spec) handle that logic themselves.
pub fn buildTools(
    alloc: std.mem.Allocator,
    spec: std.json.Value,
    workspace_path: []const u8,
    cfg: *const Config,
    policy: ?*const context_budget.ExecutionPolicy,
    cred_channel: ?credential_request.Channel,
) !BuildResult {
    const ctx = BuildCtx{
        .alloc = alloc,
        .workspace_path = workspace_path,
        .cfg = cfg,
        .policy = policy,
        .cred_channel = cred_channel,
    };

    var list: std.ArrayList(tools_mod.Tool) = .empty;
    errdefer {
        for (list.items) |t| t.deinit(alloc);
        list.deinit(alloc);
    }

    var skipped: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (skipped.items) |s| alloc.free(s);
        skipped.deinit(alloc);
    }

    if (spec != .array) return .{
        .tools = try list.toOwnedSlice(alloc),
        .skipped = try skipped.toOwnedSlice(alloc),
    };

    for (spec.array.items) |item| {
        if (item != .object) continue;
        const tool_name = jsonGetStr(item, "name") orelse continue;
        if (!jsonGetBoolDefault(item, "enabled", true)) continue;
        if (isUnsupportedHostedToolName(tool_name)) {
            log.err("unsupported_hosted_tool", .{ .error_code = ERR_TOOL_UNKNOWN, .name = tool_name });
            return error.UnsupportedHostedTool;
        }

        const entry = resolve(tool_name) orelse {
            log.warn("unknown_tool", .{ .error_code = ERR_TOOL_UNKNOWN, .name = tool_name });
            const duped = try alloc.dupe(u8, tool_name);
            try skipped.append(alloc, duped);
            continue;
        };

        const t = entry.buildFn(ctx) catch |err| {
            log.err("build_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .name = tool_name, .err = @errorName(err) });
            continue;
        };
        list.append(alloc, t) catch |err| {
            t.deinit(alloc);
            return err;
        };
    }

    return .{
        .tools = try list.toOwnedSlice(alloc),
        .skipped = try skipped.toOwnedSlice(alloc),
    };
}

// ── JSON helpers ───────────────────────────────────────────────────────────
// Duplicated — runner binary boundary prevents import.

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
        "shell",         "file_read",    "file_write",       "file_edit",
        "file_append",   "file_delete",  "file_read_hashed", "file_edit_hashed",
        "git",           "image",        "calculator",       "memory_store",
        "memory_recall", "memory_list",  "memory_forget",    "delegate",
        "spawn",         "http_request", "web_search",       "web_fetch",
        "pushover",      "browser",      "screenshot",       "browser_open",
        "message",
    };
    for (core) |name| {
        try std.testing.expect(resolve(name) != null);
    }
    try std.testing.expectEqual(@as(usize, core.len), TOOL_COUNT);
}

test "resolve: hosted local scheduler tools are unsupported" {
    for (UNSUPPORTED_HOSTED_TOOLS) |name| {
        try std.testing.expect(resolve(name) == null);
        try std.testing.expect(isUnsupportedHostedToolName(name));
    }
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
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}

test "buildTools: non-array value returns empty slice" {
    const alloc = std.testing.allocator;
    const result = try buildTools(alloc, .{ .integer = 42 }, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
}

test "buildTools: unknown tool name skipped and reported" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "name", .{ .string = "unknown_future_tool" });
    try arr.array.append(.{ .object = obj });
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 1), result.skipped.len);
    try std.testing.expectEqualStrings("unknown_future_tool", result.skipped[0]);
}

test "buildTools: disabled tool skipped" {
    const alloc = std.testing.allocator;
    var arr = std.json.Value{ .array = std.json.Array.init(alloc) };
    defer arr.array.deinit();
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(alloc);
    try obj.put(alloc, "name", .{ .string = "file_read" });
    try obj.put(alloc, "enabled", .{ .bool = false });
    try arr.array.append(.{ .object = obj });
    const result = try buildTools(alloc, arr, "/tmp", undefined, null, null);
    defer result.deinit(alloc);
    try std.testing.expectEqual(@as(usize, 0), result.tools.len);
    try std.testing.expectEqual(@as(usize, 0), result.skipped.len);
}
