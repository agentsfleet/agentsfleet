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
const api_key_lookup = @import("api_key_lookup.zig");
const metrics = @import("../observability/metrics.zig");
const logging = @import("log");
const telemetry_mod = @import("../observability/telemetry.zig");
const preflight = @import("preflight.zig");
const common = @import("common.zig");
const error_codes = @import("../errors/error_registry.zig");
const serve_args = @import("serve_args.zig");
const pg = @import("pg");
const approval_gate_sweeper = @import("../zombie/approval_gate_sweeper.zig");
const serve_webhook_lookup = @import("serve_webhook_lookup.zig");
const model_rate_cache = @import("../state/model_rate_cache.zig");

const log = logging.scoped(.zombied);

var shutdown_requested = std.atomic.Value(bool).init(false);
/// Published by run() before listen() begins; signalWatcher reads this to call stop().
var active_server = std.atomic.Value(?*http_server.Server).init(null);
var stop_server_fn: *const fn () void = defaultStopServer;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

fn defaultStopServer() void {
    if (active_server.load(.acquire)) |s| s.stop();
}

const webhook_sig = auth_mw.webhook_sig_mod;
const svix_signature = auth_mw.svix_signature_mod;

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
    log.info("startup.serve_start", .{});

    const serve_port_override = parseServeArgOverrides() catch |err| {
        switch (err) {
            serve_args.ServeArgError.InvalidServeArgument => log.err("startup.args_parse_failed", .{ .reason = "invalid_argument" }),
            serve_args.ServeArgError.MissingPortValue => log.err("startup.args_parse_failed", .{ .reason = "missing_port_value" }),
            serve_args.ServeArgError.InvalidPortValue => log.err("startup.args_parse_failed", .{ .reason = "invalid_port_value" }),
        }
        std.process.exit(2);
    };

    log.info("startup.env_check_start", .{});
    env_vars.enforceFromEnv(alloc) catch |err| {
        const env_code = error_codes.ERR_STARTUP_ENV_CHECK;
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "DATABASE_URL_API not set" }),
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "DATABASE_URL_WORKER not set" }),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "REDIS_URL_API not set" }),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "REDIS_URL_WORKER not set" }),
            env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "DATABASE_URL_API and DATABASE_URL_WORKER must differ" }),
            env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "REDIS_URL_API and REDIS_URL_WORKER must differ" }),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "REDIS_URL_API must use rediss://" }),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check_failed", .{ .error_code = env_code, .err = "REDIS_URL_WORKER must use rediss://" }),
        }
        std.process.exit(1);
    };
    log.info("startup.env_check_ok", .{});

    log.info("startup.config_load_start", .{});
    var serve_cfg = runtime_config.ServeConfig.load(alloc) catch |err| {
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
                log.err("startup.config_load_failed", .{ .error_code = "UZ-STARTUP-002", .err = @errorName(err) });
            },
            else => log.err("startup.config_load_failed", .{ .error_code = "UZ-STARTUP-002", .err = @errorName(err) }),
        }
        std.process.exit(1);
    };
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    log.info("startup.config_load_ok", .{});

    const api_pool = preflight.connectDbPool(alloc, .api) catch std.process.exit(1);
    defer api_pool.deinit();

    log.info("startup.redis_connect_start", .{ .role = "api" });
    var api_queue = queue_redis.Client.connectFromEnv(alloc, .api) catch |err| {
        log.err("startup.redis_connect_failed", .{
            .role = "api",
            .error_code = error_codes.ERR_STARTUP_REDIS_CONNECT,
            .err = @errorName(err),
        });
        std.process.exit(1);
    };
    defer api_queue.deinit();
    log.info("startup.redis_connect_ok", .{ .role = "api" });

    const migrate_on_start = preflight.parseMigrateOnStart(alloc) catch std.process.exit(1);
    preflight.checkMigrations(api_pool, migrate_on_start) catch std.process.exit(1);

    log.info("startup.model_rate_cache_start", .{});
    {
        const cache_conn = api_pool.acquire() catch |err| {
            log.err("startup.model_rate_cache_failed", .{ .err = @errorName(err) });
            std.process.exit(1);
        };
        defer api_pool.release(cache_conn);
        model_rate_cache.populate(alloc, cache_conn) catch |err| {
            log.err("startup.model_rate_cache_failed", .{ .err = @errorName(err) });
            std.process.exit(1);
        };
    }
    defer model_rate_cache.deinit();
    log.info("startup.model_rate_cache_ok", .{});

    var sessions = auth_sessions.SessionStore.init(alloc);
    defer sessions.deinit();

    var ctx = http_handler.Context{
        .pool = api_pool,
        .queue = &api_queue,
        .alloc = alloc,
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
    // Signing secrets are loaded from env vars here so each request does not
    // pay a getenv syscall. Missing secrets → empty slice → the middleware
    // rejects every request on that route (correct fail-closed behavior).
    //
    // LIFETIME: `registry` is a stack-allocated var in run(). It must stay
    // alive for the duration of the server. `initChains()` captures pointers
    // into registry fields; do NOT call initChains() before all fields are set,
    // and do NOT move/copy registry after calling initChains().
    const approval_signing_secret_owned = std.process.getEnvVarOwned(alloc, "APPROVAL_SIGNING_SECRET") catch null;
    defer if (approval_signing_secret_owned) |s| alloc.free(s);
    const approval_signing_secret: []const u8 = if (approval_signing_secret_owned) |s| s else "";

    var api_key_lookup_ctx = api_key_lookup.Ctx{ .pool = ctx.pool };

    var registry = auth_mw.MiddlewareRegistry{
        .bearer_or_api_key = .{
            .verifier = ctx.oidc,
        },
        .tenant_api_key_mw = .{
            .host = &api_key_lookup_ctx,
            .lookup = api_key_lookup.lookup,
        },
        .require_role_admin = .{ .required = .admin },
        .require_role_operator = .{ .required = .operator },
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
    const srv = http_server.Server.init(&ctx, &registry, .{
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
    std.Thread.sleep(15 * std.time.ns_per_ms);
    shutdown_requested.store(true, .release);
    thread.join();

    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}

// Arg-parsing tests extracted to serve_test.zig (M10_002).
comptime {
    _ = @import("serve_test.zig");
}
