// Integration tests for ServeConfig.load via the runtime façade.
//
// Each test sets a deterministic env-var slate, calls load(), and verifies
// the populated ServeConfig matches expectation. setTestEnv/unsetTestEnv
// keep the harness shared. test "..." names are deliberately
// milestone-free (RULE TST-NAM).
//
// Test isolation: every test calls clearAllRuntimeEnv() before configuring
// its own env. setTestEnv mutates real process env, so a host shell setting
// (e.g. KEK_VERSION=2 in the developer's shell) or a leak from a prior test
// would otherwise pollute the next case. Belt + suspenders: clear at the
// start, the existing `defer unsetTestEnv` cleans the test's own keys.

const std = @import("std");
const oidc = @import("../auth/oidc.zig");
const runtime = @import("runtime.zig");
const loader = @import("runtime_loader.zig");

const ServeConfig = runtime.ServeConfig;
const ValidationError = runtime.ValidationError;

const test_encryption_master_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const ALL_RUNTIME_ENV_VARS = [_][]const u8{
    "PORT",                       "API_HTTP_THREADS",
    "API_HTTP_WORKERS",           "API_MAX_CLIENTS",
    "API_MAX_IN_FLIGHT_REQUESTS", "READY_MAX_QUEUE_DEPTH",
    "READY_MAX_QUEUE_AGE_MS",     "OIDC_JWKS_URL",
    "OIDC_ISSUER",                "OIDC_AUDIENCE",
    "OIDC_PROVIDER",              "API_KEY",
    "ENCRYPTION_MASTER_KEY",      "APP_URL",
};

fn clearAllRuntimeEnv() void {
    for (ALL_RUNTIME_ENV_VARS) |name| std.posix.unsetenv(name);
}

fn setTestEnv(env_pairs: []const [2][]const u8) !void {
    clearAllRuntimeEnv();
    for (env_pairs) |entry| try std.posix.setenv(entry[0], entry[1], true);
}

fn unsetTestEnv(env_pairs: []const [2][]const u8) void {
    for (env_pairs) |entry| std.posix.unsetenv(entry[0]);
}

test "ServeConfig.load accepts custom provider" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    var cfg = try ServeConfig.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expect(cfg.oidc_enabled);
    try std.testing.expectEqual(oidc.Provider.custom, cfg.oidc_provider);
}

test "ServeConfig.load rejects invalid provider deterministically" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_PROVIDER", "not-real" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.InvalidOidcProvider, ServeConfig.load(std.testing.allocator));
}

test "ServeConfig.load rejects provider without required OIDC_JWKS_URL" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.MissingOidcJwksUrl, ServeConfig.load(std.testing.allocator));
}

test "ServeConfig.load rejects empty OIDC_JWKS_URL" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.MissingOidcJwksUrl, ServeConfig.load(std.testing.allocator));
}

test "ServeConfig.load accepts api key only auth mode" {
    const env_pairs = [_][2][]const u8{
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    var cfg = try ServeConfig.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expect(!cfg.oidc_enabled);
    try std.testing.expectEqualStrings("dev-key", cfg.api_keys);
}

test "ServeConfig.load accepts oidc plus api key auth mode" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_PROVIDER", "custom" },
        .{ "API_KEY", "issued-key" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    var cfg = try ServeConfig.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expect(cfg.oidc_enabled);
    try std.testing.expectEqual(oidc.Provider.custom, cfg.oidc_provider);
    try std.testing.expectEqualStrings("issued-key", cfg.api_keys);
}

test "ServeConfig.load accepts whitespace api key as oidc-only when oidc enabled" {
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "API_KEY", "   " },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    var cfg = try ServeConfig.load(std.testing.allocator);
    defer cfg.deinit();
    try std.testing.expectEqual(@as(usize, 0), cfg.api_keys.len);
}

