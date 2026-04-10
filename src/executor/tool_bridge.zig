//! Tool bridge — generic tool resolver for the executor sidecar.
//!
//! Replaces the hardcoded if/else chain in runner.buildToolsFromSpec().
//! The bridge owns a static registry of {name, builderFn} entries.
//! To add a new executor-side tool:
//!   1. Write a builder function below.
//!   2. Add one ToolEntry to BRIDGE_REGISTRY.
//!   Zero other changes required.
//!
//! Binary boundary: the executor imports only `nullclaw`. This file must
//! NOT import anything from src/zombie/, src/pipeline/, or src/main.zig.
//! JSON helpers are duplicated here rather than imported from json_helpers.zig
//! because executor tests build against src/executor/main.zig only.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;

const log = std.log.scoped(.tool_bridge);

// Duplicated from src/errors/codes.zig — executor cannot cross the binary
// boundary to import from src/errors/.
const ERR_TOOL_UNKNOWN = "UZ-TOOL-005";

/// Minimum credential length before echo-stripping is attempted.
/// Shorter values are too likely to produce false positives.
const MIN_CRED_LEN_FOR_ECHO_CHECK: usize = 16;

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
    /// Optional backward-compat aliases resolved to this entry.
    aliases: []const []const u8 = &.{},
    /// Factory — instantiates the NullClaw Tool.
    buildFn: BuildFn,
};

/// Result of credential echo stripping.
pub const StripResult = struct {
    /// Output with credential occurrences replaced by "[REDACTED]".
    /// Points to the original input slice when echo_detected=false.
    /// Caller owns the returned slice when echo_detected=true.
    output: []const u8,
    /// True when at least one redaction occurred.
    echo_detected: bool,
};

// ── Builder functions ──────────────────────────────────────────────────────
// Each function constructs one NullClaw tool type from the build context.

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
// Edit ONLY this table to add or remove executor-side tools.
// Adding a new tool: (1) write buildFn above, (2) add entry here.

const BRIDGE_REGISTRY = [_]ToolEntry{
    .{
        .name = "file_read",
        .buildFn = buildFileRead,
    },
    .{
        .name = "memory_recall",
        .aliases = &.{"memory_read"},
        .buildFn = buildMemoryRecall,
    },
    .{
        .name = "memory_list",
        .aliases = &.{"memory_write"},
        .buildFn = buildMemoryList,
    },
};

// ── Public API ─────────────────────────────────────────────────────────────

/// Resolve a tool name to its registry entry.
/// Checks canonical name first, then aliases.
/// Returns null for names not in BRIDGE_REGISTRY.
pub fn resolve(tool_name: []const u8) ?*const ToolEntry {
    for (&BRIDGE_REGISTRY) |*entry| {
        if (std.mem.eql(u8, entry.name, tool_name)) return entry;
        for (entry.aliases) |alias| {
            if (std.mem.eql(u8, alias, tool_name)) return entry;
        }
    }
    return null;
}

/// Build NullClaw tools from a JSON tools-spec array.
///
/// spec must be a .array value. Unknown names are logged and skipped;
/// they do not cause an error. Disabled tools ({enabled: false}) are skipped.
/// Caller must deinit each returned tool and free the slice.
///
/// Callers that need the allTools() fallback (null spec, non-array spec)
/// must handle that logic themselves — the bridge only processes arrays.
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

/// Strip credential values from tool output before returning to the agent.
///
/// Checks both the raw credential string and its base64-encoded form.
/// Credentials shorter than MIN_CRED_LEN_FOR_ECHO_CHECK are not checked
/// to avoid false positives on short common substrings.
///
/// When echo_detected=true, the returned output is a new allocation owned
/// by the caller. When false, output points to the original input.
pub fn stripCredentialEcho(
    alloc: std.mem.Allocator,
    output: []const u8,
    credential: []const u8,
) !StripResult {
    if (credential.len < MIN_CRED_LEN_FOR_ECHO_CHECK) {
        return .{ .output = output, .echo_detected = false };
    }

    var current = output;
    var detected = false;

    // Check and strip the raw credential.
    if (std.mem.indexOf(u8, current, credential) != null) {
        current = try std.mem.replaceOwned(u8, alloc, current, credential, "[REDACTED]");
        detected = true;
    }

    // Check and strip the base64-encoded credential.
    const b64_cap = std.base64.standard.Encoder.calcSize(credential.len);
    const b64_buf = try alloc.alloc(u8, b64_cap);
    defer alloc.free(b64_buf);
    const b64 = std.base64.standard.Encoder.encode(b64_buf, credential);

    if (std.mem.indexOf(u8, current, b64) != null) {
        const stripped = try std.mem.replaceOwned(u8, alloc, current, b64, "[REDACTED]");
        if (detected) alloc.free(current); // release intermediate allocation
        current = stripped;
        detected = true;
    }

    return .{ .output = current, .echo_detected = detected };
}

// ── JSON helpers ───────────────────────────────────────────────────────────
// Duplicated from json_helpers.zig — executor binary boundary prevents import.

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

test "resolve: alias resolved to canonical" {
    const entry = resolve("memory_read").?;
    try std.testing.expectEqualStrings("memory_recall", entry.name);
}

test "resolve: alias memory_write resolves to memory_list" {
    const entry = resolve("memory_write").?;
    try std.testing.expectEqualStrings("memory_list", entry.name);
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

test "stripCredentialEcho: no-op for short credential" {
    const alloc = std.testing.allocator;
    const result = try stripCredentialEcho(alloc, "some output text", "short");
    try std.testing.expect(!result.echo_detected);
    try std.testing.expectEqualStrings("some output text", result.output);
}

test "stripCredentialEcho: strips raw credential from output" {
    const alloc = std.testing.allocator;
    const cred = "xoxb-test-credential-value-1234";
    const output = "Authorization: Bearer xoxb-test-credential-value-1234 ok";
    const result = try stripCredentialEcho(alloc, output, cred);
    defer if (result.echo_detected) alloc.free(result.output);
    try std.testing.expect(result.echo_detected);
    try std.testing.expect(std.mem.indexOf(u8, result.output, cred) == null);
    try std.testing.expect(std.mem.indexOf(u8, result.output, "[REDACTED]") != null);
}

test "stripCredentialEcho: strips base64-encoded credential" {
    const alloc = std.testing.allocator;
    const cred = "xoxb-test-credential-value-1234";
    const b64_len = std.base64.standard.Encoder.calcSize(cred.len);
    var b64_buf: [64]u8 = undefined;
    const b64 = std.base64.standard.Encoder.encode(b64_buf[0..b64_len], cred);
    const output = try std.fmt.allocPrint(alloc, "encoded: {s} end", .{b64});
    defer alloc.free(output);
    const result = try stripCredentialEcho(alloc, output, cred);
    defer if (result.echo_detected) alloc.free(result.output);
    try std.testing.expect(result.echo_detected);
    try std.testing.expect(std.mem.indexOf(u8, result.output, b64) == null);
}

test "stripCredentialEcho: no false positive on partial credential match" {
    const alloc = std.testing.allocator;
    // First 8 chars of cred appear in output — not enough to trigger stripping.
    const cred = "xoxb-test-credential-value-1234";
    const output = "xoxb-tes appears here but not the full credential";
    const result = try stripCredentialEcho(alloc, output, cred);
    try std.testing.expect(!result.echo_detected);
    try std.testing.expectEqualStrings(output, result.output);
}
