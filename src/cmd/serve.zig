const std = @import("std");
const db = @import("../db/pool.zig");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const oidc_auth = @import("../auth/oidc.zig");
const http_server = @import("../http/server.zig");
const http_handler = @import("../http/handler.zig");
const auth_sessions = @import("../auth/sessions.zig");
const queue_redis = @import("../queue/redis.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const preflight = @import("preflight.zig");
const common = @import("common.zig");
const error_codes = @import("../errors/error_registry.zig");
const serve_args = @import("serve_args.zig");

const log = std.log.scoped(.zombied);

var shutdown_requested = std.atomic.Value(bool).init(false);
/// Published by run() before listen() begins; signalWatcher reads this to call stop().
var active_server = std.atomic.Value(?*http_server.Server).init(null);
var stop_server_fn: *const fn () void = defaultStopServer;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

fn defaultStopServer() void {
    if (active_server.load(.acquire)) |s| s.stop();
}

// ── M18_002 C.2: MiddlewareRegistry callbacks ─────────────────────────────

/// Atomically consume an OAuth nonce from Redis. Called by `OAuthState`
/// middleware when the OAuth callback route is handled by the chain.
///
/// Key format: `slack:oauth:nonce:<nonce>` — mirrors `slack_oauth.zig::validateState`.
/// Returns true if the nonce existed and was deleted; false if it was already
/// consumed (0-key DEL) or if Redis is unavailable.
fn consumeOAuthNonce(user: *anyopaque, nonce: []const u8) anyerror!bool {
    const q: *queue_redis.Client = @ptrCast(@alignCast(user));
    var key_buf: [64]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "slack:oauth:nonce:{s}", .{nonce}) catch return error.KeyTooLong;
    const resp = q.command(&.{ "DEL", key }) catch return error.RedisUnavailable;
    return switch (resp) {
        .integer => |n| n > 0,
        else => false,
    };
}

/// Stub webhook URL-secret lookup. Returns null (no secret configured) until
/// Batch D wires the real vault/DB lookup. With an empty route table (C.2),
/// this function is never invoked at runtime.
fn stubWebhookSecretLookup(
    _: *anyopaque,
    _: []const u8,
    _: std.mem.Allocator,
) anyerror!?[]const u8 {
    return null;
}

fn parseServeArgOverrides() serve_args.ServeArgError!?u16 {
    var it = std.process.args();
    _ = it.next(); // binary name
    _ = it.next(); // subcommand
    return serve_args.parseArgs(&it);
}

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher() void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    stop_server_fn();
}

