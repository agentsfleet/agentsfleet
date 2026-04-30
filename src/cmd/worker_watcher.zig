//! Worker control-plane watcher.
//!
//! One thread per worker process. Subscribes to the fleet-wide Redis stream
//! `zombie:control` (consumer group `zombie_workers`) and dispatches each
//! lifecycle signal to a handler:
//!
//!   - `zombie_created`        → spawn a per-zombie worker thread.
//!   - `zombie_status_changed` → flip the per-zombie cancel flag (and ask
//!                               the executor to abort any in-flight call).
//!   - `zombie_config_changed` → log; full hot-reload deferred.
//!   - `worker_drain_request`  → kick the WorkerState drain phase.
//!
//! Owns the per-zombie runtime map shared with per-zombie threads. Each
//! runtime carries a cancel atomic + exited atomic; the wrapper at
//! `worker_watcher_runtime.zombieRuntimeWrapper` flips exited on return,
//! and `sweepExitedLocked` (called from `spawnZombieThread`) reaps the
//! entry on the next spawn attempt. Map storage is freed in `deinit()`
//! after every spawned thread has been joined — never sooner.

const std = @import("std");
const pg = @import("pg");

const queue_redis = @import("../queue/redis_client.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const control_stream = @import("../zombie/control_stream.zig");
const worker_state_mod = @import("worker/state.zig");
const worker_zombie = @import("worker_zombie.zig");
const runtime_mod = @import("worker_watcher_runtime.zig");
const poll_mod = @import("worker_watcher_poll.zig");
const executor_client = @import("../executor/client.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = std.log.scoped(.worker_watcher);

/// Reconcile sweep cadence. The watcher walks core.zombies every Nth tick
/// (≈ N × 5s) and calls spawnZombieThread for every active row. Picks up:
/// orphans from a failed install-time XADD; missed zombie_created control
/// messages; any drift between PG state and the watcher's in-memory map.
/// 6 ticks ≈ 30 seconds — bounded staleness, low PG load.
const reconcile_every_ticks: u32 = 6;

const WatcherConfig = struct {
    redis: *queue_redis.Client,
    pool: *pg.Pool,
    executor: ?*executor_client.ExecutorClient,
    workspace_path: []const u8,
    telemetry: ?*telemetry_mod.Telemetry,
    worker_state: *worker_state_mod.WorkerState,
    shutdown_requested: *const std.atomic.Value(bool),
    /// Stable consumer name within `zombie_workers`. Worker.run derives this
    /// from the hostname + pid so multiple replicas land in distinct slots.
    consumer_name: []const u8,
};

pub const Watcher = struct {
    alloc: std.mem.Allocator,
    cfg: WatcherConfig,
    /// Per-zombie runtime (cancel + exited atomics). See `worker_watcher_runtime.zig`.
    runtimes: std.StringHashMap(*runtime_mod.ZombieRuntime),
    threads: std.StringHashMap(std.Thread),
    map_lock: std.Thread.Mutex = .{},

    pub fn init(alloc: std.mem.Allocator, cfg: WatcherConfig) Watcher {
        return .{
            .alloc = alloc,
            .cfg = cfg,
            .runtimes = std.StringHashMap(*runtime_mod.ZombieRuntime).init(alloc),
            .threads = std.StringHashMap(std.Thread).init(alloc),
        };
    }

    /// Joins every spawned per-zombie thread (must already have observed
    /// drain / cancel) and frees the runtime map. Caller must have
    /// stopped the run loop before this point. Threads that the wrapper
    /// already detached via `sweepExitedLocked` are not in the threads
    /// map, so this iteration only joins still-live entries.
    pub fn deinit(self: *Watcher) void {
        var thr_it = self.threads.iterator();
        while (thr_it.next()) |entry| {
            entry.value_ptr.*.join();
            self.alloc.free(entry.key_ptr.*);
        }
        self.threads.deinit();

        var rt_it = self.runtimes.iterator();
        while (rt_it.next()) |entry| {
            self.alloc.free(entry.key_ptr.*);
            self.alloc.destroy(entry.value_ptr.*);
        }
        self.runtimes.deinit();
    }

    /// Spawn a per-zombie thread. Idempotent — a duplicate call for a
    /// zombie whose wrapper has not exited returns without doing work.
    /// Calls for a zombie whose previous wrapper already exited (early
    /// failure path: Redis connect, claim, missing executor) reap the
    /// stale entry first, then spawn fresh — no permanent stuck state.
    ///
    /// Memory ownership rules (greptile P1 review on PR #251):
    ///   - `id_for_runtimes` / `id_for_threads`: each map's `kv.key`,
    ///     freed by `sweepExitedLocked` (or `deinit`) when the entry is
    ///     removed.
    ///   - `id_for_thread`: passed to the worker via `cfg.zombie_id` and
    ///     freed by `zombieRuntimeWrapper` after `zombieWorkerLoop`
    ///     returns. Wrapper is the ONLY owner once `Thread.spawn`
    ///     succeeds.
    ///   - `runtime`: held by the live wrapper AND the runtimes map; the
    ///     map's `sweepExitedLocked` destroys it once the wrapper signals
    ///     `exited`.
    ///
    /// Failure-handling rules:
    ///   - All fallible work — including hashmap capacity reservation —
    ///     happens BEFORE `Thread.spawn`. After the spawn the only
    ///     remaining mutations are `putAssumeCapacity` (infallible) and
    ///     a log line. So no `errdefer` ever fires while the wrapper is
    ///     live, closing the OOM-driven UAF where `errdefer
    ///     destroy(runtime)` could free a struct the wrapper still held.
    pub fn spawnZombieThread(self: *Watcher, zombie_id: []const u8) !void {
        self.map_lock.lock();
        defer self.map_lock.unlock();

        // Reap any wrapper that already returned (early-exit path) so the
        // `contains` check below is the authoritative "live thread" probe.
        try runtime_mod.sweepExitedLocked(self.alloc, &self.runtimes, &self.threads);

        if (self.runtimes.contains(zombie_id)) {
            log.debug("watcher.spawn_skipped reason=already_running zombie_id={s}", .{zombie_id});
            return;
        }

        // Self-heal: bootstrap may be walking a zombie row whose install-time
        // XADD failed (see `publishInstallSignals` in zombies/create.zig).
        // Idempotent (BUSYGROUP-as-success).
        try control_stream.ensureZombieEventsGroup(self.cfg.redis, zombie_id);

        const runtime = try self.alloc.create(runtime_mod.ZombieRuntime);
        errdefer self.alloc.destroy(runtime);
        runtime.* = runtime_mod.ZombieRuntime.init();

        const id_for_runtimes = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_for_runtimes);

        const id_for_thread = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_for_thread);

        const id_for_threads = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_for_threads);

        // Reserve a slot in each map BEFORE Thread.spawn so the post-spawn
        // putAssumeCapacity calls are infallible. Without this, a hashmap
        // OOM after spawn-success would fire `errdefer destroy(runtime)`
        // while the wrapper still held the runtime pointer (UAF).
        try self.runtimes.ensureUnusedCapacity(1);
        try self.threads.ensureUnusedCapacity(1);

        const wcfg = worker_zombie.ZombieWorkerConfig{
            .pool = self.cfg.pool,
            .zombie_id = id_for_thread,
            .shutdown_requested = self.cfg.shutdown_requested,
            .executor = self.cfg.executor,
            .workspace_path = self.cfg.workspace_path,
            .telemetry = self.cfg.telemetry,
            .cancel_flag = &runtime.cancel,
            .reload_pending = &runtime.reload_pending,
            .worker_state = self.cfg.worker_state,
        };
        const thread = try std.Thread.spawn(
            .{},
            runtime_mod.zombieRuntimeWrapper,
            .{ runtime, self.alloc, wcfg },
        );
        // ── Post-spawn: NOTHING below may fail. Wrapper now owns `runtime`
        // and `id_for_thread`. Every remaining call is infallible.

        self.runtimes.putAssumeCapacity(id_for_runtimes, runtime);
        self.threads.putAssumeCapacity(id_for_threads, thread);

        log.info("watcher.spawned zombie_id={s}", .{zombie_id});
    }

    /// Main XREADGROUP loop. Returns when shutdown_requested fires or the
    /// WorkerState leaves the running phase. Caller spawns this in a thread.
    /// Every `reconcile_every_ticks` poll cycles, the loop also runs a PG
    /// sweep so a zombie row whose install-time XADD failed gets picked up
    /// without waiting for the next worker restart.
    pub fn run(self: *Watcher) void {
        log.info("watcher.start consumer={s}", .{self.cfg.consumer_name});
        var ticks_since_reconcile: u32 = 0;
        while (self.shouldKeepRunning()) {
            poll_mod.pollOnce(self) catch |err| {
                log.err("watcher.poll_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{@errorName(err)});
                std.Thread.sleep(100 * std.time.ns_per_ms);
            };
            ticks_since_reconcile += 1;
            if (ticks_since_reconcile >= reconcile_every_ticks) {
                self.reconcileTick() catch |err| {
                    log.warn("watcher.reconcile_fail err={s}", .{@errorName(err)});
                };
                ticks_since_reconcile = 0;
            }
        }
        log.info("watcher.stop consumer={s}", .{self.cfg.consumer_name});
    }

    /// Periodic two-direction sweep against PG state.
    ///
    /// 1. `status='active'` rows → `spawnZombieThread` (idempotent). Heals
    ///    "PG says active, watcher has no thread" — e.g. install-time
    ///    `publishInstallSignals` exhausted retries before the rollback
    ///    landed and the orphan row needs to be claimed.
    /// 2. `status != 'active'` rows whose runtime is currently in the map
    ///    → `cancelZombie`. Heals "PG says killed/paused, watcher thread
    ///    still running" — e.g. the kill handler's `publishKillSignal`
    ///    XADD failed (greptile P1 on PR #251). Without this branch the
    ///    thread keeps consuming events until worker restart.
    fn reconcileTick(self: *Watcher) !void {
        try self.reconcileSpawnActive();
        try self.reconcileCancelNonActive();
    }

    fn reconcileSpawnActive(self: *Watcher) !void {
        const ids = try worker_zombie.listActiveZombieIds(self.cfg.pool, self.alloc);
        defer {
            for (ids) |id| self.alloc.free(id);
            if (ids.len > 0) self.alloc.free(ids);
        }
        for (ids) |zombie_id| {
            self.spawnZombieThread(zombie_id) catch |err| {
                log.warn("watcher.reconcile_spawn_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
            };
        }
    }

    fn reconcileCancelNonActive(self: *Watcher) !void {
        const ids = try worker_zombie.listNonActiveZombieIds(self.cfg.pool, self.alloc);
        defer {
            for (ids) |id| self.alloc.free(id);
            if (ids.len > 0) self.alloc.free(ids);
        }
        for (ids) |zombie_id| {
            // cancelZombie is a no-op if the runtime isn't local or has
            // already exited — safe to call on every non-active id.
            self.cancelZombie(zombie_id);
        }
    }

    fn shouldKeepRunning(self: *const Watcher) bool {
        if (self.cfg.shutdown_requested.load(.acquire)) return false;
        if (!self.cfg.worker_state.isAcceptingWork()) return false;
        return true;
    }

    /// Public so `worker_watcher_poll.zig` (sibling module that owns the
    /// XREADGROUP loop) can dispatch entries through the watcher's
    /// decoder + handler chain without needing to live in this file.
    pub fn processEntry(self: *Watcher, entry: redis_protocol.RespValue) !void {
        if (entry != .array) return error.WatcherMalformedEntry;
        const tuple = entry.array orelse return error.WatcherMalformedEntry;
        if (tuple.len != 2) return error.WatcherMalformedEntry;
        const msg_id = redis_protocol.valueAsString(tuple[0]) orelse return error.WatcherMalformedEntry;
        if (tuple[1] != .array) return error.WatcherMalformedEntry;
        const fields = tuple[1].array orelse return error.WatcherMalformedEntry;

        var decoded = try control_stream.decodeEntry(self.alloc, msg_id, fields);
        defer decoded.deinit(self.alloc);

        try self.dispatch(decoded.message);
        try self.xack(decoded.message_id);
    }

    fn dispatch(self: *Watcher, msg: control_stream.ControlMessage) !void {
        switch (msg) {
            .zombie_created => |m| try self.spawnZombieThread(m.zombie_id),
            .zombie_status_changed => |m| switch (m.status) {
                .killed, .paused => self.cancelZombie(m.zombie_id),
                .active => log.debug("watcher.status_active zombie_id={s}", .{m.zombie_id}),
            },
            .zombie_config_changed => |m| self.signalReload(m.zombie_id),
            // {d} formatter handles both u32 and i64 — no type-specific change needed here.
            .worker_drain_request => |m| self.requestDrain(m.reason),
        }
    }

    // Holds map_lock across cancel.store to close the UAF window the lazy-sweep
    // wrapper opens: between get-returning-pointer and the store, no other
    // thread can sweep the runtime. Lock released before the executor RPC,
    // which is Redis I/O and may block.
    fn tryMarkCancel(self: *Watcher, zombie_id: []const u8) bool {
        self.map_lock.lock();
        defer self.map_lock.unlock();
        if (self.runtimes.get(zombie_id)) |rt| {
            if (!rt.exited.load(.acquire)) {
                rt.cancel.store(true, .release);
                return true;
            }
        }
        return false;
    }

    /// §9 hot-reload — flip reload_pending under map_lock so the
    /// runtime pointer can't sweep mid-store. No-op for non-local zombies.
    fn signalReload(self: *Watcher, zombie_id: []const u8) void {
        self.map_lock.lock();
        defer self.map_lock.unlock();
        if (self.runtimes.get(zombie_id)) |rt| {
            if (!rt.exited.load(.acquire)) rt.reload_pending.store(true, .release);
        }
    }

    fn cancelZombie(self: *Watcher, zombie_id: []const u8) void {
        if (!self.tryMarkCancel(zombie_id)) {
            log.debug("watcher.cancel_skip reason=not_local_or_exited zombie_id={s}", .{zombie_id});
            return;
        }
        log.info("watcher.cancel_set zombie_id={s}", .{zombie_id});

        if (self.cfg.executor) |exec| {
            exec.cancelExecution(zombie_id) catch |err| {
                log.warn("watcher.executor_cancel_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
            };
        }
    }

    fn requestDrain(self: *Watcher, reason: ?[]const u8) void {
        const started = self.cfg.worker_state.startDrain();
        const r = reason orelse "control";
        if (started) {
            log.info("watcher.drain_started reason={s}", .{r});
        } else {
            log.debug("watcher.drain_already_in_progress reason={s}", .{r});
        }
    }

    fn xack(self: *Watcher, msg_id: []const u8) !void {
        var resp = try self.cfg.redis.command(&.{
            "XACK",
            control_stream.stream_key,
            control_stream.consumer_group,
            msg_id,
        });
        defer resp.deinit(self.cfg.redis.alloc);
        switch (resp) {
            .integer => |n| if (n < 0) return error.WatcherXackFailed,
            else => return error.WatcherXackFailed,
        }
    }
};
