//! Runtime config loader/validator for zombied serve mode.
//! Centralizes env parsing and validation in one place.

const std = @import("std");

pub const ValidationError = error{
    MissingApiKey,
    InvalidApiKeyList,
    MissingClerkJwksUrl,
    MissingEncryptionMasterKey,
    InvalidEncryptionMasterKey,
    MissingGitHubAppId,
    MissingGitHubAppPrivateKey,
    InvalidPort,
    InvalidMaxAttempts,
    InvalidWorkerConcurrency,
    InvalidApiHttpThreads,
    InvalidApiHttpWorkers,
    InvalidApiMaxClients,
    InvalidApiMaxInFlightRequests,
    InvalidRateLimitCapacity,
    InvalidRateLimitRefillPerSec,
    InvalidRunTimeoutMs,
    InvalidReadyMaxQueueDepth,
    InvalidReadyMaxQueueAgeMs,
};

pub const ServeConfig = struct {
    port: u16,
    api_keys: []u8,
    cache_root: []u8,
    github_app_id: []u8,
    github_app_private_key: []u8,
    config_dir: []u8,
    pipeline_profile_path: []u8,
    max_attempts: u32,
    worker_concurrency: u32,
    api_http_threads: i16,
    api_http_workers: i16,
    api_max_clients: u32,
    api_max_in_flight_requests: u32,
    run_timeout_ms: u64,
    rate_limit_capacity: u32,
    rate_limit_refill_per_sec: f64,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
    app_url: []u8,
    clerk_enabled: bool,
    clerk_jwks_url: ?[]u8,
    clerk_issuer: ?[]u8,
    clerk_audience: ?[]u8,
    encryption_master_key: []u8,

    alloc: std.mem.Allocator,

    pub fn load(alloc: std.mem.Allocator) !ServeConfig {
        const port = try parseU16Env(alloc, "PORT", 3000, ValidationError.InvalidPort);
        const max_attempts = try parseU32Env(alloc, "DEFAULT_MAX_ATTEMPTS", 3, ValidationError.InvalidMaxAttempts);
        const worker_concurrency = try parseU32Env(alloc, "WORKER_CONCURRENCY", 1, ValidationError.InvalidWorkerConcurrency);
        const api_http_threads = try parseI16Env(alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
        const api_http_workers = try parseI16Env(alloc, "API_HTTP_WORKERS", 1, ValidationError.InvalidApiHttpWorkers);
        const api_max_clients = try parseU32Env(alloc, "API_MAX_CLIENTS", 1024, ValidationError.InvalidApiMaxClients);
        const api_max_in_flight_requests = try parseU32Env(alloc, "API_MAX_IN_FLIGHT_REQUESTS", 256, ValidationError.InvalidApiMaxInFlightRequests);
        const run_timeout_ms = try parseU64Env(alloc, "RUN_TIMEOUT_MS", 300_000, ValidationError.InvalidRunTimeoutMs);
        const rate_limit_capacity = try parseU32Env(alloc, "RATE_LIMIT_CAPACITY", 30, ValidationError.InvalidRateLimitCapacity);
        const rate_limit_refill_per_sec = try parseF64Env(alloc, "RATE_LIMIT_REFILL_PER_SEC", 5.0, ValidationError.InvalidRateLimitRefillPerSec);
        const ready_max_queue_depth = try parseOptionalI64Env(alloc, "READY_MAX_QUEUE_DEPTH", ValidationError.InvalidReadyMaxQueueDepth);
        const ready_max_queue_age_ms = try parseOptionalI64Env(alloc, "READY_MAX_QUEUE_AGE_MS", ValidationError.InvalidReadyMaxQueueAgeMs);

        if (max_attempts == 0) return ValidationError.InvalidMaxAttempts;
        if (worker_concurrency == 0) return ValidationError.InvalidWorkerConcurrency;
        if (api_http_threads <= 0) return ValidationError.InvalidApiHttpThreads;
        if (api_http_workers <= 0) return ValidationError.InvalidApiHttpWorkers;
        if (api_max_clients == 0) return ValidationError.InvalidApiMaxClients;
        if (api_max_in_flight_requests == 0) return ValidationError.InvalidApiMaxInFlightRequests;
        if (run_timeout_ms == 0) return ValidationError.InvalidRunTimeoutMs;
        if (rate_limit_capacity == 0) return ValidationError.InvalidRateLimitCapacity;
        if (!(rate_limit_refill_per_sec > 0)) return ValidationError.InvalidRateLimitRefillPerSec;
        if (ready_max_queue_depth) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueDepth;
        if (ready_max_queue_age_ms) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueAgeMs;

        const clerk_secret_key = std.process.getEnvVarOwned(alloc, "CLERK_SECRET_KEY") catch null;
        const clerk_enabled = if (clerk_secret_key) |raw| blk: {
            defer alloc.free(raw);
            break :blk std.mem.trim(u8, raw, " \t\r\n").len > 0;
        } else false;

        const api_keys = if (clerk_enabled)
            std.process.getEnvVarOwned(alloc, "API_KEY") catch try alloc.dupe(u8, "")
        else
            try requiredEnvOwned(alloc, "API_KEY", ValidationError.MissingApiKey);
        errdefer alloc.free(api_keys);
        if (!clerk_enabled and !hasUsableApiKey(api_keys)) return ValidationError.InvalidApiKeyList;

        const encryption_master_key = try requiredEnvOwned(alloc, "ENCRYPTION_MASTER_KEY", ValidationError.MissingEncryptionMasterKey);
        errdefer alloc.free(encryption_master_key);
        if (encryption_master_key.len != 64 or !isHexString(encryption_master_key)) {
            return ValidationError.InvalidEncryptionMasterKey;
        }

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

        const clerk_jwks_url = if (clerk_enabled)
            try requiredEnvOwned(alloc, "CLERK_JWKS_URL", ValidationError.MissingClerkJwksUrl)
        else
            std.process.getEnvVarOwned(alloc, "CLERK_JWKS_URL") catch null;
        errdefer if (clerk_jwks_url) |v| alloc.free(v);
        const clerk_issuer = std.process.getEnvVarOwned(alloc, "CLERK_ISSUER") catch null;
        errdefer if (clerk_issuer) |v| alloc.free(v);
        const clerk_audience = std.process.getEnvVarOwned(alloc, "CLERK_AUDIENCE") catch null;
        errdefer if (clerk_audience) |v| alloc.free(v);

        const app_url = try envOrDefaultOwned(alloc, "APP_URL", "https://app.usezombie.com");
        errdefer alloc.free(app_url);

        return .{
            .port = port,
            .api_keys = api_keys,
            .cache_root = cache_root,
            .github_app_id = github_app_id,
            .github_app_private_key = github_app_private_key,
            .config_dir = config_dir,
            .pipeline_profile_path = pipeline_profile_path,
            .max_attempts = max_attempts,
            .worker_concurrency = worker_concurrency,
            .api_http_threads = api_http_threads,
            .api_http_workers = api_http_workers,
            .api_max_clients = api_max_clients,
            .api_max_in_flight_requests = api_max_in_flight_requests,
            .run_timeout_ms = run_timeout_ms,
            .rate_limit_capacity = rate_limit_capacity,
            .rate_limit_refill_per_sec = rate_limit_refill_per_sec,
            .ready_max_queue_depth = ready_max_queue_depth,
            .ready_max_queue_age_ms = ready_max_queue_age_ms,
            .app_url = app_url,
            .clerk_enabled = clerk_enabled,
            .clerk_jwks_url = clerk_jwks_url,
            .clerk_issuer = clerk_issuer,
            .clerk_audience = clerk_audience,
            .encryption_master_key = encryption_master_key,
            .alloc = alloc,
        };
    }

    pub fn deinit(self: *ServeConfig) void {
        self.alloc.free(self.api_keys);
        self.alloc.free(self.cache_root);
        self.alloc.free(self.github_app_id);
        self.alloc.free(self.github_app_private_key);
        self.alloc.free(self.config_dir);
        self.alloc.free(self.pipeline_profile_path);
        self.alloc.free(self.app_url);
        if (self.clerk_jwks_url) |v| self.alloc.free(v);
        if (self.clerk_issuer) |v| self.alloc.free(v);
        if (self.clerk_audience) |v| self.alloc.free(v);
        self.alloc.free(self.encryption_master_key);
    }

    pub fn printValidationError(err: ValidationError) void {
        switch (err) {
            ValidationError.MissingApiKey => std.debug.print("fatal: API_KEY not set\n", .{}),
            ValidationError.InvalidApiKeyList => std.debug.print("fatal: API_KEY has no usable keys\n", .{}),
            ValidationError.MissingClerkJwksUrl => std.debug.print("fatal: CLERK_JWKS_URL not set while CLERK_SECRET_KEY is configured\n", .{}),
            ValidationError.MissingEncryptionMasterKey => std.debug.print("fatal: ENCRYPTION_MASTER_KEY not set\n", .{}),
            ValidationError.InvalidEncryptionMasterKey => std.debug.print("fatal: ENCRYPTION_MASTER_KEY must be 64 hex chars\n", .{}),
            ValidationError.MissingGitHubAppId => std.debug.print("fatal: GITHUB_APP_ID not set\n", .{}),
            ValidationError.MissingGitHubAppPrivateKey => std.debug.print("fatal: GITHUB_APP_PRIVATE_KEY not set\n", .{}),
            ValidationError.InvalidPort => std.debug.print("fatal: invalid PORT value\n", .{}),
            ValidationError.InvalidMaxAttempts => std.debug.print("fatal: invalid DEFAULT_MAX_ATTEMPTS value\n", .{}),
            ValidationError.InvalidWorkerConcurrency => std.debug.print("fatal: invalid WORKER_CONCURRENCY value\n", .{}),
            ValidationError.InvalidApiHttpThreads => std.debug.print("fatal: invalid API_HTTP_THREADS value\n", .{}),
            ValidationError.InvalidApiHttpWorkers => std.debug.print("fatal: invalid API_HTTP_WORKERS value\n", .{}),
            ValidationError.InvalidApiMaxClients => std.debug.print("fatal: invalid API_MAX_CLIENTS value\n", .{}),
            ValidationError.InvalidApiMaxInFlightRequests => std.debug.print("fatal: invalid API_MAX_IN_FLIGHT_REQUESTS value\n", .{}),
            ValidationError.InvalidRunTimeoutMs => std.debug.print("fatal: invalid RUN_TIMEOUT_MS value\n", .{}),
            ValidationError.InvalidRateLimitCapacity => std.debug.print("fatal: invalid RATE_LIMIT_CAPACITY value\n", .{}),
            ValidationError.InvalidRateLimitRefillPerSec => std.debug.print("fatal: invalid RATE_LIMIT_REFILL_PER_SEC value\n", .{}),
            ValidationError.InvalidReadyMaxQueueDepth => std.debug.print("fatal: invalid READY_MAX_QUEUE_DEPTH value\n", .{}),
            ValidationError.InvalidReadyMaxQueueAgeMs => std.debug.print("fatal: invalid READY_MAX_QUEUE_AGE_MS value\n", .{}),
        }
    }
};

fn requiredEnvOwned(alloc: std.mem.Allocator, name: []const u8, missing_error: ValidationError) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch missing_error;
}

