// Integration tests for ServeConfig.load via the runtime façade.
//
// Each test builds a hermetic env map via `common.env.fromPairs` (the Zig
// 0.16 env-DI seam — load() reads only the injected map, never the process
// environment), calls load(), and verifies the populated ServeConfig.
// test "..." names are deliberately milestone-free (RULE TST-NAM).

const std = @import("std");
const common = @import("common");
const oidc = @import("../auth/oidc.zig");
const runtime = @import("runtime.zig");
const loader = @import("runtime_loader.zig");
const DEFAULT_MAX_CLIENTS = 1024;
const DEFAULT_MAX_IN_FLIGHT = 256;

const ServeConfig = runtime.ServeConfig;
const ValidationError = runtime.ValidationError;

const test_encryption_master_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const test_session_code_pepper = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const test_audit_log_pepper = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const test_jwks_url = "https://idp.example.com/.well-known/jwks.json";
// test_session_code_pepper + test_audit_log_pepper are referenced by every
// ServeConfig.load test below; the loadAuthPeppers-specific tests live in
// runtime_pepper_loader_test.zig.

fn envOf(pairs: []const [2][]const u8) !common.env.Map {
    return common.env.fromPairs(std.testing.allocator, pairs);
}

test "ServeConfig.load accepts custom provider" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    var cfg = try ServeConfig.load(&env_map, std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expect(cfg.oidc_enabled);
    try std.testing.expectEqual(oidc.Provider.custom, cfg.oidc_provider);
}

