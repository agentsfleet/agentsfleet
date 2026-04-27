const std = @import("std");

pub const RedisRole = enum {
    default,
    api,
    worker,
};

pub fn roleEnvVarName(role: RedisRole) []const u8 {
    return switch (role) {
        .api => "REDIS_URL_API",
        .worker => "REDIS_URL_WORKER",
        .default => "REDIS_URL",
    };
}

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const QueueMessage = struct {
    message_id: []u8,
    run_id: []u8,
    attempt: u32,
    workspace_id: ?[]u8,

    pub fn deinit(self: *QueueMessage, alloc: std.mem.Allocator) void {
        alloc.free(self.message_id);
        alloc.free(self.run_id);
        if (self.workspace_id) |v| alloc.free(v);
    }
};
