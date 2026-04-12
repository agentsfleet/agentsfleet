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

    // bvisor pattern: comptime size assertion catches silent field drift.
    // 6 fields: 2 const slices + ZombieConfig(inline) + 3 const slices
    comptime {
        std.debug.assert(@sizeOf(ZombieSession) == 296);
    }

    pub fn deinit(self: *ZombieSession, alloc: Allocator) void {
        alloc.free(self.zombie_id);
        alloc.free(self.workspace_id);
        self.config.deinit(alloc);
        alloc.free(self.source_markdown);
        alloc.free(self.context_json);
    }
};

pub const EventResult = struct {
    status: Status,
    agent_response: []const u8,
    token_count: u64,
    wall_seconds: u64,

    pub const Status = enum { processed, skipped_duplicate, agent_error };

    pub fn deinit(self: *const EventResult, alloc: Allocator) void {
        alloc.free(self.agent_response);
    }
};

pub const EventLoopConfig = struct {
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    executor: *executor_client.ExecutorClient,
    /// Cooperative shutdown flag — checked between events.
    running: *const std.atomic.Value(bool),
    /// Poll interval for backoff on consecutive errors (ms).
    poll_interval_ms: u64 = 2_000,
    /// Executor socket path for createExecution workspace_path.
    workspace_path: []const u8 = "/tmp/zombie",
    /// M15_002: optional telemetry for PostHog `zombie_completed` events.
    telemetry: ?*telemetry_mod.Telemetry = null,
};
