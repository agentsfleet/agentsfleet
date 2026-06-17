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
const test_issuer = "https://idp.example.com";
// test_session_code_pepper + test_audit_log_pepper are referenced by every
// ServeConfig.load test below; the loadAuthPeppers-specific tests live in
// runtime_pepper_loader_test.zig.

fn envOf(pairs: []const [2][]const u8) !common.env.Map {
    return common.env.fromPairs(std.testing.allocator, pairs);
}

test "ServeConfig.load accepts custom provider" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_ISSUER", test_issuer },
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
    // custom provider keeps its non-standard JWKS path: explicit override wins.
    try std.testing.expectEqualStrings(test_jwks_url, cfg.oidc_jwks_url.?);
}

test "ServeConfig.load rejects invalid provider deterministically" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", test_jwks_url },
        .{ "OIDC_ISSUER", test_issuer },
        .{ "OIDC_PROVIDER", "not-real" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidOidcProvider, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects an OIDC slate with a provider but no issuer" {
    // The enable-gate is the issuer now: a provider (or any OIDC var) without
    // OIDC_ISSUER is rejected — issuer is the single source of identity truth.
    var env_map = try envOf(&.{
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.MissingOidcIssuer, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load treats an empty OIDC_JWKS_URL as absent and derives from issuer" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", "" },
        .{ "OIDC_ISSUER", test_issuer },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    var cfg = try ServeConfig.load(&env_map, std.testing.allocator);
    defer cfg.deinit();

    try std.testing.expect(cfg.oidc_enabled);
    try std.testing.expectEqualStrings(test_jwks_url, cfg.oidc_jwks_url.?);
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
        .{ "OIDC_ISSUER", test_issuer },
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
        .{ "OIDC_ISSUER", test_issuer },
        .{ "ENCRYPTION_MASTER_KEY", "tooshort" },
    });
    defer env_map.deinit();

    try std.testing.expectError(ValidationError.InvalidEncryptionMasterKey, ServeConfig.load(&env_map, std.testing.allocator));
}

test "ServeConfig.load rejects non-hex encryption key" {
    var env_map = try envOf(&.{
        .{ "OIDC_ISSUER", test_issuer },
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
    // explicit override is retained verbatim alongside the stored issuer.
    try std.testing.expectEqualStrings(test_jwks_url, cfg.jwks_url.?);
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

// ── derive JWKS URL from issuer ──────────────────────────────────────────
//
// The issuer is the single source of identity truth; the JWKS URL is derived
// from it unless OIDC_JWKS_URL is explicitly set (override). These pin the
// derive / override / trailing-slash / enable-gate / runtime-doctor-parity
// behaviour so the issuer and key-source can never drift again.

test "loadOidc derives the JWKS URL from issuer when no override is set" {
    var env_map = try envOf(&.{
        .{ "OIDC_ISSUER", "https://clerk.agentsfleet.net" },
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    try std.testing.expect(cfg.enabled);
    // pin test: literal is the contract (the endpoint Clerk publishes keys at).
    try std.testing.expectEqualStrings("https://clerk.agentsfleet.net/.well-known/jwks.json", cfg.jwks_url.?);
}

test "loadOidc returns an explicit OIDC_JWKS_URL verbatim, overriding derivation" {
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", "https://custom.example.com/keys/jwks.json" },
        .{ "OIDC_ISSUER", "https://clerk.agentsfleet.net" },
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    // override wins — NOT the derived clerk.agentsfleet.net/.well-known path.
    try std.testing.expectEqualStrings("https://custom.example.com/keys/jwks.json", cfg.jwks_url.?);
}

test "loadOidc normalises an issuer trailing slash with no double slash" {
    var env_map = try envOf(&.{
        .{ "OIDC_ISSUER", "https://clerk.agentsfleet.net/" },
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    try std.testing.expectEqualStrings("https://clerk.agentsfleet.net/.well-known/jwks.json", cfg.jwks_url.?);
    // no `//` after the scheme — the single trailing slash was collapsed.
    try std.testing.expect(std.mem.indexOf(u8, cfg.jwks_url.?["https://".len..], "//") == null);
    // robustness: even multiple trailing slashes collapse (the goal is no `//` 404).
    const multi = (try oidc.resolveJwksUrl(std.testing.allocator, null, "https://clerk.agentsfleet.net///")).?;
    defer std.testing.allocator.free(multi);
    try std.testing.expectEqualStrings("https://clerk.agentsfleet.net/.well-known/jwks.json", multi);
}

test "loadOidc is enabled when only OIDC_ISSUER is present" {
    var env_map = try envOf(&.{
        .{ "OIDC_ISSUER", "https://clerk.agentsfleet.net" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, std.testing.allocator);
    defer loader.freeOidc(std.testing.allocator, cfg);
    try std.testing.expect(cfg.enabled);
    try std.testing.expect(cfg.jwks_url != null);
    try std.testing.expect(cfg.audience == null);
}

test "loadOidc rejects an OIDC slate that sets audience but no issuer" {
    var env_map = try envOf(&.{
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.MissingOidcIssuer, loader.loadOidc(&env_map, std.testing.allocator));
}

test "doctor and loader resolve the same JWKS URL from one issuer" {
    const alloc = std.testing.allocator;
    const issuer = "https://clerk.agentsfleet.net";
    var env_map = try envOf(&.{
        .{ "OIDC_ISSUER", issuer },
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
    });
    defer env_map.deinit();
    const cfg = try loader.loadOidc(&env_map, alloc);
    defer loader.freeOidc(alloc, cfg);
    // `doctor` probes the URL returned by the SAME shared helper — proving the
    // doctor can never test a different URL than the daemon will fetch.
    const doctor_url = (try oidc.resolveJwksUrl(alloc, null, issuer)).?;
    defer alloc.free(doctor_url);
    try std.testing.expectEqualStrings(cfg.jwks_url.?, doctor_url);
}

test "resolveJwksUrl leaks nothing when an allocation fails on either path" {
    // checkAllAllocationFailures fails each internal allocation in turn and asserts
    // the error return leaks nothing — the deterministic proof that the derive
    // (allocPrint) and override (dupe) paths own their memory correctly under OOM.
    const Probe = struct {
        fn run(alloc: std.mem.Allocator) !void {
            if (try oidc.resolveJwksUrl(alloc, null, "https://clerk.agentsfleet.net/")) |u| alloc.free(u); // derive
            if (try oidc.resolveJwksUrl(alloc, "https://custom.example.com/jwks.json", "https://x")) |u| alloc.free(u); // override
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Probe.run, .{});
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
