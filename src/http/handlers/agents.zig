const get = @import("agents/get.zig");
const scores = @import("agents/scores.zig");

pub const handleGetAgent = get.handleGetAgent;
pub const handleGetAgentScores = scores.handleGetAgentScores;

comptime {
    _ = @import("agents/get.zig");
    _ = @import("agents/scores.zig");
}