fn envOrDefaultOwned(alloc: std.mem.Allocator, name: []const u8, default_value: []const u8) ![]u8 {
    return std.process.getEnvVarOwned(alloc, name) catch try alloc.dupe(u8, default_value);
}

fn parseU16Env(alloc: std.mem.Allocator, name: []const u8, default_value: u16, invalid_error: ValidationError) !u16 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u16, raw, 10) catch invalid_error;
}

fn parseU32Env(alloc: std.mem.Allocator, name: []const u8, default_value: u32, invalid_error: ValidationError) !u32 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(u32, raw, 10) catch invalid_error;
}

fn parseI16Env(alloc: std.mem.Allocator, name: []const u8, default_value: i16, invalid_error: ValidationError) !i16 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return default_value;
    defer alloc.free(raw);
    return std.fmt.parseInt(i16, raw, 10) catch invalid_error;
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

fn parseOptionalI64Env(alloc: std.mem.Allocator, name: []const u8, invalid_error: ValidationError) !?i64 {
    const raw = std.process.getEnvVarOwned(alloc, name) catch return null;
    defer alloc.free(raw);
    return std.fmt.parseInt(i64, raw, 10) catch invalid_error;
}

fn isHexString(s: []const u8) bool {
    for (s) |ch| {
        if (!std.ascii.isHex(ch)) return false;
    }
    return true;
}

fn hasUsableApiKey(list: []const u8) bool {
    var it = std.mem.tokenizeScalar(u8, list, ',');
    while (it.next()) |candidate_raw| {
        if (std.mem.trim(u8, candidate_raw, " \t").len > 0) return true;
    }
    return false;
}

test "hasUsableApiKey validates rotation list" {
    try std.testing.expect(hasUsableApiKey("key1,key2"));
    try std.testing.expect(hasUsableApiKey(" key1 "));
    try std.testing.expect(!hasUsableApiKey(""));
    try std.testing.expect(!hasUsableApiKey(" , , "));
}

test "isHexString validates encryption key format" {
    try std.testing.expect(isHexString("abcdef0123"));
    try std.testing.expect(!isHexString("abcxyz"));
}

test "parseI16Env parses signed short values" {
    const alloc = std.testing.allocator;
    try std.posix.setenv("API_HTTP_THREADS", "3", true);
    defer std.posix.unsetenv("API_HTTP_THREADS");

    const value = try parseI16Env(alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
    try std.testing.expectEqual(@as(i16, 3), value);
}
