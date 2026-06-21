//! child_process.zig — raw fork/exec/kill plumbing for a leased child.
//!
//! The mechanical process layer under `child_supervisor`: it forks, wires the
//! child's stdio onto pipes, execs the sandbox wrapper, and kills the whole
//! child tree. It owns no lifecycle policy (deadlines, renewal, reaping order,
//! result classification) — that stays in the supervisor. The split keeps each
//! file focused and within the line budget.
//!
//! The lease's inline secrets ride the child's stdin pipe — never argv or env
//! (both readable in /proc) — per RULE VLT.

const std = @import("std");
const logging = @import("log");

const cgroup = @import("engine/CgroupScope.zig");
const client_errors = @import("engine/client_errors.zig");
const sandbox = @import("sandbox_args.zig");
const Config = @import("daemon/config.zig");

const log = logging.scoped(.runner_supervisor);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;
const ERR_RUN_SANDBOX_ESTABLISH_FAILED = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED;

/// Spawn the sandboxed child. Zig 0.16 removed raw fork/pipe/dup2/close/waitpid,
/// so std.process.spawn does pipe → fork → dup2 → setpgid(0,0) → execvpe, and the
/// returned `process.Child` is the only portable reap/close path (child_supervisor
/// drives wait()/close). `pgid = 0` keeps the child its own process-group leader
/// (killChild's `kill(-pgid)` fallback). Containment stays cgroup-centric — the
/// wrapper's kill() is never called: it touches only the single pid, letting a
/// hostile child's descendants outlive the lease; scope.kill reaps the whole tree
/// atomically.
///
/// The child inherits ONLY the allowlisted environment (filtered from `daemon_env`
/// via `environ_map`), so `AGENTSFLEET_RUNNER_TOKEN` and every other daemon-only var
/// never reach a prompt-injectable fleet. `argv[0]` is asserted absolute before
/// spawn so a relative path can never be resolved via the parent `$PATH`.
pub fn forkExec(io: std.Io, alloc: std.mem.Allocator, cfg: Config, daemon_env: *const std.process.Environ.Map, workspace_path: []const u8, egress: ?sandbox.EgressFiles) !std.process.Child {
    const argv = try sandbox.buildArgv(io, alloc, cfg, workspace_path, egress);
    defer sandbox.freeArgv(alloc, argv);

    try requireAbsoluteArgv0(argv);

    // Build the child's environ from the allowlist only; freed after spawn
    // bakes it into the child's envp (the returned Child does not retain it).
    var child_env = try buildChildEnviron(alloc, daemon_env);
    defer child_env.deinit();

    return std.process.spawn(io, .{
        .argv = argv,
        .stdin = .pipe,
        .stdout = .pipe,
        .stderr = .inherit,
        .pgid = 0,
        .environ_map = &child_env,
    }) catch |err| {
        log.err("child_spawn_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .err = @errorName(err) });
        return err;
    };
}

/// FAIL-CLOSED argv[0] guard: `std.process.spawn` resolves a
/// relative argv[0] against the PARENT `$PATH`. `buildArgv` already emits absolute
/// paths (bwrap path / `executablePathAlloc`); a relative one here means a future
/// `buildArgv` change re-introduced a `$PATH` trust dependency — refuse it before
/// spawn rather than resolve an attacker-influenced name.
fn requireAbsoluteArgv0(argv: []const []const u8) error{SandboxArgvNotAbsolute}!void {
    if (argv.len == 0 or !std.fs.path.isAbsolute(argv[0]))
        return error.SandboxArgvNotAbsolute;
}

/// Build the sandboxed child's environment: ONLY the allowlisted daemon vars
/// that are actually set (`sandbox.ENV_PASSTHROUGH_ALLOWLIST`, RULE UFS).
/// Fail-closed — anything off the allowlist (incl. the `AGENTSFLEET_` control-plane
/// credentials) is simply never copied in. Caller owns the map and must `deinit`
/// it after the spawn consumes it. Pub so the integration lane can spawn a real
/// child with the filtered map and prove the daemon environ never crosses.
pub fn buildChildEnviron(alloc: std.mem.Allocator, daemon_env: *const std.process.Environ.Map) !std.process.Environ.Map {
    var env: std.process.Environ.Map = .init(alloc);
    errdefer env.deinit();
    inline for (sandbox.ENV_PASSTHROUGH_ALLOWLIST) |name| {
        // Defense-in-depth: the deny-prefix must never be allowlisted, so a
        // daemon secret can never reach the child no matter how the allowlist is
        // edited. Comptime — validated at every build, release included, at zero
        // runtime cost; an offending allowlist edit fails compilation, not Debug.
        comptime std.debug.assert(!std.mem.startsWith(u8, name, sandbox.ENV_DENY_PREFIX));
        if (daemon_env.get(name)) |value| try env.put(name, value);
    }
    return env;
}

/// Kill the whole child tree. On Linux the cgroup is the primary atomic kill
/// domain — signal it first when present. Then ALWAYS also signal the child's
/// process group (it leads its own via setpgid), regardless of the cgroup-kill
/// result: a child whose cgroup enrollment failed runs in an EMPTY
/// exec-cgroup, where `cgroup.kill` succeeds but reaps nothing, so the pgroup
/// signal is the only thing that actually kills it; it also covers the
/// enroll-succeeds-then-races path. Safe against pid reuse because this always
/// runs BEFORE the supervisor's single `wait()` — the target is still an
/// un-reaped (fleet-at-most) pid. `ESRCH` (group already gone via the cgroup
/// kill) is harmless.
pub fn killChild(pid: std.posix.pid_t, scope: *?cgroup) void {
    if (scope.*) |*s| s.kill() catch |err|
        log.warn("cgroup_kill_failed_fallback_signal", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .err = @errorName(err) });
    std.posix.kill(-pid, std.posix.SIG.KILL) catch |err|
        log.warn("child_group_kill_failed", .{ .err = @errorName(err) });
}

