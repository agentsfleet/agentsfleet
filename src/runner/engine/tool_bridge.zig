//! Tool bridge — the POLICY half of hosted tool resolution.
//!
//! The registry of {name, builderFn} entries moved to
//! `tool_bridge_registry.zig`; this file answers the narrower question "which
//! of those may a hosted Fleet actually have, and what happens when it asks
//! for one it may not". The registry is the lower layer, so the allowlist
//! below can prove itself a subset of it at comptime.
//!
//! To add a new runner-side hosted NullClaw tool:
//!   1. Write a builder function in tool_builders.zig.
//!   2. Add one ToolEntry to BRIDGE_REGISTRY in tool_bridge_registry.zig.
//!   3. Add its name to HOSTED_TOOL_ALLOWLIST below, against the membership
//!      rule documented there.
//!   Step 3 is NOT optional: a tool in the registry but off the allowlist is
//!   refused, and the refusal FAILS THE LEASE. Stopping after step 2 ships a
//!   tool that kills every Fleet declaring it.
//!
//! This file is NOT about skill tools (Slack, GitHub, AgentMail). Skills are
//! dynamic — the fleet reaches skill APIs through `http_request` with injected
//! credentials. No compiled Zig per skill. (`shell` is deliberately NOT part of
//! that story any more; it is refused by the allowlist below.)
//!
//! Binary boundary: the runner imports only `nullclaw`. This file must
//! NOT import anything from src/fleet/, src/pipeline/, or src/main.zig.

const std = @import("std");
const logging = @import("log");
const nullclaw = @import("nullclaw");
const tools_mod = nullclaw.tools;
const Config = nullclaw.config.Config;
const context_budget = @import("context_budget.zig");
const client_errors = @import("client_errors.zig");
const credential_request = @import("credential_request.zig");

const log = logging.scoped(.tool_bridge);

/// The registry this file gates. Lower layer by construction: the allowlist
/// below proves itself a subset of it at comptime.
const registry = @import("tool_bridge_registry.zig");
pub const BuildCtx = registry.BuildCtx;
pub const TOOL_COUNT = registry.TOOL_COUNT;
pub const resolve = registry.resolve;
const BRIDGE_REGISTRY = registry.BRIDGE_REGISTRY;

const ERR_TOOL_UNKNOWN = client_errors.ERR_TOOL_UNKNOWN;
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;

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
    registry.TOOL_HTTP_REQUEST,
    // Fleet memory — the control plane owns the store; these only read/write it.
    registry.TOOL_MEMORY_RECALL,
    registry.TOOL_MEMORY_STORE,
    registry.TOOL_MEMORY_LIST,
    registry.TOOL_MEMORY_FORGET,
    // Files, every one workspace-scoped with symlink-escape resolution.
    registry.TOOL_FILE_READ,
    registry.TOOL_FILE_READ_HASHED,
    registry.TOOL_FILE_WRITE,
    registry.TOOL_FILE_EDIT,
    registry.TOOL_FILE_EDIT_HASHED,
    registry.TOOL_FILE_APPEND,
    registry.TOOL_FILE_DELETE,
    // Pure computation, no I/O at all.
    registry.TOOL_CALCULATOR,
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
/// Disabled tools are skipped silently. A null or non-array spec yields ZERO
/// tools — there is no registry-default fallback any more, and its removal is
/// what turned a silent producer/consumer mismatch into a visible one.
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
        // TWO shapes reach here, and only one of them was ever handled. The
        // lease wire carries `tools: []const []const u8` — bare strings
        // (`protocol_lease_v1.ExecutionPolicy`), and `child_exec_input` emits
        // them as `.string`. This loop required `.object` and skipped anything
        // else, so in production EVERY declared tool was dropped before any
        // refusal arm ran. The suite never saw it because its `specOf` helper
        // builds `{name: …}` objects — the one shape the wire does not send.
        //
        // It stayed invisible while an empty list fell back to the whole
        // registry: a Fleet got tools by accident, not by declaration. Removing
        // that fallback is what turned a silent mismatch into "no Fleet gets any
        // tool", which is how it finally surfaced.
        const tool_name = switch (item) {
            .string => |s| s,
            .object => blk: {
                // `enabled` exists only on the object shape; a bare string is
                // enabled by construction (naming it IS the declaration).
                if (!jsonGetBoolDefault(item, "enabled", true)) continue;
                break :blk jsonGetStr(item, "name") orelse continue;
            },
            // A shape that is neither cannot name a tool, so it grants nothing.
            // It is skipped rather than fatal for the same reason an unknown
            // name is: it asks for nothing, so it gets nothing.
            else => continue,
        };
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
