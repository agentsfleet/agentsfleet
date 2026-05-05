const std = @import("std");

pub const ValidationError = error{
    InvalidDrainTimeoutMs,
    InvalidExecutorStartupTimeoutMs,
    InvalidExecutorLeaseTimeoutMs,
    InvalidExecutorMemoryLimitMb,
    InvalidExecutorCpuLimitPercent,
};

pub const Config = struct {
    worker_concurrency: u32,
    drain_timeout_ms: u64,
    executor_socket_path: ?[]u8 = null,
    executor_startup_timeout_ms: u64 = 5_000,
    executor_lease_timeout_ms: u64 = 30_000,
    executor_memory_limit_mb: u64 = 512,
    executor_cpu_limit_percent: u64 = 100,
    alloc: std.mem.Allocator,

    pub fn load(alloc: std.mem.Allocator) !Config {
        const worker_concurrency = std.process.getEnvVarOwned(alloc, "WORKER_CONCURRENCY") catch {
            return Config{
                .worker_concurrency = 1,
                .drain_timeout_ms = 270_000,
                .alloc = alloc,
            };
        };
        defer alloc.free(worker_concurrency);
        const concurrency = std.fmt.parseInt(u32, worker_concurrency, 10) catch 1;

        const drain_timeout_ms = try parseU64Env(alloc, "DRAIN_TIMEOUT_MS", 270_000, ValidationError.InvalidDrainTimeoutMs);
        const executor_socket_path: ?[]u8 = std.process.getEnvVarOwned(alloc, "EXECUTOR_SOCKET_PATH") catch null;
        errdefer if (executor_socket_path) |p| alloc.free(p);
        const executor_startup_timeout_ms = try parseU64Env(alloc, "EXECUTOR_STARTUP_TIMEOUT_MS", 5_000, ValidationError.InvalidExecutorStartupTimeoutMs);
        const executor_lease_timeout_ms = try parseU64Env(alloc, "EXECUTOR_LEASE_TIMEOUT_MS", 30_000, ValidationError.InvalidExecutorLeaseTimeoutMs);
        const executor_memory_limit_mb = try parseU64Env(alloc, "EXECUTOR_MEMORY_LIMIT_MB", 512, ValidationError.InvalidExecutorMemoryLimitMb);
        const executor_cpu_limit_percent = try parseU64Env(alloc, "EXECUTOR_CPU_LIMIT_PERCENT", 100, ValidationError.InvalidExecutorCpuLimitPercent);

        if (executor_cpu_limit_percent == 0 or executor_cpu_limit_percent > 100) return ValidationError.InvalidExecutorCpuLimitPercent;

        return .{
            .worker_concurrency = concurrency,
            .drain_timeout_ms = drain_timeout_ms,
            .executor_socket_path = executor_socket_path,
            .executor_startup_timeout_ms = executor_startup_timeout_ms,
            .executor_lease_timeout_ms = executor_lease_timeout_ms,
            .executor_memory_limit_mb = executor_memory_limit_mb,
            .executor_cpu_limit_percent = executor_cpu_limit_percent,
            .alloc = alloc,
        };
    }

    pub fn deinit(self: *Config) void {
        if (self.executor_socket_path) |p| self.alloc.free(p);
    }
};

pub fn printValidationError(err: ValidationError) void {
    switch (err) {
        ValidationError.InvalidDrainTimeoutMs => std.debug.print("fatal: invalid DRAIN_TIMEOUT_MS value\n", .{}),
        ValidationError.InvalidExecutorStartupTimeoutMs => std.debug.print("fatal: invalid EXECUTOR_STARTUP_TIMEOUT_MS value\n", .{}),
        ValidationError.InvalidExecutorLeaseTimeoutMs => std.debug.print("fatal: invalid EXECUTOR_LEASE_TIMEOUT_MS value\n", .{}),
        ValidationError.InvalidExecutorMemoryLimitMb => std.debug.print("fatal: invalid EXECUTOR_MEMORY_LIMIT_MB value\n", .{}),
        ValidationError.InvalidExecutorCpuLimitPercent => std.debug.print("fatal: invalid EXECUTOR_CPU_LIMIT_PERCENT value (must be 1-100)\n", .{}),
    }
}

fn requiredEnvOwned(alloc: std.mem.Allocator, name: []const u8, missing_error: ValidationError) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch missing_error;
}

fn envOrDefaultOwned(alloc: std.mem.Allocator, name: []const u8, default_value: []const u8) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch try alloc.dupe(u8, default_value);
}

fn parseU32Env(alloc: std.mem.Allocator, name: []const u8, default_value: u32, invalid_error: ValidationError) !u32 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u32, raw, 10) catch invalid_error;
}

fn parseF64Env(alloc: std.mem.Allocator, name: []const u8, default_value: f64, invalid_error: ValidationError) !f64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseFloat(f64, raw) catch invalid_error;
}

fn parseU64Env(alloc: std.mem.Allocator, name: []const u8, default_value: u64, invalid_error: ValidationError) !u64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u64, raw, 10) catch invalid_error;
}

// --- Tests ---

test "Config.load succeeds with defaults" {
    std.posix.unsetenv("EXECUTOR_SOCKET_PATH");
    std.posix.unsetenv("DRAIN_TIMEOUT_MS");

    var cfg = try Config.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expectEqual(@as(u64, 270_000), cfg.drain_timeout_ms);
    try std.testing.expect(cfg.executor_socket_path == null);
}

test "Config.load rejects executor CPU limit above 100" {
    std.posix.setenv("EXECUTOR_CPU_LIMIT_PERCENT", "101", true) catch {};
    defer std.posix.unsetenv("EXECUTOR_CPU_LIMIT_PERCENT");

    try std.testing.expectError(ValidationError.InvalidExecutorCpuLimitPercent, Config.load(std.testing.allocator));
}

test "Config.load picks up custom DRAIN_TIMEOUT_MS" {
    std.posix.setenv("DRAIN_TIMEOUT_MS", "120000", true) catch {};
    defer std.posix.unsetenv("DRAIN_TIMEOUT_MS");

    var cfg = try Config.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expectEqual(@as(u64, 120_000), cfg.drain_timeout_ms);
}

test "Config.load rejects non-numeric DRAIN_TIMEOUT_MS" {
    std.posix.setenv("DRAIN_TIMEOUT_MS", "abc", true) catch {};
    defer std.posix.unsetenv("DRAIN_TIMEOUT_MS");

    try std.testing.expectError(ValidationError.InvalidDrainTimeoutMs, Config.load(std.testing.allocator));
}
