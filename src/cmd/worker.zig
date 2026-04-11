const std = @import("std");

const worker_config = @import("worker_config.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const obs_log = @import("../observability/logging.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const error_codes = @import("../errors/codes.zig");
const preflight = @import("preflight.zig");
const executor_client = @import("../executor/client.zig");
const worker_zombie = @import("worker_zombie.zig");

const log = std.log.scoped(.worker);

var shutdown_requested = std.atomic.Value(bool).init(false);

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher(event_bus: *events_bus.Bus) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    event_bus.stop();
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
            worker_config.ValidationError.InvalidDrainTimeoutMs,
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

    var tel = preflight.initTelemetry(alloc);
    defer tel.deinit(alloc);

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
            tel.ptr().capture(telemetry_mod.StartupFailed, .{ .command = "worker", .phase = "executor_connect", .reason = @errorName(err), .error_code = error_codes.ERR_EXEC_STARTUP_POSTURE });
            std.process.exit(1);
        };
        exec_client = ec;
        log.info("startup.executor_connect status=ok socket={s}", .{path});
    } else {
        log.info("startup.executor_mode status=direct hint=set_EXECUTOR_SOCKET_PATH_to_enable", .{});
    }
    defer if (exec_client) |*ec| ec.close();

    const worker_pool = preflight.connectDbPool(alloc, .worker) catch |err| {
        tel.ptr().capture(telemetry_mod.StartupFailed, .{ .command = "worker", .phase = "db_connect", .reason = @errorName(err), .error_code = error_codes.ERR_STARTUP_DB_CONNECT });
        tel.deinit(alloc);
        std.process.exit(1);
    };
    defer worker_pool.deinit();

    preflight.checkMigrations(worker_pool, false) catch |err| {
        tel.ptr().capture(telemetry_mod.StartupFailed, .{ .command = "worker", .phase = "migration_check", .reason = @errorName(err), .error_code = error_codes.ERR_STARTUP_MIGRATION_CHECK });
        tel.deinit(alloc);
        std.process.exit(1);
    };

    _ = preflight.prepareCacheRoot(alloc, worker_cfg.cache_root, "startup");

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

    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{&event_bus});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    // Spawn zombie worker threads (one per active zombie).
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

    for (zombie_threads.items) |*t| t.join();
    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();

    _ = preflight.prepareCacheRoot(alloc, worker_cfg.cache_root, "shutdown");
}
