//! Request-independent `core.fleets` row-write primitives — the single insert
//! site (Invariant 7), the guarded activation flip, the workspace-scoped rollback
//! delete, and the unique-violation probe. Extracted from `create.zig` (RULE FLL)
//! so the three callers — the HTTP create handler, the Slack channel-fleet
//! materialization, and the install-progression worker — share ONE copy without
//! importing the httpz create handler (and thus with no import cycle: none of
//! these primitives touch `httpz.Request` or a principal).

const std = @import("std");
const sql = @import("sql.zig");
const pg = @import("pg");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const create_fleet_bundle = @import("create_fleet_bundle.zig");

/// Insert one `core.fleets` row — the single request-independent fleet-insert
/// site (Invariant 7). `innerCreateFleet` wraps it for the HTTP create path;
/// the principal-less Slack channel-fleet materialization
/// (`connectors/slack/channel_fleet.zig`) calls it directly under
/// install-delegated authority. Callers own the ensuing `create_stream`
/// setup; this fn only writes the row (born `installing`).
pub fn insertFleetOnConn(
    conn: *pg.Conn,
    workspace_id: []const u8,
    source_markdown: []const u8,
    trigger_markdown: []const u8,
    parsed: fleet_config.ParsedTrigger,
    required_tags: []const []const u8,
    bundle_ref: ?create_fleet_bundle.BundleRef,
    fleet_id: []const u8,
    now_ms: i64,
) !void {
    const bundle_hash: ?[]const u8 = if (bundle_ref) |b| b.content_hash else null;
    const bundle_key: ?[]const u8 = if (bundle_ref) |b| b.snapshot_key else null;
    _ = try conn.exec(sql.INSERT_FLEET, .{
        fleet_id,
        workspace_id,
        parsed.config.name,
        source_markdown,
        trigger_markdown,
        parsed.config_json,
        // Born installing: the synthetic install steps run on a deferred tick
        // after the 201 (create_install_steps), flipping the row to active on
        // the ready step. A subscriber that reconnects mid-install reconciles
        // from this column (the activity channel has no replay).
        fleet_config.FleetStatus.installing.toSlice(),
        required_tags,
        bundle_hash,
        bundle_key,
        now_ms,
    });
}

/// Flip a fleet from `installing` to `active` — the guarded, idempotent
/// activation UPDATE. Scoped to both ids and guarded on the current status so a
/// concurrent operator action (e.g. a kill mid-install) is never clobbered, and
/// a repeat call once already `active` is a 0-row no-op. Request-independent
/// (mirrors `insertFleetOnConn`): `create_install_steps.flipToActive` runs it on
/// its detached worker for the dashboard create path (after the cosmetic install
/// beat); the Slack channel-fleet materialization calls it inline, because a
/// reactive resident fleet has no provisioning beat and is leaseable the instant
/// its row + event stream exist.
pub fn activateFleetOnConn(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, now_ms: i64) !void {
    _ = try conn.exec(sql.UPDATE_FLEET_STATUS, .{
        fleet_config.FleetStatus.active.toSlice(),
        now_ms,
        fleet_id,
        workspace_id,
        fleet_config.FleetStatus.installing.toSlice(),
    });
}

/// Roll back a freshly-INSERTed fleet row. Workspace-scoped to prevent
/// cross-tenant deletes. Returns errors so the caller can decide whether
/// to log loudly (rare double-fault) or swallow. Shared with the Slack
/// channel-fleet materialization, which deletes its own orphan when it loses
/// the concurrent first-mention race (Invariant 6).
pub fn deleteFleetRow(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8) !void {
    _ = try conn.exec(sql.DELETE_FLEET, .{ fleet_id, workspace_id });
}

/// True when the last statement on `conn` failed the `uq_fleets_workspace_id_name`
/// unique constraint (a duplicate fleet name in the workspace). The pg driver
/// surfaces the structured SQLSTATE on `conn.err` after a failed `exec`, so the
/// 409 path is reachable — same introspection the api-keys and signup handlers use.
/// Shared with the Slack channel-fleet materialization, which converges a
/// concurrent same-channel first-mention on this constraint (Invariant 6).
pub fn isUniqueViolation(conn: *pg.Conn) bool {
    const pg_err = conn.err orelse return false;
    return isUniqueViolationCode(pg_err.code);
}

/// SQLSTATE `23505` is `unique_violation`.
fn isUniqueViolationCode(sqlstate: []const u8) bool {
    return std.mem.eql(u8, sqlstate, "23505");
}

test "isUniqueViolationCode matches 23505 only" {
    try std.testing.expect(isUniqueViolationCode("23505"));
    try std.testing.expect(!isUniqueViolationCode("23503")); // foreign_key_violation
    try std.testing.expect(!isUniqueViolationCode("XX000"));
    try std.testing.expect(!isUniqueViolationCode(""));
}