test "ServeConfig.load applies default port" {
    const env_pairs = [_][2][]const u8{
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    var cfg = try ServeConfig.load(std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
    try std.testing.expectEqual(@as(i16, 1), cfg.api_http_threads);
    try std.testing.expectEqual(@as(u32, 1024), cfg.api_max_clients);
}

test "ServeConfig.load rejects short encryption key" {
    const env_pairs = [_][2][]const u8{
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", "tooshort" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(std.testing.allocator));
}

test "ServeConfig.load rejects non-hex encryption key" {
    const env_pairs = [_][2][]const u8{
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(std.testing.allocator));
}

test "ServeConfig.load rejects negative READY_MAX_QUEUE_DEPTH" {
    const env_pairs = [_][2][]const u8{
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "READY_MAX_QUEUE_DEPTH", "-5" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    try std.testing.expectError(ValidationError.InvalidReadyMaxQueueDepth, ServeConfig.load(std.testing.allocator));
}

// ── per-loader unit tests ────────────────────────────────────────────────
//
// The split's payoff is per-concern testability. The tests above exercise
// load() end-to-end; the tests below hit each sub-loader directly so a
// future regression is localized to the loader that broke.

test "loadSizes rejects API_HTTP_THREADS=0" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "API_HTTP_THREADS", "0" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidApiHttpThreads, loader.loadSizes(alloc));
}

test "loadSizes rejects API_HTTP_WORKERS=-1" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "API_HTTP_WORKERS", "-1" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidApiHttpWorkers, loader.loadSizes(alloc));
}

test "loadSizes rejects API_MAX_CLIENTS=0" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "API_MAX_CLIENTS", "0" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidApiMaxClients, loader.loadSizes(alloc));
}

test "loadSizes rejects API_MAX_IN_FLIGHT_REQUESTS=0" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "API_MAX_IN_FLIGHT_REQUESTS", "0" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidApiMaxInFlightRequests, loader.loadSizes(alloc));
}

test "loadSizes rejects negative READY_MAX_QUEUE_AGE_MS" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "READY_MAX_QUEUE_AGE_MS", "-1" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidReadyMaxQueueAgeMs, loader.loadSizes(alloc));
}

test "loadSizes applies all defaults when env empty" {
    const alloc = std.testing.allocator;
    clearAllRuntimeEnv();
    const sizes = try loader.loadSizes(alloc);
    try std.testing.expectEqual(@as(u16, 3000), sizes.port);
    try std.testing.expectEqual(@as(i16, 1), sizes.api_http_threads);
    try std.testing.expectEqual(@as(i16, 1), sizes.api_http_workers);
    try std.testing.expectEqual(@as(u32, 1024), sizes.api_max_clients);
    try std.testing.expectEqual(@as(u32, 256), sizes.api_max_in_flight_requests);
    try std.testing.expect(sizes.ready_max_queue_depth == null);
    try std.testing.expect(sizes.ready_max_queue_age_ms == null);
}

test "loadOidc populates issuer and audience when set" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "zombied-prod" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    const cfg = try loader.loadOidc(alloc);
    defer loader.freeOidc(alloc, cfg);
    try std.testing.expect(cfg.enabled);
    try std.testing.expectEqualStrings("https://idp.example.com/", cfg.issuer.?);
    try std.testing.expectEqualStrings("zombied-prod", cfg.audience.?);
}

test "loadOidc returns disabled with all-null fields when env empty" {
    const alloc = std.testing.allocator;
    clearAllRuntimeEnv();
    const cfg = try loader.loadOidc(alloc);
    defer loader.freeOidc(alloc, cfg);
    try std.testing.expect(!cfg.enabled);
    try std.testing.expect(cfg.jwks_url == null);
    try std.testing.expect(cfg.issuer == null);
    try std.testing.expect(cfg.audience == null);
    try std.testing.expectEqual(oidc.Provider.clerk, cfg.provider);
}

test "ServeConfig.load partial-build frees oidc + api_keys when encryption rejected (RULE OWN)" {
    // Proves the orchestrator's per-section errdefer chain frees every prior
    // heap-owning section when a late sub-loader fails. Loads valid OIDC +
    // API_KEY (each allocates), then forces loadEncryption to fail via a
    // wrong-length ENCRYPTION_MASTER_KEY. std.testing.allocator panics on
    // any leak, so a clean exit means the chain is intact.
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "zombied-prod" },
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", "tooshort" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(std.testing.allocator));
}

test "loadSizes rejects PORT overflow (>u16 max)" {
    const alloc = std.testing.allocator;
    const env_pairs = [_][2][]const u8{.{ "PORT", "70000" }};
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidPort, loader.loadSizes(alloc));
}
