const std = @import("std");

pub const DEFAULT_RUN_MAX_REPAIR_LOOPS: u32 = 3;
pub const DEFAULT_RUN_MAX_TOKENS: u64 = 100_000;
pub const DEFAULT_RUN_MAX_WALL_TIME_SECONDS: u64 = 600;
pub const DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET: u64 = 10_000_000;
pub const DEFAULT_SCORING_CONTEXT_MAX_TOKENS: u32 = 2048;

test "run defaults match schema spec" {
    try std.testing.expectEqual(@as(u32, 3), DEFAULT_RUN_MAX_REPAIR_LOOPS);
    try std.testing.expectEqual(@as(u64, 100_000), DEFAULT_RUN_MAX_TOKENS);
    try std.testing.expectEqual(@as(u64, 600), DEFAULT_RUN_MAX_WALL_TIME_SECONDS);
}

test "workspace budget default matches schema spec" {
    try std.testing.expectEqual(@as(u64, 10_000_000), DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET);
}

test "scoring context default matches schema spec" {
    try std.testing.expectEqual(@as(u32, 2048), DEFAULT_SCORING_CONTEXT_MAX_TOKENS);
}
