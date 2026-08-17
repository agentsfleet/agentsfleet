//! selftest_exec.zig — run one self-test probe and grade it.
//!
//! The parent half of `selftest_probe`. It builds the probe argv through the
//! SAME `buildSandboxPrefix` a lease uses, spawns it, bounds it, reaps it, and
//! turns the one line the child printed into an operator-facing verdict.
//!
//! Every failure here is a RESULT, never an error return: a host with no
//! bubblewrap, a sandbox that would not start, a probe that hung — each is a
//! named failed check the operator can read. Returning an error instead would
//! leave the runner page with an empty self-test and the operator back where
//! this milestone started, reading a journal over Secure Shell (SSH).
//!
//! The child's stdout is parsed into BOOLEANS and discarded. No byte the child
//! printed reaches the stored result, which is what makes Invariant 7 ("results
//! carry no secrets") a property of the shape rather than of review discipline.

const std = @import("std");
const logging = @import("log");

const Config = @import("daemon/config.zig");
const sandbox_args = @import("sandbox_args.zig");
const selftest = @import("selftest.zig");
const selftest_probe = @import("selftest_probe.zig");

const log = logging.scoped(.runner_selftest);

/// Log events, named once because both the reaper thread and the spawn-failure
/// path emit them (RULE UFS).
const EVENT_KILL_FAILED = "selftest_probe_kill_failed";
const EVENT_WAIT_FAILED = "selftest_probe_wait_failed";

/// The child prints one short fixed-shape line; anything longer is a wire skew,
/// not a verdict. Bounded so a wedged or hostile child cannot stream into the
/// daemon's memory while we wait for it.
const VERDICT_READ_CAP = 128;

/// How often the reaper wakes to check whether the probe already finished. The
/// probe is normally sub-second, so this keeps the thread joinable promptly
/// instead of parking it for the whole bound.
const REAP_POLL_MS = 100;

/// Watches one probe and kills its process group if it outlives the bound.
///
/// A timer thread rather than a poll loop around `wait()`: `wait()` blocks the
/// calling thread, so a timeout check after it never runs (write_zig, §Memory
/// Safety). `done` is what makes the kill safe against pid reuse — the parent
/// sets it before reaping, so the reaper can never signal a recycled pid.
const Reaper = struct {
    io: std.Io,
    pgid: std.posix.pid_t,
    done: std.atomic.Value(bool) = .init(false),
    fired: std.atomic.Value(bool) = .init(false),

    fn watch(self: *Reaper) void {
        var waited: u64 = 0;
        while (waited < selftest.PROBE_TIMEOUT_MS) {
            if (self.done.load(.seq_cst)) return;
            self.io.sleep(std.Io.Duration.fromMilliseconds(REAP_POLL_MS), .awake) catch return;
            waited += REAP_POLL_MS;
        }
        if (self.done.load(.seq_cst)) return;
        self.fired.store(true, .seq_cst);
        // Negative pid = the whole process group. The probe leads its own via
        // `pgid = 0` at spawn, so this reaps bwrap AND whatever it started —
        // killing only the pid would leave the sandbox's children orphaned.
        std.posix.kill(-self.pgid, std.posix.SIG.KILL) catch |err|
            log.warn(EVENT_KILL_FAILED, .{ .err = @errorName(err) });
    }
};

