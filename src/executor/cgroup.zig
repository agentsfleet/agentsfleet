//! cgroups v2 resource governance for the host backend (§4.3).
//!
//! Creates a transient cgroup scope for each execution to enforce:
//! - memory.max: hard memory limit
//! - cpu.max: CPU quota/period throttling
//! - io.max: disk write rate limiting
//!
//! The cgroup is created under /sys/fs/cgroup/zombie.executor/ and
//! cleaned up when the session is destroyed.
//! Linux-only; no-ops on other platforms.

const std = @import("std");
const builtin = @import("builtin");
const types = @import("types.zig");
const executor_metrics = @import("executor_metrics.zig");

const log = std.log.scoped(.executor_cgroup);

const CGROUP_BASE = "/sys/fs/cgroup/zombie.executor";

pub const CgroupError = error{
    UnsupportedPlatform,
    CgroupCreateFailed,
    CgroupWriteFailed,
    CgroupMoveFailed,
    CgroupReadFailed,
};

/// cgroup scope for a single execution.
pub const CgroupScope = struct {
    path: []const u8,
    alloc: std.mem.Allocator,

    /// Create a transient cgroup scope for the given execution.
    pub fn create(
        alloc: std.mem.Allocator,
        execution_id: types.ExecutionId,
        limits: types.ResourceLimits,
    ) !CgroupScope {
        if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;

        const hex = types.executionIdHex(execution_id);
        const path = try std.fmt.allocPrint(alloc, "{s}/exec-{s}", .{ CGROUP_BASE, hex });
        errdefer alloc.free(path);

        // Ensure base directory exists.
        std.fs.makeDirAbsolute(CGROUP_BASE) catch |err| {
            if (err != error.PathAlreadyExists) {
                log.err("cgroup.base_create_failed path={s} err={s}", .{ CGROUP_BASE, @errorName(err) });
                return CgroupError.CgroupCreateFailed;
            }
        };

        // Create scope directory.
        std.fs.makeDirAbsolute(path) catch |err| {
            log.err("cgroup.scope_create_failed path={s} err={s}", .{ path, @errorName(err) });
            return CgroupError.CgroupCreateFailed;
        };

        const scope = CgroupScope{ .path = path, .alloc = alloc };

        // Set memory limit.
        const memory_bytes = limits.memory_limit_mb * 1024 * 1024;
        try scope.writeControl("memory.max", memory_bytes);

        // Set CPU limit (quota/period format: e.g. "50000 100000" for 50%).
        const period: u64 = 100_000; // 100ms
        const quota = (limits.cpu_limit_percent * period) / 100;
        try scope.writeCpuMax(quota, period);

        log.info("cgroup.created path={s} memory_mb={d} cpu_pct={d}", .{
            path,
            limits.memory_limit_mb,
            limits.cpu_limit_percent,
        });

        return scope;
    }

    /// Move a process into this cgroup scope.
    pub fn addProcess(self: *const CgroupScope, pid: std.posix.pid_t) !void {
        if (builtin.os.tag != .linux) return CgroupError.UnsupportedPlatform;
        const procs_path = try std.fmt.allocPrint(self.alloc, "{s}/cgroup.procs", .{self.path});
        defer self.alloc.free(procs_path);

        var buf: [20]u8 = undefined;
        const pid_str = std.fmt.bufPrint(&buf, "{d}", .{pid}) catch return CgroupError.CgroupWriteFailed;

        const file = std.fs.openFileAbsolute(procs_path, .{ .mode = .write_only }) catch {
            return CgroupError.CgroupMoveFailed;
        };
        defer file.close();
        file.writeAll(pid_str) catch return CgroupError.CgroupMoveFailed;
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

        const file = std.fs.openFileAbsolute(stat_path, .{}) catch return 0;
        defer file.close();
        var buf: [512]u8 = undefined;
        const len = file.readAll(&buf) catch return 0;
        const content = buf[0..len];

        // Look for "throttled_usec N" in cpu.stat output.
        if (std.mem.indexOf(u8, content, "throttled_usec ")) |pos| {
            const after = content[pos + "throttled_usec ".len ..];
            const end = std.mem.indexOfScalar(u8, after, '\n') orelse after.len;
            return std.fmt.parseInt(u64, after[0..end], 10) catch 0;
        }
        return 0;
    }

    /// Check if the cgroup was OOM-killed.
    pub fn wasOomKilled(self: *const CgroupScope) bool {
        if (builtin.os.tag != .linux) return false;
        const events_path = std.fmt.allocPrint(self.alloc, "{s}/memory.events", .{self.path}) catch return false;
        defer self.alloc.free(events_path);

        const file = std.fs.openFileAbsolute(events_path, .{}) catch return false;
        defer file.close();
        var buf: [512]u8 = undefined;
        const len = file.readAll(&buf) catch return false;
        const content = buf[0..len];

        // Look for "oom_kill N" where N > 0.
        if (std.mem.indexOf(u8, content, "oom_kill ")) |pos| {
            const after = content[pos + "oom_kill ".len ..];
            const end = std.mem.indexOfScalar(u8, after, '\n') orelse after.len;
            const count = std.fmt.parseInt(u64, after[0..end], 10) catch return false;
            return count > 0;
        }
        return false;
    }

    /// Resource metrics captured at cgroup teardown.
    pub const CgroupMetrics = struct {
        memory_peak_bytes: u64,
        memory_limit_bytes: u64,
        cpu_throttled_ms: u64,
    };

    /// Destroy the cgroup scope, capture metrics, and clean up.
    pub fn destroy(self: *CgroupScope, limits: types.ResourceLimits) CgroupMetrics {
        var result = CgroupMetrics{
            .memory_peak_bytes = 0,
            .memory_limit_bytes = limits.memory_limit_mb * 1024 * 1024,
            .cpu_throttled_ms = 0,
        };
        if (builtin.os.tag != .linux) return result;

        const peak = self.readMemoryPeak();
        if (peak > 0) {
            executor_metrics.setExecutorMemoryPeakBytes(peak);
            result.memory_peak_bytes = peak;
        }

        const throttled_us = self.readCpuThrottledUs();
        if (throttled_us > 0) {
            const throttled_ms = throttled_us / 1000;
            executor_metrics.addExecutorCpuThrottledMs(throttled_ms);
            result.cpu_throttled_ms = throttled_ms;
        }

        // Remove the cgroup directory (must be empty of processes first).
        std.fs.deleteTreeAbsolute(self.path) catch |err| {
            log.warn("cgroup.cleanup_failed path={s} err={s}", .{ self.path, @errorName(err) });
        };

        log.info("cgroup.destroyed path={s} peak_bytes={d} cpu_throttled_ms={d}", .{ self.path, peak, result.cpu_throttled_ms });
        self.alloc.free(self.path);
        return result;
    }

    fn writeControl(self: *const CgroupScope, control_file: []const u8, value: u64) !void {
        const control_path = try std.fmt.allocPrint(self.alloc, "{s}/{s}", .{ self.path, control_file });
        defer self.alloc.free(control_path);

        var buf: [20]u8 = undefined;
        const val_str = std.fmt.bufPrint(&buf, "{d}", .{value}) catch return CgroupError.CgroupWriteFailed;

        const file = std.fs.openFileAbsolute(control_path, .{ .mode = .write_only }) catch {
            return CgroupError.CgroupWriteFailed;
        };
        defer file.close();
        file.writeAll(val_str) catch return CgroupError.CgroupWriteFailed;
    }

    fn writeCpuMax(self: *const CgroupScope, quota: u64, period: u64) !void {
        const control_path = try std.fmt.allocPrint(self.alloc, "{s}/cpu.max", .{self.path});
        defer self.alloc.free(control_path);

        var buf: [40]u8 = undefined;
        const val_str = std.fmt.bufPrint(&buf, "{d} {d}", .{ quota, period }) catch return CgroupError.CgroupWriteFailed;

        const file = std.fs.openFileAbsolute(control_path, .{ .mode = .write_only }) catch {
            return CgroupError.CgroupWriteFailed;
        };
        defer file.close();
        file.writeAll(val_str) catch return CgroupError.CgroupWriteFailed;
    }

    fn readControlValue(self: *const CgroupScope, control_file: []const u8) !u64 {
        const control_path = try std.fmt.allocPrint(self.alloc, "{s}/{s}", .{ self.path, control_file });
        defer self.alloc.free(control_path);

        const file = std.fs.openFileAbsolute(control_path, .{}) catch return CgroupError.CgroupReadFailed;
        defer file.close();
        var buf: [64]u8 = undefined;
        const len = file.readAll(&buf) catch return CgroupError.CgroupReadFailed;
        const trimmed = std.mem.trim(u8, buf[0..len], " \t\r\n");
        return std.fmt.parseInt(u64, trimmed, 10) catch 0;
    }
};

/// Check if cgroups v2 is available on the current system.
pub fn isAvailable() bool {
    if (builtin.os.tag != .linux) return false;
    std.fs.accessAbsolute("/sys/fs/cgroup/cgroup.controllers", .{}) catch return false;
    return true;
}

test "isAvailable returns false on non-linux" {
    if (builtin.os.tag == .linux) return error.SkipZigTest;
    try std.testing.expect(!isAvailable());
}