test "ServeConfig.load rejects invalid provider deterministically" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_PROVIDER", "not-real" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidOidcProvider, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects provider without required OIDC_JWKS_URL" {
    var env_map = try envOf(&.{
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.MissingOidcJwksUrl, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects empty OIDC_JWKS_URL" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", "" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.MissingOidcJwksUrl, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects a slate without any OIDC config" {
    // OIDC is mandatory — the env-var API-key bootstrap was removed.
    var env_map = try envOf(&.{
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.OidcRequired, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load applies size defaults; SSE cap independent of the thread pool" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    var cfg = try ServeConfig.load(&env_map, std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
    try std.testing.expectEqual(@as(i16, 1), cfg.api_http_threads);
    try std.testing.expectEqual(@as(u32, DEFAULT_MAX_CLIENTS), cfg.api_max_clients);
    // Streams run on dedicated detached threads, so the cap holds its default
    // even on a 1-thread handler pool — no pool relation, no clamp.
    try std.testing.expectEqual(loader.SSE_MAX_STREAMS_DEFAULT, cfg.sse_max_streams);
}

test "ServeConfig.load rejects short encryption key" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "ENCRYPTION_MASTER_KEY", "tooshort" },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects non-hex encryption key" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "ENCRYPTION_MASTER_KEY", "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg" },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects negative READY_MAX_QUEUE_DEPTH" {
    // loadSizes runs first, so no OIDC slate is needed to reach this error.
    var env_map = try envOf(&.{
        .{ "READY_MAX_QUEUE_DEPTH", "-5" },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidReadyMaxQueueDepth, ServeConfig.load(&env_map, std.testing.allocator));
}

// ── per-loader unit tests ────────────────────────────────────────────────
//
// The split's payoff is per-concern testability. The tests above exercise
// load() end-to-end; the tests below hit each sub-loader directly so a
// future regression is localized to the loader that broke.

test "loadSizes rejects API_HTTP_THREADS=0" {
    var env_map = try envOf(&.{.{ "API_HTTP_THREADS", "0" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidApiHttpThreads, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes rejects API_HTTP_WORKERS=-1" {
    var env_map = try envOf(&.{.{ "API_HTTP_WORKERS", "-1" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidApiHttpWorkers, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes rejects API_MAX_CLIENTS=0" {
    var env_map = try envOf(&.{.{ "API_MAX_CLIENTS", "0" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidApiMaxClients, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes rejects API_MAX_IN_FLIGHT_REQUESTS=0" {
    var env_map = try envOf(&.{.{ "API_MAX_IN_FLIGHT_REQUESTS", "0" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidApiMaxInFlightRequests, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes rejects negative READY_MAX_QUEUE_AGE_MS" {
    var env_map = try envOf(&.{.{ "READY_MAX_QUEUE_AGE_MS", "-1" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidReadyMaxQueueAgeMs, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes applies all defaults when env empty" {
    var env_map = try envOf(&.{});
    defer env_map.deinit();
    const sizes = try loader.loadSizes(&env_map, std.testing.allocator);
    try std.testing.expectEqual(@as(u16, 3000), sizes.port);
    try std.testing.expectEqual(@as(i16, 1), sizes.api_http_threads);
    try std.testing.expectEqual(@as(i16, 1), sizes.api_http_workers);
    try std.testing.expectEqual(@as(u32, DEFAULT_MAX_CLIENTS), sizes.api_max_clients);
    try std.testing.expectEqual(@as(u32, DEFAULT_MAX_IN_FLIGHT), sizes.api_max_in_flight_requests);
    try std.testing.expectEqual(loader.SSE_MAX_STREAMS_DEFAULT, sizes.sse_max_streams);
    try std.testing.expect(sizes.ready_max_queue_depth == null);
    try std.testing.expect(sizes.ready_max_queue_age_ms == null);
}

test "loadSizes honors an SSE_MAX_STREAMS override" {
    var env_map = try envOf(&.{.{ "SSE_MAX_STREAMS", "200" }});
    defer env_map.deinit();
    const sizes = try loader.loadSizes(&env_map, std.testing.allocator);
    try std.testing.expectEqual(@as(u32, 200), sizes.sse_max_streams);
}

test "loadSizes rejects SSE_MAX_STREAMS=0" {
    var env_map = try envOf(&.{.{ "SSE_MAX_STREAMS", "0" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidSseMaxStreams, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes rejects garbage SSE_MAX_STREAMS" {
    var env_map = try envOf(&.{.{ "SSE_MAX_STREAMS", "lots" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidSseMaxStreams, loader.loadSizes(&env_map, std.testing.allocator));
}

test "loadSizes keeps the SSE cap independent of a tiny thread pool" {
    var env_map = try envOf(&.{.{ "API_HTTP_THREADS", "1" }});
    defer env_map.deinit();
    const sizes = try loader.loadSizes(&env_map, std.testing.allocator);
    try std.testing.expectEqual(loader.SSE_MAX_STREAMS_DEFAULT, sizes.sse_max_streams);
}

test "loadOidc populates issuer and audience when set" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "agentsfleetd-prod" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    try std.testing.expect(cfg.enabled);
    try std.testing.expectEqualStrings("https://idp.example.com/", cfg.issuer.?);
    try std.testing.expectEqualStrings("agentsfleetd-prod", cfg.audience.?);
}

test "loadOidc returns disabled with all-null fields when env empty" {
    var env_map = try envOf(&.{});
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    try std.testing.expect(!cfg.enabled);
    try std.testing.expect(cfg.jwks_url == null);
    try std.testing.expect(cfg.issuer == null);
    try std.testing.expect(cfg.audience == null);
    try std.testing.expectEqual(oidc.Provider.clerk, cfg.provider);
}

test "ServeConfig.load partial-build frees oidc when encryption rejected (RULE OWN)" {
    // Proves the orchestrator's per-section errdefer chain frees every prior
    // heap-owning section when a late sub-loader fails. Loads valid OIDC
    // (allocates jwks/issuer/audience), then forces loadEncryption to fail
    // via a wrong-length ENCRYPTION_MASTER_KEY. std.testing.allocator panics
    // on any leak, so a clean exit means the chain is intact. The
    // peppers-rejected variant lives in runtime_pepper_loader_test.zig.
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "agentsfleetd-prod" },
        .{ "ENCRYPTION_MASTER_KEY", "tooshort" },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(&env_map, std.testing.allocator));
}

test "loadSizes rejects PORT overflow (>u16 max)" {
    var env_map = try envOf(&.{.{ "PORT", "70000" }});
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidPort, loader.loadSizes(&env_map, std.testing.allocator));
}

// loadAuthPeppers tests live in runtime_pepper_loader_test.zig — extracted
// to keep this file reviewable. Discovery happens via the test {} block in
// runtime.zig.
