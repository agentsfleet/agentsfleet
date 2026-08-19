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
const common = @import("common");
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
/// Safety).
///
/// Retirement is what makes the kill safe against pid reuse, and the ORDER is
/// load-bearing. `wait()` reaps the pid, and the kernel may hand that number to
/// an unrelated process immediately after; a reaper that checked a flag, got
/// descheduled, and then signalled would `SIGKILL` a stranger's process group.
/// So the parent retires the reaper while the child is still un-reaped — after
/// the child's stdout has hit end-of-file, which only happens once the whole
/// sandboxed tree has released the pipe — and joins before it calls `wait()`.
/// The mutex makes retire-vs-kill mutually exclusive rather than merely ordered.
const Reaper = struct {
    io: std.Io,
    pgid: std.posix.pid_t,
    mutex: common.Mutex = .{},
    done: bool = false,
    fired: bool = false,

    fn watch(self: *Reaper) void {
        var waited: u64 = 0;
        while (waited < selftest.PROBE_TIMEOUT_MS) {
            if (self.isDone()) return;
            // Fail CLOSED on a sleep error: stop waiting and go enforce the
            // bound. Returning here would retire the only watchdog while the
            // parent is still blocked reading the child's pipe.
            self.io.sleep(std.Io.Duration.fromMilliseconds(REAP_POLL_MS), .awake) catch break;
            waited += REAP_POLL_MS;
        }
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.done) return;
        self.fired = true;
        // Negative pid = the whole process group. The probe leads its own via
        // `pgid = 0` at spawn, so this reaps bwrap AND whatever it started —
        // killing only the pid would leave the sandbox's children orphaned.
        std.posix.kill(-self.pgid, std.posix.SIG.KILL) catch |err|
            log.warn(EVENT_KILL_FAILED, .{ .err = @errorName(err) });
    }

    fn isDone(self: *Reaper) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.done;
    }

    /// Parent side: no signal may be sent from here on. Returns whether the
    /// reaper had already fired, which is the timeout verdict.
    fn retire(self: *Reaper) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.done = true;
        return self.fired;
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
        // A bind that resolves onto protected host state refuses every lease on
        // this runner. That is precisely what a self-test exists to surface, so
        // it is a named check rather than an error the panel cannot render.
        if (err == error.UnsafeBindTarget)
            return selftest.unavailable(alloc, cfg, selftest.DETAIL_UNSAFE_BIND);
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
    // Reads to end-of-file, not to the first chunk: EOF is what proves the
    // sandboxed tree has exited, and that is the precondition for retiring the
    // reaper safely below. A hung probe never reaches EOF — the reaper kills it,
    // the pipe closes, and this returns.
    const line = readVerdict(io, &child, &buf);

    // Retire and join BEFORE reaping. Reversing these two lines re-opens the
    // pid-reuse window described on `Reaper`.
    const timed_out = reaper.retire();
    watcher.join();
    const term = child.wait(io) catch |err| blk: {
        log.warn(EVENT_WAIT_FAILED, .{ .err = @errorName(err) });
        break :blk null;
    };

    // A line from a child that did not exit cleanly is not a verdict. The probe
    // prints and then returns 0, so a non-zero status means it died partway —
    // and a half-run that printed three passes before crashing would otherwise
    // read as a healthy sandbox.
    if (!timed_out and !exitedClean(term)) {
        log.warn("selftest_probe_unclean_exit", .{});
        return selftest.unavailable(alloc, cfg, selftest.DETAIL_SPAWN_FAILED);
    }
    return selftest.grade(alloc, cfg, outcomeFrom(line, timed_out));
}

/// Did the probe exit 0? A missing status (the `wait` failed) is not clean —
/// we could not prove the child finished, so we do not trust what it said.
/// `pub` so the fail-closed arms are asserted without spawning a process.
pub fn exitedClean(term: ?std.process.Child.Term) bool {
    const t = term orelse return false;
    return switch (t) {
        .exited => |code| code == 0,
        else => false,
    };
}

/// Drain the child's stdout to end-of-file. A failed read yields an empty
/// slice, which parses to every check failing — a probe that said nothing
/// proved nothing.
///
/// Draining rather than taking the first chunk is deliberate on two counts: it
/// bounds what a wedged child can hand us at `VERDICT_READ_CAP`, and reaching
/// EOF is the signal the parent needs before it may retire the reaper.
fn readVerdict(io: std.Io, child: *std.process.Child, buf: []u8) []const u8 {
    const out = child.stdout orelse return "";
    return drainVerdict(io, out, buf);
}

/// The drain itself, split from the child so it can be proven against a real
/// file descriptor. `pub` for that reason: the overflow arm is what keeps a
/// chatty child from returning early without end-of-file, and an untested
/// version of it deadlocked the heartbeat.
pub fn drainVerdict(io: std.Io, out: std.Io.File, buf: []u8) []const u8 {
    var fr = out.reader(io, &.{});
    var len: usize = 0;
    // Overflow past the cap is read and DISCARDED rather than left in the pipe.
    // Stopping at a full buffer would return without EOF, and the caller retires
    // the watchdog on this return — a child that wrote 128 bytes and then hung
    // would leave `wait()` blocked with nothing left alive to kill it.
    var overflow: [VERDICT_READ_CAP]u8 = undefined;
    while (true) {
        const dest = if (len < buf.len) buf[len..] else overflow[0..];
        const n = fr.interface.readSliceShort(dest) catch |err| {
            log.warn("selftest_probe_read_failed", .{ .err = @errorName(err) });
            return buf[0..len];
        };
        if (n == 0) break; // EOF: the whole sandboxed tree released the pipe.
        if (len < buf.len) len += n;
    }
    return buf[0..len];
}

/// Turn the child's line into the booleans `grade` consumes.
///
/// A timeout short-circuits every check: a reaped probe observed nothing, and
/// reporting its partial line as fact would present a half-run as a verdict.
pub fn outcomeFrom(line: []const u8, timed_out: bool) selftest.Outcome {
    if (timed_out) return .{
        .resolver_readable = false,
        .scratch_writable = false,
        .dns_resolved = false,
        .egress_reachable = false,
        .extra_binds_present = false,
        .timed_out = true,
    };
    return .{
        .resolver_readable = verdictOf(line, selftest_probe.KEY_RESOLVER) == .passed,
        // Absent key reads as failed (an old probe paired with this parser
        // cannot certify a write it never attempted) — fail-closed like binds.
        .scratch_writable = verdictOf(line, selftest_probe.KEY_SCRATCH) == .passed,
        .dns_resolved = verdictOf(line, selftest_probe.KEY_DNS) == .passed,
        .egress_reachable = verdictOf(line, selftest_probe.KEY_EGRESS) == .passed,
        // An assigned bind is healthy ONLY on an explicit pass. Treating
        // "untested" as present would let a probe that never saw the bind
        // arguments certify mounts it never looked for; `grade` iterates the
        // assigned list, so with nothing assigned this value is never read.
        .extra_binds_present = verdictOf(line, selftest_probe.KEY_BINDS) == .passed,
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
