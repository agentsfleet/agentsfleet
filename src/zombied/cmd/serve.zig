const std = @import("std");
const common = @import("common");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const balance_policy = @import("../config/balance_policy.zig");
const events_bus = @import("../events/bus.zig");
const oidc_auth = @import("../auth/oidc.zig");
const http_server = @import("../http/server.zig");
const http_handler = @import("../http/handler.zig");
const session_store_redis = @import("../session/session_store_redis.zig");
const audit_events = @import("../auth/audit_events.zig");
const queue_redis = @import("../queue/redis.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");
const metrics = @import("../observability/metrics.zig");
const logging = @import("log");
const telemetry_mod = @import("../observability/telemetry.zig");
const preflight = @import("preflight.zig");
const error_codes = @import("../errors/error_registry.zig");
const serve_args = @import("serve_args.zig");
const pg = @import("pg");
const approval_gate_sweeper = @import("../zombie/approval_gate_sweeper.zig");
const serve_webhook_lookup = @import("serve_webhook_lookup.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");
const env_resolve = @import("../config/env_resolve.zig");
const clerk_backend = @import("../auth/clerk_backend.zig");

const log = logging.scoped(.zombied);

const EnvMap = common.env.Map;

const S_STARTUP_CONFIG_LOAD_FAILED = "startup.config_load_failed";
const S_STARTUP_MODEL_RATE_CACHE_FAILED = "startup.model_rate_cache_failed";
const S_STARTUP_ARGS_PARSE_FAILED = "startup.args_parse_failed";
const S_STARTUP_ENV_CHECK_FAILED = "startup.env_check_failed";
const S_API = "api";

var shutdown_requested = std.atomic.Value(bool).init(false);
/// Published by run() before listen() begins; signalWatcher reads this to call stop().
var active_server = std.atomic.Value(?*http_server.Server).init(null);
var stop_server_fn: *const fn () void = defaultStopServer;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

fn defaultStopServer() void {
    if (active_server.load(.acquire)) |s| s.stop();
}

/// Boot-path read of the request-path Redis read-timeout knob (spec §6).
/// Absent env → default; present-but-malformed → fail loud (env hygiene
/// matches DATABASE_URL / REDIS_URL validation upstream).
fn readRedisRequestTimeoutMs(env_map: *const EnvMap, alloc: std.mem.Allocator) u32 {
    const raw = env_resolve.config(env_map, alloc, queue_redis.REDIS_REQUEST_TIMEOUT_MS_ENV) orelse
        return queue_redis.REDIS_REQUEST_TIMEOUT_MS_DEFAULT;
    defer alloc.free(raw);
    return queue_redis.parseRequestTimeoutMs(raw) catch {
        log.err(S_STARTUP_ENV_CHECK_FAILED, .{
            .error_code = error_codes.ERR_STARTUP_ENV_CHECK,
            .err = queue_redis.REDIS_REQUEST_TIMEOUT_MS_ENV ++ " must parse as a non-negative integer (ms)",
        });
        std.process.exit(1);
    };
}

const webhook_sig = auth_mw.webhook_sig_mod;
const svix_signature = auth_mw.svix_signature_mod;

/// Minimal `.next()`-yielding iterator over the threaded argv. Zig 0.16
/// removed `std.process.args()`; argv now arrives via `std.process.Init`.
const ArgvIter = struct {
    argv: []const [:0]const u8,
    i: usize = 0,

    pub fn next(self: *ArgvIter) ?[:0]const u8 {
        if (self.i >= self.argv.len) return null;
        defer self.i += 1;
        return self.argv[self.i];
    }
};

fn parseServeArgOverrides(argv: []const [:0]const u8) serve_args.ServeArgError!?u16 {
    var it = ArgvIter{ .argv = argv };
    _ = it.next(); // binary name
    _ = it.next(); // subcommand
    return serve_args.parseArgs(&it);
}

