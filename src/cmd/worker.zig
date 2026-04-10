const std = @import("std");

const worker_config = @import("worker_config.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const worker = @import("../pipeline/worker.zig");
const obs_log = @import("../observability/logging.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const error_codes = @import("../errors/codes.zig");
const preflight = @import("preflight.zig");
const sandbox_runtime = @import("../pipeline/sandbox_runtime.zig");
const executor_client = @import("../executor/client.zig");
const worker_zombie = @import("worker_zombie.zig");

const log = std.log.scoped(.worker);

const worker_state_mod = @import("../pipeline/worker_state.zig");

var shutdown_requested = std.atomic.Value(bool).init(false);

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

const DrainConfig = struct {
    drain_timeout_ms: u64 = 270_000,
};

fn signalWatcher(wstate: *worker.WorkerState, drain_cfg: DrainConfig) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }

    if (!wstate.startDrain()) {
        return;
    }
    log.info("worker.drain_start status=draining", .{});

    const drain_start_ms = std.time.milliTimestamp();
    const deadline_ms: i64 = drain_start_ms + @as(i64, @intCast(drain_cfg.drain_timeout_ms));

    while (true) {
        const in_flight = wstate.currentInFlightRuns();
        const elapsed_ms: u64 = @intCast(@max(std.time.milliTimestamp() - drain_start_ms, 0));

        if (in_flight == 0) {
            log.info("worker.drain_complete status=drained elapsed_ms={d}", .{elapsed_ms});
            break;
        }

        if (std.time.milliTimestamp() >= deadline_ms) {
            log.warn("worker.drain_timeout status=timeout in_flight={d} elapsed_ms={d} timeout_ms={d}", .{
                in_flight,
                elapsed_ms,
                drain_cfg.drain_timeout_ms,
            });
            break;
        }

        log.info("worker.drain_progress in_flight={d} elapsed_ms={d}", .{ in_flight, elapsed_ms });
        std.Thread.sleep(5 * std.time.ns_per_s);
    }

    wstate.completeDrain();
}

