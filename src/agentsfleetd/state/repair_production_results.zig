//! Durable, idempotent provider production evidence.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const sql = @import("repair_sql.zig");

pub const InsertOutcome = enum { inserted, replayed };

pub const NewResult = struct {
    workspace_id: []const u8,
    provider: []const u8,
    provider_deployment_id: []const u8,
    provider_status_id: []const u8,
    repository: []const u8,
    environment: []const u8,
    commit_sha: []const u8,
    conclusion: []const u8,
    completed_at: i64,
};

/// Store terminal provider evidence before correlation. A provider replay has
/// no mutation path and remains distinguishable from a new result.
pub fn insert(conn: *pg.Conn, id: []const u8, result: NewResult) !InsertOutcome {
    const affected = try conn.exec(sql.INSERT_REPAIR_PRODUCTION_RESULT, .{
        id,
        result.workspace_id,
        result.provider,
        result.provider_deployment_id,
        result.provider_status_id,
        result.repository,
        result.environment,
        result.commit_sha,
        result.conclusion,
        result.completed_at,
        clock.nowMillis(),
    });
    return if ((affected orelse 0) > 0) .inserted else .replayed;
}

test "test_provider_status_is_the_append_identity" {
    try std.testing.expect(std.mem.indexOf(u8, sql.INSERT_REPAIR_PRODUCTION_RESULT, "ON CONFLICT (workspace_id, provider, provider_status_id)") != null);
}
