const get = @import("agents/get.zig");

pub const handleGetAgent = get.handleGetAgent;

comptime {
    _ = @import("agents/get.zig");
}
