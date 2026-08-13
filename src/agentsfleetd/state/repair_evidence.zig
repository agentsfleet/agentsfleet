//! Transactional repair-evidence arrivals and exact, bounded reconciliation.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const ec = @import("../errors/error_registry.zig");
const fleet_config = @import("../fleet_runtime/config.zig");
const grant_lookup = @import("integration_grant_lookup.zig");
const id_format = @import("../types/id_format.zig");
const metrics = @import("../observability/metrics_repair_verification.zig");
const repair_links = @import("repair_pr_links.zig");
const repair_results = @import("repair_production_results.zig");
const repair_runs = @import("repair_run_results.zig");
const fanout = @import("repair_verification_fanout.zig");
const repair_verifications = @import("repair_verifications.zig");
const sql = @import("repair_sql.zig");

const log = logging.scoped(.repair_evidence);
const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";
const EMPTY_UUID = "00000000-0000-0000-0000-000000000000";
const CORRELATION_PAGE_LIMIT: i64 = 100;
const OBSERVATION_WINDOW_MS: i64 = 15 * std.time.ms_per_min;
pub const PRODUCTION_ENVIRONMENT = "production";
pub const SUCCESS_CONCLUSION = "success";
pub const GITHUB_PROVIDER = "github";
const WEBHOOK_TRIGGER = "webhook";

pub const ProductionArrival = struct {
    outcome: repair_results.InsertOutcome,
    verification_attempts: usize,
};

pub const MergeArrival = struct {
    outcome: repair_links.MergeOutcome,
    verification_attempts: usize,
};

pub const MergeEvidence = struct {
    link: repair_links.NewLink,
    merged_commit_sha: []const u8,
    merged_at: i64,
};

const Correlation = struct {
    outcome: metrics.Correlation,
    attempts: usize,
};

const LinkMatch = union(enum) {
    missing,
    ambiguous,
    unique: []u8,

    fn deinit(self: *LinkMatch, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .unique => |value| alloc.free(value),
            else => {},
        }
    }
};

const CandidatePage = struct {
    items: []fanout.Candidate,

    fn deinit(self: *CandidatePage, alloc: std.mem.Allocator) void {
        for (self.items) |*item| item.deinit(alloc);
        alloc.free(self.items);
    }
};

/// Store a production arrival and reconcile its exact commit in one unit.
pub fn recordProduction(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    result: repair_results.NewResult,
) !ProductionArrival {
    _ = try conn.exec(SQL_BEGIN, .{});
    errdefer rollback(conn);
    try lockCorrelation(conn, result.workspace_id, result.repository, result.commit_sha);
    const result_id = try id_format.generateRepairProductionResultId(alloc);
    defer alloc.free(result_id);
    const outcome = try repair_results.insert(conn, result_id, result);
    const correlation = if (outcome == .inserted)
        try reconcile(alloc, conn, result.workspace_id, result.repository, result.commit_sha, result_id)
    else
        null;
    _ = try conn.exec(SQL_COMMIT, .{});
    metrics.incProviderResult(if (outcome == .inserted) .accepted else .replayed);
    if (correlation) |value| observeCorrelation(value, result.workspace_id, result.repository, result.commit_sha);
    return .{
        .outcome = outcome,
        .verification_attempts = if (correlation) |value| value.attempts else 0,
    };
}

/// Store a merge arrival and reconcile every matching production result in
/// bounded pages. Replays skip reconciliation because the original unit either
/// committed both the merge and its intents or committed neither.
pub fn recordMerge(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    evidence: MergeEvidence,
) !MergeArrival {
    _ = try conn.exec(SQL_BEGIN, .{});
    errdefer rollback(conn);
    try lockCorrelation(conn, evidence.link.workspace_id, evidence.link.repository, evidence.merged_commit_sha);
    _ = try repair_links.insert(alloc, conn, evidence.link);
    const outcome = try repair_links.recordMerged(
        conn,
        evidence.link.fleet_id,
        evidence.link.repository,
        evidence.link.branch,
        evidence.link.pr_number,
        evidence.merged_commit_sha,
        evidence.merged_at,
    );
    const correlation = if (outcome == .recorded)
        try reconcile(alloc, conn, evidence.link.workspace_id, evidence.link.repository, evidence.merged_commit_sha, null)
    else
        null;
    _ = try conn.exec(SQL_COMMIT, .{});
    if (correlation) |value| observeCorrelation(value, evidence.link.workspace_id, evidence.link.repository, evidence.merged_commit_sha);
    return .{
        .outcome = outcome,
        .verification_attempts = if (correlation) |value| value.attempts else 0,
    };
}

pub fn recordOpened(alloc: std.mem.Allocator, conn: *pg.Conn, link: repair_links.NewLink) !repair_links.InsertOutcome {
    return repair_links.insert(alloc, conn, link);
}

pub fn recordRun(alloc: std.mem.Allocator, conn: *pg.Conn, result: repair_runs.NewResult) !repair_runs.InsertOutcome {
    return repair_runs.insert(alloc, conn, result);
}

/// Serialize both arrival paths for one workspace, repository, and commit.
pub fn lockCorrelation(conn: *pg.Conn, workspace_id: []const u8, repository: []const u8, commit_sha: []const u8) !void {
    var query = PgQuery.from(try conn.query(sql.LOCK_REPAIR_CORRELATION, .{ workspace_id, repository, commit_sha }));
    defer query.deinit();
    _ = try query.next() orelse return error.CorrelationLockFailed;
}

