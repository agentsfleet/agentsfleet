//! child_exec.zig — the `__execute` child mode of the runner.
//!
//! Runs in the forked (and, on a sandboxed tier, bubblewrapped) child that
//! `child_supervisor` spawns. It reads the lease from stdin, applies the
//! mandatory in-process sandbox (Landlock — fail-closed on a sandboxed tier,
//! Invariant 7), runs the NullClaw engine in this clean address space, and
//! writes the `ExecutionResult` to stdout. Logs go to stderr, so the parent
//! reads a clean JSON result frame off stdout.
//!
//! The lease (incl. inline secrets) arrives on stdin — never argv/env, which
//! are /proc-readable (RULE VLT). The workspace path arrives as `--workspace=`
//! (a path, not a secret).

const std = @import("std");
const logging = @import("log");
const contract = @import("contract");

const engine = @import("engine/runner.zig");
const types = @import("engine/types.zig");
const landlock = @import("engine/landlock.zig");
const context_budget = @import("engine/context_budget.zig");
const wire = @import("engine/wire.zig");
const pipe_proto = @import("pipe_proto.zig");

const log = logging.scoped(.runner_exec);
const LeasePayload = contract.protocol.LeasePayload;

/// argv subcommand selecting child-execute mode (vs the daemon loop).
pub const SUBCOMMAND = "__execute";
/// argv flag carrying the per-lease workspace path (not a secret).
pub const WORKSPACE_FLAG_PREFIX = "--workspace=";
/// argv flag the parent sets when the tier requires in-child sandboxing.
pub const SANDBOXED_FLAG = "--sandboxed";

/// Distinct exit code for a fail-closed sandbox-setup failure — lets the parent
/// classify it as a sandbox failure (UZ-RUN-007), not a clean exit.
const SANDBOX_FAIL_EXIT: u8 = 78;
const GENERIC_FAIL_EXIT: u8 = 1;
const MAX_LEASE_BYTES: usize = 4 * 1024 * 1024;
const READ_CHUNK: usize = 64 * 1024;

