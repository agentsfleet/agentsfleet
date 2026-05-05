const std = @import("std");
const builtin = @import("builtin");

const worker_config = @import("worker_config.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const obs_log = @import("../observability/logging.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const error_codes = @import("../errors/error_registry.zig");
const preflight = @import("preflight.zig");
const executor_client = @import("../executor/client.zig");
const worker_zombie = @import("worker_zombie.zig");
const worker_watcher = @import("worker_watcher.zig");
const worker_state_mod = @import("worker/state.zig");
const queue_redis = @import("../queue/redis_client.zig");
const control_stream = @import("../zombie/control_stream.zig");

const log = std.log.scoped(.worker);

var shutdown_requested = std.atomic.Value(bool).init(false);

fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    shutdown_requested.store(true, .release);
}

fn signalWatcher(event_bus: *events_bus.Bus, ws: *worker_state_mod.WorkerState) void {
    while (!shutdown_requested.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    // Triggers WorkerState drain so per-zombie threads exit at the next loop
    // iteration (they poll isAcceptingWork() between events). Idempotent for
    // repeated SIGTERM.
    _ = ws.startDrain();
    event_bus.stop();
}

fn makeConsumerName(alloc: std.mem.Allocator) ![]u8 {
    var host_buf: [std.posix.HOST_NAME_MAX]u8 = undefined;
    const host = std.posix.gethostname(&host_buf) catch "localhost";
    // Hostname alone collides on crash-restart and on multi-process replicas
    // sharing a host (greptile P1 on PR #251). PID disambiguates so each
    // worker process lands in a distinct slot inside the `zombie_workers`
    // consumer group, preventing PEL sharing + double-dispatch.
    const pid: i32 = if (builtin.os.tag == .linux)
        std.os.linux.getpid()
    else
        @intCast(std.c.getpid());
    return std.fmt.allocPrint(alloc, "worker-control-{s}-{d}", .{ host, pid });
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

    shutdown_requested.store(false, .release);
    preflight.installSignalHandlers(onSignal);

    var event_bus = events_bus.Bus.init();
    events_bus.install(&event_bus);
    defer events_bus.uninstall();

    var ws = worker_state_mod.WorkerState.init();

    var signal_thread: ?std.Thread = null;
    var event_thread: ?std.Thread = null;
    errdefer {
        shutdown_requested.store(true, .release);
        _ = ws.startDrain();
        event_bus.stop();
        if (signal_thread) |*t| t.join();
        if (event_thread) |*t| t.join();
    }

    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{ &event_bus, &ws });
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    var watcher_redis = queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        log.err("startup.watcher_redis_connect status=fail error_code={s} err={s}", .{ error_codes.ERR_STARTUP_REDIS_CONNECT, @errorName(err) });
        std.process.exit(1);
    };
    defer watcher_redis.deinit();

    control_stream.ensureControlGroup(&watcher_redis) catch |err| {
        log.err("startup.control_group_ensure status=fail error_code={s} err={s}", .{ error_codes.ERR_STARTUP_REDIS_CONNECT, @errorName(err) });
        std.process.exit(1);
    };

    const consumer_name = try makeConsumerName(alloc);
    defer alloc.free(consumer_name);

    var watcher = worker_watcher.Watcher.init(alloc, .{
        .redis = &watcher_redis,
        .pool = worker_pool,
        .executor = if (exec_client) |*ec| ec else null,
        .workspace_path = "/tmp/zombie",
        .telemetry = tel.ptr(),
        .worker_state = &ws,
        .shutdown_requested = &shutdown_requested,
        .consumer_name = consumer_name,
    });
    // Trigger drain before joining zombie threads so they exit cleanly even
    // on an error path. Idempotent — startDrain() returns false if already
    // draining, watcher.deinit() then joins what's there.
    defer {
        shutdown_requested.store(true, .release);
        _ = ws.startDrain();
        watcher.deinit();
    }

    // Bootstrap-spawn currently-active zombies before the watcher starts
    // claiming control-stream messages. Idempotent on duplicates: the
    // watcher's spawnZombieThread no-ops if the zombie is already mapped
    // (covers the case where `zombie_created` is still in the stream backlog).
    const zombie_ids: [][]const u8 = worker_zombie.listActiveZombieIds(worker_pool, alloc) catch |err| blk: {
        log.warn("worker.zombie_discovery_failed err={s} hint=no_zombies_will_run", .{@errorName(err)});
        break :blk &.{};
    };
    defer {
        for (zombie_ids) |id| alloc.free(id);
        if (zombie_ids.len > 0) alloc.free(zombie_ids);
    }
    for (zombie_ids) |zombie_id| {
        watcher.spawnZombieThread(zombie_id) catch |err| {
            log.err("worker.bootstrap_spawn_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        };
    }
    if (zombie_ids.len > 0)
        log.info("worker.bootstrap_zombies count={d}", .{zombie_ids.len});

    var watcher_thread = try std.Thread.spawn(.{}, worker_watcher.Watcher.run, .{&watcher});
    watcher_thread.join();

    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();

    // Final drain transition. Per-zombie threads have observed drain via the
    // watcher's watchShutdown poller and are exiting; watcher.deinit() (defer
    // above) will join them. The completeDrain CAS asserts we passed through
    // `draining` — if it wasn't entered we skip rather than panic.
    if (ws.getDrainPhase() == .draining) ws.completeDrain();
}
