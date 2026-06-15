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
const common = @import("common");
const Allocator = std.mem.Allocator;
const EnvMap = common.env.Map;
const oidc = @import("../auth/oidc.zig");

const runtime_types = @import("runtime_types.zig");
const env = @import("runtime_env_parse.zig");
const validate = @import("runtime_validate.zig");

const ValidationError = runtime_types.ValidationError;

const S_T_R_N = " \t\r\n";

/// Default ceiling on concurrent Server-Sent-Events streams per instance,
/// overridable via SSE_MAX_STREAMS (0 rejected). Each live stream costs one
/// DEDICATED detached thread (16 MiB virtual stack, ~128 KiB committed), one
/// client fd, and a 64-frame bounded queue (~64 KiB worst case) — Redis is
/// the SubscriptionHub's ONE shared pub/sub connection process-wide, never
/// per-stream — so ~0.25 MiB per stream, ~16 MiB at the default on the 4 GB
/// prod box. Streams never occupy handler-pool threads (httpz
/// `startEventStream` spawns the dedicated thread — events_stream.zig module
/// header has the full story). The empirical ceiling is Redis fan-out CPU,
/// not memory — the M88-gated load test refines it, fed by the
/// zombie_sse_in_flight_streams gauge.
pub const SSE_MAX_STREAMS_DEFAULT: u32 = 64;

const SizesConfig = struct {
    port: u16,
    api_http_threads: i16,
    api_http_workers: i16,
    api_max_clients: u32,
    api_max_in_flight_requests: u32,
    sse_max_streams: u32,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
};

pub fn loadSizes(env_map: *const EnvMap, alloc: Allocator) !SizesConfig {
    const port = try env.parseU16Env(env_map, alloc, "PORT", 3000, ValidationError.InvalidPort);
    const threads = try env.parseI16Env(env_map, alloc, "API_HTTP_THREADS", 1, ValidationError.InvalidApiHttpThreads);
    const workers = try env.parseI16Env(env_map, alloc, "API_HTTP_WORKERS", 1, ValidationError.InvalidApiHttpWorkers);
    const max_clients = try env.parseU32Env(env_map, alloc, "API_MAX_CLIENTS", 1024, ValidationError.InvalidApiMaxClients);
    const max_inflight = try env.parseU32Env(env_map, alloc, "API_MAX_IN_FLIGHT_REQUESTS", 256, ValidationError.InvalidApiMaxInFlightRequests);
    const sse_max_streams = try env.parseU32Env(env_map, alloc, "SSE_MAX_STREAMS", SSE_MAX_STREAMS_DEFAULT, ValidationError.InvalidSseMaxStreams);
    const queue_depth = try env.parseOptionalI64Env(env_map, alloc, "READY_MAX_QUEUE_DEPTH", ValidationError.InvalidReadyMaxQueueDepth);
    const queue_age = try env.parseOptionalI64Env(env_map, alloc, "READY_MAX_QUEUE_AGE_MS", ValidationError.InvalidReadyMaxQueueAgeMs);

    if (threads <= 0) return ValidationError.InvalidApiHttpThreads;
    if (workers <= 0) return ValidationError.InvalidApiHttpWorkers;
    if (max_clients == 0) return ValidationError.InvalidApiMaxClients;
    if (max_inflight == 0) return ValidationError.InvalidApiMaxInFlightRequests;
    if (sse_max_streams == 0) return ValidationError.InvalidSseMaxStreams;
    if (queue_depth) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueDepth;
    if (queue_age) |v| if (v <= 0) return ValidationError.InvalidReadyMaxQueueAgeMs;

    return .{
        .port = port,
        .api_http_threads = threads,
        .api_http_workers = workers,
        .api_max_clients = max_clients,
        .api_max_in_flight_requests = max_inflight,
        .sse_max_streams = sse_max_streams,
        .ready_max_queue_depth = queue_depth,
        .ready_max_queue_age_ms = queue_age,
    };
}

const OidcConfig = struct {
    enabled: bool,
    provider: oidc.Provider,
    jwks_url: ?[]const u8,
    issuer: ?[]const u8,
    audience: ?[]const u8,
};