/// Run one probe under `cfg` and return its graded verdict. Caller owns the
/// result and frees it with `Result.deinit`.
pub fn run(io: std.Io, alloc: std.mem.Allocator, cfg: Config, workspace_path: []const u8) !selftest.Result {
    const argv = selftest.buildProbeArgv(io, alloc, cfg, workspace_path) catch |err| {
        // No bubblewrap means no sandboxed tier can be established at all —
        // the operator needs to read that, not an empty panel.
        if (err == error.BwrapUnavailable)
            return selftest.unavailable(alloc, cfg, selftest.DETAIL_NO_BWRAP);
        return err;
    };
    defer sandbox_args.freeArgv(alloc, argv);

    var child = std.process.spawn(io, .{
        .argv = argv,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
        .pgid = 0,
    }) catch |err| {
        log.warn("selftest_probe_spawn_failed", .{ .err = @errorName(err) });
        return selftest.unavailable(alloc, cfg, selftest.DETAIL_SPAWN_FAILED);
    };

    var reaper: Reaper = .{ .io = io, .pgid = child.id.? };
    const watcher = std.Thread.spawn(.{}, Reaper.watch, .{&reaper}) catch |err| {
        // No watchdog means no bound. Kill now rather than run a probe that
        // could hang the heartbeat it rides on.
        std.posix.kill(-child.id.?, std.posix.SIG.KILL) catch |kill_err|
            log.warn(EVENT_KILL_FAILED, .{ .err = @errorName(kill_err) });
        _ = child.wait(io) catch |wait_err|
            log.warn(EVENT_WAIT_FAILED, .{ .err = @errorName(wait_err) });
        log.warn("selftest_probe_reaper_spawn_failed", .{ .err = @errorName(err) });
        return selftest.unavailable(alloc, cfg, selftest.DETAIL_SPAWN_FAILED);
    };

    var buf: [VERDICT_READ_CAP]u8 = undefined;
    const line = readVerdict(io, &child, &buf);

    _ = child.wait(io) catch |err|
        log.warn(EVENT_WAIT_FAILED, .{ .err = @errorName(err) });
    reaper.done.store(true, .seq_cst);
    watcher.join();

    const timed_out = reaper.fired.load(.seq_cst);
    return selftest.grade(alloc, cfg, outcomeFrom(line, timed_out));
}

/// Read the child's single verdict line. A short read is normal (the line is
/// well under the cap); a failed read yields an empty slice, which parses to
/// every check failing — a probe that said nothing proved nothing.
fn readVerdict(io: std.Io, child: *std.process.Child, buf: []u8) []const u8 {
    const out = child.stdout orelse return "";
    var fr = out.reader(io, &.{});
    const len = fr.interface.readSliceShort(buf) catch |err| {
        log.warn("selftest_probe_read_failed", .{ .err = @errorName(err) });
        return "";
    };
    return buf[0..len];
}

/// Turn the child's line into the booleans `grade` consumes.
///
/// A timeout short-circuits every check: a reaped probe observed nothing, and
/// reporting its partial line as fact would present a half-run as a verdict.
pub fn outcomeFrom(line: []const u8, timed_out: bool) selftest.Outcome {
    if (timed_out) return .{
        .resolver_readable = false,
        .dns_resolved = false,
        .egress_reachable = false,
        .extra_binds_present = false,
        .timed_out = true,
    };
    return .{
        .resolver_readable = verdictOf(line, selftest_probe.KEY_RESOLVER) == .passed,
        .dns_resolved = verdictOf(line, selftest_probe.KEY_DNS) == .passed,
        .egress_reachable = verdictOf(line, selftest_probe.KEY_EGRESS) == .passed,
        // Untested = no bind was assigned, so there is nothing to have missed.
        .extra_binds_present = verdictOf(line, selftest_probe.KEY_BINDS) != .failed,
        // A resolver tool is never missing now that the probe IS the runner, so
        // the only untestable DNS is one nothing asked for.
        .dns_testable = verdictOf(line, selftest_probe.KEY_DNS) != .untested,
    };
}

/// Find `key` in the line and decode the single character after it. Matched by
/// key rather than by position so adding a check cannot shift an existing one.
/// An absent or unrecognised key reads as `failed` — a verdict we could not
/// read is not a pass.
fn verdictOf(line: []const u8, key: []const u8) selftest_probe.Verdict {
    const at = std.mem.indexOf(u8, line, key) orelse return .failed;
    const value_at = at + key.len;
    if (value_at >= line.len) return .failed;
    return switch (line[value_at]) {
        @intFromEnum(selftest_probe.Verdict.passed) => .passed,
        @intFromEnum(selftest_probe.Verdict.untested) => .untested,
        else => .failed,
    };
}

test {
    _ = @import("selftest_exec_test.zig");
}
