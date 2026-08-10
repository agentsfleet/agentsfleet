//! Store for `core.repair_pr_links` — the incident → repair PR → deploy-result
//! linkage. Insert-only by schema trigger; the deploy stamp is the single
//! permitted mutation. This table is the deferred verifier member reduced to
//! data: "did the fix work" is a column, not a model run.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const sql = @import("sql.zig");
const id_format = @import("../types/id_format.zig");

/// `deploy_status` vocabulary (RULE STS — app-enforced, no CHECK in DDL).
pub const DEPLOY_STATUS_PENDING = "pending";
pub const DEPLOY_STATUS_OK = "deploy_ok";
pub const DEPLOY_STATUS_FAILED = "deploy_failed";

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
        row_id,                link.workspace_id, link.fleet_id,  link.event_id,
        link.repository,       link.branch,       link.pr_number, link.pr_url,
        DEPLOY_STATUS_PENDING, clock.nowMillis(),
    });
    return if ((affected orelse 0) > 0) .inserted else .duplicate;
}

/// Stamp the deploy result for the linked branch in `repository`. Returns
/// whether a row was stamped — an unknown (repository, branch) is the caller's
/// no-op, not an error. The repository is matched, never assumed: branch names
/// collide across repositories and a fleet may hear from several.
pub fn stampDeploy(
    conn: *pg.Conn,
    fleet_id: []const u8,
    repository: []const u8,
    branch: []const u8,
    status: []const u8,
) !bool {
    const affected = try conn.exec(sql.STAMP_REPAIR_PR_DEPLOY, .{
        fleet_id, branch, repository, status, clock.nowMillis(),
    });
    return (affected orelse 0) > 0;
}

// No read surface here yet, deliberately: nothing in production reads a
// linkage row back. The operator-facing reader arrives with the dashboard that
// displays it (RULE NDC — a `pub` function whose only caller is a test is dead
// code that rots before its first real use). Until then the integration test
// asserts the stored row with its own query.
//
// DB-backed behaviour (insert/duplicate/stamp/immutability) is proven in
// `http/webhook_http_integration_test.zig` beside the arms that drive it —
// the state-layer convention (see `fleet_events_store_test.zig`).
