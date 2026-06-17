//! cgroups v2 resource governance for the host backend.
//!
//! Creates a transient cgroup scope for each execution to enforce:
//! - memory.max: hard memory limit
//! - cpu.max: CPU quota/period throttling
//! - pids.max: process/thread cap (fork-bomb / runaway-spawn bound)
//!
//! The cgroup is created under /sys/fs/cgroup/agent.runner/ and
//! cleaned up when the session is destroyed.
//! Linux-only; no-ops on other platforms.

const CgroupScope = @This();

path: []const u8,
alloc: std.mem.Allocator,
/// Blocking Io for the cgroup's /sys/fs filesystem ops — Zig 0.16 routes fs
/// through Io. Borrowed from the daemon's Threaded; never owned/closed here.
io: std.Io,

const CGROUP_BASE = "/sys/fs/cgroup/agent.runner";
const S_THROTTLED_USEC = "throttled_usec ";
const S_D = "{d}";
const S_S_S = "{s}/{s}";
const S_OOM_KILL = "oom_kill ";
const S_PIDS_MAX = "max ";
const BYTES_PER_KIB = 1024;
const log = logging.scoped(.runner_cgroup);

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

/// Create a transient cgroup scope for the given execution.
pub fn create(
    io: std.Io,
    alloc: std.mem.Allocator,
    execution_id: types.ExecutionId,
    limits: types.ResourceLimits,
) !CgroupScope {
    if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;

    const hex = types.executionIdHex(execution_id);
    const path = try std.fmt.allocPrint(alloc, "{s}/exec-{s}", .{ CGROUP_BASE, hex });
    errdefer alloc.free(path);

    // Ensure base directory exists.
    std.Io.Dir.createDirAbsolute(io, CGROUP_BASE, .default_dir) catch |err| {
        if (err != error.PathAlreadyExists) {
            log.err("base_create_failed", .{ .path = CGROUP_BASE, .err = @errorName(err) });
            return CgroupError.CgroupCreateFailed;
        }
    };

    // Create scope directory.
    std.Io.Dir.createDirAbsolute(io, path, .default_dir) catch |err| {
        log.err("scope_create_failed", .{ .path = path, .err = @errorName(err) });
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

    log.info("created", .{ .path = path, .memory_mb = limits.memory_limit_mb, .cpu_pct = limits.cpu_limit_percent, .pids_max = limits.pids_limit });

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

/// Atomically SIGKILL every process in the scope (bwrap + agent + any
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
    const content = buf[0..len];

    // Look for "throttled_usec N" in cpu.stat output.
    if (std.mem.indexOf(u8, content, S_THROTTLED_USEC)) |pos| {
        const after = content[pos + S_THROTTLED_USEC.len ..];
        const end = std.mem.indexOfScalar(u8, after, '\n') orelse after.len;
        return std.fmt.parseInt(u64, after[0..end], 10) catch 0;
    }
    return 0;
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
    const content = buf[0..len];

    if (std.mem.indexOf(u8, content, key)) |pos| {
        const after = content[pos + key.len ..];
        const end = std.mem.indexOfScalar(u8, after, '\n') orelse after.len;
        return std.fmt.parseInt(u64, after[0..end], 10) catch 0;
    }
    return 0;
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

    log.info("destroyed", .{ .path = self.path, .peak_bytes = peak, .cpu_throttled_ms = result.cpu_throttled_ms });
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