/// Child entry. Returns the process exit code (main calls `std.process.exit`
/// with it); never returns an error — every failure maps to an exit code.
pub fn run(alloc: std.mem.Allocator) u8 {
    const workspace = flagValue(WORKSPACE_FLAG_PREFIX) orelse {
        log.err("no_workspace_flag", .{});
        return SANDBOX_FAIL_EXIT;
    };

    // FAIL-CLOSED (Invariant 7): on a sandboxed tier the mandatory in-child
    // Landlock policy MUST apply before we read the lease or run the agent — a
    // sandbox we cannot establish aborts, never running tool execution
    // unsandboxed.
    if (hasFlag(SANDBOXED_FLAG)) landlock.applyPolicy(workspace) catch |err| {
        log.err("landlock_failed_fail_closed", .{ .err = @errorName(err) });
        return SANDBOX_FAIL_EXIT;
    };

    const lease_json = readStdin(alloc) catch |err| {
        log.err("lease_read_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    defer alloc.free(lease_json);

    const parsed = std.json.parseFromSlice(LeasePayload, alloc, lease_json, .{
        .ignore_unknown_fields = true,
    }) catch |err| {
        log.err("lease_parse_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    defer parsed.deinit();

    // The process exits immediately after writing; the engine result's content
    // is reclaimed by exit (no free needed on this single-shot path).
    const result = runEngine(alloc, workspace, parsed.value);
    writeResult(alloc, result) catch |err| {
        log.err("result_write_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    return 0;
}

/// Map the lease to engine args and run NullClaw in this child's address space.
/// stdout is the progress sink: the engine streams `activity` frames there
/// (`pipe_proto`) while running, then `writeResult` appends the terminal frame.
fn runEngine(alloc: std.mem.Allocator, workspace: []const u8, payload: LeasePayload) types.ExecutionResult {
    var args = buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ep: context_budget.ExecutionPolicy = payload.policy;
    return engine.execute(alloc, workspace, args.agent_config, args.tools_spec, args.message, null, &ep, std.posix.STDOUT_FILENO);
}

/// Write the terminal `result` frame to stdout. Activity frames (if any) were
/// already streamed ahead of it on the same fd; the parent reads frames in
/// order and treats the `result` frame as the execution outcome.
fn writeResult(alloc: std.mem.Allocator, result: types.ExecutionResult) !void {
    const json = try std.json.Stringify.valueAlloc(alloc, result, .{});
    defer alloc.free(json);
    try pipe_proto.writeFrame(std.posix.STDOUT_FILENO, .result, json);
}

// BUFFER GATE: ArrayList(u8) for the stdin accumulator — read-to-EOF, size
// unknown up front, need one contiguous slice to JSON-parse.
fn readStdin(alloc: std.mem.Allocator) ![]u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);
    var chunk: [READ_CHUNK]u8 = undefined;
    while (true) {
        const n = try std.posix.read(std.posix.STDIN_FILENO, &chunk);
        if (n == 0) break; // EOF — parent closed our stdin
        if (buf.items.len + n > MAX_LEASE_BYTES) return error.LeaseTooLarge;
        try buf.appendSlice(alloc, chunk[0..n]);
    }
    return buf.toOwnedSlice(alloc);
}

/// Value of the first `prefix…` argv entry, or null. argv is not secret.
fn flagValue(prefix: []const u8) ?[]const u8 {
    for (std.os.argv) |arg| {
        const a = std.mem.span(arg);
        if (std.mem.startsWith(u8, a, prefix)) return a[prefix.len..];
    }
    return null;
}

fn hasFlag(name: []const u8) bool {
    for (std.os.argv) |arg| {
        if (std.mem.eql(u8, std.mem.span(arg), name)) return true;
    }
    return false;
}

/// Engine-call args resolved from the lease. `deinit` releases the two JSON
/// containers (caller-owned allocator pattern).
const CallArgs = struct {
    agent_config: ?std.json.Value,
    tools_spec: ?std.json.Value,
    message: ?[]const u8,
    agent_obj: std.json.ObjectMap,
    tools_arr: std.json.Array,
    req_parsed: ?std.json.Parsed(std.json.Value),

    fn deinit(self: CallArgs, alloc: std.mem.Allocator) void {
        _ = alloc;
        var a = self.agent_obj;
        a.deinit();
        var t = self.tools_arr;
        t.deinit();
        if (self.req_parsed) |p| p.deinit();
    }
};

/// Build engine args from the leased policy + event. Agent-config keys reuse
/// the `wire` constants the engine reads them back with (RULE UFS).
fn buildCallArgs(alloc: std.mem.Allocator, payload: LeasePayload) CallArgs {
    var agent_obj = std.json.ObjectMap.init(alloc);
    if (payload.policy.context.model.len > 0)
        agent_obj.put(wire.model, .{ .string = payload.policy.context.model }) catch |err| log.warn("agent_model_arg_dropped", .{ .err = @errorName(err) });
    if (extractApiKey(payload.policy.secrets_map)) |key|
        agent_obj.put(wire.api_key, .{ .string = key }) catch |err| log.warn("agent_apikey_arg_dropped", .{ .err = @errorName(err) });

    var tools_arr = std.json.Array.init(alloc);
    for (payload.policy.tools) |name|
        tools_arr.append(.{ .string = name }) catch |err| log.warn("agent_tool_arg_dropped", .{ .err = @errorName(err) });

    const req_parsed: ?std.json.Parsed(std.json.Value) =
        std.json.parseFromSlice(std.json.Value, alloc, payload.event.request_json, .{}) catch null;

    const message: ?[]const u8 = blk: {
        const pv = if (req_parsed) |p| p.value else break :blk payload.event.request_json;
        if (pv != .object) break :blk payload.event.request_json;
        const mv = pv.object.get(MESSAGE_KEY) orelse break :blk payload.event.request_json;
        if (mv != .string) break :blk payload.event.request_json;
        break :blk mv.string;
    };

    return .{
        .agent_config = if (agent_obj.count() > 0) .{ .object = agent_obj } else null,
        .tools_spec = if (tools_arr.items.len > 0) .{ .array = tools_arr } else null,
        .message = message,
        .agent_obj = agent_obj,
        .tools_arr = tools_arr,
        .req_parsed = req_parsed,
    };
}

const MESSAGE_KEY = "message";
const SECRETS_LLM_KEY = "llm";

/// Extract `secrets_map["llm"]["api_key"]`; null when absent or wrong-shaped.
fn extractApiKey(secrets_map: ?std.json.Value) ?[]const u8 {
    const sm = secrets_map orelse return null;
    if (sm != .object) return null;
    const llm = sm.object.get(SECRETS_LLM_KEY) orelse return null;
    if (llm != .object) return null;
    const key = llm.object.get(wire.api_key) orelse return null;
    return if (key == .string) key.string else null;
}
