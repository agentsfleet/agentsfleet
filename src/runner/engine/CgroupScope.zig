//! cgroups v2 resource governance for the host backend.
//!
//! Creates a transient cgroup scope for each execution to enforce:
//! - memory.max: hard memory limit
//! - cpu.max: CPU quota/period throttling
//! - pids.max: process/thread cap (fork-bomb / runaway-spawn bound)
//!
//! The cgroup is created beneath the runner service's delegated cgroup and
//! cleaned up when the session is destroyed.
//! Linux-only; no-ops on other platforms.

const CgroupScope = @This();

path: []const u8,
alloc: std.mem.Allocator,
/// Blocking Io for the cgroup's /sys/fs filesystem ops — Zig 0.16 routes fs
/// through Io. Borrowed from the daemon's Threaded; never owned/closed here.
io: std.Io,

const CGROUP_MOUNT = "/sys/fs/cgroup";
const CGROUP_PROC_PATH = "/proc/self/cgroup";
const S_CGROUP_BASE = "{s}{s}";
const S_EXECUTION_SCOPE = "{s}/exec-{s}";
const S_UNIFIED_CGROUP_PREFIX = "0::";
const S_RUNNER_SUBGROUP_SUFFIX = "/runner";
const S_THROTTLED_USEC = "throttled_usec ";
const S_D = "{d}";
const S_S_S = "{s}/{s}";
const S_OOM_KILL = "oom_kill ";
const S_PIDS_MAX = "max ";
const F_SUBTREE_CONTROL = "cgroup.subtree_control";
/// The three controllers every execution scope needs files for: memory.max,
/// cpu.max, pids.max. Kept in the `+ctrl` write form the kernel expects.
const S_ENABLE_CONTROLLERS = "+cpu +memory +pids";
const MAX_CGROUP_PLACEMENT_BYTES = 4096;
const BYTES_PER_KIB = 1024;
const log = logging.scoped(.runner_cgroup);
const ERR_RUN_SANDBOX_ESTABLISH_FAILED = client_errors.ERR_RUN_SANDBOX_ESTABLISH_FAILED;

pub const CgroupError = error{
    UnsupportedPlatform,
    CgroupCreateFailed,
    CgroupWriteFailed,
    CgroupMoveFailed,
    CgroupReadFailed,
};

/// Resource metrics captured at cgroup teardown.
pub const CgroupMetrics = struct {
    memory_peak_bytes: u64,
    memory_limit_bytes: u64,
    cpu_throttled_ms: u64,
};

