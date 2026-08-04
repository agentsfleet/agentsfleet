//! repo_fetch_bounds.zig — what actually bounds a repository fetch: one child
//! process run under a wall-clock deadline and a byte ceiling.
//!
//! A depth bound is not a byte bound. Three commits bounds HISTORY, and one
//! commit can carry arbitrarily large blobs; `disk_write_limit_mb` has no
//! enforcement in `CgroupScope`, and the daemon-side fetch runs OUTSIDE the
//! child's cgroup, so nothing above this file would stop a fetch from filling
//! the host. git offers no total-byte cap of its own, so the ceiling is measured
//! rather than declared: the run wakes on a fixed cadence, totals the bytes
//! under the target, and kills the fetch the moment it goes over.
//!
//! The deadline is enforced the way the lease supervisor already enforces its
//! own (`child_supervisor_read.readResult`): poll the child's stderr under an
//! absolute deadline, and kill BEFORE the single blocking `wait`. A timeout
//! checked after `child.wait()` is dead code — `wait` blocks the calling thread
//! — and a timer thread that killed while this thread waited would race the same
//! `child.id` the wait consumes, reintroducing the kill-after-reap hazard
//! `child_process.killChild` is careful to avoid. One thread owns the child from
//! spawn to reap; every bound is checked before the wait, never after.
//!
//! stderr is the exit signal as well as the diagnostic: every process holding
//! that descriptor — git and the transport helpers it execs — has to be gone
//! before it reads EOF, so the `wait` that follows EOF is bounded in fact rather
//! than in hope. A read error on it is treated as a lost bound, not as EOF: the
//! run is killed rather than allowed into an unbounded wait.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const pipe_proto = @import("pipe_proto.zig");
const client_errors = @import("engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TIMEOUT_KILL = client_errors.ERR_EXEC_TIMEOUT_KILL;
const ERR_EXEC_RESOURCE_KILL = client_errors.ERR_EXEC_RESOURCE_KILL;
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

/// The two limits every step of one fetch shares. The deadline is ABSOLUTE and
/// spans the whole sequence, so three steps cannot each spend the budget; the
/// ceiling is re-measured per tick against the same target for the same reason.
pub const Bounds = struct {
    /// Absolute epoch-ms wall clock. The caller clamps it to the lease.
    deadline_ms: i64,
    /// Ceiling on bytes under `Spec.target`, across every step.
    max_bytes: u64,
    /// How often the run wakes to re-measure. Production takes the default;
    /// tests inject a small value so a quota breach is observed in milliseconds
    /// rather than seconds (the `RenewHook.tick_ms` convention).
    check_interval_ms: i64 = QUOTA_CHECK_INTERVAL_MS,
};

/// One child process to run under `Bounds`. Every field is borrowed for the call.
pub const Spec = struct {
    /// argv[0] must be absolute — resolution via the parent `$PATH` is a trust
    /// dependency the runner refuses everywhere else (`requireAbsoluteArgv0`).
    argv: []const []const u8,
    /// The child's COMPLETE environment. Never inherited: the daemon's environ
    /// carries `AGENTSFLEET_RUNNER_TOKEN`, and git has no business seeing it.
    environ: *const std.process.Environ.Map,
    /// Working directory, passed as an open handle so no step re-resolves a name
    /// the sandboxed child could have swapped underneath it.
    cwd: std.Io.Dir,
    /// The tree metered against `bounds.max_bytes`.
    target: std.Io.Dir,
    bounds: Bounds,
};

/// Why a run stopped. Distinct variants because the caller reports the reason
/// and the three mean genuinely different things to whoever reads it: a slow
/// remote, a repository too big for the bound, and a broken pipe (RULE ECL).
pub const Stop = enum {
    /// The child exited on its own; read `exit_code` for its verdict.
    completed,
    /// The shared wall-clock deadline elapsed. Killed.
    timed_out,
    /// The target exceeded `max_bytes` (or grew past what the scan accounts
    /// for). Killed.
    over_quota,
    /// stderr could not be read, so the run could no longer be bounded. Killed
    /// rather than left to an unbounded wait.
    transport_lost,
};

pub const RunOutcome = struct {
    stop: Stop,
    /// The child's exit status when `stop == .completed`; null when killed or
    /// when it died on a signal (a signal death is never a success).
    exit_code: ?u8,
    /// The head of the child's stderr, borrowed from the caller's buffer. A LOG
    /// TAIL, never a scan window: nothing branches on its contents, so the
    /// truncation is not a RULE FXS bypass. It is deliberately not forwarded to
    /// the child — remote-authored bytes do not enter a model's context here.
    stderr: []const u8,

    /// True only for a child that ran to completion and said so.
    pub fn succeeded(self: RunOutcome) bool {
        return self.stop == .completed and self.exit_code == 0;
    }
};

pub const RunError = error{ SpawnFailed, ReapFailed };

/// Spawn one child and hold it to `spec.bounds`. Returns how it stopped; the
/// caller maps that onto its own failure vocabulary. Allocates nothing — argv,
/// environ, and `stderr_buf` are all caller-owned.
pub fn run(io: std.Io, spec: Spec, stderr_buf: []u8) RunError!RunOutcome {
    var child = std.process.spawn(io, .{
        .argv = spec.argv,
        .cwd = .{ .dir = spec.cwd },
        .environ_map = spec.environ,
        .stdin = .ignore,
        .stdout = .ignore,
        .stderr = .pipe,
        // Its own process-group leader, matching the lease child: a kill can
        // reach a helper git forked even if the cgroup is not in play.
        .pgid = 0,
    }) catch |err| {
        log.err("repo_fetch_spawn_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        return error.SpawnFailed;
    };
    // OWNERSHIP CONTRACT: this function is the sole reaper. `kill` both kills and
    // reaps (and is idempotent), so the defer covers every early return — a
    // `wait` that itself failed included.
    var reaped = false;
    defer if (!reaped) child.kill(io);

    var filled: usize = 0;
    const stop = watch(io, child.stderr.?.handle, spec, stderr_buf, &filled);
    if (stop != .completed) {
        child.kill(io);
        reaped = true;
        return .{ .stop = stop, .exit_code = null, .stderr = stderr_buf[0..filled] };
    }

    const term = child.wait(io) catch |err| {
        log.err("repo_fetch_reap_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        return error.ReapFailed;
    };
    reaped = true;
    return .{
        .stop = .completed,
        .exit_code = switch (term) {
            .exited => |code| code,
            else => null,
        },
        .stderr = stderr_buf[0..filled],
    };
}

/// Watch the child until it closes stderr or breaks a bound. Returns
/// `.completed` only on EOF — every other return means the caller must kill.
fn watch(io: std.Io, err_fd: std.posix.fd_t, spec: Spec, stderr_buf: []u8, filled: *usize) Stop {
    while (true) {
        const tick_deadline = @min(spec.bounds.deadline_ms, clock.nowMillis() + spec.bounds.check_interval_ms);
        const ready = pipe_proto.waitReadable(err_fd, tick_deadline) catch |err| {
            log.warn("repo_fetch_stderr_poll_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
            return .transport_lost;
        };
        switch (ready) {
            .readable => {
                // Drained every wake, whether or not the tail buffer still has
                // room: a full pipe would otherwise block git forever and the
                // deadline would be the only thing left to stop it.
                const n = drain(err_fd, stderr_buf, filled) orelse return .transport_lost;
                if (n == 0) return .completed;
            },
            .timed_out => {
                if (clock.nowMillis() >= spec.bounds.deadline_ms) {
                    log.warn("repo_fetch_timed_out", .{ .error_code = ERR_EXEC_TIMEOUT_KILL, .deadline_ms = spec.bounds.deadline_ms });
                    return .timed_out;
                }
                switch (measure(io, spec.target, spec.bounds.max_bytes)) {
                    .bytes => {},
                    .over_limit => {
                        log.warn("repo_fetch_over_quota", .{ .error_code = ERR_EXEC_RESOURCE_KILL, .max_bytes = spec.bounds.max_bytes });
                        return .over_quota;
                    },
                }
            },
        }
    }
}

/// Read one chunk of the child's stderr. Returns the byte count (0 = EOF), or
/// null when the read itself failed. Bytes past the tail buffer are discarded
/// into scratch so the pipe keeps draining.
fn drain(err_fd: std.posix.fd_t, stderr_buf: []u8, filled: *usize) ?usize {
    var scratch: [DRAIN_CHUNK_BYTES]u8 = undefined;
    const has_room = filled.* < stderr_buf.len;
    const dst = if (has_room) stderr_buf[filled.*..] else scratch[0..];
    const n = std.posix.read(err_fd, dst) catch |err| {
        log.warn("repo_fetch_stderr_read_failed", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS, .err = @errorName(err) });
        return null;
    };
    if (has_room) filled.* += n;
    return n;
}

/// Total bytes under `dir`, or `.over_limit`. Fail-closed by construction: a
/// scan error, a tree deeper than `MAX_MEASURE_DEPTH`, or more than
/// `MAX_MEASURED_ENTRIES` entries all read as over the limit, because a tree
/// this code cannot account for is not one it can certify as small.
pub const Measure = union(enum) { bytes: u64, over_limit };

pub fn measure(io: std.Io, dir: std.Io.Dir, ceiling: u64) Measure {
    var total: u64 = 0;
    var scanned: u32 = 0;
    if (!accumulateWithin(io, dir, ceiling, &total, &scanned, 0)) return .over_limit;
    return .{ .bytes = total };
}

/// Add `dir`'s tree into `total`, returning false the moment the walk exceeds
/// `ceiling` or runs past a bound. Recursion carries one `Iterator` (~2 KiB) per
/// level and is depth-capped, so the stack cost is bounded and small; `Dir.walk`
/// is deliberately not used because it allocates, and this runs on a tick.
fn accumulateWithin(io: std.Io, dir: std.Io.Dir, ceiling: u64, total: *u64, scanned: *u32, depth: u8) bool {
    if (depth >= MAX_MEASURE_DEPTH) return false;
    var it = dir.iterate();
    while (true) {
        const next = it.next(io) catch return false;
        const entry = next orelse return true;
        scanned.* += 1;
        if (scanned.* > MAX_MEASURED_ENTRIES) return false;
        switch (entry.kind) {
            .directory => {
                var sub = dir.openDir(io, entry.name, .{ .iterate = true, .follow_symlinks = false }) catch return false;
                defer sub.close(io);
                if (!accumulateWithin(io, sub, ceiling, total, scanned, depth + 1)) return false;
            },
            // A symlink costs its own name, not its target's bytes — those are
            // counted where they live if they live under this tree at all, and
            // following one would count outside it.
            .sym_link => {},
            else => {
                const st = dir.statFile(io, entry.name, .{ .follow_symlinks = false }) catch continue;
                total.* += st.size;
                if (total.* > ceiling) return false;
            },
        }
    }
}

/// How often the run wakes to re-measure the target. Fast enough that a runaway
/// fetch is caught within a second of crossing, slow enough that the walk is
/// noise beside the network the fetch is waiting on.
const QUOTA_CHECK_INTERVAL_MS: i64 = 1_000;

/// Discard buffer for stderr past the caller's tail — sized for one pipe read.
const DRAIN_CHUNK_BYTES: usize = 512;

/// Recursion ceiling. Deeper than any repository layout a depth-bounded fetch
/// produces, shallow enough that the per-level iterators stay tens of KiB.
const MAX_MEASURE_DEPTH: u8 = 24;
/// Entry ceiling per measurement. A tree larger than this is over any fetch
/// bound worth allowing, so hitting it IS the answer rather than a reason to
/// keep counting.
const MAX_MEASURED_ENTRIES: u32 = 200_000;

test {
    _ = @import("repo_fetch_bounds_test.zig");
}
