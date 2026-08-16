//! Durable dispatch state for independent verifier attempts.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const sql = @import("repair_sql.zig");

pub const SYNTHETIC_EVENT = "repair_production_result";
pub const VERIFIER_EVENT_ACTOR = "system:repair-verifier";
pub const WEBHOOK_TRIGGER = "webhook";
pub const DUE_BATCH_LIMIT: i64 = 32;
pub const REDIS_CLEANUP_BATCH_LIMIT: i64 = 32;
pub const CLAIM_STALE_MS: i64 = 30 * std.time.ms_per_s;

/// One due attempt, including all event context; every slice is caller-owned and released with `deinit`.
pub const Due = struct {
    const Self = @This();

    id: []const u8,
    repair_link_id: []const u8,
    repository: []const u8,
    workspace_id: []const u8,
    verifier_fleet_id: []const u8,
    incident_fleet_id: []const u8,
    incident_event_id: []const u8,
    incident_request_json: []const u8,
    incident_response_text: []const u8,
    pr_number: i64,
    pr_url: []const u8,
    merged_commit_sha: []const u8,
    merged_at: i64,
    provider: []const u8,
    provider_deployment_id: []const u8,
    conclusion: []const u8,
    completed_at: i64,
    verify_after: i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.repair_link_id);
        alloc.free(self.repository);
        alloc.free(self.workspace_id);
        alloc.free(self.verifier_fleet_id);
        alloc.free(self.incident_fleet_id);
        alloc.free(self.incident_event_id);
        alloc.free(self.incident_request_json);
        alloc.free(self.incident_response_text);
        alloc.free(self.pr_url);
        alloc.free(self.merged_commit_sha);
        alloc.free(self.provider);
        alloc.free(self.provider_deployment_id);
        alloc.free(self.conclusion);
    }
};

pub const RedisCleanup = struct {
    id: []u8,

    pub fn deinit(self: *RedisCleanup, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
    }
};

pub const ClaimedBatch = struct {
    token: []const u8,
    items: []Due,

    pub fn deinit(self: *ClaimedBatch, alloc: std.mem.Allocator) void {
        freeDueItems(alloc, self.items);
        alloc.free(self.items);
        alloc.free(self.token);
    }
};

/// Claim a bounded due batch in one short statement. The caller releases its
/// database connection before doing Redis input/output.
pub fn claimDue(alloc: std.mem.Allocator, conn: *pg.Conn, now_ms: i64) !ClaimedBatch {
    const token = try id_format.generateRepairVerificationId(alloc);
    errdefer alloc.free(token);
    var rows: std.ArrayList(Due) = .empty;
    errdefer {
        freeDueItems(alloc, rows.items);
        rows.deinit(alloc);
    }
    var q = PgQuery.from(try conn.query(sql.CLAIM_DUE_REPAIR_VERIFICATIONS, .{
        now_ms,
        now_ms - CLAIM_STALE_MS,
        DUE_BATCH_LIMIT,
        token,
    }));
    defer q.deinit();
    while (try q.next()) |row| {
        const item = try copyDue(alloc, row);
        errdefer {
            var owned = item;
            owned.deinit(alloc);
        }
        try rows.append(alloc, item);
    }
    return .{ .token = token, .items = try rows.toOwnedSlice(alloc) };
}

/// Complete only the claim token this dispatcher owns.
pub fn complete(conn: *pg.Conn, verification_id: []const u8, claim_token: []const u8, event_id: []const u8, now_ms: i64) !bool {
    const affected = try conn.exec(sql.COMPLETE_REPAIR_VERIFICATION, .{ verification_id, claim_token, event_id, now_ms });
    return (affected orelse 0) > 0;
}

pub fn redisCleanupDue(alloc: std.mem.Allocator, conn: *pg.Conn, now_ms: i64) ![]RedisCleanup {
    var rows: std.ArrayList(RedisCleanup) = .empty;
    errdefer {
        freeRedisCleanupItems(alloc, rows.items);
        rows.deinit(alloc);
    }
    var q = PgQuery.from(try conn.query(sql.SELECT_REPAIR_VERIFICATION_REDIS_CLEANUP, .{
        now_ms - CLAIM_STALE_MS,
        REDIS_CLEANUP_BATCH_LIMIT,
    }));
    defer q.deinit();
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        try rows.append(alloc, .{ .id = id });
    }
    return rows.toOwnedSlice(alloc);
}