/// Enable the delegated controllers on the runner's cgroup base, so every
/// execution scope created beneath it actually has `memory.max`, `cpu.max`, and
/// `pids.max` to write.
///
/// systemd's `Delegate=cpu memory pids` only makes the controllers AVAILABLE in
/// the unit cgroup (`cgroup.controllers`); writing `cgroup.subtree_control` is
/// the delegatee's job and systemd never does it. Skipping it is silent and
/// total: `create()` makes the scope directory, then fails writing `memory.max`
/// because the file does not exist, so every lease is refused
/// `sandbox_unavailable` while orphan scope directories pile up.
///
/// `DelegateSubgroup=runner` keeps the daemon's own process in a child cgroup,
/// which is what makes this write legal under cgroup v2's no-internal-processes
/// rule. Idempotent — re-enabling an already-enabled controller is a no-op, so
/// this is safe across restarts.
pub fn enableDelegatedControllers(io: std.Io, alloc: std.mem.Allocator) !void {
    if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;

    const base = try resolveCgroupBase(io, alloc);
    defer alloc.free(base);

    const path = try std.fmt.allocPrint(alloc, S_S_S, .{ base, F_SUBTREE_CONTROL });
    defer alloc.free(path);

    const file = std.Io.Dir.openFileAbsolute(io, path, .{ .mode = .write_only }) catch |err| {
        log.err("subtree_control_open_failed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .path = path, .err = @errorName(err) });
        return CgroupError.CgroupWriteFailed;
    };
    defer file.close(io);

    file.writeStreamingAll(io, S_ENABLE_CONTROLLERS) catch |err| {
        log.err("subtree_control_write_failed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .path = path, .err = @errorName(err) });
        return CgroupError.CgroupWriteFailed;
    };

    log.info("cgroup_controllers_enabled", .{ .path = path, .controllers = S_ENABLE_CONTROLLERS });
}

/// Create a transient cgroup scope for the given execution.
pub fn create(
    io: std.Io,
    alloc: std.mem.Allocator,
    execution_id: types.ExecutionId,
    limits: types.ResourceLimits,
) !CgroupScope {
    if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;

    const base = resolveCgroupBase(io, alloc) catch |err| {
        log.err("base_resolve_failed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .err = @errorName(err) });
        return err;
    };
    defer alloc.free(base);

    const hex = types.executionIdHex(execution_id);
    const path = try std.fmt.allocPrint(alloc, S_EXECUTION_SCOPE, .{ base, hex });
    errdefer alloc.free(path);

    // Create scope directory.
    std.Io.Dir.createDirAbsolute(io, path, .default_dir) catch |err| {
        log.err("scope_create_failed", .{ .error_code = ERR_RUN_SANDBOX_ESTABLISH_FAILED, .path = path, .err = @errorName(err) });
        return CgroupError.CgroupCreateFailed;
    };

    const scope = CgroupScope{ .path = path, .alloc = alloc, .io = io };

    // Set memory limit.
    const memory_bytes = limits.memory_limit_mb * 1024 * 1024;
    try scope.writeControl("memory.max", memory_bytes);

    // Set CPU limit (quota/period format: e.g. "50000 100000" for 50%).
    const period: u64 = 100_000; // 100ms
    const quota = (limits.cpu_limit_percent * period) / 100;
    try scope.writeCpuMax(quota, period);

    // PID cap — bounds fork bombs; a fork past it fails EAGAIN (not a kill) and
    // bumps pids.events, which wasPidsExhausted() reads to attribute the failure.
    try scope.writeControl("pids.max", limits.pids_limit);

    log.debug("cgroup_created", .{ .path = path, .memory_mb = limits.memory_limit_mb, .cpu_pct = limits.cpu_limit_percent, .pids_max = limits.pids_limit });

    return scope;
}

/// Move a process into this cgroup scope.
pub fn addProcess(self: *const CgroupScope, pid: std.posix.pid_t) !void {
    if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;
    const procs_path = try std.fmt.allocPrint(self.alloc, "{s}/cgroup.procs", .{self.path});
    defer self.alloc.free(procs_path);

    var buf: [20]u8 = undefined;
    const pid_str = std.fmt.bufPrint(&buf, S_D, .{pid}) catch return CgroupError.CgroupWriteFailed;

    const file = std.Io.Dir.openFileAbsolute(self.io, procs_path, .{ .mode = .write_only }) catch {
        return CgroupError.CgroupMoveFailed;
    };
    defer file.close(self.io);
    file.writeStreamingAll(self.io, pid_str) catch return CgroupError.CgroupMoveFailed;
}

/// Atomically SIGKILL every process in the scope (bwrap + Fleet + any
/// sub-tool it spawned) via cgroup.kill — the kill switch for a wall-clock
/// timeout, a heartbeat-carried revocation, or teardown. Atomic and
/// PID-chase-free, so no process escapes. Needs cgroup v2 kernel >= 5.14;
/// callers treat a write failure as "fall back to per-PID SIGKILL".
pub fn kill(self: *const CgroupScope) !void {
    if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;
    try self.writeControl("cgroup.kill", 1);
}

/// Read peak memory usage from the cgroup.
pub fn readMemoryPeak(self: *const CgroupScope) u64 {
    if (builtin.os.tag != .linux) return 0;
    return self.readControlValue("memory.peak") catch 0;
}

/// Read current memory usage.
pub fn readMemoryCurrent(self: *const CgroupScope) u64 {
    if (builtin.os.tag != .linux) return 0;
    return self.readControlValue("memory.current") catch 0;
}

/// Read CPU throttled time in microseconds from cpu.stat.
/// Returns 0 if not on Linux or if the file cannot be read.
pub fn readCpuThrottledUs(self: *const CgroupScope) u64 {
    if (builtin.os.tag != .linux) return 0;
    const stat_path = std.fmt.allocPrint(self.alloc, "{s}/cpu.stat", .{self.path}) catch return 0;
    defer self.alloc.free(stat_path);

    const file = std.Io.Dir.openFileAbsolute(self.io, stat_path, .{}) catch return 0;
    defer file.close(self.io);
    var fr = file.reader(self.io, &.{});
    var buf: [2048]u8 = undefined;
    const len = fr.interface.readSliceShort(&buf) catch return 0;
    // "throttled_usec N" is the same `<key> N` shape as the events files.
    return parseEventCount(buf[0..len], S_THROTTLED_USEC);
}

/// Check if the cgroup was OOM-killed (memory.events `oom_kill` > 0).
pub fn wasOomKilled(self: *const CgroupScope) bool {
    return self.readEventCount("memory.events", S_OOM_KILL) > 0;
}

/// True if a fork past `pids.max` was refused (pids.events `max` > 0). Not a kill
/// itself — the classifier reads it to attribute a resulting failure.
pub fn wasPidsExhausted(self: *const CgroupScope) bool {
    return self.readEventCount("pids.events", S_PIDS_MAX) > 0;
}

/// Parse a `<key> N` counter out of cgroup events-file content —
/// pure, no I/O, so it is fixture-testable off-Linux. `key` includes its
/// trailing space (e.g. `"oom_kill "`). Returns 0 when the key is absent or its
/// value is empty/malformed — the same fail-safe the live readers rely on.
pub fn parseEventCount(content: []const u8, key: []const u8) u64 {
    const pos = std.mem.indexOf(u8, content, key) orelse return 0;
    const after = content[pos + key.len ..];
    const end = std.mem.indexOfScalar(u8, after, '\n') orelse after.len;
    return std.fmt.parseInt(u64, after[0..end], 10) catch 0;
}

/// Return the delegated service cgroup from `/proc/self/cgroup` content.
/// The runner must run in systemd's `runner` leaf before it may create children.
pub fn delegatedCgroupPath(placement: []const u8) ?[]const u8 {
    var lines = std.mem.splitScalar(u8, placement, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, S_UNIFIED_CGROUP_PREFIX)) continue;
        const runner_path = line[S_UNIFIED_CGROUP_PREFIX.len..];
        if (!safeCgroupPath(runner_path)) return null;
        if (!std.mem.endsWith(u8, runner_path, S_RUNNER_SUBGROUP_SUFFIX)) return null;
        const delegated_path = runner_path[0 .. runner_path.len - S_RUNNER_SUBGROUP_SUFFIX.len];
        return if (safeCgroupPath(delegated_path)) delegated_path else null;
    }
    return null;
}

/// Absolute path of the runner's delegated cgroup base. Pub for the
/// capability probe, which reads what the subtree actually enables.
pub fn resolveCgroupBase(io: std.Io, alloc: std.mem.Allocator) ![]u8 {
    const file = std.Io.Dir.openFileAbsolute(io, CGROUP_PROC_PATH, .{}) catch return CgroupError.CgroupReadFailed;
    defer file.close(io);

    var reader = file.reader(io, &.{});
    var buf: [MAX_CGROUP_PLACEMENT_BYTES]u8 = undefined;
    const len = reader.interface.readSliceShort(&buf) catch return CgroupError.CgroupReadFailed;
    const path = delegatedCgroupPath(buf[0..len]) orelse return CgroupError.CgroupReadFailed;
    return std.fmt.allocPrint(alloc, S_CGROUP_BASE, .{ CGROUP_MOUNT, path });
}

fn safeCgroupPath(path: []const u8) bool {
    if (path.len <= 1 or path[0] != '/') return false;
    if (std.mem.indexOfScalar(u8, path, 0) != null) return false;

    var segments = std.mem.splitScalar(u8, path[1..], '/');
    while (segments.next()) |segment| {
        if (segment.len == 0 or std.mem.eql(u8, segment, ".") or std.mem.eql(u8, segment, "..")) return false;
    }
    return true;
}

/// Read a `<key> N` counter from a cgroup events file (0 if absent/unreadable/off-linux).
fn readEventCount(self: *const CgroupScope, events_file: []const u8, key: []const u8) u64 {
    if (builtin.os.tag != .linux) return 0;
    const events_path = std.fmt.allocPrint(self.alloc, S_S_S, .{ self.path, events_file }) catch return 0;
    defer self.alloc.free(events_path);

    const file = std.Io.Dir.openFileAbsolute(self.io, events_path, .{}) catch return 0;
    defer file.close(self.io);
    var fr = file.reader(self.io, &.{});
    var buf: [512]u8 = undefined;
    const len = fr.interface.readSliceShort(&buf) catch return 0;
    return parseEventCount(buf[0..len], key);
}

/// Destroy the cgroup scope, capture metrics, and clean up.
pub fn destroy(self: *CgroupScope, limits: types.ResourceLimits) CgroupMetrics {
    var result = CgroupMetrics{
        .memory_peak_bytes = 0,
        .memory_limit_bytes = limits.memory_limit_mb * BYTES_PER_KIB * BYTES_PER_KIB,
        .cpu_throttled_ms = 0,
    };
    if (builtin.os.tag != .linux) return result;

    const peak = self.readMemoryPeak();
    if (peak > 0) {
        result.memory_peak_bytes = peak;
    }

    const throttled_us = self.readCpuThrottledUs();
    if (throttled_us > 0) {
        const throttled_ms = throttled_us / 1000;
        result.cpu_throttled_ms = throttled_ms;
    }

    // Remove the cgroup directory (must be empty of processes first).
    std.Io.Dir.cwd().deleteTree(self.io, self.path) catch |err| {
        log.warn("cleanup_failed", .{ .path = self.path, .err = @errorName(err) });
    };

    log.debug("cgroup_destroyed", .{ .path = self.path, .peak_bytes = peak, .cpu_throttled_ms = result.cpu_throttled_ms });
    self.alloc.free(self.path);
    return result;
}

fn writeControl(self: *const CgroupScope, control_file: []const u8, value: u64) !void {
    const control_path = try std.fmt.allocPrint(self.alloc, S_S_S, .{ self.path, control_file });
    defer self.alloc.free(control_path);

    var buf: [20]u8 = undefined;
    const val_str = std.fmt.bufPrint(&buf, S_D, .{value}) catch return CgroupError.CgroupWriteFailed;

    const file = std.Io.Dir.openFileAbsolute(self.io, control_path, .{ .mode = .write_only }) catch {
        return CgroupError.CgroupWriteFailed;
    };
    defer file.close(self.io);
    file.writeStreamingAll(self.io, val_str) catch return CgroupError.CgroupWriteFailed;
}

fn writeCpuMax(self: *const CgroupScope, quota: u64, period: u64) !void {
    const control_path = try std.fmt.allocPrint(self.alloc, "{s}/cpu.max", .{self.path});
    defer self.alloc.free(control_path);

    var buf: [40]u8 = undefined;
    const val_str = std.fmt.bufPrint(&buf, "{d} {d}", .{ quota, period }) catch return CgroupError.CgroupWriteFailed;

    const file = std.Io.Dir.openFileAbsolute(self.io, control_path, .{ .mode = .write_only }) catch {
        return CgroupError.CgroupWriteFailed;
    };
    defer file.close(self.io);
    file.writeStreamingAll(self.io, val_str) catch return CgroupError.CgroupWriteFailed;
}

fn readControlValue(self: *const CgroupScope, control_file: []const u8) !u64 {
    const control_path = try std.fmt.allocPrint(self.alloc, S_S_S, .{ self.path, control_file });
    defer self.alloc.free(control_path);

    const file = std.Io.Dir.openFileAbsolute(self.io, control_path, .{}) catch return CgroupError.CgroupReadFailed;
    defer file.close(self.io);
    var fr = file.reader(self.io, &.{});
    var buf: [64]u8 = undefined;
    const len = fr.interface.readSliceShort(&buf) catch return CgroupError.CgroupReadFailed;
    const trimmed = std.mem.trim(u8, buf[0..len], " \t\r\n");
    return std.fmt.parseInt(u64, trimmed, 10) catch 0;
}

const std = @import("std");
const logging = @import("log");
const builtin = @import("builtin");
const types = @import("types.zig");
const client_errors = @import("client_errors.zig");
