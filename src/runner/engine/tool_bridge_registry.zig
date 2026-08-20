//! tool_bridge_registry.zig — every hosted NullClaw built-in tool, and the
//! factory that instantiates one.
//!
//! Split from `tool_bridge.zig` on the 350-line bound (RULE FLL), on the seam
//! the spec named before the work began: this file answers "what tools exist
//! and how is one built", `tool_bridge.zig` answers "which of them may a
//! hosted Fleet have, and what happens when it asks for one it may not".
//!
//! The registry is deliberately the LOWER layer: the policy file imports it to
//! prove its allowlist is a subset, so a name that drifts fails the build.

const std = @import("std");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;
const builders = @import("tool_builders.zig");
const context_budget = @import("context_budget.zig");
const credential_request = @import("credential_request.zig");

/// Tool names the registry and the hosted allowlist BOTH reference. Named so
/// the two lists cannot drift on a spelling (RULE UFS) — a rename now moves
/// one constant instead of two literals that look alike.
pub const TOOL_HTTP_REQUEST = "http_request";
pub const TOOL_MEMORY_RECALL = "memory_recall";
pub const TOOL_MEMORY_STORE = "memory_store";
pub const TOOL_MEMORY_LIST = "memory_list";
pub const TOOL_MEMORY_FORGET = "memory_forget";
pub const TOOL_FILE_READ = "file_read";
pub const TOOL_FILE_READ_HASHED = "file_read_hashed";
pub const TOOL_FILE_WRITE = "file_write";
pub const TOOL_FILE_EDIT = "file_edit";
pub const TOOL_FILE_EDIT_HASHED = "file_edit_hashed";
pub const TOOL_FILE_APPEND = "file_append";
pub const TOOL_FILE_DELETE = "file_delete";
pub const TOOL_CALCULATOR = "calculator";

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
pub const ToolEntry = struct {
    /// Canonical tool name (matches RPC "name" field).
    name: []const u8,
    /// Factory — instantiates the NullClaw Tool.
    buildFn: BuildFn,
};

// ── Static registry ────────────────────────────────────────────────────────
// Every hosted NullClaw built-in tool. Skills are dynamic — no entries here.
//
// When tools: [] or absent → zero tools. There is no fallback that grants more
// than the Fleet declared; the registry default that once filled that gap is
// what handed `shell` to a Fleet asking for nothing.
// When tools: ["http_request"] → the bridge resolves only that, and only
// because it is on HOSTED_TOOL_ALLOWLIST above.

pub const BRIDGE_REGISTRY = [_]ToolEntry{
    // Core file tools
    .{ .name = "shell", .buildFn = builders.buildShell },
    .{ .name = TOOL_FILE_READ, .buildFn = builders.buildFileRead },
    .{ .name = TOOL_FILE_WRITE, .buildFn = builders.buildFileWrite },
    .{ .name = TOOL_FILE_EDIT, .buildFn = builders.buildFileEdit },
    .{ .name = TOOL_FILE_APPEND, .buildFn = builders.buildFileAppend },
    .{ .name = TOOL_FILE_DELETE, .buildFn = builders.buildFileDelete },
    .{ .name = TOOL_FILE_READ_HASHED, .buildFn = builders.buildFileReadHashed },
    .{ .name = TOOL_FILE_EDIT_HASHED, .buildFn = builders.buildFileEditHashed },
    // Git
    .{ .name = "git", .buildFn = builders.buildGit },
    // Stateless
    .{ .name = "image", .buildFn = builders.buildImage },
    .{ .name = TOOL_CALCULATOR, .buildFn = builders.buildCalculator },
    // Memory
    .{ .name = TOOL_MEMORY_STORE, .buildFn = builders.buildMemoryStore },
    .{ .name = TOOL_MEMORY_RECALL, .buildFn = builders.buildMemoryRecall },
    .{ .name = TOOL_MEMORY_LIST, .buildFn = builders.buildMemoryList },
    .{ .name = TOOL_MEMORY_FORGET, .buildFn = builders.buildMemoryForget },
    // Fleet orchestration
    .{ .name = "delegate", .buildFn = builders.buildDelegate },
    .{ .name = "spawn", .buildFn = builders.buildSpawn },
    // Network (HTTP/search/fetch)
    .{ .name = TOOL_HTTP_REQUEST, .buildFn = builders.buildHttpRequest },
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
/// Public for `tool_bridge_test.zig` alone (see HOSTED_TOOL_ALLOWLIST).
pub const TOOL_COUNT = BRIDGE_REGISTRY.len;

/// Resolve a tool name to its registry entry.
pub fn resolve(tool_name: []const u8) ?*const ToolEntry {
    for (&BRIDGE_REGISTRY) |*entry| {
        if (std.mem.eql(u8, entry.name, tool_name)) return entry;
    }
    return null;
}
