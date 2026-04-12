// M8_001: workspace_integrations state module.
//
// Routing metadata only — maps (provider, external_id) → workspace_id.
// No credentials stored here. vault.secrets is the single source of truth
// for bot tokens. See docs/v2/agent-docs/RIPLEYS_LOG_APR_12_15_30.md §2.
//
// Two acquisition paths converge here:
//   OAuth:  source="oauth" — set by handleCallback after Slack OAuth completes
//   CLI:    source="cli"   — set by `zombiectl credential add slack`

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.workspace_integrations);

pub const Source = enum { oauth, cli };
pub const Status = enum { active, paused, revoked };

pub const UpsertResult = struct {
    integration_id: []const u8, // owned by caller
    created: bool,
};

/// Upsert a workspace_integrations row for the given (provider, external_id).
/// On conflict (existing row): refresh updated_at and scopes_granted.
/// Returns UpsertResult with an owned integration_id string. Caller must free.
pub fn upsertIntegration(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    provider: []const u8,
    external_id: []const u8,
    scopes_granted: []const u8,
    source: Source,
) !UpsertResult {
    const now_ms = std.time.milliTimestamp();

    // Read existing row first — materialise before any write (ZIG_RULES: no concurrent read+write)
    const existing = try lookupIntegrationId(conn, alloc, provider, external_id);

    if (existing) |id| {
        defer alloc.free(id);
        try touchIntegration(conn, id, scopes_granted, now_ms);
        return .{ .integration_id = try alloc.dupe(u8, id), .created = false };
    }

    const new_id = try id_format.generateIntegrationId(alloc);
    errdefer alloc.free(new_id);

    const source_str: []const u8 = switch (source) {
        .oauth => "oauth",
        .cli => "cli",
    };

    _ = conn.exec(
        \\INSERT INTO core.workspace_integrations
        \\  (integration_id, workspace_id, provider, external_id,
        \\   scopes_granted, source, status, installed_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, 'active', $7, $7)
    , .{ new_id, workspace_id, provider, external_id, scopes_granted, source_str, now_ms }) catch |err| {
        log.err("workspace_integrations.insert_fail provider={s} err={s}", .{ provider, @errorName(err) });
        return err;
    };

    return .{ .integration_id = new_id, .created = true };
}

/// Look up workspace_id for an active integration. Returns owned slice or null.
/// Used by event routing: given Slack team_id → find UseZombie workspace.
pub fn lookupWorkspace(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    provider: []const u8,
    external_id: []const u8,
) !?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT workspace_id::text
        \\FROM core.workspace_integrations
        \\WHERE provider = $1 AND external_id = $2 AND status = 'active'
    , .{ provider, external_id }) catch |err| {
        log.err("workspace_integrations.lookup_fail provider={s} err={s}", .{ provider, @errorName(err) });
        return err;
    });
    defer q.deinit();

    const row = try q.next() orelse return null;
    const raw = try row.get([]u8, 0);
    return try alloc.dupe(u8, raw);
}

// ── Internal ─────────────────────────────────────────────────────────────────

fn lookupIntegrationId(conn: *pg.Conn, alloc: std.mem.Allocator, provider: []const u8, external_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT integration_id::text
        \\FROM core.workspace_integrations
        \\WHERE provider = $1 AND external_id = $2
    , .{ provider, external_id }) catch |err| {
        log.err("workspace_integrations.id_lookup_fail provider={s} err={s}", .{ provider, @errorName(err) });
        return err;
    });
    defer q.deinit();

    const row = try q.next() orelse return null;
    const raw = try row.get([]u8, 0);
    return try alloc.dupe(u8, raw);
}

fn touchIntegration(conn: *pg.Conn, integration_id: []const u8, scopes_granted: []const u8, now_ms: i64) !void {
    _ = conn.exec(
        \\UPDATE core.workspace_integrations
        \\SET scopes_granted = $1, updated_at = $2
        \\WHERE integration_id = $3::uuid
    , .{ scopes_granted, now_ms, integration_id }) catch |err| {
        log.err("workspace_integrations.touch_fail id={s} err={s}", .{ integration_id, @errorName(err) });
        return err;
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "Source enum: oauth and cli variants exist" {
    const o: Source = .oauth;
    const c: Source = .cli;
    try std.testing.expect(o != c);
}

test "UpsertResult: created flag distinguishes new from existing" {
    const new = UpsertResult{ .integration_id = "abc", .created = true };
    const existing = UpsertResult{ .integration_id = "abc", .created = false };
    try std.testing.expect(new.created);
    try std.testing.expect(!existing.created);
}
