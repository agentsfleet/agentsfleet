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
const builtin = @import("builtin");
const build_options = @import("build_options");
const logging = @import("log");
const contract = @import("contract");

const engine = @import("engine/runner.zig");
const types = @import("engine/types.zig");
const landlock = @import("engine/landlock.zig");
const context_budget = @import("engine/context_budget.zig");
const wire = @import("engine/wire.zig");
const client_errors = @import("engine/client_errors.zig");
const pipe_proto = @import("pipe_proto.zig");

const log = logging.scoped(.runner_exec);
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;
const LeasePayload = contract.protocol.LeasePayload;
const RunnerChildInput = contract.protocol.RunnerChildInput;
const MemoryDelta = contract.protocol.MemoryDelta;

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
pub fn run(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, alloc: std.mem.Allocator) u8 {
    const workspace = flagValue(argv, WORKSPACE_FLAG_PREFIX) orelse {
        log.err("no_workspace_flag", .{});
        return SANDBOX_FAIL_EXIT;
    };

    // FAIL-CLOSED (Invariant 7): on a sandboxed tier the mandatory in-child
    // hardening MUST apply before we read the lease or run the agent — a sandbox
    // we cannot establish aborts, never running tool execution unsandboxed.
    // no_new_privs is set BEFORE Landlock: it defangs setuid binaries in the RO
    // mounts and is the precondition landlock_restrict_self will rely on once a
    // later cap-drop removes the userns CAP_SYS_ADMIN it rides today.
    if (hasFlag(argv, SANDBOXED_FLAG)) {
        applyNoNewPrivs() catch |err| {
            log.err("no_new_privs_failed_fail_closed", .{ .err = @errorName(err) });
            return SANDBOX_FAIL_EXIT;
        };
        landlock.applyPolicy(workspace) catch |err| {
            log.err("landlock_failed_fail_closed", .{ .err = @errorName(err) });
            return SANDBOX_FAIL_EXIT;
        };
    }

    const input_json = readStdin(alloc) catch |err| {
        log.err("lease_read_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    defer alloc.free(input_json);

    // The parent pipes a RunnerChildInput { lease, hydrated_memory } — the lease
    // to run plus the prior memory it hydrated over the trusted plane. The child
    // never fetches memory itself, so it holds no token, URL, or DSN (RULE VLT).
    const parsed = std.json.parseFromSlice(RunnerChildInput, alloc, input_json, .{
        .ignore_unknown_fields = true,
    }) catch |err| {
        log.err("lease_parse_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    defer parsed.deinit();

    // The process exits immediately after writing; the engine result's content
    // is reclaimed by exit (no free needed on this single-shot path). A
    // `executor_provider_stub` build (tests only) skips the engine and emits a
    // canned success result so the worker-pool integration lane can exercise the
    // real fork→execute→report path with no LLM; comptime-false in production.
    const result = if (build_options.executor_provider_stub)
        stubResult()
    else
        runEngine(env_map, alloc, workspace, parsed.value.lease, parsed.value.hydrated_memory);
    writeResult(alloc, result) catch |err| {
        log.err("result_write_failed", .{ .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    return 0;
}

/// Canned success result for an `executor_provider_stub` build — a clean exit
/// with a sentinel body, no engine, no LLM, no network. Lets a test prove the
/// pool forks N children, each runs, and each reports, without a model call.
fn stubResult() types.ExecutionResult {
    return .{ .exit_ok = true, .content = "stub-ok", .token_count = 0 };
}

/// Map the lease to engine args and run NullClaw in this child's address space.
/// stdout is the progress sink: the engine streams `activity` frames there
/// (`pipe_proto`) while running, then `writeResult` appends the terminal frame.
fn runEngine(env_map: *const std.process.Environ.Map, alloc: std.mem.Allocator, workspace: []const u8, payload: LeasePayload, hydrated_memory: []const MemoryDelta) types.ExecutionResult {
    var args = buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ep: context_budget.ExecutionPolicy = payload.policy;
    return engine.execute(env_map, alloc, workspace, args.agent_config, args.tools_spec, args.message, null, &ep, std.posix.STDOUT_FILENO, hydrated_memory);
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
    var buf: std.ArrayList(u8) = .empty;
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
fn flagValue(argv: []const [:0]const u8, prefix: []const u8) ?[]const u8 {
    for (argv) |arg| {
        if (std.mem.startsWith(u8, arg, prefix)) return arg[prefix.len..];
    }
    return null;
}

fn hasFlag(argv: []const [:0]const u8, name: []const u8) bool {
    for (argv) |arg| {
        if (std.mem.eql(u8, arg, name)) return true;
    }
    return false;
}

/// Set `PR_SET_NO_NEW_PRIVS`: after this, no exec in this child or any
/// descendant can gain privilege via a setuid/setgid binary — the setuid helpers
/// in the RO-bound /usr,/bin,/sbin become inert. It is additive (does NOT remove
/// the userns CAP_SYS_ADMIN that Landlock rides today, so Landlock keeps working)
/// and must run BEFORE landlock_restrict_self. Linux-only: the `--sandboxed` flag
/// is set only on Linux tiers (the parent's establishSandbox fail-closes any
/// other host), so the non-Linux branch is just compile-portability.
fn applyNoNewPrivs() error{NoNewPrivsFailed}!void {
    if (builtin.os.tag != .linux) return;
    // prctl(PR_SET_NO_NEW_PRIVS=38, 1=set, 0,0,0) → 0 on success.
    if (std.os.linux.prctl(@intFromEnum(std.os.linux.PR.SET_NO_NEW_PRIVS), 1, 0, 0, 0) != 0)
        return error.NoNewPrivsFailed;
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
        var a = self.agent_obj;
        a.deinit(alloc);
        var t = self.tools_arr;
        t.deinit();
        if (self.req_parsed) |p| p.deinit();
    }
};

/// Build engine args from the leased policy + event. Agent-config keys reuse
/// the `wire` constants the engine reads them back with (RULE UFS).
fn buildCallArgs(alloc: std.mem.Allocator, payload: LeasePayload) CallArgs {
    var agent_obj: std.json.ObjectMap = .empty;
    if (payload.policy.context.model.len > 0)
        agent_obj.put(alloc, wire.model, .{ .string = payload.policy.context.model }) catch |err| log.warn("agent_model_arg_dropped", .{ .err = @errorName(err) });
    // Provider + key are the authoritative resolved values delivered on the
    // lease (the key the tenant is billed for) — atomic: the resolver always
    // produces both or neither. A half-populated pair is a malformed lease; we
    // inject nothing so the engine fails to authenticate cleanly rather than
    // running against the wrong provider. `secrets_map` carries tool credentials
    // only — a tool secret named "llm" is NOT the provider key.
    if (payload.policy.provider.len > 0 and payload.policy.api_key.len > 0) {
        agent_obj.put(alloc, wire.provider, .{ .string = payload.policy.provider }) catch |err| log.warn("agent_provider_arg_dropped", .{ .err = @errorName(err) });
        agent_obj.put(alloc, wire.api_key, .{ .string = payload.policy.api_key }) catch |err| log.warn("agent_apikey_arg_dropped", .{ .err = @errorName(err) });
    } else if (payload.policy.provider.len > 0 or payload.policy.api_key.len > 0) {
        log.warn("agent_provider_key_incomplete", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .has_provider = payload.policy.provider.len > 0 });
    }

    var tools_arr = std.json.Array.init(alloc);
    for (payload.policy.tools) |name|
        tools_arr.append(.{ .string = name }) catch |err| log.warn("agent_tool_arg_dropped", .{ .err = @errorName(err) });

    const req_parsed: ?std.json.Parsed(std.json.Value) =
        std.json.parseFromSlice(std.json.Value, alloc, payload.event.request_json, .{}) catch null;

    const message: ?[]const u8 = blk: {
        const pv = if (req_parsed) |p| p.value else break :blk payload.event.request_json;
        if (pv != .object) break :blk payload.event.request_json;
        const mv = pv.object.get(wire.message) orelse break :blk payload.event.request_json;
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

// ── tests ────────────────────────────────────────────────────────────────────
const testing = std.testing;
const ExecutionPolicy = contract.execution_policy.ExecutionPolicy;

/// A minimal lease whose only inputs `buildCallArgs` reads are `policy` and
/// `event.request_json`.
fn testLease(policy: ExecutionPolicy) LeasePayload {
    return .{
        .lease_id = "l1",
        .fencing_token = 1,
        .lease_expires_at = 0,
        .secret_delivery = .@"inline",
        .event = .{
            .event_id = "1700000000000-0",
            .zombie_id = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee",
            .workspace_id = "0190cccc-dddd-7eee-8fff-aaaaaaaaaaaa",
            .actor = "steer:test",
            .event_type = .chat,
            .request_json = "{\"message\":\"hi\"}",
            .created_at = 1700000000000,
        },
        .policy = policy,
    };
}

test "buildCallArgs injects the policy provider and api_key into agent_config" {
    const alloc = testing.allocator;
    const payload = testLease(.{ .provider = "fireworks", .api_key = "fw_secret_key" });
    var args = buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("fireworks", ac.get(wire.provider).?.string);
    try testing.expectEqualStrings("fw_secret_key", ac.get(wire.api_key).?.string);
}

test "buildCallArgs treats an llm-named tool secret as a tool secret, not the provider key" {
    const alloc = testing.allocator;
    // The retired heuristic used to pull the provider key from secrets_map["llm"].
    // A tool secret literally named `llm` must now be left alone.
    var sm = try std.json.parseFromSlice(std.json.Value, alloc, "{\"llm\":{\"api_key\":\"sk-should-not-leak\"}}", .{});
    defer sm.deinit();
    const payload = testLease(.{ .secrets_map = sm.value, .context = .{ .model = "claude-x" } });
    var args = buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("claude-x", ac.get(wire.model).?.string); // agent_config is populated…
    try testing.expect(ac.get(wire.api_key) == null); // …but the llm tool secret is NOT promoted to the provider key
    try testing.expect(ac.get(wire.provider) == null);
}

test "buildCallArgs injects neither half of an incomplete provider key pair" {
    const alloc = testing.allocator;
    // api_key present, provider empty — a malformed lease. Inject nothing so the
    // engine fails to authenticate cleanly rather than running the wrong provider.
    const payload = testLease(.{ .api_key = "fw_orphan_key", .context = .{ .model = "claude-x" } });
    var args = buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expect(ac.get(wire.api_key) == null);
    try testing.expect(ac.get(wire.provider) == null);
}
