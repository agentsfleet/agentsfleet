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
//! Owns the cancel-flag map shared with per-zombie threads. Cancel flags are
//! heap-allocated, kept alive for the life of the worker process, and freed
//! in `deinit()` after every spawned thread has been joined — never sooner.

const std = @import("std");
const pg = @import("pg");

const queue_redis = @import("../queue/redis_client.zig");
const redis_protocol = @import("../queue/redis_protocol.zig");
const control_stream = @import("../zombie/control_stream.zig");
const worker_state_mod = @import("worker/state.zig");
const worker_zombie = @import("worker_zombie.zig");
const executor_client = @import("../executor/client.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const error_codes = @import("../errors/error_registry.zig");

const log = std.log.scoped(.worker_watcher);

/// XREADGROUP idle wakeup interval. Short enough that drain / shutdown is
/// observed promptly, long enough that idle workers do not hammer Redis.
const block_ms = "5000";
const batch_count = "16";

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
    cancel_flags: std.StringHashMap(*std.atomic.Value(bool)),
    threads: std.StringHashMap(std.Thread),
    map_lock: std.Thread.Mutex = .{},

    pub fn init(alloc: std.mem.Allocator, cfg: WatcherConfig) Watcher {
        return .{
            .alloc = alloc,
            .cfg = cfg,
            .cancel_flags = std.StringHashMap(*std.atomic.Value(bool)).init(alloc),
            .threads = std.StringHashMap(std.Thread).init(alloc),
        };
    }

    /// Joins every spawned per-zombie thread (must already have observed
    /// drain / cancel) and frees the cancel-flag map. Caller must have
    /// stopped the run loop before this point.
    pub fn deinit(self: *Watcher) void {
        var thr_it = self.threads.iterator();
        while (thr_it.next()) |entry| {
            entry.value_ptr.*.join();
            self.alloc.free(entry.key_ptr.*);
        }
        self.threads.deinit();

        var cf_it = self.cancel_flags.iterator();
        while (cf_it.next()) |entry| {
            self.alloc.free(entry.key_ptr.*);
            self.alloc.destroy(entry.value_ptr.*);
        }
        self.cancel_flags.deinit();
    }

    /// Spawn a per-zombie thread. Idempotent — duplicate calls are a no-op
    /// (e.g. bootstrap path collides with a `zombie_created` already in the
    /// stream backlog). Caller owns nothing; the watcher takes a duped
    /// copy of `zombie_id` for its maps.
    pub fn spawnZombieThread(self: *Watcher, zombie_id: []const u8) !void {
        self.map_lock.lock();
        defer self.map_lock.unlock();

        if (self.cancel_flags.contains(zombie_id)) {
            log.debug("watcher.spawn_skipped reason=already_running zombie_id={s}", .{zombie_id});
            return;
        }

        const flag = try self.alloc.create(std.atomic.Value(bool));
        errdefer self.alloc.destroy(flag);
        flag.* = std.atomic.Value(bool).init(false);

        const id_owned_for_flags = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_owned_for_flags);
        try self.cancel_flags.put(id_owned_for_flags, flag);

        const id_owned_for_thread = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_owned_for_thread);

        const thread = try std.Thread.spawn(.{}, worker_zombie.zombieWorkerLoop, .{
            self.alloc,
            worker_zombie.ZombieWorkerConfig{
                .pool = self.cfg.pool,
                .zombie_id = id_owned_for_thread,
                .shutdown_requested = self.cfg.shutdown_requested,
                .executor = self.cfg.executor,
                .workspace_path = self.cfg.workspace_path,
                .telemetry = self.cfg.telemetry,
                .cancel_flag = flag,
            },
        });

        const id_owned_for_threads_map = try self.alloc.dupe(u8, zombie_id);
        errdefer self.alloc.free(id_owned_for_threads_map);
        try self.threads.put(id_owned_for_threads_map, thread);

        log.info("watcher.spawned zombie_id={s}", .{zombie_id});
    }

    /// Main XREADGROUP loop. Returns when shutdown_requested fires or the
    /// WorkerState leaves the running phase. Caller spawns this in a thread.
    pub fn run(self: *Watcher) void {
        log.info("watcher.start consumer={s}", .{self.cfg.consumer_name});
        while (self.shouldKeepRunning()) {
            self.pollOnce() catch |err| {
                log.err("watcher.poll_fail err={s} error_code=" ++ error_codes.ERR_INTERNAL_OPERATION_FAILED, .{@errorName(err)});
                std.Thread.sleep(100 * std.time.ns_per_ms);
            };
        }
        log.info("watcher.stop consumer={s}", .{self.cfg.consumer_name});
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
            .worker_drain_request => |m| self.requestDrain(m.reason),
        }
    }

    fn cancelZombie(self: *Watcher, zombie_id: []const u8) void {
        self.map_lock.lock();
        const flag_ptr = self.cancel_flags.get(zombie_id);
        self.map_lock.unlock();

        if (flag_ptr) |flag| {
            flag.store(true, .release);
            log.info("watcher.cancel_set zombie_id={s}", .{zombie_id});
        } else {
            log.debug("watcher.cancel_skip reason=not_local zombie_id={s}", .{zombie_id});
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
