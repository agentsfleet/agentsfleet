const std = @import("std");

const db = @import("../db/pool.zig");
const runtime_config = @import("../config/runtime.zig");
const env_vars = @import("../config/env_vars.zig");
const events_bus = @import("../events/bus.zig");
const worker = @import("../pipeline/worker.zig");
const git_ops = @import("../git/ops.zig");
const obs_log = @import("../observability/logging.zig");
const common = @import("common.zig");

const log = std.log.scoped(.worker);

var shutdown_requested = std.atomic.Value(bool).init(false);

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
}

pub fn run(alloc: std.mem.Allocator) !void {
    log.info("starting zombied worker", .{});

    env_vars.enforceFromEnvWithMode(alloc, .worker) catch |err| {
        switch (err) {
            env_vars.EnvVarsErrors.MissingDatabaseUrlWorker => std.debug.print("fatal: DATABASE_URL_WORKER not set\n", .{}),
            env_vars.EnvVarsErrors.MissingRedisUrlWorker => std.debug.print("fatal: REDIS_URL_WORKER not set\n", .{}),
            env_vars.EnvVarsErrors.RedisWorkerTlsRequired => std.debug.print("fatal: REDIS_URL_WORKER must use rediss:// (TLS required)\n", .{}),
            else => std.debug.print("fatal: worker env validation failed: {}\n", .{err}),
        }
        std.process.exit(1);
    };

    var worker_cfg = runtime_config.WorkerConfig.load(alloc) catch |err| {
        switch (err) {
            runtime_config.ValidationError.MissingGitHubAppId,
            runtime_config.ValidationError.MissingGitHubAppPrivateKey,
            runtime_config.ValidationError.InvalidMaxAttempts,
            runtime_config.ValidationError.InvalidWorkerConcurrency,
            runtime_config.ValidationError.InvalidRunTimeoutMs,
            runtime_config.ValidationError.InvalidRateLimitCapacity,
            runtime_config.ValidationError.InvalidRateLimitRefillPerSec,
            => runtime_config.ServeConfig.printValidationError(@errorCast(err)),
            else => std.debug.print("fatal: failed to load worker config: {}\n", .{err}),
        }
        std.process.exit(1);
    };
    defer worker_cfg.deinit();

    const worker_pool = db.initFromEnvForRole(alloc, .worker) catch |err| {
        std.debug.print("fatal: worker database init failed: {}\n", .{err});
        std.process.exit(1);
    };
    defer worker_pool.deinit();

    common.enforceServeMigrationSafety(worker_pool, false) catch |err| {
        switch (err) {
            common.MigrationGuardError.MigrationPending => std.debug.print(
                "fatal: pending schema migrations; run `zombied migrate` before worker startup\n",
                .{},
            ),
            common.MigrationGuardError.MigrationFailed => std.debug.print(
                "fatal: unsafe migration failure state detected; inspect schema_migration_failures and rerun `zombied migrate`\n",
                .{},
            ),
            common.MigrationGuardError.MigrationSchemaAhead => std.debug.print(
                "fatal: database schema version is ahead of this binary; deploy matching binary before worker startup\n",
                .{},
            ),
            else => std.debug.print("fatal: schema migration safety check failed: {}\n", .{err}),
        }
        std.process.exit(1);
    };

    std.fs.makeDirAbsolute(worker_cfg.cache_root) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => obs_log.logWarnErr(.worker, err, "could not create cache root {s}", .{worker_cfg.cache_root}),
    };
    {
        const stats = git_ops.cleanupRuntimeArtifacts(alloc, worker_cfg.cache_root, "/tmp");
        log.info(
            "worker cleanup startup removed_worktrees={d} failed_worktrees={d} pruned_bare={d} failed_prunes={d}",
            .{ stats.removed_worktrees, stats.failed_worktree_removals, stats.pruned_bare_repos, stats.failed_bare_prunes },
        );
    }

    var wstate = worker.WorkerState.init();
    shutdown_requested.store(false, .release);
    installSignalHandlers();

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
    };

    for (worker_threads) |*t| {
        t.* = try std.Thread.spawn(.{}, worker.workerLoop, .{ pipeline_cfg, &wstate });
        spawned_workers += 1;
    }
    signal_thread = try std.Thread.spawn(.{}, signalWatcher, .{&wstate});
    event_thread = try std.Thread.spawn(.{}, events_bus.runThread, .{&event_bus});

    log.info("worker threads started concurrency={d}", .{thread_count});

    for (worker_threads) |*t| t.join();
    shutdown_requested.store(true, .release);
    event_bus.stop();
    if (signal_thread) |*t| t.join();
    if (event_thread) |*t| t.join();

    {
        const stats = git_ops.cleanupRuntimeArtifacts(alloc, worker_cfg.cache_root, "/tmp");
        log.info(
            "worker cleanup shutdown removed_worktrees={d} failed_worktrees={d} pruned_bare={d} failed_prunes={d}",
            .{ stats.removed_worktrees, stats.failed_worktree_removals, stats.pruned_bare_repos, stats.failed_bare_prunes },
        );
    }
}
