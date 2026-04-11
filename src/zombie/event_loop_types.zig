// Zombie event loop types — extracted per RULE FLL (350-line gate).
//
// Re-exported by event_loop.zig. External consumers should import event_loop.zig.

const std = @import("std");
const Allocator = std.mem.Allocator;
const zombie_config = @import("config.zig");

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
