//! Workspace credential list projection (§1).
//!
//! Two passes over one `pg.Conn`: pass 1 materializes the stored keys (the read
//! result must close before pass 2 issues per-row loads — two open results on a
//! single connection is forbidden); pass 2 decrypts each body and projects its
//! non-secret descriptors via `credential_metadata`. The api_key is never read.
//! A row that fails to load/parse degrades to an opaque `custom_secret` so the
//! list still returns 200.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const vault = @import("../../../state/vault.zig");
const credential_metadata = @import("credential_metadata.zig");

/// `fleet:` storage-key prefix; stripped for the display name. Mirrors the
/// LIKE pattern in the list query below.
const S_AGENT = "fleet:";

/// One wire row. `kind` is a static `@tagName` slice (never freed); `name` and
/// the descriptors are heap-owned (see `freeRow`). No `api_key` field exists.
pub const CredentialListRow = struct {
    name: []const u8,
    created_at: i64,
    kind: []const u8,
    provider: ?[]const u8 = null,
    model: ?[]const u8 = null,
    base_url: ?[]const u8 = null,
};

/// Pass-1 staging: the raw `fleet:`-prefixed key (needed to re-load the body in
/// pass 2) + created_at. Heap-owned by the projection allocator, transient.
const StagedRow = struct {
    raw_key: []const u8,
    created_at: i64,
};

/// Project every workspace credential into its non-secret list row. Caller owns
/// the returned slice; on the request arena it is reclaimed wholesale.
pub fn fetchCredentialListOnConn(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8) ![]CredentialListRow {
    // ── Pass 1: materialize keys, then close the read result. ──
    var staged: std.ArrayList(StagedRow) = .empty;
    defer {
        for (staged.items) |s| alloc.free(s.raw_key);
        staged.deinit(alloc);
    }
    {
        var q = PgQuery.from(try conn.query(
            \\SELECT key_name, created_at FROM vault.secrets
            \\WHERE workspace_id = $1::uuid AND key_name LIKE 'fleet:%'
            \\ORDER BY key_name ASC
        , .{workspace_id}));
        defer q.deinit();
        while (try q.next()) |row| {
            const raw_key = try alloc.dupe(u8, try row.get([]const u8, 0));
            errdefer alloc.free(raw_key);
            try staged.append(alloc, .{ .raw_key = raw_key, .created_at = try row.get(i64, 1) });
        }
    } // q drained + closed — pass-2 loads on `conn` are now safe.

    // ── Pass 2: decrypt + project each row. ──
    var rows: std.ArrayList(CredentialListRow) = .empty;
    errdefer {
        for (rows.items) |r| freeRow(alloc, r);
        rows.deinit(alloc);
    }
    for (staged.items) |s| {
        const row = try projectStagedRow(conn, alloc, workspace_id, s);
        errdefer freeRow(alloc, row);
        try rows.append(alloc, row);
    }
    return rows.toOwnedSlice(alloc);
}

fn projectStagedRow(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8, s: StagedRow) !CredentialListRow {
    const display = if (std.mem.startsWith(u8, s.raw_key, S_AGENT)) s.raw_key[S_AGENT.len..] else s.raw_key;
    const name = try alloc.dupe(u8, display);
    errdefer alloc.free(name);

    // Legacy/corrupt body → opaque custom_secret; the list still returns 200.
    var parsed = vault.loadJson(alloc, conn, workspace_id, s.raw_key) catch {
        return .{ .name = name, .created_at = s.created_at, .kind = credential_metadata.Kind.custom_secret.wire() };
    };
    defer parsed.deinit();

    const p = credential_metadata.project(parsed.value);
    const provider = try dupeOpt(alloc, p.provider);
    errdefer if (provider) |v| alloc.free(v);
    const model = try dupeOpt(alloc, p.model);
    errdefer if (model) |v| alloc.free(v);
    const base_url = try dupeOpt(alloc, p.base_url);
    errdefer if (base_url) |v| alloc.free(v);

    return .{
        .name = name,
        .created_at = s.created_at,
        .kind = p.kind.wire(),
        .provider = provider,
        .model = model,
        .base_url = base_url,
    };
}

fn dupeOpt(alloc: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try alloc.dupe(u8, v) else null;
}

fn freeRow(alloc: std.mem.Allocator, r: CredentialListRow) void {
    alloc.free(r.name);
    if (r.provider) |v| alloc.free(v);
    if (r.model) |v| alloc.free(v);
    if (r.base_url) |v| alloc.free(v);
    // r.kind is a static @tagName slice — not owned, never freed.
}
