//! Store for `core.repair_pr_links`: one provenance-checked incident to repair
//! Pull Request linkage, with GitHub's exact merged commit recorded once.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("repair_sql.zig");
const id_format = @import("../types/id_format.zig");

/// Slot 830 requires an initial value. Slot 832 permits only a rolling old
/// daemon's deploy-status stamp; current verification ignores that field and
/// uses append-only production-result rows instead.
const INITIAL_DEPLOY_STATUS_PENDING = "pending";

pub const InsertOutcome = enum { inserted, duplicate };

pub const NewLink = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    repository: []const u8,
    branch: []const u8,
    pr_number: i64,
    pr_url: []const u8,
};

/// Insert the linkage row for a freshly opened repair PR. A second row for the
/// same (fleet, event) is a duplicate — reported, never overwritten: the first
/// shipped repair is the record.
pub fn insert(alloc: std.mem.Allocator, conn: *pg.Conn, link: NewLink) !InsertOutcome {
    const row_id = try id_format.generateActivityEventId(alloc);
    defer alloc.free(row_id);
    const affected = try conn.exec(sql.INSERT_REPAIR_PR_LINK, .{
        row_id,                        link.workspace_id, link.fleet_id,  link.event_id,
        link.repository,               link.branch,       link.pr_number, link.pr_url,
        INITIAL_DEPLOY_STATUS_PENDING, clock.nowMillis(),
    });
    return if ((affected orelse 0) > 0) .inserted else .duplicate;
}

pub const MergeOutcome = enum { recorded, same, ignored };

/// Record the exact provider merge hash once. Closed-unmerged, hashless, replayed,
/// or non-matching Pull Requests change nothing.
pub fn recordMerged(
    conn: *pg.Conn,
    fleet_id: []const u8,
    repository: []const u8,
    branch: []const u8,
    pr_number: i64,
    merged_commit_sha: []const u8,
    merged_at: i64,
) !MergeOutcome {
    const affected = try conn.exec(sql.RECORD_REPAIR_PR_MERGE, .{
        fleet_id, repository, branch, pr_number, merged_commit_sha, merged_at,
    });
    if ((affected orelse 0) > 0) return .recorded;
    var q = PgQuery.from(try conn.query(sql.SELECT_REPAIR_PR_MERGE_MATCH, .{
        fleet_id,
        repository,
        branch,
        pr_number,
        merged_commit_sha,
    }));
    defer q.deinit();
    const row = try q.next();
    return if (row != null) .same else .ignored;
}

// Database (DB)-backed behaviour (insert/duplicate/merge/immutability) is proven in
// `http/webhook_http_integration_test.zig` beside the arms that drive it —
// the state-layer convention (see `fleet_events_store_test.zig`).