pub fn run(alloc: std.mem.Allocator) !void {
    preflight.initOtelLogs(alloc);
    defer preflight.deinitOtelLogs();
    preflight.initOtelTraces(alloc);
    defer preflight.deinitOtelTraces();
    log.info("startup.serve status=start", .{});

    const serve_port_override = parseServeArgOverrides() catch |err| {
        switch (err) {
            serve_args.ServeArgError.InvalidServeArgument => log.err("startup.args_parse status=fail reason=invalid_argument", .{}),
            serve_args.ServeArgError.MissingPortValue => log.err("startup.args_parse status=fail reason=missing_port_value", .{}),
            serve_args.ServeArgError.InvalidPortValue => log.err("startup.args_parse status=fail reason=invalid_port_value", .{}),
        }
        std.process.exit(2);
    };

    log.info("startup.env_check status=start", .{});
    env_vars.enforceFromEnv(alloc) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_API and DATABASE_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API and REDIS_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_API must use rediss://", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER must use rediss://", .{}),
        }
        std.process.exit(1);
    };
    log.info("startup.env_check status=ok", .{});

    log.info("startup.config_load status=start", .{});
    var serve_cfg = runtime_config.ServeConfig.load(alloc) catch |err| {
        switch (err) {
            runtime_config.ValidationError.MissingApiKey,
            runtime_config.ValidationError.InvalidApiKeyList,
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
            runtime_config.ValidationError.InvalidKekVersion,
            runtime_config.ValidationError.MissingEncryptionMasterKeyV2,
            runtime_config.ValidationError.InvalidEncryptionMasterKeyV2,
            => {
                runtime_config.ServeConfig.printValidationError(@errorCast(err));
                log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)});
            },
            else => log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    log.info("startup.config_load status=ok", .{});

    const api_pool = preflight.connectDbPool(alloc, .api) catch std.process.exit(1);
    defer api_pool.deinit();

    log.info("startup.redis_connect role=api status=start", .{});
    var api_queue = queue_redis.Client.connectFromEnv(alloc, .api) catch |err| {
        log.err("startup.redis_connect role=api status=fail error_code=" ++ error_codes.ERR_STARTUP_REDIS_CONNECT ++ " err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer api_queue.deinit();
    api_queue.ensureConsumerGroup() catch |err| {
        log.err("startup.redis_group role=api status=fail error_code=" ++ error_codes.ERR_STARTUP_REDIS_GROUP ++ " err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("startup.redis_connect role=api status=ok", .{});

    const migrate_on_start = preflight.parseMigrateOnStart(alloc) catch std.process.exit(1);
    preflight.checkMigrations(api_pool, migrate_on_start) catch std.process.exit(1);

    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "startup");

    var sessions = auth_sessions.SessionStore.init(alloc);
    defer sessions.deinit();

    var ctx = http_handler.Context{
        .pool = api_pool,
        .queue = &api_queue,
        .alloc = alloc,
        .api_keys = serve_cfg.api_keys,
        .oidc = null,
        .auth_sessions = &sessions,
        .app_url = serve_cfg.app_url,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = serve_cfg.api_max_in_flight_requests,
        .ready_max_queue_depth = serve_cfg.ready_max_queue_depth,
        .ready_max_queue_age_ms = serve_cfg.ready_max_queue_age_ms,
        .telemetry = undefined,
    };
    metrics.setApiInFlightRequests(0);

    var tel = preflight.initTelemetry(alloc);
    defer tel.deinit(alloc);
    ctx.telemetry = tel.ptr();

    if (serve_cfg.oidc_enabled) {
        log.info("startup.oidc_init status=start provider={s}", .{@tagName(serve_cfg.oidc_provider)});
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
        log.info("startup.oidc_init status=ok", .{});
    }

    // M18_002 C.2: Build the middleware registry at boot.
    //
    // Signing secrets are loaded from env vars here so each request does not
    // pay a getenv syscall. Missing secrets → empty slice → the middleware
    // rejects every request on that route (correct fail-closed behavior).
    //
    // LIFETIME: `registry` is a stack-allocated var in run(). It must stay
    // alive for the duration of the server. `initChains()` captures pointers
    // into registry fields; do NOT call initChains() before all fields are set,
    // and do NOT move/copy registry after calling initChains().
    const slack_signing_secret = std.process.getEnvVarOwned(alloc, "SLACK_SIGNING_SECRET") catch "";
    const approval_signing_secret = std.process.getEnvVarOwned(alloc, "APPROVAL_SIGNING_SECRET") catch "";

    var registry = auth_mw.MiddlewareRegistry{
        .bearer_or_api_key = .{
            .api_keys = serve_cfg.api_keys,
            .verifier = ctx.oidc,
        },
        .admin_api_key_mw = .{
            .api_keys = serve_cfg.api_keys,
        },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
        .slack_sig = .{ .secret = slack_signing_secret },
        .webhook_hmac_mw = .{ .secret = approval_signing_secret },
        .oauth_state_mw = .{
            .signing_secret = slack_signing_secret,
            .consume_ctx = &api_queue,
            .consume_nonce = consumeOAuthNonce,
        },
        .webhook_url_secret_mw = .{
            .lookup_ctx = &api_queue, // unused by stub; Batch D will wire real lookup
            .lookup_fn = stubWebhookSecretLookup,
        },
    };
    registry.initChains();
    log.info("startup.middleware_registry status=ok", .{});

    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    var signal_thread: ?std.Thread = null;
    var event_thread: ?std.Thread = null;
    errdefer {
        shutdown_requested.store(true, .release);
        event_bus.stop();
        if (signal_thread) |*t| t.join();
        if (event_thread) |*t| t.join();
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("http.server_starting port={d} api_threads={d} api_workers={d} api_max_clients={d} api_max_in_flight={d}", .{
        serve_cfg.port,
        serve_cfg.api_http_threads,
        serve_cfg.api_http_workers,
        serve_cfg.api_max_clients,
        serve_cfg.api_max_in_flight_requests,
    });
    ctx.telemetry.capture(telemetry_mod.ServerStarted, .{ .port = serve_cfg.port });
    const srv = http_server.Server.init(&ctx, &registry, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        obs_log.logErr(.zombied, err, "http.server_init status=fail", .{});
        return err;
    };
    defer srv.deinit();
    active_server.store(srv, .release);
    defer active_server.store(null, .release);

    srv.listen() catch |err| {
        obs_log.logErr(.zombied, err, "http.server_exit status=fail", .{});
    };

    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();
    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "shutdown");
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
    std.Thread.sleep(15 * std.time.ns_per_ms);
    shutdown_requested.store(true, .release);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}

// Arg-parsing tests extracted to serve_test.zig (M10_002).
comptime {
    _ = @import("serve_test.zig");
}
