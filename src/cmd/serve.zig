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
const git_ops = @import("../git/ops.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
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

fn installSignalHandlers() void {
    const action = std.posix.Sigaction{
        .handler = .{ .handler = onSignal },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &action, null);
    std.posix.sigaction(std.posix.SIG.TERM, &action, null);
}

fn signalWatcher(wstate: *worker.WorkerState) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }

    wstate.running.store(false, .release);
    stop_server_fn();
}

pub fn run(alloc: std.mem.Allocator) !void {
    log.info("phase=serve status=start", .{});

    const serve_port_override = parseServeArgOverrides() catch |err| {
        switch (err) {
            ServeArgError.InvalidServeArgument => log.err("phase=args status=fail reason=invalid_argument", .{}),
            ServeArgError.MissingPortValue => log.err("phase=args status=fail reason=missing_port_value", .{}),
            ServeArgError.InvalidPortValue => log.err("phase=args status=fail reason=invalid_port_value", .{}),
        }
        std.process.exit(2);
    };

    log.info("phase=env_check status=start", .{});
    env_vars.enforceFromEnv(alloc) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlApi => log.err("phase=env_check status=fail err=DATABASE_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("phase=env_check status=fail err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlApi => log.err("phase=env_check status=fail err=REDIS_URL_API not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("phase=env_check status=fail err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.SameDatabaseUrlForApiAndWorker => log.err("phase=env_check status=fail err=DATABASE_URL_API and DATABASE_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.SameRedisUrlForApiAndWorker => log.err("phase=env_check status=fail err=REDIS_URL_API and REDIS_URL_WORKER must differ", .{}),
            env_vars.EnvVarsErrors.RedisApiTlsRequired => log.err("phase=env_check status=fail err=REDIS_URL_API must use rediss://", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("phase=env_check status=fail err=REDIS_URL_WORKER must use rediss://", .{}),
        }
        std.process.exit(1);
    };
    log.info("phase=env_check status=ok", .{});

    log.info("phase=config_load status=start", .{});
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
                log.err("phase=config_load status=fail err={s}", .{@errorName(err)});
            },
            else => log.err("phase=config_load status=fail err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    defer serve_cfg.deinit();
    if (serve_port_override) |override| {
        serve_cfg.port = override;
    }
    log.info("phase=config_load status=ok", .{});

    log.info("phase=db_connect role=api status=start", .{});
    const api_pool = db.initFromEnvForRole(alloc, .api) catch |err| {
        log.err("phase=db_connect role=api status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer api_pool.deinit();
    log.info("phase=db_connect role=api status=ok", .{});

    log.info("phase=db_connect role=worker status=start", .{});
    const worker_pool = db.initFromEnvForRole(alloc, .worker) catch |err| {
        log.err("phase=db_connect role=worker status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer worker_pool.deinit();
    log.info("phase=db_connect role=worker status=ok", .{});

    log.info("phase=redis_connect role=api status=start", .{});
    var api_queue = queue_redis.Client.connectFromEnv(alloc, .api) catch |err| {
        log.err("phase=redis_connect role=api status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer api_queue.deinit();
    api_queue.ensureConsumerGroup() catch |err| {
        log.err("phase=redis_group role=api status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("phase=redis_connect role=api status=ok", .{});

    log.info("phase=redis_connect role=worker status=start", .{});
    var worker_queue_check = queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        log.err("phase=redis_connect role=worker status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    defer worker_queue_check.deinit();
    worker_queue_check.ensureConsumerGroup() catch |err| {
        log.err("phase=redis_group role=worker status=fail err={s}", .{@errorName(err)});
        std.process.exit(1);
    };
    log.info("phase=redis_connect role=worker status=ok", .{});

    log.info("phase=migration_check status=start", .{});
    const migrate_on_start = common.migrateOnStartEnabledFromEnv(alloc) catch |err| {
        log.err("phase=migration_check status=fail err=invalid_MIGRATE_ON_START err_detail={s}", .{@errorName(err)});
        std.process.exit(1);
    };

    common.enforceServeMigrationSafety(api_pool, migrate_on_start) catch |err| {
        switch (err) {
            common.MigrationGuardError.MigrationPending => log.err(
                "phase=migration_check status=fail err=pending_migrations hint=run zombied migrate or set MIGRATE_ON_START=1",
                .{},
            ),
            common.MigrationGuardError.MigrationFailed => log.err(
                "phase=migration_check status=fail err=migration_failure_state hint=inspect schema_migration_failures then rerun zombied migrate",
                .{},
            ),
            common.MigrationGuardError.MigrationSchemaAhead => log.err(
                "phase=migration_check status=fail err=schema_ahead hint=deploy matching binary",
                .{},
            ),
            common.MigrationGuardError.MigrationLockUnavailable => log.err(
                "phase=migration_check status=fail err=migration_lock_unavailable hint=another node is migrating",
                .{},
            ),
            else => log.err("phase=migration_check status=fail err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    log.info("phase=migration_check status=ok", .{});

    std.fs.makeDirAbsolute(serve_cfg.cache_root) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => obs_log.logWarnErr(.zombied, err, "could not create cache root {s}", .{serve_cfg.cache_root}),
    };
    {
        const stats = git_ops.cleanupRuntimeArtifacts(alloc, serve_cfg.cache_root, "/tmp");
        log.info(
            "runtime cleanup startup removed_worktrees={d} failed_worktrees={d} pruned_bare={d} failed_prunes={d}",
            .{ stats.removed_worktrees, stats.failed_worktree_removals, stats.pruned_bare_repos, stats.failed_bare_prunes },
        );
    }

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

    const posthog_api_key = std.process.getEnvVarOwned(alloc, "POSTHOG_API_KEY") catch null;
    defer if (posthog_api_key) |key| alloc.free(key);
    const ph_client: ?*posthog.PostHogClient = if (posthog_api_key) |key| blk: {
        break :blk posthog.init(alloc, .{
            .api_key = key,
            .host = "https://us.i.posthog.com",
            .flush_interval_ms = 10_000,
            .flush_at = 20,
            .max_retries = 3,
        }) catch |err| {
            obs_log.logWarnErr(.zombied, err, "posthog init failed; analytics disabled", .{});
            break :blk null;
        };
    } else null;
    defer if (ph_client) |client| client.deinit();
    ctx.posthog = ph_client;

    if (serve_cfg.oidc_enabled) {
        log.info("phase=oidc_init status=start provider={s}", .{@tagName(serve_cfg.oidc_provider)});
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
        log.info("phase=oidc_init status=ok", .{});
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
        .posthog = ph_client,
    };

    shutdown_requested.store(false, .release);
    installSignalHandlers();

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

    log.info("HTTP server starting port={d} worker_concurrency={d} api_threads={d} api_workers={d} api_max_clients={d} api_max_in_flight={d}", .{
        serve_cfg.port,
        worker_count,
        serve_cfg.api_http_threads,
        serve_cfg.api_http_workers,
        serve_cfg.api_max_clients,
        serve_cfg.api_max_in_flight_requests,
    });
    http_server.serve(&ctx, .{
        .port = serve_cfg.port,
        .threads = serve_cfg.api_http_threads,
        .workers = serve_cfg.api_http_workers,
        .max_clients = @intCast(serve_cfg.api_max_clients),
    }) catch |err| {
        obs_log.logErr(.zombied, err, "http server exited with error", .{});
    };

    wstate.running.store(false, .release);
    shutdown_requested.store(true, .release);
    event_bus.stop();
    for (worker_threads) |*t| t.join();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();
    {
        const stats = git_ops.cleanupRuntimeArtifacts(alloc, serve_cfg.cache_root, "/tmp");
        log.info(
            "runtime cleanup shutdown removed_worktrees={d} failed_worktrees={d} pruned_bare={d} failed_prunes={d}",
            .{ stats.removed_worktrees, stats.failed_worktree_removals, stats.pruned_bare_repos, stats.failed_bare_prunes },
        );
    }
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

test "integration: migrate_on_start env parser accepts deterministic values" {
    const alloc = std.testing.allocator;
    try std.posix.setenv("MIGRATE_ON_START", "1", true);
    try std.testing.expect(try common.migrateOnStartEnabledFromEnv(alloc));
    try std.posix.setenv("MIGRATE_ON_START", "0", true);
    try std.testing.expect(!try common.migrateOnStartEnabledFromEnv(alloc));
}