pub fn run(alloc: std.mem.Allocator) !void {
    preflight.initOtelLogs(alloc);
    defer preflight.deinitOtelLogs();
    preflight.initOtelTraces(alloc);
    defer preflight.deinitOtelTraces();
    log.info("startup.worker status=start", .{});

    log.info("startup.env_check status=start", .{});
    env_vars.enforceFromEnvWithMode(alloc, .worker) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=DATABASE_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER not set", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err=REDIS_URL_WORKER must use rediss://", .{}),
            else => log.err("startup.env_check status=fail error_code=" ++ error_codes.ERR_STARTUP_ENV_CHECK ++ " err={s}", .{@errorName(err)}),
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
            worker_config.ValidationError.InvalidDrainTimeoutMs,
            worker_config.ValidationError.InvalidRateLimitCapacity,
            worker_config.ValidationError.InvalidRateLimitRefillPerSec,
            worker_config.ValidationError.InvalidSandboxBackend,
            worker_config.ValidationError.InvalidSandboxKillGraceMs,
            worker_config.ValidationError.InvalidExecutorStartupTimeoutMs,
            worker_config.ValidationError.InvalidExecutorLeaseTimeoutMs,
            worker_config.ValidationError.InvalidExecutorMemoryLimitMb,
            worker_config.ValidationError.InvalidExecutorCpuLimitPercent,
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

    const ph = preflight.initPostHog(alloc);
    defer ph.deinit(alloc);

    worker_cfg.sandbox.preflight() catch |err| {
        log.err("startup.sandbox_preflight status=fail error_code={s} backend={s} err={s}", .{
            error_codes.ERR_SANDBOX_BACKEND_UNAVAILABLE,
            worker_cfg.sandbox.label(),
            @errorName(err),
        });
        posthog_events.trackStartupFailed(ph.client, "worker", "sandbox_preflight", @errorName(err), error_codes.ERR_SANDBOX_BACKEND_UNAVAILABLE);
        std.process.exit(1);
    };
    log.info("startup.sandbox_preflight status=ok backend={s}", .{worker_cfg.sandbox.label()});

    // Executor client: connect to the zombied-executor daemon if configured.
    // The zombied-executor is a separate systemd service on the host — the
    // worker does NOT spawn it. It just connects to its Unix socket.
    var exec_client: ?executor_client.ExecutorClient = null;
    if (worker_cfg.executor_socket_path) |path| {
        log.info("startup.executor_mode status=enabled socket={s}", .{path});
        var ec = executor_client.ExecutorClient.init(alloc, path);
        ec.connect() catch |err| {
            log.err("startup.executor_connect status=fail error_code={s} socket={s} err={s}", .{
                error_codes.ERR_EXEC_STARTUP_POSTURE,
                path,
                @errorName(err),
            });
            posthog_events.trackStartupFailed(ph.client, "worker", "executor_connect", @errorName(err), error_codes.ERR_EXEC_STARTUP_POSTURE);
            std.process.exit(1);
        };
        exec_client = ec;
        log.info("startup.executor_connect status=ok socket={s}", .{path});
    } else {
        log.info("startup.executor_mode status=direct hint=set_EXECUTOR_SOCKET_PATH_to_enable", .{});
    }
    defer if (exec_client) |*ec| ec.close();

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
        wstate.completeDrain();
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
        .sandbox = worker_cfg.sandbox,
        .rate_limit_capacity = worker_cfg.rate_limit_capacity,
        .rate_limit_refill_per_sec = worker_cfg.rate_limit_refill_per_sec,
        .posthog = ph.client,
    };

    for (worker_threads) |*t| {
        t.* = try std.Thread.spawn(.{}, worker.workerLoop, .{ pipeline_cfg, &wstate });
        spawned_workers += 1;
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{ &wstate, DrainConfig{ .drain_timeout_ms = worker_cfg.drain_timeout_ms } });
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("worker.threads_started concurrency={d}", .{thread_count});
    posthog_events.trackWorkerStarted(ph.client, @intCast(thread_count));

    // M2_001: Spawn Zombie worker threads (1 per active Zombie).
    const zombie_ids: [][]const u8 = worker_zombie.listActiveZombieIds(worker_pool, alloc) catch |err| blk: {
        log.warn("worker.zombie_discovery_failed err={s} hint=no_zombies_will_run", .{@errorName(err)});
        break :blk &.{};
    };
    defer {
        for (zombie_ids) |id| alloc.free(id);
        if (zombie_ids.len > 0) alloc.free(zombie_ids);
    }

    var zombie_threads: std.ArrayList(std.Thread) = .{};
    defer zombie_threads.deinit(alloc);
    for (zombie_ids) |zombie_id| {
        const zt = std.Thread.spawn(.{}, worker_zombie.zombieWorkerLoop, .{
            alloc,
            worker_zombie.ZombieWorkerConfig{
                .pool = worker_pool,
                .zombie_id = zombie_id,
                .shutdown_requested = &shutdown_requested,
                .executor = if (exec_client) |*ec| ec else null,
            },
        }) catch |err| {
            log.err("worker.zombie_thread_spawn_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
            continue;
        };
        zombie_threads.append(alloc, zt) catch |err| {
            log.err("worker.zombie_thread_track_fail zombie_id={s} err={s} hint=thread_untracked", .{ zombie_id, @errorName(err) });
        };
    }
    if (zombie_ids.len > 0)
        log.info("worker.zombie_threads_started count={d}", .{zombie_ids.len});

    for (worker_threads) |*t| t.join();
    for (zombie_threads.items) |*t| t.join();
    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();

    _ = preflight.prepareCacheRoot(alloc, worker_cfg.cache_root, "shutdown");
}
