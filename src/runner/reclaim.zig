//! Lease reclaim — re-leasing an expired holder's event from Postgres alone.
//!
//! When `affinity.claim` wins a zombie whose prior claim had expired, the dead
//! holder's still-`active` lease row carries the durable event envelope + the
//! billing context. `findPriorActive` returns it and `markExpired` retires it,
//! so the caller can re-lease the SAME event under the fresh higher fencing
//! token — no Redis re-read (the envelope is durable in Postgres) and no
//! re-billing (the original lease already debited). If there is no prior active
//! lease the zombie is simply free and the caller takes a fresh event instead.
//!
//! Arena allocator (`hx.alloc`): every returned slice is arena-dup'd and freed
//! when the request ends — see service.zig's module note.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const protocol = @import("protocol.zig");

/// The dead holder's lease: the event envelope to re-lease + the billing
/// context to reuse (no re-charge). All slices arena-dup'd.
pub const PriorLease = struct {
    lease_id: []const u8,
    event_id: []const u8,
    actor: []const u8,
    event_type: []const u8,
    request_json: []const u8,
    event_created_at: i64,
    workspace_id: []const u8,
    tenant_id: []const u8,
    posture: []const u8,
    model: []const u8,
};

/// The zombie's latest `active` lease — the holder whose claim just expired, or
/// null when the zombie is free (no active lease ⇒ the caller takes a fresh
/// event). Called only after `affinity.claim` won, so an active row here is
/// unambiguously the reclaimed holder.
pub fn findPriorActive(conn: *pg.Conn, alloc: std.mem.Allocator, zombie_id: []const u8) !?PriorLease {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, event_id, actor, event_type, request_json,
        \\       event_created_at, workspace_id::text, tenant_id::text, posture, model
        \\FROM fleet.runner_leases
        \\WHERE zombie_id = $1::uuid AND status = $2
        \\ORDER BY fencing_token DESC LIMIT 1
    , .{ zombie_id, protocol.RUNNER_LEASE_STATUS_ACTIVE }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return PriorLease{
        .lease_id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .event_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .actor = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .event_type = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .request_json = try alloc.dupe(u8, try row.get([]const u8, 4)),
        .event_created_at = try row.get(i64, 5),
        .workspace_id = try alloc.dupe(u8, try row.get([]const u8, 6)),
        .tenant_id = try alloc.dupe(u8, try row.get([]const u8, 7)),
        .posture = try alloc.dupe(u8, try row.get([]const u8, 8)),
        .model = try alloc.dupe(u8, try row.get([]const u8, 9)),
    };
}

/// Retire the reclaimed holder's lease so it is no longer the zombie's active
/// lease. Its late report is independently fenced by the bumped token.
pub fn markExpired(conn: *pg.Conn, lease_id: []const u8) !void {
    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        \\UPDATE fleet.runner_leases SET status = $2, updated_at = $3 WHERE id = $1::uuid
    , .{ lease_id, protocol.RUNNER_LEASE_STATUS_EXPIRED, now_ms }) catch return error.LeaseExpireFailed;
}
