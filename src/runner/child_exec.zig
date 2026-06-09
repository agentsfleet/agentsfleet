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
const input = @import("child_exec_input.zig");
const pipe_proto = @import("pipe_proto.zig");

const log = logging.scoped(.runner_exec);
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
/// Operator-facing outcome when a lease carries no installed instructions — the
/// agent has no `SKILL.md` behaviour to run, so the model is not invoked.
const NO_INSTRUCTIONS_MESSAGE = "no installed instructions: the agent has no SKILL.md behaviour to run; awaiting configuration";

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
    // FAIL CLOSED: an installed agent with no behaviour prose must NOT run a
    // generic model turn. NullClaw is never invoked on a no-playbook run — we
    // report a clear "no installed instructions" outcome instead. Covers a
    // misconfigured frontmatter-only SKILL.md AND the mixed-version rollout
    // window where an older `zombied` omits the field; either way the agent runs
    // its playbook or nothing, never a generic chat.
    if (payload.instructions.len == 0) {
        log.warn("no_installed_instructions_fail_closed", .{ .zombie_id = payload.event.zombie_id });
        return noInstructionsResult();
    }

    // Build the reasoning context FIRST (cheap, fail-fast) so a context-build
    // failure fails closed before the heavier buildCallArgs / engine call. The
    // installed SKILL.md body becomes reasoning context; secrets never enter it
    // (they stay in `ep` / the tool bridge). Fail closed if it cannot be attached
    // — never run a generic turn because the delivery failed (fail-closed, not -open).
    var ctx_obj = input.buildInstructionsContext(alloc, payload.instructions) catch |err| {
        log.err("installed_instructions_ctx_failed_fail_closed", .{ .err = @errorName(err) });
        return noInstructionsResult();
    };
    defer ctx_obj.deinit(alloc);

    var args = input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ep: context_budget.ExecutionPolicy = payload.policy;

    return engine.execute(env_map, alloc, workspace, args.agent_config, args.tools_spec, args.message, .{ .object = ctx_obj }, &ep, std.posix.STDOUT_FILENO, hydrated_memory);
}

/// Fail-closed result for a lease whose installed instructions are absent (an
/// empty `SKILL.md` body, or an older `zombied` that omitted the field): the
/// model is never invoked, so the agent never runs as a generic chat. Reported
/// as a startup-posture failure carrying an operator-facing message.
fn noInstructionsResult() types.ExecutionResult {
    return .{ .content = NO_INSTRUCTIONS_MESSAGE, .exit_ok = false, .failure = .startup_posture };
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

// ── tests ────────────────────────────────────────────────────────────────────
const testing = std.testing;
const common = @import("common");
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
    var args = input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expectEqualStrings("fireworks", ac.get(wire.provider).?.string);
    try testing.expectEqualStrings("fw_secret_key", ac.get(wire.api_key).?.string);
}

test "runEngine fails closed with no model call when installed instructions are empty" {
    const alloc = testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{});
    defer env_map.deinit();
    // testLease defaults `instructions` to "" → the fail-closed guard returns
    // before buildCallArgs / engine.execute, so NullClaw is never invoked.
    const result = runEngine(&env_map, alloc, "/tmp/ws", testLease(.{ .context = .{ .model = "m" } }), &.{});
    try testing.expect(!result.exit_ok);
    try testing.expectEqual(types.FailureClass.startup_posture, result.failure.?);
    try testing.expect(std.mem.indexOf(u8, result.content, "no installed instructions") != null);
}

test "runEngine fails closed with no model call when the instructions context cannot be built" {
    const alloc = testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{});
    defer env_map.deinit();
    // fail_index 0 fails the FIRST allocation — `buildInstructionsContext`'s put,
    // which runs before buildCallArgs/engine — so runEngine takes the ctx-build
    // fail-closed branch and NEVER reaches `engine.execute` (no model call, no
    // network). Proves the fail-open OOM path the codex review flagged is closed.
    var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = 0 });
    var payload = testLease(.{ .context = .{ .model = "m" } });
    payload.instructions = "do platform ops"; // non-empty → past the empty guard
    const result = runEngine(&env_map, fa.allocator(), "/tmp/ws", payload, &.{});
    try testing.expect(!result.exit_ok);
    try testing.expectEqual(types.FailureClass.startup_posture, result.failure.?);
}

test "buildInstructionsContext attaches the installed instructions under the wire key" {
    const alloc = testing.allocator;
    var ctx = try input.buildInstructionsContext(alloc, "do platform ops");
    defer ctx.deinit(alloc);
    try testing.expectEqualStrings("do platform ops", ctx.get(wire.installed_instructions).?.string);
}

test "buildInstructionsContext leaks nothing on allocation failure (every alloc site)" {
    // checkAllAllocationFailures fails each allocation site in turn and asserts
    // the function returns error.OutOfMemory and frees everything (the errdefer
    // is correct) — the canonical Zig zero-leak proof for the error path.
    try testing.checkAllAllocationFailures(testing.allocator, struct {
        fn run(a: std.mem.Allocator) !void {
            var ctx = try input.buildInstructionsContext(a, "do platform ops");
            ctx.deinit(a);
        }
    }.run, .{});
}

test "buildCallArgs treats an llm-named tool secret as a tool secret, not the provider key" {
    const alloc = testing.allocator;
    // The retired heuristic used to pull the provider key from secrets_map["llm"].
    // A tool secret literally named `llm` must now be left alone.
    var sm = try std.json.parseFromSlice(std.json.Value, alloc, "{\"llm\":{\"api_key\":\"sk-should-not-leak\"}}", .{});
    defer sm.deinit();
    const payload = testLease(.{ .secrets_map = sm.value, .context = .{ .model = "claude-x" } });
    var args = input.buildCallArgs(alloc, payload);
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
    var args = input.buildCallArgs(alloc, payload);
    defer args.deinit(alloc);
    const ac = args.agent_config.?.object;
    try testing.expect(ac.get(wire.api_key) == null);
    try testing.expect(ac.get(wire.provider) == null);
}
