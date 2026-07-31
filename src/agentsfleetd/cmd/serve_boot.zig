//! serve_boot.zig — process-exiting boot-stage helpers extracted from
//! serve.run (function-length cap). Pure prologue: no defers live here, so
//! run() keeps the entire teardown choreography in one frame.

const std = @import("std");
const common = @import("common");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const oidc_auth = @import("../auth/oidc.zig");
const queue_redis = @import("../queue/redis.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");
const logging = @import("log");
const error_codes = @import("../errors/error_registry.zig");
const serve_args = @import("serve_args.zig");
const serve_redis_timeout = @import("serve_redis_timeout.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const log = logging.scoped(.agentsfleetd);

const EnvMap = common.env.Map;

const S_STARTUP_CONFIG_LOAD_FAILED = "startup.config_load_failed";
const S_STARTUP_ARGS_PARSE_FAILED = "startup.args_parse_failed";
const S_STARTUP_ENV_CHECK_FAILED = "startup.env_check_failed";
const S_API = "api";

pub fn parseArgsOrExit(argv: []const [:0]const u8) ?u16 {
    return serve_args.parseServeArgOverrides(argv) catch |err| {
        switch (err) {
            serve_args.ServeArgError.InvalidServeArgument => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "invalid_argument" }),
            serve_args.ServeArgError.MissingPortValue => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "missing_port_value" }),
            serve_args.ServeArgError.InvalidPortValue => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "invalid_port_value" }),
        }
        std.process.exit(2);
    };
}

pub fn enforceEnvOrExit(env_map: *const EnvMap, alloc: std.mem.Allocator) void {
    log.info("startup.env_check_start", .{});
    env_vars.enforceFromEnv(env_map, alloc) catch |err| {
        const env_code = error_codes.ERR_STARTUP_ENV_CHECK;
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err(S_STARTUP_ENV_CHECK_FAILED, .{ .error_code = env_code, .err = "DATABASE_URL_API not set" }),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err(S_STARTUP_ENV_CHECK_FAILED, .{ .error_code = env_code, .err = "REDIS_URL_API not set" }),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err(S_STARTUP_ENV_CHECK_FAILED, .{ .error_code = env_code, .err = "REDIS_URL_API must use rediss://" }),
            else => log.err(S_STARTUP_ENV_CHECK_FAILED, .{ .error_code = env_code, .err = @errorName(err) }),
        }
        std.process.exit(1);
    };
    log.info("startup.env_check_ok", .{});
}

pub fn loadServeConfigOrExit(env_map: *const EnvMap, alloc: std.mem.Allocator) runtime_config.ServeConfig {
    log.info("startup.config_load_start", .{});
    const serve_cfg = runtime_config.ServeConfig.load(env_map, alloc) catch |err| {
        switch (err) {
            runtime_config.ValidationError.OidcRequired,
            runtime_config.ValidationError.MissingOidcIssuer,
            runtime_config.ValidationError.InvalidOidcProvider,
            runtime_config.ValidationError.MissingEncryptionMasterKey,
            runtime_config.ValidationError.InvalidEncryptionMasterKey,
            runtime_config.ValidationError.InvalidPort,
            runtime_config.ValidationError.InvalidApiHttpThreads,
            runtime_config.ValidationError.InvalidApiHttpWorkers,
            runtime_config.ValidationError.InvalidApiMaxClients,
            runtime_config.ValidationError.InvalidApiMaxInFlightRequests,
            runtime_config.ValidationError.InvalidSseMaxStreams,
            runtime_config.ValidationError.InvalidReadyMaxQueueDepth,
            runtime_config.ValidationError.InvalidReadyMaxQueueAgeMs,
            => {
                runtime_config.ServeConfig.printValidationError(@errorCast(err));
                log.err(S_STARTUP_CONFIG_LOAD_FAILED, .{ .error_code = error_codes.ERR_STARTUP_CONFIG_LOAD, .err = @errorName(err) });
            },
            else => log.err(S_STARTUP_CONFIG_LOAD_FAILED, .{ .error_code = error_codes.ERR_STARTUP_CONFIG_LOAD, .err = @errorName(err) }),
        }
        std.process.exit(1);
    };
    log.info("startup.config_load_ok", .{});
    return serve_cfg;
}

/// Resolve the Key-Encryption Key (KEK) ONCE from the already-validated
/// config value — the crypto/vault layer reads it without re-touching env.
/// Must precede any request-path vault decrypt.
pub fn setKekOrExit(master_key_hex: []const u8) void {
    crypto_primitives.setKekFromHex(master_key_hex) catch |err| {
        log.err(S_STARTUP_CONFIG_LOAD_FAILED, .{ .error_code = error_codes.ERR_STARTUP_CONFIG_LOAD, .err = @errorName(err) });
        std.process.exit(1);
    };
}

pub fn connectRedisOrExit(io: std.Io, env_map: *const EnvMap, alloc: std.mem.Allocator) queue_redis.Client {
    log.info("startup.redis_connect_start", .{ .role = S_API });
    const redis_request_timeout_ms = serve_redis_timeout.read(env_map, alloc);
    log.info("startup.redis_request_timeout_resolved", .{ .ms = redis_request_timeout_ms });
    const client = queue_redis.Client.connectFromEnvWithOptions(io, env_map, alloc, .api, .{
        .read_timeout_ms = redis_request_timeout_ms,
    }) catch |err| {
        log.err("startup.redis_connect_failed", .{
            .role = S_API,
            .error_code = error_codes.ERR_STARTUP_REDIS_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    log.info("startup.redis_connect_ok", .{ .role = S_API });
    return client;
}

pub fn initOidc(alloc: std.mem.Allocator, serve_cfg: *const runtime_config.ServeConfig) !?oidc_auth.Verifier {
    if (!serve_cfg.oidc_enabled) return null;
    log.info("startup.oidc_init_start", .{ .provider = @tagName(serve_cfg.oidc_provider), .jwks_url = serve_cfg.oidc_jwks_url orelse "" });
    return try oidc_auth.Verifier.init(alloc, .{
        .provider = serve_cfg.oidc_provider,
        .jwks_url = serve_cfg.oidc_jwks_url orelse "",
        .issuer = serve_cfg.oidc_issuer,
        .audience = serve_cfg.oidc_audience,
    });
}

/// Registry built BY VALUE and returned into the caller's stable var —
/// initChains() (which captures field pointers) runs only on that storage,
/// never here.
pub fn buildRegistry(
    verifier: ?*oidc_auth.Verifier,
    api_key_lookup_ctx: *api_key_lookup.Ctx,
    runner_lookup_ctx: *serve_runner_lookup.Ctx,
    approval_signing_secret: []const u8,
) auth_mw.MiddlewareRegistry {
    return .{
        .bearer_or_api_key = .{
            .verifier = verifier,
        },
        .tenant_api_key_mw = .{
            .host = api_key_lookup_ctx,
            .lookup = api_key_lookup.lookup,
        },
        .runner_bearer_mw = .{
            .host = runner_lookup_ctx,
            .lookup = serve_runner_lookup.lookup,
        },
        .require_scope_mw = .{},
        .webhook_hmac_mw = .{ .secret = approval_signing_secret },
    };
}
