const std = @import("std");
const sandbox_runtime = @import("../pipeline/sandbox_runtime.zig");

pub const ValidationError = error{
    MissingGitHubAppId,
    MissingGitHubAppPrivateKey,
    InvalidMaxAttempts,
    InvalidWorkerConcurrency,
    InvalidRunTimeoutMs,
    InvalidRateLimitCapacity,
    InvalidRateLimitRefillPerSec,
    InvalidSandboxBackend,
    InvalidSandboxKillGraceMs,
};

pub const Config = struct {
    cache_root: []u8,
    github_app_id: []u8,
    github_app_private_key: []u8,
    config_dir: []u8,
    pipeline_profile_path: []u8,
    max_attempts: u32,
    worker_concurrency: u32,
    run_timeout_ms: u64,
    sandbox: sandbox_runtime.Config,
    rate_limit_capacity: u32,
    rate_limit_refill_per_sec: f64,
    alloc: std.mem.Allocator,

    pub fn load(alloc: std.mem.Allocator) !Config {
        const max_attempts = try parseU32Env(alloc, "DEFAULT_MAX_ATTEMPTS", 3, ValidationError.InvalidMaxAttempts);
        const worker_concurrency = try parseU32Env(alloc, "WORKER_CONCURRENCY", 1, ValidationError.InvalidWorkerConcurrency);
        const run_timeout_ms = try parseU64Env(alloc, "RUN_TIMEOUT_MS", 300_000, ValidationError.InvalidRunTimeoutMs);
        const rate_limit_capacity = try parseU32Env(alloc, "RATE_LIMIT_CAPACITY", 30, ValidationError.InvalidRateLimitCapacity);
        const rate_limit_refill_per_sec = try parseF64Env(alloc, "RATE_LIMIT_REFILL_PER_SEC", 5.0, ValidationError.InvalidRateLimitRefillPerSec);
        const sandbox = sandbox_runtime.loadFromEnv(alloc) catch |err| switch (err) {
            sandbox_runtime.ValidationError.InvalidSandboxBackend => return ValidationError.InvalidSandboxBackend,
            sandbox_runtime.ValidationError.InvalidSandboxKillGraceMs => return ValidationError.InvalidSandboxKillGraceMs,
            else => return err,
        };

        if (max_attempts == 0) return ValidationError.InvalidMaxAttempts;
        if (worker_concurrency == 0) return ValidationError.InvalidWorkerConcurrency;
        if (run_timeout_ms == 0) return ValidationError.InvalidRunTimeoutMs;
        if (rate_limit_capacity == 0) return ValidationError.InvalidRateLimitCapacity;
        if (!(rate_limit_refill_per_sec > 0)) return ValidationError.InvalidRateLimitRefillPerSec;

        const github_app_id = try requiredEnvOwned(alloc, "GITHUB_APP_ID", ValidationError.MissingGitHubAppId);
        errdefer alloc.free(github_app_id);
        if (github_app_id.len == 0) return ValidationError.MissingGitHubAppId;

        const github_app_private_key = try requiredEnvOwned(alloc, "GITHUB_APP_PRIVATE_KEY", ValidationError.MissingGitHubAppPrivateKey);
        errdefer alloc.free(github_app_private_key);
        if (github_app_private_key.len == 0) return ValidationError.MissingGitHubAppPrivateKey;

        const cache_root = try envOrDefaultOwned(alloc, "GIT_CACHE_ROOT", "/tmp/zombie-git-cache");
        errdefer alloc.free(cache_root);
        const config_dir = try envOrDefaultOwned(alloc, "AGENT_CONFIG_DIR", "./config");
        errdefer alloc.free(config_dir);
        const pipeline_profile_path = try envOrDefaultOwned(alloc, "PIPELINE_PROFILE_PATH", "./config/pipeline-default.json");
        errdefer alloc.free(pipeline_profile_path);

        return .{
            .cache_root = cache_root,
            .github_app_id = github_app_id,
            .github_app_private_key = github_app_private_key,
            .config_dir = config_dir,
            .pipeline_profile_path = pipeline_profile_path,
            .max_attempts = max_attempts,
            .worker_concurrency = worker_concurrency,
            .run_timeout_ms = run_timeout_ms,
            .sandbox = sandbox,
            .rate_limit_capacity = rate_limit_capacity,
            .rate_limit_refill_per_sec = rate_limit_refill_per_sec,
            .alloc = alloc,
        };
    }

    pub fn deinit(self: *Config) void {
        self.alloc.free(self.cache_root);
        self.alloc.free(self.github_app_id);
        self.alloc.free(self.github_app_private_key);
        self.alloc.free(self.config_dir);
        self.alloc.free(self.pipeline_profile_path);
    }
};

pub fn printValidationError(err: ValidationError) void {
    switch (err) {
        ValidationError.MissingGitHubAppId => std.debug.print("fatal: GITHUB_APP_ID not set\n", .{}),
        ValidationError.MissingGitHubAppPrivateKey => std.debug.print("fatal: GITHUB_APP_PRIVATE_KEY not set\n", .{}),
        ValidationError.InvalidMaxAttempts => std.debug.print("fatal: invalid DEFAULT_MAX_ATTEMPTS value\n", .{}),
        ValidationError.InvalidWorkerConcurrency => std.debug.print("fatal: invalid WORKER_CONCURRENCY value\n", .{}),
        ValidationError.InvalidRunTimeoutMs => std.debug.print("fatal: invalid RUN_TIMEOUT_MS value\n", .{}),
        ValidationError.InvalidRateLimitCapacity => std.debug.print("fatal: invalid RATE_LIMIT_CAPACITY value\n", .{}),
        ValidationError.InvalidRateLimitRefillPerSec => std.debug.print("fatal: invalid RATE_LIMIT_REFILL_PER_SEC value\n", .{}),
        ValidationError.InvalidSandboxBackend => std.debug.print("fatal: invalid SANDBOX_BACKEND value\n", .{}),
        ValidationError.InvalidSandboxKillGraceMs => std.debug.print("fatal: invalid SANDBOX_KILL_GRACE_MS value\n", .{}),
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
