const std = @import("std");
const common = @import("../http/handlers/common.zig");
const scoring = @import("scoring.zig");
const defaults = @import("../types/defaults.zig");

fn createTempWorkspaceEntitlements(conn: anytype) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE workspace_entitlements (
        \\  entitlement_id TEXT PRIMARY KEY,
        \\  workspace_id TEXT NOT NULL UNIQUE,
        \\  plan_tier TEXT NOT NULL,
        \\  max_profiles INTEGER NOT NULL,
        \\  max_stages INTEGER NOT NULL,
        \\  max_distinct_skills INTEGER NOT NULL,
        \\  allow_custom_skills BOOLEAN NOT NULL,
        \\  enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE,
        \\  agent_scoring_weights_json TEXT NOT NULL DEFAULT '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\  enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE,
        \\  scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});
}

// Verifies that a DB value at the lower boundary of the CHECK constraint
// (scoring_context_max_tokens >= 512) is returned as-is — no clamping applied.
test "queryScoringConfig returns min-boundary token cap unchanged" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempWorkspaceEntitlements(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000305', '0195b4ba-8d3a-7f13-8abc-cc0000000305', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', true, 512, 0, 0)
    , .{});

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-cc0000000305");
    try std.testing.expectEqual(@as(u32, 512), cfg.scoring_context_max_tokens);
}