pub fn completeRedisCleanup(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    verification_ids: []const []const u8,
    now_ms: i64,
) !usize {
    if (verification_ids.len == 0) return 0;
    const payload = try std.json.Stringify.valueAlloc(alloc, verification_ids, .{});
    defer alloc.free(payload);
    const affected = try conn.exec(sql.COMPLETE_REPAIR_VERIFICATION_REDIS_CLEANUP, .{ payload, now_ms });
    return @intCast(affected orelse 0);
}

/// Returns the persisted Fleet-event enqueue time only when this report belongs
/// to a verifier intent. Other Fleet reports are deliberately invisible here.
pub fn verifierQueuedAt(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !?i64 {
    var q = PgQuery.from(try conn.query(sql.SELECT_REPAIR_VERIFICATION_QUEUED_AT, .{ fleet_id, event_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return @as(?i64, try row.get(i64, 0));
}

/// True only for Fleet events created by the repair-verification dispatcher.
/// The runner report path reads the actor with its existing lease query, so
/// ordinary Fleet reports do not acquire a repair-verification database
/// connection merely to discover that they are unrelated.
pub fn isVerifierEventActor(actor: []const u8) bool {
    return std.mem.eql(u8, actor, VERIFIER_EVENT_ACTOR);
}

fn copyDue(alloc: std.mem.Allocator, row: pg.Row) !Due {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(id);
    const repair_link_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(repair_link_id);
    const repository = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(repository);
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(workspace_id);
    const verifier_fleet_id = try alloc.dupe(u8, try row.get([]const u8, 4));
    errdefer alloc.free(verifier_fleet_id);
    const incident_fleet_id = try alloc.dupe(u8, try row.get([]const u8, 5));
    errdefer alloc.free(incident_fleet_id);
    const incident_event_id = try alloc.dupe(u8, try row.get([]const u8, 6));
    errdefer alloc.free(incident_event_id);
    const incident_request_json = try alloc.dupe(u8, try row.get([]const u8, 7));
    errdefer alloc.free(incident_request_json);
    const incident_response_text = try alloc.dupe(u8, try row.get([]const u8, 8));
    errdefer alloc.free(incident_response_text);
    const pr_number = try row.get(i64, 9);
    const pr_url = try alloc.dupe(u8, try row.get([]const u8, 10));
    errdefer alloc.free(pr_url);
    const merged_commit_sha = try alloc.dupe(u8, try row.get([]const u8, 11));
    errdefer alloc.free(merged_commit_sha);
    const merged_at = try row.get(i64, 12);
    const provider = try alloc.dupe(u8, try row.get([]const u8, 13));
    errdefer alloc.free(provider);
    const provider_deployment_id = try alloc.dupe(u8, try row.get([]const u8, 14));
    errdefer alloc.free(provider_deployment_id);
    const conclusion = try alloc.dupe(u8, try row.get([]const u8, 15));
    errdefer alloc.free(conclusion);
    const completed_at = try row.get(i64, 16);
    const verify_after = try row.get(i64, 17);
    return .{
        .id = id,
        .repair_link_id = repair_link_id,
        .repository = repository,
        .workspace_id = workspace_id,
        .verifier_fleet_id = verifier_fleet_id,
        .incident_fleet_id = incident_fleet_id,
        .incident_event_id = incident_event_id,
        .incident_request_json = incident_request_json,
        .incident_response_text = incident_response_text,
        .pr_number = pr_number,
        .pr_url = pr_url,
        .merged_commit_sha = merged_commit_sha,
        .merged_at = merged_at,
        .provider = provider,
        .provider_deployment_id = provider_deployment_id,
        .conclusion = conclusion,
        .completed_at = completed_at,
        .verify_after = verify_after,
    };
}

/// Releases an owned page returned by `redisCleanupDue`, slice included.
pub fn freeRedisCleanup(alloc: std.mem.Allocator, rows: []RedisCleanup) void {
    freeRedisCleanupItems(alloc, rows);
    alloc.free(rows);
}

/// Releases only the ids, leaving the backing slice to its owner. The unwind
/// paths hold a live `ArrayList`, whose buffer belongs to `deinit` — freeing
/// `items` there frees a capacity-length allocation by its length and then
/// hands the same pointer to `deinit` a second time.
fn freeRedisCleanupItems(alloc: std.mem.Allocator, rows: []RedisCleanup) void {
    for (rows) |*row| row.deinit(alloc);
}

fn freeDueItems(alloc: std.mem.Allocator, rows: []Due) void {
    for (rows) |*row| row.deinit(alloc);
}

test {
    _ = @import("repair_verifications_test.zig");
}
