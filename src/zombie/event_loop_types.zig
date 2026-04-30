// Zombie event loop types — extracted per RULE FLL (350-line gate).
//
// Re-exported by event_loop.zig. External consumers should import event_loop.zig.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;
const zombie_config = @import("config.zig");
const queue_redis = @import("../queue/redis_client.zig");
const executor_client = @import("../executor/client.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const balance_policy_mod = @import("../config/balance_policy.zig");

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const ZombieSession = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    config: zombie_config.ZombieConfig,
    instructions: []const u8,
    /// Session context (conversation memory) from core.zombie_sessions.
    /// JSON string. "{}" for a fresh session.
    context_json: []const u8,
    /// Source markdown — owns the memory that instructions borrows from.
    source_markdown: []const u8,
    /// M23_001: active executor session handle. NULL when zombie is idle.
    /// Set at createExecution, cleared at destroyExecution and on claimZombie (crash recovery).
    /// Persisted to core.zombie_sessions.execution_id so the steer API can read it.
    execution_id: ?[]const u8 = null,
    /// M23_001: millis timestamp when execution_id was set. 0 when idle.
    execution_started_at: i64 = 0,

    comptime {
        if (@sizeOf(ZombieSession) != 392) @compileError("ZombieSession size changed; update this assertion");
    }

    pub fn deinit(self: *ZombieSession, alloc: Allocator) void {
        alloc.free(self.zombie_id);
        alloc.free(self.workspace_id);
        self.config.deinit(alloc);
        alloc.free(self.source_markdown);
        alloc.free(self.context_json);
        if (self.execution_id) |eid| alloc.free(eid);
    }
};

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const EventResult = struct {
    status: Status,
    agent_response: []const u8,
    token_count: u64,
    wall_seconds: u64,
    /// M18_001: ms from stage start to first token. 0 if executor did not report.
    time_to_first_token_ms: u64 = 0,
    /// M18_001: Unix epoch ms at the start of deliverEvent(). 0 on gate-blocked paths.
    epoch_wall_time_ms: i64 = 0,

    pub const Status = enum { processed, skipped_duplicate, agent_error };

    pub fn deinit(self: *const EventResult, alloc: Allocator) void {
        alloc.free(self.agent_response);
    }
};

pub const EventLoopConfig = struct {
    pool: *pg.Pool,
    /// Stream client (XADD, XACK, XAUTOCLAIM, XREADGROUP, approval gate
    /// SET/GET). Mutex-locked Client; one TCP connection per worker.
    redis: *queue_redis.Client,
    /// Dedicated PUBLISH client for the activity channel. Decoupling
    /// pub/sub from the writepath stream commands prevents the per-frame
    /// PUBLISH from contending on the queue client's mutex during chunk
    /// bursts. Worker owns both clients; tests may pass the same pointer
    /// when the contention path isn't under test.
    redis_publish: *queue_redis.Client,
    executor: *executor_client.ExecutorClient,
    /// Cooperative shutdown flag — checked between events.
    running: *const std.atomic.Value(bool),
    /// Poll interval for backoff on consecutive errors (ms).
    poll_interval_ms: u64 = 2_000,
    /// Executor socket path for createExecution workspace_path.
    workspace_path: []const u8 = "/tmp/zombie",
    /// M15_002: optional telemetry for PostHog `zombie_completed` events.
    telemetry: ?*telemetry_mod.Telemetry = null,
    /// M11_006: behavior when a tenant's `balance_cents` has been exhausted.
    /// `stop` short-circuits delivery pre-claim; `warn`/`continue` allow the
    /// run and differ only in activity-event emission.
    balance_policy: balance_policy_mod.Policy = balance_policy_mod.DEFAULT,
    /// §9 hot-reload signal — flipped by the watcher on
    /// `zombie_config_changed`, read + cleared by the per-zombie thread
    /// between events. Null in tests + the legacy non-watcher path;
    /// reload then never fires. The atomic outlives the event loop
    /// (owned by the watcher's ZombieRuntime).
    reload_pending: ?*std.atomic.Value(bool) = null,
};
