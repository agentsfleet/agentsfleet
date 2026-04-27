const std = @import("std");

const DEFAULT_RUN_MAX_REPAIR_LOOPS: u32 = 3;
const DEFAULT_RUN_MAX_TOKENS: u64 = 100_000;
const DEFAULT_RUN_MAX_WALL_TIME_SECONDS: u64 = 600;
const DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET: u64 = 10_000_000;
const DEFAULT_SCORING_CONTEXT_MAX_TOKENS: u32 = 2048;

// SchemaSpec holds the raw SQL DEFAULT values as literals.
// It is an independent source of truth: keep these in sync with the DDL.
// If a constant diverges from its schema DEFAULT, the tests below will fail.
//   001_core_foundation.sql  — monthly_token_budget, max_repair_loops, max_tokens, max_wall_time_seconds
//   004_workspace_entitlements.sql — scoring_context_max_tokens
const SchemaSpec = struct {
    run_max_repair_loops: u32 = 3,
    run_max_tokens: u64 = 100_000,
    run_max_wall_time_seconds: u64 = 600,
    workspace_monthly_token_budget: u64 = 10_000_000,
    scoring_context_max_tokens: u32 = 2048,
};
const schema_spec = SchemaSpec{};

test "run defaults match schema spec" {
    try std.testing.expectEqual(schema_spec.run_max_repair_loops, DEFAULT_RUN_MAX_REPAIR_LOOPS);
    try std.testing.expectEqual(schema_spec.run_max_tokens, DEFAULT_RUN_MAX_TOKENS);
    try std.testing.expectEqual(schema_spec.run_max_wall_time_seconds, DEFAULT_RUN_MAX_WALL_TIME_SECONDS);
}

test "workspace budget default matches schema spec" {
    try std.testing.expectEqual(schema_spec.workspace_monthly_token_budget, DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET);
}

test "scoring context default matches schema spec" {
    try std.testing.expectEqual(schema_spec.scoring_context_max_tokens, DEFAULT_SCORING_CONTEXT_MAX_TOKENS);
}
