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
const common = @import("common");
const clock = common.clock;
const contract = @import("contract");

const engine = @import("engine/runner.zig");
const credential_request = @import("engine/credential_request.zig");
const types = @import("engine/types.zig");
const landlock = @import("engine/landlock.zig");
const seccomp = @import("engine/seccomp.zig");
const context_budget = @import("engine/context_budget.zig");
const client_errors = @import("engine/client_errors.zig");
const input = @import("child_exec_input.zig");
const pipe_proto = @import("pipe_proto.zig");

const log = logging.scoped(.runner_exec);
const LeasePayload = contract.protocol.LeasePayload;
const RunnerChildInput = contract.protocol.RunnerChildInput;
const MemoryDelta = contract.protocol.MemoryDelta;
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;
const ERR_EXEC_RUNNER_INVALID_CONFIG = client_errors.ERR_EXEC_RUNNER_INVALID_CONFIG;
const ERR_RUN_SANDBOX_ESTABLISH_FAILED = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED;

/// argv subcommand selecting child-execute mode (vs the daemon loop).
pub const SUBCOMMAND = "__execute";
/// argv flag carrying the per-lease workspace path (not a secret).
pub const WORKSPACE_FLAG_PREFIX = "--workspace=";
/// argv flag the parent sets when the tier requires in-child sandboxing.
pub const SANDBOXED_FLAG = "--sandboxed";

/// Distinct exit code for a fail-closed sandbox-setup failure — lets the parent's
/// `classify` map it to a startup-posture failure, not a crash. Canonical in
/// `pipe_proto` (the child↔parent protocol vocabulary); referenced here so the
/// child and the supervisor agree on the value (RULE UFS).
const SANDBOX_FAIL_EXIT: u8 = pipe_proto.SANDBOX_FAIL_EXIT;
const GENERIC_FAIL_EXIT: u8 = pipe_proto.GENERIC_FAIL_EXIT;
const MAX_LEASE_BYTES: usize = 4 * 1024 * 1024;
/// Startup bound on the parent's `lease` frame write. The parent frames the lease
/// immediately post-fork, so this is a wedged-parent guard, not a normal wait.
const LEASE_READ_TIMEOUT_MS: i64 = 60_000;
/// Operator-facing outcome when a lease carries no installed instructions — the
/// fleet has no `SKILL.md` behaviour to run, so the model is not invoked.
const NO_INSTRUCTIONS_MESSAGE = "no installed instructions: the fleet has no SKILL.md behaviour to run; awaiting configuration";
/// Operator-facing outcome when the engine config could not be assembled from the
/// lease (allocation failure mid-build) — the model is not invoked with a partial
/// config. Distinct from the no-instructions outcome for triage.
const CONFIG_BUILD_FAILED_MESSAGE = "engine configuration could not be assembled (resource exhaustion): the run was stopped before the model was invoked";

