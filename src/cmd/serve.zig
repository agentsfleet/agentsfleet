const std = @import("std");
const posthog = @import("posthog");

const db = @import("../db/pool.zig");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const oidc_auth = @import("../auth/oidc.zig");
const http_server = @import("../http/server.zig");
const http_handler = @import("../http/handler.zig");
const auth_sessions = @import("../auth/sessions.zig");
const queue_redis = @import("../queue/redis.zig");
const worker = @import("../pipeline/worker.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const preflight = @import("preflight.zig");
const common = @import("common.zig");

const log = std.log.scoped(.zombied);

var shutdown_requested = std.atomic.Value(bool).init(false);
var stop_server_fn: *const fn () void = http_server.stop;
var stop_server_test_counter: ?*std.atomic.Value(u32) = null;

const ServeArgError = error{
    InvalidServeArgument,
    MissingPortValue,
    InvalidPortValue,
};

fn parseServeArgOverrides() ServeArgError!?u16 {
    var override_port: ?u16 = null;
    var it = std.process.args();
    _ = it.next();
    _ = it.next();
    while (it.next()) |arg| {
        if (std.mem.eql(u8, arg, "--port")) {
            const port_raw = it.next() orelse return ServeArgError.MissingPortValue;
            const parsed = std.fmt.parseInt(u16, port_raw, 10) catch return ServeArgError.InvalidPortValue;
            if (parsed == 0) return ServeArgError.InvalidPortValue;
            override_port = parsed;
            continue;
        }
        if (std.mem.startsWith(u8, arg, "--port=")) {
            const port_raw = arg["--port=".len..];
            const parsed = std.fmt.parseInt(u16, port_raw, 10) catch return ServeArgError.InvalidPortValue;
            if (parsed == 0) return ServeArgError.InvalidPortValue;
            override_port = parsed;
            continue;
        }
        return ServeArgError.InvalidServeArgument;
    }
    return override_port;
}

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher(wstate: *worker.WorkerState) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }

    wstate.running.store(false, .release);
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
            ServeArgError.InvalidServeArgument => log.err("startup.args_parse status=fail reason=invalid_argument", .{}),
            ServeArgError.MissingPortValue => log.err("startup.args_parse status=fail reason=missing_port_value", .{}),
            ServeArgError.InvalidPortValue => log.err("startup.args_parse status=fail reason=invalid_port_value", .{}),
        }
        std.process.exit(2);
    };

    log.info("startup.env_check status=start", .{});
    env_vars.enforceFromEnv(alloc) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=DATABASE_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=DATABASE_URL_API and DATABASE_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_API and REDIS_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_API must use rediss://", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_WORKER must use rediss://", .{}),
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
            runtime_config.ValidationError.MissingGitHubAppId,
            runtime_config.ValidationError.MissingGitHubAppPrivateKey,
            runtime_config.ValidationError.InvalidPort,
            runtime_config.ValidationError.InvalidMaxAttempts,
            runtime_config.ValidationError.InvalidWorkerConcurrency,
            runtime_config.ValidationError.InvalidApiHttpThreads,
            runtime_config.ValidationError.InvalidApiHttpWorkers,
            runtime_config.ValidationError.InvalidApiMaxClients,
            runtime_config.ValidationError.InvalidApiMaxInFlightRequests,
            runtime_config.ValidationError.InvalidRunTimeoutMs,
            runtime_config.ValidationError.InvalidRateLimitCapacity,
            runtime_config.ValidationError.InvalidRateLimitRefillPerSec,
            runtime_config.ValidationError.InvalidReadyMaxQueueDepth,
            runtime_config.ValidationError.InvalidReadyMaxQueueAgeMs,
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

    const worker_pool = preflight.connectDbPool(alloc, .worker) catch std.process.exit(1);
    defer worker_pool.deinit();

    log.info("startup.redis_connect role=api status=start", .{});
    var api_queue = queue_redis.Client.connectFromEnv(alloc, .api) catch |err| {
        log.err("startup.redis_connect role=api status=fail error_code=UZ-STARTUP-004 err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer api_queue.deinit();
    api_queue.ensureConsumerGroup() catch |err| {
        log.err("startup.redis_group role=api status=fail error_code=UZ-STARTUP-004 err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("startup.redis_connect role=api status=ok", .{});

    log.info("startup.redis_connect role=worker status=start", .{});
    var worker_queue_check = queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        log.err("startup.redis_connect role=worker status=fail error_code=UZ-STARTUP-004 err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer worker_queue_check.deinit();
    worker_queue_check.ensureConsumerGroup() catch |err| {
        log.err("startup.redis_group role=worker status=fail error_code=UZ-STARTUP-004 err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("startup.redis_connect role=worker status=ok", .{});

    const migrate_on_start = preflight.parseMigrateOnStart(alloc) catch std.process.exit(1);
    preflight.checkMigrations(api_pool, migrate_on_start) catch std.process.exit(1);

    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "startup");

    var wstate = worker.WorkerState.init();
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
        .worker_state = &wstate,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = serve_cfg.api_max_in_flight_requests,
        .ready_max_queue_depth = serve_cfg.ready_max_queue_depth,
        .ready_max_queue_age_ms = serve_cfg.ready_max_queue_age_ms,
        .posthog = null,
    };
    metrics.setApiInFlightRequests(0);

    const ph = preflight.initPostHog(alloc);
    defer ph.deinit(alloc);
    ctx.posthog = ph.client;

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

    const wcfg = worker.WorkerConfig{
        .pool = worker_pool,
        .config_dir = serve_cfg.config_dir,
        .cache_root = serve_cfg.cache_root,
        .github_app_id = serve_cfg.github_app_id,
        .github_app_private_key = serve_cfg.github_app_private_key,
        .pipeline_profile_path = serve_cfg.pipeline_profile_path,
        .max_attempts = serve_cfg.max_attempts,
        .run_timeout_ms = serve_cfg.run_timeout_ms,
        .rate_limit_capacity = serve_cfg.rate_limit_capacity,
        .rate_limit_refill_per_sec = serve_cfg.rate_limit_refill_per_sec,
        .posthog = ph.client,
    };

    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    const worker_count: usize = @max(@as(usize, @intCast(serve_cfg.worker_concurrency)), 1);
    var worker_threads = try alloc.alloc(std.Thread, worker_count);
    defer alloc.free(worker_threads);
    var spawned_workers: usize = 0;
    var signal_thread: ?std.Thread = null;
    var event_thread: ?std.Thread = null;
    errdefer {
        wstate.running.store(false, .release);
        shutdown_requested.store(true, .release);
        event_bus.stop();
        if (signal_thread) |*t| t.join();
        if (event_thread) |*t| t.join();
        while (spawned_workers > 0) {
            spawned_workers -= 1;
            worker_threads[spawned_workers].join();
        }
    }
    for (worker_threads) |*t| {
        t.* = try std.Thread.spawn(.{}, worker.workerLoop, .{ wcfg, &wstate });
        spawned_workers += 1;
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{&wstate});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("http.server_starting port={d} worker_concurrency={d} api_threads={d} api_workers={d} api_max_clients={d} api_max_in_flight={d}", .{
        serve_cfg.port,
        worker_count,
        serve_cfg.api_http_threads,
        serve_cfg.api_http_workers,
        serve_cfg.api_max_clients,
        serve_cfg.api_max_in_flight_requests,
    });
    posthog_events.trackServerStarted(ph.client, serve_cfg.port, @intCast(serve_cfg.worker_concurrency));
    http_server.serve(&ctx, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        obs_log.logErr(.zombied, err, "http.server_exit status=fail", .{});
    };

    wstate.running.store(false, .release);
    shutdown_requested.store(true, .release);
    event_bus.stop();
    for (worker_threads) |*t| t.join();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();
    _ = preflight.prepareCacheRoot(alloc, serve_cfg.cache_root, "shutdown");
}

fn testStopServerHook() void {
    if (stop_server_test_counter) |counter| {
        _ = counter.fetchAdd(1, .acq_rel);
    }
}

test "integration: signalWatcher stops worker and invokes server stop hook" {
    var ws = worker.WorkerState.init();
    ws.running.store(true, .release);
    shutdown_requested.store(false, .release);

    var stop_calls = std.atomic.Value(u32).init(0);
    stop_server_test_counter = &stop_calls;
    stop_server_fn = testStopServerHook;
    defer {
        stop_server_fn = http_server.stop;
        stop_server_test_counter = null;
        shutdown_requested.store(false, .release);
    }

    const thread = try std.Thread.spawn(.{}, signalWatcher, .{&ws});
    std.Thread.sleep(15 * std.time.ns_per_ms);
    shutdown_requested.store(true, .release);
    thread.join();

    try std.testing.expect(!ws.running.load(.acquire));
    try std.testing.expectEqual(@as(u32, 1), stop_calls.load(.acquire));
}
