const start = @import("runs/start.zig");
const get = @import("runs/get.zig");
const list = @import("runs/list.zig");
const retry = @import("runs/retry.zig");

pub const handleStartRun = start.handleStartRun;
pub const handleGetRun = get.handleGetRun;
pub const handleListRuns = list.handleListRuns;
pub const handleRetryRun = retry.handleRetryRun;

comptime {
    _ = @import("runs/tests.zig");
}
