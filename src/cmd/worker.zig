const std = @import("std");

const worker_config = @import("worker_config.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const worker = @import("../pipeline/worker.zig");
const obs_log = @import("../observability/logging.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const error_codes = @import("../errors/codes.zig");
const langfuse = @import("../observability/langfuse.zig");
const preflight = @import("preflight.zig");

const log = std.log.scoped(.worker);

var shutdown_requested = std.atomic.Value(bool).init(false);

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher(wstate: *worker.WorkerState) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    wstate.running.store(false, .release);
}

pub fn run(alloc: std.mem.Allocator) !void {
    log.info("startup.worker status=start", .{});

    log.info("startup.env_check status=start", .{});
    env_vars.enforceFromEnvWithMode(alloc, .worker) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err=REDIS_URL_WORKER must use rediss://", .{}),
            else => log.err("startup.env_check status=fail error_code=UZ-STARTUP-001 err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    log.info("startup.env_check status=ok", .{});

    log.info("startup.config_load status=start", .{});
    var worker_cfg = worker_config.Config.load(alloc) catch |err| {
        switch (err) {
            worker_config.ValidationError.MissingGitHubAppId,
            worker_config.ValidationError.MissingGitHubAppPrivateKey,
            worker_config.ValidationError.InvalidMaxAttempts,
            worker_config.ValidationError.InvalidWorkerConcurrency,
            worker_config.ValidationError.InvalidRunTimeoutMs,
            worker_config.ValidationError.InvalidRateLimitCapacity,
            worker_config.ValidationError.InvalidRateLimitRefillPerSec,
            => {
                worker_config.printValidationError(@errorCast(err));
                log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)});
            },
            else => log.err("startup.config_load status=fail error_code=UZ-STARTUP-002 err={s}", .{@errorName(err)}),
        }
        std.process.exit(1);
    };
    defer worker_cfg.deinit();
    log.info("startup.config_load status=ok", .{});

    if (langfuse.configFromEnv(alloc)) |cfg| {
        langfuse.installAsyncExporter(alloc, cfg) catch |err| {
            alloc.free(cfg.host);
            alloc.free(cfg.public_key);
            alloc.free(cfg.secret_key);
            obs_log.logWarnErr(.worker, err, "startup.langfuse_init status=fail reason=fallback_mode", .{});
        };
        if (langfuse.isAsyncExporterInstalled()) {
            defer langfuse.uninstallAsyncExporter();
            log.info("startup.langfuse_init status=ok", .{});
        }
    }

    const ph = preflight.initPostHog(alloc);
    defer ph.deinit(alloc);

    const worker_pool = preflight.connectDbPool(alloc, .worker) catch |err| {
        posthog_events.trackStartupFailed(ph.client, "worker", "db_connect", @errorName(err), error_codes.ERR_STARTUP_DB_CONNECT);
        ph.deinit(alloc);
        std.process.exit(1);
    };
    defer worker_pool.deinit();

    preflight.checkMigrations(worker_pool, false) catch |err| {
        posthog_events.trackStartupFailed(ph.client, "worker", "migration_check", @errorName(err), error_codes.ERR_STARTUP_MIGRATION_CHECK);
        ph.deinit(alloc);
        std.process.exit(1);
    };

    _ = preflight.prepareCacheRoot(alloc, worker_cfg.cache_root, "startup");

    var wstate = worker.WorkerState.init();
    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    const thread_count: usize = @max(@as(usize, @intCast(worker_cfg.worker_concurrency)), 1);
    var worker_threads = try alloc.alloc(std.Thread, thread_count);
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

    const pipeline_cfg = worker.WorkerConfig{
        .pool = worker_pool,
        .config_dir = worker_cfg.config_dir,
        .cache_root = worker_cfg.cache_root,
        .github_app_id = worker_cfg.github_app_id,
        .github_app_private_key = worker_cfg.github_app_private_key,
        .pipeline_profile_path = worker_cfg.pipeline_profile_path,
        .max_attempts = worker_cfg.max_attempts,
        .run_timeout_ms = worker_cfg.run_timeout_ms,
        .rate_limit_capacity = worker_cfg.rate_limit_capacity,
        .rate_limit_refill_per_sec = worker_cfg.rate_limit_refill_per_sec,
        .posthog = ph.client,
    };

    for (worker_threads) |*t| {
        t.* = try std.Thread.spawn(.{}, worker.workerLoop, .{ pipeline_cfg, &wstate });
        spawned_workers += 1;
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{&wstate});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("worker.threads_started concurrency={d}", .{thread_count});
    posthog_events.trackWorkerStarted(ph.client, @intCast(thread_count));

    for (worker_threads) |*t| t.join();
    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();

    _ = preflight.prepareCacheRoot(alloc, worker_cfg.cache_root, "shutdown");
}
