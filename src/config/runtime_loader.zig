// Sub-loaders for ServeConfig.load.
//
// Each loader returns an intermediate "slice" struct that the orchestrator
// in runtime.zig assembles into the final ServeConfig. Decomposing the
// 95-line load() body into per-concern loaders keeps every function under
// the 50-line cap and lets the orchestrator thread one errdefer per slice
// rather than one per individual heap allocation.
//
// Ownership: every []u8 returned in these structs is heap-owned. Callers
// must call the matching `freeX` helper on failure (orchestrator does this
// via errdefer) or transfer ownership into ServeConfig (which frees on
// deinit).

const std = @import("std");
const Allocator = std.mem.Allocator;
const oidc = @import("../auth/oidc.zig");

const runtime_types = @import("runtime_types.zig");
const env = @import("runtime_env_parse.zig");
const validate = @import("runtime_validate.zig");

const ValidationError = runtime_types.ValidationError;

const SizesConfig = struct {
    port: u16,
    api_http_threads: i16,
    api_http_workers: i16,
    api_max_clients: u32,
    api_max_in_flight_requests: u32,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
};

pub fn loadSizes(alloc: Allocator) !SizesConfig {
    const port = try env.parseU16Env(alloc, "PORT", 3000, ValidationError.InvalidPort);
    const threads = try env.parseI16Env(alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
    const workers = try env.parseI16Env(alloc, "API_HTTP_WORKERS", 1, ValidationError.InvalidApiHttpWorkers);
    const max_clients = try env.parseU32Env(alloc, "API_MAX_CLIENTS", 1024, ValidationError.InvalidApiMaxClients);
    const max_inflight = try env.parseU32Env(alloc, "API_MAX_IN_FLIGHT_REQUESTS", 256, ValidationError.InvalidApiMaxInFlightRequests);
    const queue_depth = try env.parseOptionalI64Env(alloc, "READY_MAX_QUEUE_DEPTH", ValidationError.InvalidReadyMaxQueueDepth);
    const queue_age = try env.parseOptionalI64Env(alloc, "READY_MAX_QUEUE_AGE_MS", ValidationError.InvalidReadyMaxQueueAgeMs);

    if (threads <= 0) return ValidationError.InvalidApiHttpThreads;
    if (workers <= 0) return ValidationError.InvalidApiHttpWorkers;
    if (max_clients == 0) return ValidationError.InvalidApiMaxClients;
    if (max_inflight == 0) return ValidationError.InvalidApiMaxInFlightRequests;
    if (queue_depth) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueDepth;
    if (queue_age) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueAgeMs;

    return .{
        .port = port,
        .api_http_threads = threads,
        .api_http_workers = workers,
        .api_max_clients = max_clients,
        .api_max_in_flight_requests = max_inflight,
        .ready_max_queue_depth = queue_depth,
        .ready_max_queue_age_ms = queue_age,
    };
}

const OidcConfig = struct {
    enabled: bool,
    provider: oidc.Provider,
    jwks_url: ?[]u8,
    issuer: ?[]u8,
    audience: ?[]u8,
};

pub fn loadOidc(alloc: Allocator) !OidcConfig {
    const jwks_url = std.process.getEnvVarOwned(alloc, "OIDC_JWKS_URL") catch null;
    errdefer if (jwks_url) |v| alloc.free(v);
    const issuer = std.process.getEnvVarOwned(alloc, "OIDC_ISSUER") catch null;
    errdefer if (issuer) |v| alloc.free(v);
    const audience = std.process.getEnvVarOwned(alloc, "OIDC_AUDIENCE") catch null;
    errdefer if (audience) |v| alloc.free(v);
    const provider_raw = std.process.getEnvVarOwned(alloc, "OIDC_PROVIDER") catch null;
    defer if (provider_raw) |v| alloc.free(v);

    const requested = jwks_url != null or issuer != null or audience != null or provider_raw != null;
    const enabled = if (jwks_url) |raw| std.mem.trim(u8, raw, " \t\r\n").len > 0 else false;
    if (requested and !enabled) return ValidationError.MissingOidcJwksUrl;

    const provider = if (provider_raw) |raw|
        oidc.parseProvider(std.mem.trim(u8, raw, " \t\r\n")) catch return ValidationError.InvalidOidcProvider
    else
        oidc.Provider.clerk;

    return .{ .enabled = enabled, .provider = provider, .jwks_url = jwks_url, .issuer = issuer, .audience = audience };
}

pub fn freeOidc(alloc: Allocator, cfg: OidcConfig) void {
    if (cfg.jwks_url) |v| alloc.free(v);
    if (cfg.issuer) |v| alloc.free(v);
    if (cfg.audience) |v| alloc.free(v);
}

const EncryptionConfig = struct {
    master_key: []u8,
};

pub fn loadEncryption(alloc: Allocator) !EncryptionConfig {
    const master_key = try env.requiredEnvOwned(alloc, "ENCRYPTION_MASTER_KEY", ValidationError.MissingEncryptionMasterKey);
    errdefer alloc.free(master_key);
    if (master_key.len != 64 or !validate.isHexString(master_key)) return ValidationError.InvalidEncryptionMasterKey;

    return .{ .master_key = master_key };
}

pub fn freeEncryption(alloc: Allocator, cfg: EncryptionConfig) void {
    alloc.free(cfg.master_key);
}

const MiscConfig = struct {
    app_url: []u8,
};

pub fn loadMisc(alloc: Allocator) !MiscConfig {
    const app_url = try env.envOrDefaultOwned(alloc, "APP_URL", "https://app.usezombie.com");
    return .{ .app_url = app_url };
}