fn onSignal(sig: std.posix.SIG) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher() void {
    while (!shutdown_requested.load(.acquire)) {
        common.sleepNanos(100 * std.time.ns_per_ms);
    }
    stop_server_fn();
}

pub fn run(io: std.Io, env_map: *const EnvMap, argv: []const [:0]const u8, alloc: std.mem.Allocator) !void {
    preflight.initOtelLogs(env_map, alloc);
    defer preflight.deinitOtelLogs();
    preflight.initOtelTraces(env_map, alloc);
    defer preflight.deinitOtelTraces();
    log.info("startup.serve_start", .{});

    const serve_port_override = parseServeArgOverrides(argv) catch |err| {
        switch (err) {
            serve_args.ServeArgError.InvalidServeArgument => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "invalid_argument" }),
            serve_args.ServeArgError.MissingPortValue => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "missing_port_value" }),
            serve_args.ServeArgError.InvalidPortValue => log.err(S_STARTUP_ARGS_PARSE_FAILED, .{ .reason = "invalid_port_value" }),
        }
        std.process.exit(2);
    };

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

    log.info("startup.config_load_start", .{});
    var serve_cfg = runtime_config.ServeConfig.load(env_map, alloc) catch |err| {
        switch (err) {
            runtime_config.ValidationError.OidcRequired,
            runtime_config.ValidationError.MissingOidcJwksUrl,
            runtime_config.ValidationError.InvalidOidcProvider,
            runtime_config.ValidationError.MissingEncryptionMasterKey,
            runtime_config.ValidationError.InvalidEncryptionMasterKey,
            runtime_config.ValidationError.InvalidPort,
            runtime_config.ValidationError.InvalidApiHttpThreads,
            runtime_config.ValidationError.InvalidApiHttpWorkers,
            runtime_config.ValidationError.InvalidApiMaxClients,
            runtime_config.ValidationError.InvalidApiMaxInFlightRequests,
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
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    log.info("startup.config_load_ok", .{});

    // Resolve the Key-Encryption Key (KEK) ONCE from the already-validated
    // config value — the crypto/vault layer reads it without re-touching env.
    // Must precede any request-path vault decrypt, so it lands right here.
    crypto_primitives.setKekFromHex(serve_cfg.encryption_master_key) catch |err| {
        log.err(S_STARTUP_CONFIG_LOAD_FAILED, .{ .error_code = error_codes.ERR_STARTUP_CONFIG_LOAD, .err = @errorName(err) });
        std.process.exit(1);
    };

    const api_pool = preflight.connectDbPool(io, env_map, alloc, .api) catch std.process.exit(1);
    defer api_pool.deinit();

    log.info("startup.redis_connect_start", .{ .role = S_API });
    const redis_request_timeout_ms = readRedisRequestTimeoutMs(env_map, alloc);
    log.info("startup.redis_request_timeout_resolved", .{ .ms = redis_request_timeout_ms });
    var api_queue = queue_redis.Client.connectFromEnvWithOptions(io, env_map, alloc, .api, .{
        .read_timeout_ms = redis_request_timeout_ms,
    }) catch |err| {
        log.err("startup.redis_connect_failed", .{
            .role = S_API,
            .error_code = error_codes.ERR_STARTUP_REDIS_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    defer api_queue.deinit();
    metrics.registerRedisPool(&api_queue.pool);
    // Defer order: clear FIRST at scope exit so a mid-shutdown /metrics
    // scrape can't dereference a deinit'd Pool.
    defer metrics.clearRegisteredRedisPool();
    log.info("startup.redis_connect_ok", .{ .role = S_API });

    const migrate_on_start = preflight.parseMigrateOnStart(env_map, alloc) catch std.process.exit(1);
    preflight.checkMigrations(api_pool, migrate_on_start) catch std.process.exit(1);

    log.info("startup.model_rate_cache_start", .{});
    {
        const cache_conn = api_pool.acquire() catch |err| {
            log.err(S_STARTUP_MODEL_RATE_CACHE_FAILED, .{ .err = @errorName(err) });
            std.process.exit(1);
        };
        defer api_pool.release(cache_conn);
        model_rate_cache.populate(alloc, cache_conn) catch |err| {
            log.err(S_STARTUP_MODEL_RATE_CACHE_FAILED, .{ .err = @errorName(err) });
            std.process.exit(1);
        };
    }
    defer model_rate_cache.deinit();
    log.info("startup.model_rate_cache_ok", .{});

    var sessions = session_store_redis.SessionStore.init(
        alloc,
        &api_queue,
        serve_cfg.auth_session_code_pepper,
        serve_cfg.audit_log_pepper,
    );

    // Webhook/backend secrets resolved ONCE at boot — handlers + the webhook
    // middleware borrow these (Context owns them for the process lifetime)
    // instead of re-reading env per request. Null = unset → consumer fails closed.
    const clerk_webhook_secret = try env_resolve.secret(env_map, alloc, env_resolve.CLERK_WEBHOOK_SECRET_ENV);
    defer if (clerk_webhook_secret) |s| alloc.free(s);
    const approval_signing_secret_owned = try env_resolve.secret(env_map, alloc, env_resolve.APPROVAL_SIGNING_SECRET_ENV);
    defer if (approval_signing_secret_owned) |s| alloc.free(s);
    const clerk_secret_key = try env_resolve.secret(env_map, alloc, clerk_backend.SECRET_ENV_VAR);
    defer if (clerk_secret_key) |s| alloc.free(s);

    var ctx = http_handler.Context{
        .pool = api_pool,
        .queue = &api_queue,
        .alloc = alloc,
        .io = io,
        .clerk_webhook_secret = clerk_webhook_secret,
        .approval_signing_secret = approval_signing_secret_owned,
        .clerk_secret_key = clerk_secret_key,
        .oidc = null,
        .auth_sessions = &sessions,
        .audit_ctx = audit_events.AuditCtx.init(serve_cfg.audit_log_pepper),
        .app_url = serve_cfg.app_url,
        .api_url = serve_cfg.api_url,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = serve_cfg.api_max_in_flight_requests,
        .ready_max_queue_depth = serve_cfg.ready_max_queue_depth,
        .ready_max_queue_age_ms = serve_cfg.ready_max_queue_age_ms,
        .balance_policy = balance_policy.resolveFromEnv(env_map, alloc),
        // SAFETY: written by surrounding init logic before any read of this storage.
        .telemetry = undefined,
    };
    metrics.setApiInFlightRequests(0);

    var tel = preflight.initTelemetry(env_map, alloc);
    defer tel.deinit(alloc);
    ctx.telemetry = tel.ptr();

    if (serve_cfg.oidc_enabled) {
        log.info("startup.oidc_init_start", .{ .provider = @tagName(serve_cfg.oidc_provider) });
    }
    var oidc = if (serve_cfg.oidc_enabled) oidc_auth.Verifier.init(alloc, .{
        .provider = serve_cfg.oidc_provider,
        .jwks_url = serve_cfg.oidc_jwks_url orelse "",
        .issuer = serve_cfg.oidc_issuer,
        .audience = serve_cfg.oidc_audience,
    }) else null;
    defer if (oidc) |*v| v.deinit();
    if (oidc) |*v| {
        ctx.oidc = v;
        log.info("startup.oidc_init_ok", .{});
    }

    // M18_002 C.2: Build the middleware registry at boot.
    //
    // The webhook signing secret is the boot-resolved `approval_signing_secret_owned`
    // (above) — each request borrows it, paying no getenv. Missing → empty slice →
    // the middleware rejects every request on that route (fail-closed).
    //
    // LIFETIME: `registry` is a stack-allocated var in run(). It must stay
    // alive for the duration of the server. `initChains()` captures pointers
    // into registry fields; do NOT call initChains() before all fields are set,
    // and do NOT move/copy registry after calling initChains().
    const approval_signing_secret: []const u8 = if (approval_signing_secret_owned) |s| s else "";

    var api_key_lookup_ctx = api_key_lookup.Ctx{ .pool = ctx.pool };
    var runner_lookup_ctx = serve_runner_lookup.Ctx{ .pool = ctx.pool };

    var registry = auth_mw.MiddlewareRegistry{
        .bearer_or_api_key = .{
            .verifier = ctx.oidc,
        },
        .tenant_api_key_mw = .{
            .host = &api_key_lookup_ctx,
            .lookup = api_key_lookup.lookup,
        },
        .runner_bearer_mw = .{
            .host = &runner_lookup_ctx,
            .lookup = serve_runner_lookup.lookup,
        },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .platform_admin_mw = .{},
        .webhook_hmac_mw = .{ .secret = approval_signing_secret },
    };
    // M28_001: construct the generic WebhookSig with concrete *pg.Pool type.
    // Must be declared before initChains() so the pointer is stable, but
    // the chain is set via setWebhookSig() after initChains().
    var webhook_sig_mw = webhook_sig.WebhookSig(*pg.Pool){
        .lookup_ctx = api_pool,
        .lookup_fn = serve_webhook_lookup.lookup,
    };
    // M28_001 §5: Svix middleware for Clerk — separate lookup fn resolves
    // the whsec_<base64> secret via the workspace vault.
    var svix_mw = svix_signature.SvixSignature(*pg.Pool){
        .lookup_ctx = api_pool,
        .lookup_fn = serve_webhook_lookup.lookupSvix,
    };
    registry.initChains();
    registry.setWebhookSig(webhook_sig_mw.middleware());
    registry.setSvixSig(svix_mw.middleware());
    log.info("startup.middleware_registry_ok", .{});

    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    var signal_thread: ?std.Thread = null;
    var event_thread: ?std.Thread = null;
    var approval_sweeper_thread: ?std.Thread = null;
    errdefer {
        shutdown_requested.store(true, .release);
        event_bus.stop();
        if (signal_thread) |*t| t.join();
        if (event_thread) |*t| t.join();
        if (approval_sweeper_thread) |*t| t.join();
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});
    approval_sweeper_thread = try std.Thread.spawn(.{}, approval_gate_sweeper.run, .{ api_pool, &api_queue, alloc, &shutdown_requested });

    log.info("http.server_starting", .{
        .port = serve_cfg.port,
        .api_threads = serve_cfg.api_http_threads,
        .api_workers = serve_cfg.api_http_workers,
        .api_max_clients = serve_cfg.api_max_clients,
        .api_max_in_flight = serve_cfg.api_max_in_flight_requests,
    });
    ctx.telemetry.capture(telemetry_mod.ServerStarted, .{ .port = serve_cfg.port });
    const srv = http_server.Server.init(io, &ctx, &registry, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        log.err("http.server_init_failed", .{ .err = @errorName(err) });
        return err;
    };
    defer srv.deinit();
    active_server.store(srv, .release);
    defer active_server.store(null, .release);

    srv.listen() catch |err| {
        log.err("http.server_exit_failed", .{ .err = @errorName(err) });
    };

    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();
    if (approval_sweeper_thread) |*t| t.join();
}

fn testStopServerHook() void {
    if (stop_server_test_counter) |counter| {
        _ = counter.fetchAdd(1, .acq_rel);
    }
}

test "integration: signalWatcher stops server on shutdown" {
    shutdown_requested.store(false, .release);

    var stop_calls = std.atomic.Value(u32).init(0);
    stop_server_test_counter = &stop_calls;
    stop_server_fn = testStopServerHook;
    defer {
        stop_server_fn = defaultStopServer;
        stop_server_test_counter = null;
        shutdown_requested.store(false, .release);
    }

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    common.sleepNanos(15 * std.time.ns_per_ms);
    shutdown_requested.store(true, .release);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}

// Arg-parsing tests extracted to serve_test.zig (M10_002).
comptime {
    _ = @import("serve_test.zig");
}
