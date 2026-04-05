const start = @import("runs/start.zig");
const get = @import("runs/get.zig");
const list = @import("runs/list.zig");
const retry = @import("runs/retry.zig");
const replay = @import("runs/replay.zig");
const stream = @import("runs/stream.zig");
const cancel = @import("runs/cancel.zig");
const interrupt = @import("runs/interrupt.zig");

pub const handleStartRun = start.handleStartRun;
pub const handleGetRun = get.handleGetRun;
pub const handleListRuns = list.handleListRuns;
pub const handleRetryRun = retry.handleRetryRun;
pub const handleGetRunReplay = replay.handleGetRunReplay;
pub const handleStreamRun = stream.handleStreamRun;
pub const handleCancelRun = cancel.handleCancelRun;
pub const handleInterruptRun = interrupt.handleInterruptRun;

comptime {
    _ = @import("runs/tests.zig");
    _ = @import("runs/cancel_test.zig");
    _ = @import("runs/interrupt_test.zig");
}