/// Child entry. Returns the process exit code (main calls `std.process.exit`
/// with it); never returns an error — every failure maps to an exit code.
pub fn run(argv: []const [:0]const u8, env_map: *const std.process.Environ.Map, alloc: std.mem.Allocator) u8 {
    const workspace = flagValue(argv, WORKSPACE_FLAG_PREFIX) orelse {
        log.err("no_workspace_flag", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG });
        return SANDBOX_FAIL_EXIT;
    };

    // FAIL-CLOSED (Invariant 7): on a sandboxed tier the mandatory in-child
    // hardening MUST apply before we read the lease or run the fleet — a sandbox
    // we cannot establish aborts, never running tool execution unsandboxed.
    // no_new_privs is set BEFORE Landlock: it defangs setuid binaries in the RO
    // mounts and is the precondition landlock_restrict_self will rely on once a
    // later cap-drop removes the userns CAP_SYS_ADMIN it rides today.
    if (hasFlag(argv, SANDBOXED_FLAG)) {
        applyNoNewPrivs() catch |err| {
            log.err("no_new_privs_failed_fail_closed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .err = @errorName(err) });
            return SANDBOX_FAIL_EXIT;
        };
        landlock.applyPolicy(workspace) catch |err| {
            log.err("landlock_failed_fail_closed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .err = @errorName(err) });
            return SANDBOX_FAIL_EXIT;
        };
        // Syscall wall atop Landlock — establish failure fails closed; a runtime
        // trap exits SECCOMP_VIOLATION_EXIT -> landlock_deny (see seccomp.zig).
        seccomp.applyFilter() catch |err| {
            log.err("seccomp_failed_fail_closed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .err = @errorName(err) });
            return SANDBOX_FAIL_EXIT;
        };
    }

    // Read exactly one `lease` frame off stdin (no longer read-to-EOF): stdin
    // stays OPEN afterward as the parent→child credential-response channel for the
    // on-demand mint round-trip (M102 §3). The lease still rides stdin only, never
    // argv/env (RULE VLT).
    const input_json = readLeaseFrame(alloc) catch |err| {
        log.err("lease_read_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
        return GENERIC_FAIL_EXIT;
    };
    defer alloc.free(input_json);

    // The parent pipes a RunnerChildInput { lease, hydrated_memory } — the lease
    // to run plus the prior memory it hydrated over the trusted plane. The child
    // never fetches memory itself, so it holds no token, URL, or DSN (RULE VLT).
    const parsed = std.json.parseFromSlice(RunnerChildInput, alloc, input_json, .{
        .ignore_unknown_fields = true,
    }) catch |err| {
        log.err("lease_parse_failed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .err = @errorName(err) });
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
        log.err("result_write_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
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
    // FAIL CLOSED: an installed fleet with no behaviour prose must NOT run a
    // generic model turn. NullClaw is never invoked on a no-playbook run — we
    // report a clear "no installed instructions" outcome instead. Covers a
    // misconfigured frontmatter-only SKILL.md AND the mixed-version rollout
    // window where an older `agentsfleetd` omits the field; either way the fleet runs
    // its playbook or nothing, never a generic chat.
    if (payload.instructions.len == 0) {
        log.warn("no_installed_instructions_fail_closed", .{ .error_code = ERR_EXEC_RUNNER_INVALID_CONFIG, .fleet_id = payload.event.fleet_id });
        return noInstructionsResult();
    }

    // Build the reasoning context FIRST (cheap, fail-fast) so a context-build
    // failure fails closed before the heavier buildCallArgs / engine call. The
    // installed SKILL.md body becomes reasoning context; secrets never enter it
    // (they stay in `ep` / the tool bridge). The only error here is OOM, and the
    // instructions ARE configured (len > 0, checked above) — this is resource
    // exhaustion, NOT a missing playbook, so report configBuildFailed rather than
    // sending the operator to chase a missing SKILL.md. Fail closed either way.
    var ctx_obj = input.buildInstructionsContext(alloc, payload.instructions) catch |err| {
        log.err("installed_instructions_ctx_failed_fail_closed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
        return configBuildFailedResult();
    };
    defer ctx_obj.deinit(alloc);

    // FAIL CLOSED: if the engine config cannot be assembled (allocation failure
    // mid-build), do not invoke the model with a partial config — report a
    // startup-posture failure. buildCallArgs is atomic, so a half-built
    // fleet_config (e.g. provider-without-key) never reaches the engine.
    var args = input.buildCallArgs(alloc, payload) catch |err| {
        log.err("call_args_build_failed_fail_closed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err), .fleet_id = payload.event.fleet_id });
        return configBuildFailedResult();
    };
    defer args.deinit(alloc);
    const ep: context_budget.ExecutionPolicy = payload.policy;

    // The on-demand mint channel (M102 §4) rides the EXISTING child pipes: the
    // child writes `credential_request` frames up its stdout (multiplexed with
    // activity frames) and reads the `credential_response` back down its stdin; the
    // round-trip is bounded by the lease deadline so a wedged parent can never
    // block the child past `lease_expires_at`. No extra fd, no new sandbox hole.
    const cred_channel = credential_request.Channel{
        .request_fd = std.posix.STDOUT_FILENO,
        .response_fd = std.posix.STDIN_FILENO,
        .deadline_ms = payload.lease_expires_at,
    };

    // `.{ .object = ctx_obj }` BORROWS ctx_obj's backing map (the struct copy
    // shares the same hash-map arrays). engine.execute is synchronous, so it must
    // fully consume `context` before this frame unwinds and `defer ctx_obj.deinit`
    // frees those arrays. A future async or context-retaining engine would have to
    // own or dupe the context instead of borrowing it here.
    return engine.execute(env_map, alloc, workspace, args.fleet_config, args.tools_spec, args.message, .{ .object = ctx_obj }, &ep, std.posix.STDOUT_FILENO, hydrated_memory, cred_channel);
}

/// Fail-closed result for a lease whose installed instructions are absent (an
/// empty `SKILL.md` body, or an older `agentsfleetd` that omitted the field): the
/// model is never invoked, so the fleet never runs as a generic chat. Reported
/// as a startup-posture failure carrying an operator-facing message.
fn noInstructionsResult() types.ExecutionResult {
    return .{ .content = NO_INSTRUCTIONS_MESSAGE, .exit_ok = false, .failure = .startup_posture };
}

/// Fail-closed result when the engine config could not be assembled from the
/// lease (allocation failure mid-build): the model is never invoked with a
/// partial config. A distinct operator message from `noInstructionsResult` so
/// triage can tell "no SKILL.md to run" from "couldn't build the config".
fn configBuildFailedResult() types.ExecutionResult {
    return .{ .content = CONFIG_BUILD_FAILED_MESSAGE, .exit_ok = false, .failure = .startup_posture };
}

/// Write the terminal `result` frame to stdout. Activity frames (if any) were
/// already streamed ahead of it on the same fd; the parent reads frames in
/// order and treats the `result` frame as the execution outcome.
fn writeResult(alloc: std.mem.Allocator, result: types.ExecutionResult) !void {
    const json = try std.json.Stringify.valueAlloc(alloc, result, .{});
    defer alloc.free(json);
    try pipe_proto.writeFrame(std.posix.STDOUT_FILENO, .result, json);
}

/// Read the single `lease` frame the parent feeds at startup (`pipe_proto`),
/// returning its payload (caller-owned). stdin is left OPEN — the parent reuses it
/// to frame `credential_response`s during the run. Fails closed on EOF (parent
/// died before framing), a non-`lease` frame (wire skew), or the startup deadline.
fn readLeaseFrame(alloc: std.mem.Allocator) ![]u8 {
    const deadline = clock.nowMillis() + LEASE_READ_TIMEOUT_MS;
    switch (try pipe_proto.readFrame(alloc, std.posix.STDIN_FILENO, deadline, MAX_LEASE_BYTES)) {
        .frame => |f| {
            if (f.ftype == .lease) return f.payload;
            alloc.free(f.payload);
            return error.UnexpectedLeaseFrame;
        },
        .eof => return error.NoLeaseFrame,
        .timed_out => return error.LeaseReadTimeout,
    }
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
const testLease = @import("child_exec_test_fixtures.zig").testLease;

// The `buildCallArgs` / `buildInstructionsContext` pub-surface tests live in
// `child_exec_input_test.zig` (kept out of this file for the RULE FLL limit). The
// tests below stay here because they exercise the private `runEngine`.

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
    // The instructions ARE configured (len > 0) — a ctx-build OOM is resource
    // exhaustion, so the operator message must be configBuildFailed, NOT the
    // missing-playbook "no installed instructions" (which would misdirect triage).
    try testing.expect(std.mem.indexOf(u8, result.content, "engine configuration could not be assembled") != null);
    try testing.expect(std.mem.indexOf(u8, result.content, "no installed instructions") == null);
}

test "runEngine fails closed with no model call when the engine config cannot be assembled" {
    const alloc = testing.allocator;
    var env_map = try common.env.fromPairs(alloc, &.{});
    defer env_map.deinit();

    // Derive — don't guess — the allocation index where buildCallArgs begins.
    // runEngine builds the instructions context FIRST, so the count of allocations
    // that build performs is exactly the first index buildCallArgs touches. The
    // FailingAllocator stays failed from `fail_index` onward, so failing there
    // guarantees the ctx build succeeds and buildCallArgs fails — self-adjusting
    // if std's allocation shape changes, no magic number.
    const ctx_alloc_count = blk: {
        var probe = std.testing.FailingAllocator.init(alloc, .{ .fail_index = std.math.maxInt(usize) });
        var ctx = try input.buildInstructionsContext(probe.allocator(), "do platform ops");
        ctx.deinit(probe.allocator());
        break :blk probe.alloc_index;
    };

    var payload = testLease(.{
        .provider = "fireworks",
        .api_key = "fw_secret_key",
        .tools = &.{ "bash", "read", "write" },
        .context = .{ .model = "claude-x" },
    });
    payload.instructions = "do platform ops";
    var fa = std.testing.FailingAllocator.init(alloc, .{ .fail_index = ctx_alloc_count });
    const result = runEngine(&env_map, fa.allocator(), "/tmp/ws", payload, &.{});
    try testing.expect(!result.exit_ok);
    try testing.expectEqual(types.FailureClass.startup_posture, result.failure.?);
    // Specifically the config-build branch — not the no-instructions / ctx-build
    // branch (proves the ctx build succeeded and the failure came from assembly).
    try testing.expect(std.mem.indexOf(u8, result.content, "engine configuration could not be assembled") != null);
    try testing.expect(std.mem.indexOf(u8, result.content, "no installed instructions") == null);
}
