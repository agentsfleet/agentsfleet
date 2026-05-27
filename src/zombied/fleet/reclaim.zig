//! Lease reclaim — re-leasing an expired holder's event from Postgres alone.
//!
//! When `affinity.claim` wins a zombie whose prior claim had expired, the dead
//! holder's still-`active` lease row carries the durable event envelope + the
//! billing context. `reclaimPriorActive` selects that row (locked), marks it
//! `expired`, and returns it in ONE atomic statement, so the caller can re-lease
//! the SAME event under the fresh higher fencing token — no Redis re-read (the
//! envelope is durable in Postgres) and no re-billing (the original lease already
//! debited). If there is no prior active lease the zombie is simply free and the
//! caller takes a fresh event instead.
//!
//! Arena allocator (`hx.alloc`): every returned slice is arena-dup'd and freed
//! when the request ends — see service.zig's module note.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;

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

/// Atomically reclaim the zombie's latest `active` lease: one statement selects
/// that row (locked `FOR UPDATE`), marks it `expired`, and returns its event
/// envelope + billing context — so the find and the expire cannot be split by a
/// concurrent write. The returned columns are the pre-update envelope (the UPDATE
/// only touches status/updated_at), re-leased under the fresh higher fencing
/// token: no Redis re-read, no re-billing. Null when the zombie has no active
/// lease ⇒ it is free and the caller takes a fresh event. Called only after
/// `affinity.claim` won, so the row found here is unambiguously the reclaimed
/// holder. All slices arena-dup'd before drain.
pub fn reclaimPriorActive(conn: *pg.Conn, alloc: std.mem.Allocator, zombie_id: []const u8) !?PriorLease {
    const now_ms = std.time.milliTimestamp();
    var q = PgQuery.from(try conn.query(
        \\UPDATE fleet.runner_leases AS l
        \\SET status = $3, updated_at = $4
        \\WHERE l.id = (
        \\    SELECT id FROM fleet.runner_leases
        \\    WHERE zombie_id = $1::uuid AND status = $2
        \\    ORDER BY fencing_token DESC LIMIT 1
        \\    FOR UPDATE
        \\)
        \\RETURNING l.id::text, l.event_id, l.actor, l.event_type, l.request_json,
        \\          l.event_created_at, l.workspace_id::text, l.tenant_id::text,
        \\          l.posture, l.model
    , .{ zombie_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, protocol.RUNNER_LEASE_STATUS_EXPIRED, now_ms }));
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