pub fn loadOidc(env_map: *const EnvMap, alloc: Allocator) !OidcConfig {
    const jwks_url = try common.env.owned(env_map, alloc, "OIDC_JWKS_URL");
    errdefer if (jwks_url) |v| alloc.free(v);
    const issuer = try common.env.owned(env_map, alloc, "OIDC_ISSUER");
    errdefer if (issuer) |v| alloc.free(v);
    const audience = try common.env.owned(env_map, alloc, "OIDC_AUDIENCE");
    errdefer if (audience) |v| alloc.free(v);
    const provider_raw = try common.env.owned(env_map, alloc, "OIDC_PROVIDER");
    defer if (provider_raw) |v| alloc.free(v);

    const requested = jwks_url != null or issuer != null or audience != null or provider_raw != null;
    const enabled = if (jwks_url) |raw| std.mem.trim(u8, raw, S_T_R_N).len > 0 else false;
    if (requested and !enabled) return ValidationError.MissingOidcJwksUrl;

    const provider = if (provider_raw) |raw|
        oidc.parseProvider(std.mem.trim(u8, raw, S_T_R_N)) catch return ValidationError.InvalidOidcProvider
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
    master_key: []const u8,
};

pub fn loadEncryption(env_map: *const EnvMap, alloc: Allocator) !EncryptionConfig {
    const master_key = try env.requiredEnvOwned(env_map, alloc, "ENCRYPTION_MASTER_KEY", ValidationError.MissingEncryptionMasterKey);
    errdefer alloc.free(master_key);
    if (master_key.len != 64 or !validate.isHexString(master_key)) return ValidationError.InvalidEncryptionMasterKey;

    return .{ .master_key = master_key };
}

pub fn freeEncryption(alloc: Allocator, cfg: EncryptionConfig) void {
    alloc.free(cfg.master_key);
}

const AuthPeppersConfig = struct {
    session_code_pepper: []const u8,
    audit_log_pepper: []const u8,
};

/// Two independent HMAC peppers loaded at boot. Both share the same shape as
/// ENCRYPTION_MASTER_KEY (64 hex chars = 32 bytes CSPRNG, hex-encoded). Held
/// in process memory only; never written to disk; never logged. Provisioned
/// via the bootstrap playbook auth-pepper subsection.
///
///   AUTH_SESSION_CODE_PEPPER — keyed HMAC for the device-flow verification
///                              code (defeats offline brute-force from a
///                              Redis dump alone).
///   AUDIT_LOG_PEPPER         — keyed HMAC for `session_id` in the
///                              `.auth_audit` log scope (pseudonymization
///                              across audit events).
pub fn loadAuthPeppers(env_map: *const EnvMap, alloc: Allocator) !AuthPeppersConfig {
    const session_code = try env.requiredEnvOwned(env_map, alloc, "AUTH_SESSION_CODE_PEPPER", ValidationError.MissingAuthSessionCodePepper);
    errdefer alloc.free(session_code);
    if (session_code.len != 64 or !validate.isHexString(session_code)) return ValidationError.InvalidAuthSessionCodePepper;

    const audit_log = try env.requiredEnvOwned(env_map, alloc, "AUDIT_LOG_PEPPER", ValidationError.MissingAuditLogPepper);
    errdefer alloc.free(audit_log);
    if (audit_log.len != 64 or !validate.isHexString(audit_log)) return ValidationError.InvalidAuditLogPepper;

    return .{ .session_code_pepper = session_code, .audit_log_pepper = audit_log };
}

pub fn freeAuthPeppers(alloc: Allocator, cfg: AuthPeppersConfig) void {
    alloc.free(cfg.session_code_pepper);
    alloc.free(cfg.audit_log_pepper);
}

const MiscConfig = struct {
    app_url: []const u8,
    api_url: []const u8,
};

pub fn loadMisc(env_map: *const EnvMap, alloc: Allocator) !MiscConfig {
    const app_url = try env.envOrDefaultOwned(env_map, alloc, "APP_URL", "https://app.agentsfleet.net");
    errdefer alloc.free(app_url);
    const api_url = try env.envOrDefaultOwned(env_map, alloc, "API_URL", "https://api.agentsfleet.net");
    return .{ .app_url = app_url, .api_url = api_url };
}
