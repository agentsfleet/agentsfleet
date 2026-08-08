//! Store for `core.repair_pr_links` — the incident → repair PR → deploy-result
//! linkage. Insert-only by schema trigger; the deploy stamp is the single
//! permitted mutation. This table is the deferred verifier member reduced to
//! data: "did the fix work" is a column, not a model run.

const std = @import("std");
const pg = @import("pg");
const clock = @import("common").clock;
const sql = @import("sql.zig");
const id_format = @import("../types/id_format.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

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

/// Stamp the deploy result for the linked branch. Returns whether a row was
/// stamped — an unknown branch is the caller's no-op, not an error.
pub fn stampDeploy(conn: *pg.Conn, fleet_id: []const u8, branch: []const u8, status: []const u8) !bool {
    const affected = try conn.exec(sql.STAMP_REPAIR_PR_DEPLOY, .{
        fleet_id, branch, status, clock.nowMillis(),
    });
    return (affected orelse 0) > 0;
}

/// A linkage row as the dashboard or a test reads it. Slices are arena/caller
/// owned copies — caller must free via `deinit`.
pub const Link = struct {
    repository: []const u8,
    branch: []const u8,
    pr_number: i64,
    pr_url: []const u8,
    deploy_status: []const u8,
    deploy_stamped_at: ?i64,

    pub fn deinit(self: *Link, alloc: std.mem.Allocator) void {
        alloc.free(self.repository);
        alloc.free(self.branch);
        alloc.free(self.pr_url);
        alloc.free(self.deploy_status);
        self.* = undefined;
    }
};

/// The linkage row for an incident, or null when no repair shipped.
/// Caller must free (`Link.deinit`).
pub fn lookupByEvent(alloc: std.mem.Allocator, conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !?Link {
    var q = PgQuery.from(try conn.query(sql.SELECT_REPAIR_PR_LINK_BY_EVENT, .{ fleet_id, event_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const repository = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(repository);
    const branch = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(branch);
    const pr_number = try row.get(i64, 2);
    const pr_url = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(pr_url);
    const deploy_status = try alloc.dupe(u8, try row.get([]const u8, 4));
    errdefer alloc.free(deploy_status);
    const deploy_stamped_at = try row.get(?i64, 5);
    return .{
        .repository = repository,
        .branch = branch,
        .pr_number = pr_number,
        .pr_url = pr_url,
        .deploy_status = deploy_status,
        .deploy_stamped_at = deploy_stamped_at,
    };
}

// DB-backed behaviour (insert/duplicate/stamp/immutability) is proven in
// `http/webhook_http_integration_test.zig` beside the arms that drive it —
// the state-layer convention (see `fleet_events_store_test.zig`).