fn reconcile(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    repository: []const u8,
    commit_sha: []const u8,
    production_result_id: ?[]const u8,
) !Correlation {
    var link = try matchingLink(alloc, conn, workspace_id, repository, commit_sha);
    defer link.deinit(alloc);
    const repair_link_id = switch (link) {
        .missing => return .{ .outcome = .missed, .attempts = 0 },
        .ambiguous => return .{ .outcome = .ambiguous, .attempts = 0 },
        .unique => |id| id,
    };
    var after_result: ?[]u8 = null;
    defer if (after_result) |value| alloc.free(value);
    var after_fleet: ?[]u8 = null;
    defer if (after_fleet) |value| alloc.free(value);
    var inserted: usize = 0;
    while (true) {
        var page = try matchingPage(
            alloc,
            conn,
            workspace_id,
            repository,
            commit_sha,
            repair_link_id,
            production_result_id,
            after_result,
            after_fleet,
        );
        defer page.deinit(alloc);
        if (page.items.len == 0) break;
        inserted += try fanout.insert(alloc, conn, workspace_id, repair_link_id, page.items);
        const last = page.items[page.items.len - 1];
        const next_result = try alloc.dupe(u8, last.production_result_id);
        const next_fleet = alloc.dupe(u8, last.verifier_fleet_id) catch |err| {
            alloc.free(next_result);
            return err;
        };
        if (after_result) |value| alloc.free(value);
        if (after_fleet) |value| alloc.free(value);
        after_result = next_result;
        after_fleet = next_fleet;
        if (page.items.len < @as(usize, @intCast(CORRELATION_PAGE_LIMIT))) break;
    }
    return .{ .outcome = if (inserted == 0) .missed else .matched, .attempts = inserted };
}

fn matchingLink(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, repository: []const u8, commit_sha: []const u8) !LinkMatch {
    var query = PgQuery.from(try conn.query(sql.SELECT_REPAIR_LINKS_FOR_CORRELATION, .{ workspace_id, repository, commit_sha }));
    defer query.deinit();
    const first = try query.next() orelse return .missing;
    const id = try alloc.dupe(u8, try first.get([]const u8, 0));
    errdefer alloc.free(id);
    if (try query.next() != null) {
        alloc.free(id);
        return .ambiguous;
    }
    return .{ .unique = id };
}

fn matchingPage(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    repository: []const u8,
    commit_sha: []const u8,
    repair_link_id: []const u8,
    production_result_id: ?[]const u8,
    after_result: ?[]const u8,
    after_fleet: ?[]const u8,
) !CandidatePage {
    var rows: std.ArrayList(fanout.Candidate) = .empty;
    errdefer {
        for (rows.items) |*item| item.deinit(alloc);
        rows.deinit(alloc);
    }
    var query = PgQuery.from(try conn.query(sql.SELECT_REPAIR_VERIFICATION_CANDIDATE_PAGE, .{
        workspace_id,
        repository,
        commit_sha,
        PRODUCTION_ENVIRONMENT,
        SUCCESS_CONCLUSION,
        fleet_config.FleetStatus.active.toSlice(),
        GITHUB_PROVIDER,
        OBSERVATION_WINDOW_MS,
        grant_lookup.GrantStatus.approved.toSlice(),
        WEBHOOK_TRIGGER,
        repair_verifications.SYNTHETIC_EVENT,
        repair_link_id,
        production_result_id != null,
        production_result_id orelse EMPTY_UUID,
        after_result != null,
        after_result orelse EMPTY_UUID,
        after_fleet orelse EMPTY_UUID,
        CORRELATION_PAGE_LIMIT,
    }));
    defer query.deinit();
    while (try query.next()) |row| {
        const production_id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(production_id);
        const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(fleet_id);
        try rows.append(alloc, .{
            .production_result_id = production_id,
            .verifier_fleet_id = fleet_id,
            .verify_after = try row.get(i64, 2),
        });
    }
    return .{ .items = try rows.toOwnedSlice(alloc) };
}

fn observeCorrelation(correlation: Correlation, workspace_id: []const u8, repository: []const u8, commit_sha: []const u8) void {
    metrics.incCorrelation(correlation.outcome);
    metrics.incIntentsCreated(correlation.attempts);
    if (correlation.outcome == .ambiguous) log.warn("repair_verification_ambiguous_repair", .{
        .workspace_id = workspace_id,
        .repository = repository,
        .commit = commit_sha,
    });
}

fn rollback(conn: *pg.Conn) void {
    conn.rollback() catch |err| log.warn("repair_evidence_rollback_failed", .{
        .error_code = ec.ERR_INTERNAL_DB_QUERY,
        .err = @errorName(err),
    });
}

test "correlation pages have a fixed memory ceiling" {
    try std.testing.expectEqual(@as(i64, 100), CORRELATION_PAGE_LIMIT);
    try std.testing.expect(std.mem.indexOf(u8, sql.SELECT_REPAIR_VERIFICATION_CANDIDATE_PAGE, "ORDER BY p.id, f.id") != null);
    try std.testing.expect(std.mem.indexOf(u8, sql.SELECT_REPAIR_LINKS_FOR_CORRELATION, "LIMIT 2") != null);
}
