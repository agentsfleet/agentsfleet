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
/// Tool names the registry and the hosted allowlist BOTH reference. Named so
/// the two lists cannot drift on a spelling (RULE UFS) — a rename now moves
/// one constant instead of two literals that look alike.
const TOOL_HTTP_REQUEST = "http_request";
const TOOL_MEMORY_RECALL = "memory_recall";
const TOOL_MEMORY_STORE = "memory_store";
const TOOL_MEMORY_LIST = "memory_list";
const TOOL_MEMORY_FORGET = "memory_forget";
const TOOL_FILE_READ = "file_read";
const TOOL_FILE_READ_HASHED = "file_read_hashed";
const TOOL_FILE_WRITE = "file_write";
const TOOL_FILE_EDIT = "file_edit";
const TOOL_FILE_EDIT_HASHED = "file_edit_hashed";
const TOOL_FILE_APPEND = "file_append";
const TOOL_FILE_DELETE = "file_delete";
const TOOL_CALCULATOR = "calculator";

/// The refusal event both fatal arms emit — one name, two sites (RULE UFS).
const LOG_TOOL_REFUSED = "tool_refused_not_hosted";

/// Every tool a hosted Fleet may reach. An ALLOWLIST, and the direction is the
/// whole point: the list it replaced named seven tools to exclude out of a
/// registry of thirty-five, so `shell`, `spawn`, `git`, `browser`, `web_fetch`
/// and the rest were reachable by default, and a tool added upstream enrolled
/// itself into hosted execution with no decision taken here.
///
/// That is the same rot `protocol_bind.zig` records about paths — "a denylist
/// alone would fail open on everything unlisted and go stale the moment a host
/// gains a new sensitive path". The reasoning was never carried across to
/// tools; this carries it.
///
/// The membership rule: a tool earns a place only if it runs IN-PROCESS and
/// reaches nothing beyond the workspace and the Fleet's own declared network
/// allowance. Anything that spawns a process, opens a browser, reaches the host,
/// or calls a third-party service stays out — a lease sandbox shares the host
/// network namespace under the interim `allow_all` posture, so a spawned
/// process is a foothold on the host's network, not just on its filesystem.
///
/// Public for the test suite and `runner_helpers`: both assert membership.
pub const HOSTED_TOOL_ALLOWLIST = [_][]const u8{
    // Outbound, already bounded by the Fleet's declared network allowance.
    TOOL_HTTP_REQUEST,
    // Fleet memory — the control plane owns the store; these only read/write it.
    TOOL_MEMORY_RECALL,
    TOOL_MEMORY_STORE,
    TOOL_MEMORY_LIST,
    TOOL_MEMORY_FORGET,
    // Files, every one workspace-scoped with symlink-escape resolution.
    TOOL_FILE_READ,
    TOOL_FILE_READ_HASHED,
    TOOL_FILE_WRITE,
    TOOL_FILE_EDIT,
    TOOL_FILE_EDIT_HASHED,
    TOOL_FILE_APPEND,
    TOOL_FILE_DELETE,
    // Pure computation, no I/O at all.
    TOOL_CALCULATOR,
};

/// Real engine tools this platform never hosts, which `BRIDGE_REGISTRY` does not
/// carry. Kept separate from the allowlist because they answer a different
/// question: the allowlist says what a Fleet MAY have, this says which absences
/// are deliberate rather than accidental.
const NEVER_HOSTED_TOOLS = [_][]const u8{
    "schedule", "cron_add",  "cron_list",   "cron_remove",
    "cron_run", "cron_runs", "cron_update",
};

comptime {
    // The allowlist is a subset of the registry. A name that drifts — an
    // upstream rename, a typo — fails the build rather than silently granting
    // nothing, which would read as "that Fleet just has no tools" at runtime.
    for (HOSTED_TOOL_ALLOWLIST) |allowed| {
        var found = false;
        for (BRIDGE_REGISTRY) |entry| {
            if (std.mem.eql(u8, entry.name, allowed)) found = true;
        }
        if (!found)
            @compileError("HOSTED_TOOL_ALLOWLIST names a tool absent from BRIDGE_REGISTRY: " ++ allowed);
    }
}

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
// When tools: [] or absent → zero tools. There is no fallback that grants more
// than the Fleet declared; the registry default that once filled that gap is
// what handed `shell` to a Fleet asking for nothing.
// When tools: ["http_request"] → the bridge resolves only that, and only
// because it is on HOSTED_TOOL_ALLOWLIST above.

const BRIDGE_REGISTRY = [_]ToolEntry{
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

/// True when a hosted Fleet may reach this tool. The allowlist is the control;
/// everything outside it is refused, whether or not anyone thought of it.
pub fn isHostedToolAllowed(tool_name: []const u8) bool {
    for (HOSTED_TOOL_ALLOWLIST) |allowed| {
        if (std.mem.eql(u8, allowed, tool_name)) return true;
    }
    return false;
}

/// True when a name is a real engine tool this platform deliberately does not
/// host, as opposed to a name nobody has heard of.
///
/// The distinction earns its keep because `BRIDGE_REGISTRY` never carried the
/// scheduler tools: `resolve` returns null for them exactly as it does for a
/// typo, so without this they would be skipped as "unknown" and a bundle that
/// asked for scheduling would run silently without it. Hosted scheduling goes
/// through agentsfleetd cron instead, and asking for the local one is a
/// misconfiguration worth failing loudly.
fn isNeverHosted(tool_name: []const u8) bool {
    for (NEVER_HOSTED_TOOLS) |never| {
        if (std.mem.eql(u8, never, tool_name)) return true;
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
        // Order is the behaviour, and each arm answers a different question.
        // A deliberately-unhosted engine tool fails the lease even though the
        // registry cannot resolve it — otherwise it reads as a typo. A name
        // nobody knows is skipped, as it always has been: it grants nothing
        // either way. A name that RESOLVES but is not allowlisted is a real
        // tool this Fleet may not have, and that is worth failing over.
        if (isNeverHosted(tool_name)) {
            log.err(LOG_TOOL_REFUSED, .{ .error_code = ERR_TOOL_UNKNOWN, .name = tool_name });
            return error.UnsupportedHostedTool;
        }

        const entry = resolve(tool_name) orelse {
            log.warn("unknown_tool", .{ .error_code = ERR_TOOL_UNKNOWN, .name = tool_name });
            const duped = try alloc.dupe(u8, tool_name);
            try skipped.append(alloc, duped);
            continue;
        };

        // Refusal fails the LEASE rather than dropping the tool and continuing.
        // A bundle asking for a real tool it may not have is misconfigured or
        // hostile; running it with a quietly different tool set would change its
        // behaviour in a way its author never wrote. Same disposition the
        // cron/schedule refusal has always had, now covering the whole set.
        if (!isHostedToolAllowed(tool_name)) {
            log.err(LOG_TOOL_REFUSED, .{ .error_code = ERR_TOOL_UNKNOWN, .name = tool_name });
            return error.UnsupportedHostedTool;
        }

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

test {
    _ = @import("tool_bridge_test.zig");
}
