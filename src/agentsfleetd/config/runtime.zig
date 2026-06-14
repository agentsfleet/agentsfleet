//! Runtime config loader for agentsfleetd serve mode.
//!
//! Façade over per-concern modules:
//!   - runtime_types.zig    — ValidationError enum
//!   - runtime_env_parse.zig — generic env-var readers
//!   - runtime_validate.zig — predicates + printValidationError
//!   - runtime_loader.zig   — per-section loaders (sizes / oidc / api keys / encryption / misc)
//!
//! Public surface preserved: `ServeConfig`, `ValidationError`,
//! `printValidationError`. Existing callers keep importing this file.

const std = @import("std");
const oidc = @import("../auth/oidc.zig");

const runtime_types = @import("runtime_types.zig");
const validate = @import("runtime_validate.zig");
const loader = @import("runtime_loader.zig");

pub const ValidationError = runtime_types.ValidationError;

pub const ServeConfig = struct {
    /// Static helper preserved here (rather than a free fn on the module)
    /// so existing callers can keep doing `ServeConfig.printValidationError(err)`.
    pub const printValidationError = validate.printValidationError;

    port: u16,
    api_http_threads: i16,
    api_http_workers: i16,
    api_max_clients: u32,
    api_max_in_flight_requests: u32,
    sse_max_streams: u32,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
    app_url: []const u8,
    api_url: []const u8,
    oidc_enabled: bool,
    oidc_provider: oidc.Provider,
    oidc_jwks_url: ?[]const u8,
    oidc_issuer: ?[]const u8,
    oidc_audience: ?[]const u8,
    encryption_master_key: []const u8,
    auth_session_code_pepper: []const u8,
    audit_log_pepper: []const u8,

    alloc: std.mem.Allocator,

    /// Read every env var, validate, and return a populated ServeConfig.
    /// Caller owns the result and must call deinit. Sub-loaders use their
    /// own errdefer chains; this orchestrator threads one errdefer per
    /// heap-owning section so a late failure frees every prior section
    /// (loadSizes returns POD only; loadMisc is last so no errdefer follows).
    pub fn load(env_map: *const std.process.Environ.Map, alloc: std.mem.Allocator) !ServeConfig {
        const sizes = try loader.loadSizes(env_map, alloc);
        const oidc_cfg = try loader.loadOidc(env_map, alloc);
        errdefer loader.freeOidc(alloc, oidc_cfg);
        // M11_006: OIDC is now required — the env-var admin bootstrap was
        // the only non-OIDC auth path and it's gone.
        if (!oidc_cfg.enabled) return ValidationError.OidcRequired;
        const enc = try loader.loadEncryption(env_map, alloc);
        errdefer loader.freeEncryption(alloc, enc);
        const peppers = try loader.loadAuthPeppers(env_map, alloc);
        errdefer loader.freeAuthPeppers(alloc, peppers);
        const misc = try loader.loadMisc(env_map, alloc);

        return .{
            .port = sizes.port,
            .api_http_threads = sizes.api_http_threads,
            .api_http_workers = sizes.api_http_workers,
            .api_max_clients = sizes.api_max_clients,
            .api_max_in_flight_requests = sizes.api_max_in_flight_requests,
            .sse_max_streams = sizes.sse_max_streams,
            .ready_max_queue_depth = sizes.ready_max_queue_depth,
            .ready_max_queue_age_ms = sizes.ready_max_queue_age_ms,
            .app_url = misc.app_url,
            .api_url = misc.api_url,
            .oidc_enabled = oidc_cfg.enabled,
            .oidc_provider = oidc_cfg.provider,
            .oidc_jwks_url = oidc_cfg.jwks_url,
            .oidc_issuer = oidc_cfg.issuer,
            .oidc_audience = oidc_cfg.audience,
            .encryption_master_key = enc.master_key,
            .auth_session_code_pepper = peppers.session_code_pepper,
            .audit_log_pepper = peppers.audit_log_pepper,
            .alloc = alloc,
        };
    }

    pub fn deinit(self: *ServeConfig) void {
        self.alloc.free(self.app_url);
        self.alloc.free(self.api_url);
        if (self.oidc_jwks_url) |v| self.alloc.free(v);
        if (self.oidc_issuer) |v| self.alloc.free(v);
        if (self.oidc_audience) |v| self.alloc.free(v);
        self.alloc.free(self.encryption_master_key);
        self.alloc.free(self.auth_session_code_pepper);
        self.alloc.free(self.audit_log_pepper);
    }
};

// Test discovery — façade fan-out. test {} is stripped in release builds.
test {
    _ = @import("runtime_env_parse_test.zig");
    _ = @import("runtime_validate_test.zig");
    _ = @import("runtime_loader_test.zig");
    _ = @import("runtime_pepper_loader_test.zig");
}