// ── tests ────────────────────────────────────────────────────────────────────
const testing = std.testing;

test "forkExec env filter forwards only the allowlist and drops daemon vars" {
    const alloc = testing.allocator;
    var daemon: std.process.Environ.Map = .init(alloc);
    defer daemon.deinit();
    // Allowlisted (forwarded) …
    try daemon.put("HOME", "/home/runner");
    try daemon.put("PATH", "/usr/bin:/bin");
    // … and daemon-only vars that must NEVER reach the child.
    try daemon.put("AGENTSFLEET_RUNNER_TOKEN", "agt_rsuper_secret");
    try daemon.put("RUNNER_HOST_ID", "host-1");
    try daemon.put("RUNNER_NETWORK_POLICY", "registry_allowlist");

    var child = try buildChildEnviron(alloc, &daemon);
    defer child.deinit();

    try testing.expectEqualStrings("/home/runner", child.get("HOME").?);
    try testing.expectEqualStrings("/usr/bin:/bin", child.get("PATH").?);
    try testing.expect(child.get("RUNNER_HOST_ID") == null);
    try testing.expect(child.get("RUNNER_NETWORK_POLICY") == null);
    // An allowlisted-but-unset var is simply absent (pass-through-if-set).
    try testing.expect(child.get("SSL_CERT_FILE") == null);
}

test "forkExec env filter omits every AGENTSFLEET_ daemon secret from the child environ" {
    const alloc = testing.allocator;
    var daemon: std.process.Environ.Map = .init(alloc);
    defer daemon.deinit();
    try daemon.put("HOME", "/home/runner");
    try daemon.put("AGENTSFLEET_RUNNER_TOKEN", "agt_rsuper_secret");
    try daemon.put("AGENTSFLEET_API_URL", "https://api.agentsfleet.net");

    var child = try buildChildEnviron(alloc, &daemon);
    defer child.deinit();

    // The deny-prefix is provably absent regardless of allowlist contents.
    var it = child.array_hash_map.iterator();
    while (it.next()) |entry| {
        try testing.expect(!std.mem.startsWith(u8, entry.key_ptr.*, sandbox.ENV_DENY_PREFIX));
    }
    try testing.expect(child.get("AGENTSFLEET_RUNNER_TOKEN") == null);
    try testing.expect(child.get("AGENTSFLEET_API_URL") == null);
}

test "requireAbsoluteArgv0 rejects a relative argv[0] and accepts an absolute one" {
    try testing.expectError(error.SandboxArgvNotAbsolute, requireAbsoluteArgv0(&.{ "bwrap", "--unshare-all" }));
    try testing.expectError(error.SandboxArgvNotAbsolute, requireAbsoluteArgv0(&.{}));
    try requireAbsoluteArgv0(&.{ "/usr/bin/bwrap", "--unshare-all" });
}
