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
/// Atomic: INSERT ... ON CONFLICT DO UPDATE — no read-then-write race.
/// On conflict: refreshes scopes_granted, updated_at, and resets status to 'active'
/// (so reinstall after revoke recovers the workspace).
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
    const new_id = try id_format.generateIntegrationId(alloc);
    defer alloc.free(new_id); // always free; result ID comes from RETURNING

    const source_str: []const u8 = switch (source) {
        .oauth => "oauth",
        .cli => "cli",
    };

    var q = PgQuery.from(conn.query(
        \\INSERT INTO core.workspace_integrations
        \\  (integration_id, workspace_id, provider, external_id,
        \\   scopes_granted, source, status, installed_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, 'active', $7, $7)
        \\ON CONFLICT (provider, external_id) DO UPDATE
        \\  SET scopes_granted = EXCLUDED.scopes_granted,
        \\      updated_at     = EXCLUDED.updated_at,
        \\      status         = 'active'
        \\RETURNING integration_id::text
    , .{ new_id, workspace_id, provider, external_id, scopes_granted, source_str, now_ms }) catch |err| {
        log.err("workspace_integrations.upsert_fail provider={s} err={s}", .{ provider, @errorName(err) });
        return err;
    });
    defer q.deinit();

    const row = try q.next() orelse {
        log.err("workspace_integrations.upsert_no_row provider={s}", .{provider});
        return error.UpsertNoRow;
    };
    const raw = try row.get([]u8, 0);
    return .{ .integration_id = try alloc.dupe(u8, raw), .created = false };
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
