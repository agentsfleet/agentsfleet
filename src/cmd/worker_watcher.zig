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
const executor_client = @import("../executor/client.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = std.log.scoped(.worker_watcher);

/// XREADGROUP idle wakeup interval. Short enough that drain / shutdown is
/// observed promptly, long enough that idle workers do not hammer Redis.
const block_ms = "5000";
const batch_count = "16";

/// Reconcile sweep cadence. The watcher walks core.zombies every Nth tick
/// (≈ N × 5s) and calls spawnZombieThread for every active row. Picks up:
/// orphans from a failed install-time XADD; missed zombie_created control
/// messages; any drift between PG state and the watcher's in-memory map.
/// 6 ticks ≈ 30 seconds — bounded staleness, low PG load.
const reconcile_every_ticks: u32 = 6;

pub const WatcherConfig = struct {
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
    /// All allocations + `Thread.spawn` happen BEFORE either map publish,
    /// so the `errdefer` chain unwinds linearly with no map mutation —
    /// closes the original P0-2 use-after-free where a `Thread.spawn`
    /// failure left a freed key inside `cancel_flags`.
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

        const wcfg = worker_zombie.ZombieWorkerConfig{
            .pool = self.cfg.pool,
            .zombie_id = id_for_thread,
            .shutdown_requested = self.cfg.shutdown_requested,
            .executor = self.cfg.executor,
            .workspace_path = self.cfg.workspace_path,
            .telemetry = self.cfg.telemetry,
            .cancel_flag = &runtime.cancel,
            .worker_state = self.cfg.worker_state,
        };
        const thread = try std.Thread.spawn(
            .{},
            runtime_mod.zombieRuntimeWrapper,
            .{ runtime, self.alloc, wcfg },
        );
        // After Thread.spawn the wrapper is live; we cannot unwind it.
        // The wrapper will run to completion regardless of the map.put
        // outcomes below. If a put fails the wrapper still exits cleanly
        // and a future sweep is a no-op for absent entries.

        try self.runtimes.put(id_for_runtimes, runtime);
        errdefer _ = self.runtimes.remove(id_for_runtimes);

        try self.threads.put(id_for_threads, thread);

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
            self.pollOnce() catch |err| {
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

    /// Periodic sweep against PG state. Walks core.zombies WHERE status='active'
    /// and calls spawnZombieThread for each id; idempotent on duplicates,
    /// recovers orphans within ≤reconcile_every_ticks × 5s.
    fn reconcileTick(self: *Watcher) !void {
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

    fn shouldKeepRunning(self: *const Watcher) bool {
        if (self.cfg.shutdown_requested.load(.acquire)) return false;
        if (!self.cfg.worker_state.isAcceptingWork()) return false;
        return true;
    }

    fn pollOnce(self: *Watcher) !void {
        var resp = try self.cfg.redis.command(&.{
            "XREADGROUP",
            "GROUP",
            control_stream.consumer_group,
            self.cfg.consumer_name,
            "COUNT",
            batch_count,
            "BLOCK",
            block_ms,
            "STREAMS",
            control_stream.stream_key,
            ">",
        });
        defer resp.deinit(self.cfg.redis.alloc);

        const entries = navigateEntries(resp) catch |err| switch (err) {
            error.NoEntries => return,
            else => return err,
        };
        for (entries) |*entry_val| {
            self.processEntry(entry_val.*) catch |err| {
                log.err("watcher.entry_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{@errorName(err)});
            };
        }
    }

    fn processEntry(self: *Watcher, entry: redis_protocol.RespValue) !void {
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
            .zombie_config_changed => |m| log.info(
                "watcher.config_changed zombie_id={s} revision={d}",
                .{ m.zombie_id, m.config_revision },
            ),
            // {d} formatter handles both u32 and i64 — no type-specific change needed here.
            .worker_drain_request => |m| self.requestDrain(m.reason),
        }
    }

    fn cancelZombie(self: *Watcher, zombie_id: []const u8) void {
        // Hold map_lock across the cancel.store to close the UAF window
        // the lazy-sweep wrapper opens: between get-returning-pointer and
        // the store, no other thread can sweep the runtime.
        // Released BEFORE the executor RPC — that's Redis I/O and may block.
        self.map_lock.lock();
        var found: bool = false;
        if (self.runtimes.get(zombie_id)) |rt| {
            if (!rt.exited.load(.acquire)) {
                rt.cancel.store(true, .release);
                found = true;
            }
        }
        self.map_lock.unlock();

        if (found) {
            log.info("watcher.cancel_set zombie_id={s}", .{zombie_id});
        } else {
            log.debug("watcher.cancel_skip reason=not_local_or_exited zombie_id={s}", .{zombie_id});
            return;
        }

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

/// Navigate the XREADGROUP response shape:
///   [["zombie:control", [[msg_id, [k, v, ...]], ...]]]
/// Returns the inner entry array, or `error.NoEntries` if Redis returned
/// nil (BLOCK timeout with no messages).
fn navigateEntries(resp: redis_protocol.RespValue) ![]redis_protocol.RespValue {
    if (resp != .array) return error.NoEntries;
    const top = resp.array orelse return error.NoEntries;
    if (top.len == 0) return error.NoEntries;
    if (top[0] != .array) return error.WatcherMalformedResp;
    const stream_tuple = top[0].array orelse return error.WatcherMalformedResp;
    if (stream_tuple.len != 2) return error.WatcherMalformedResp;
    if (stream_tuple[1] != .array) return error.WatcherMalformedResp;
    const entries = stream_tuple[1].array orelse return error.NoEntries;
    if (entries.len == 0) return error.NoEntries;
    return entries;
}
