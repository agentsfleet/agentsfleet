//! Immutable completed-workflow evidence for approved repair branches.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const id_format = @import("../types/id_format.zig");
const sql = @import("repair_sql.zig");

pub const InsertOutcome = enum { inserted, replayed };

pub const NewResult = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    repository: []const u8,
    branch: []const u8,
    workflow_name: []const u8,
    provider_run_id: i64,
    head_commit_sha: []const u8,
    conclusion: []const u8,
    completed_at: i64,
};

/// Append one provider run, absorbing only an exact provider replay.
pub fn insert(alloc: std.mem.Allocator, conn: *pg.Conn, result: NewResult) !InsertOutcome {
    const row_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(row_id);
    const affected = try conn.exec(sql.INSERT_REPAIR_RUN_RESULT, .{
        row_id,
        result.workspace_id,
        result.fleet_id,
        result.event_id,
        result.repository,
        result.branch,
        result.workflow_name,
        result.provider_run_id,
        result.head_commit_sha,
        result.conclusion,
        result.completed_at,
        clock.nowMillis(),
    });
    return if ((affected orelse 0) > 0) .inserted else .replayed;
}
