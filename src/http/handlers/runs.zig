const start = @import("runs/start.zig");
const get = @import("runs/get.zig");
const retry = @import("runs/retry.zig");

pub const handleStartRun = start.handleStartRun;
pub const handleGetRun = get.handleGetRun;
pub const handleRetryRun = retry.handleRetryRun;

comptime {
    _ = @import("runs/tests.zig");
}
