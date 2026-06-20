const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const credential_key = @import("../fleet_runtime/credential_key.zig");

pub const InsertParams = struct {
    id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    source_kind: []const u8,
    source_ref: []const u8,
    visibility: []const u8,
    content_hash: []const u8,
    snapshot_key: []const u8,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    support_files_json: []const u8,
    requirements_json: []const u8,
    validation_status: []const u8,
    now_ms: i64,
};

pub const BundleDetail = struct {
    id: []const u8,
    name: []const u8,
    source_kind: []const u8,
    source_ref: []const u8,
    visibility: []const u8,
    content_hash: []const u8,
    snapshot_key: []const u8,
    support_files_json: []const u8,
    requirements_json: []const u8,
    validation_status: []const u8,
    created_at: i64,
    updated_at: i64,

    pub fn deinit(self: *const BundleDetail, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.name);
        alloc.free(self.source_kind);
        alloc.free(self.source_ref);
        alloc.free(self.visibility);
        alloc.free(self.content_hash);
        alloc.free(self.snapshot_key);
        alloc.free(self.support_files_json);
        alloc.free(self.requirements_json);
        alloc.free(self.validation_status);
    }
};

pub const InstallBundle = struct {
    id: []const u8,
    content_hash: []const u8,
    snapshot_key: []const u8,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,

    pub fn deinit(self: *const InstallBundle, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.content_hash);
        alloc.free(self.snapshot_key);
        alloc.free(self.skill_markdown);
        if (self.trigger_markdown) |tm| alloc.free(tm);
    }
};

pub fn insertOrFetchId(conn: *pg.Conn, alloc: std.mem.Allocator, p: InsertParams) ![]const u8 {
    var q = PgQuery.from(try conn.query(
        \\WITH inserted AS (
        \\  INSERT INTO core.fleet_bundles
        \\    (id, workspace_id, name, source_kind, source_ref, visibility,
        \\     content_hash, snapshot_key, skill_markdown, trigger_markdown,
        \\     support_files_json, requirements_json, validation_status,
        \\     created_at, updated_at)
        \\  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10,
        \\          $11::jsonb, $12::jsonb, $13, $14, $14)
        \\  ON CONFLICT (workspace_id, content_hash) DO NOTHING
        \\  RETURNING id::text
        \\)
        \\SELECT id FROM inserted
        \\UNION ALL
        \\SELECT id::text FROM core.fleet_bundles
        \\WHERE workspace_id = $2::uuid AND content_hash = $7
        \\LIMIT 1
    , .{
        p.id,
        p.workspace_id,
        p.name,
        p.source_kind,
        p.source_ref,
        p.visibility,
        p.content_hash,
        p.snapshot_key,
        p.skill_markdown,
        p.trigger_markdown,
        p.support_files_json,
        p.requirements_json,
        p.validation_status,
        p.now_ms,
    }));
    defer q.deinit();
    const row = try q.next() orelse return error.BundleInsertMissing;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

pub fn fetchDetail(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    bundle_id: []const u8,
) !?BundleDetail {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, name, source_kind, source_ref, visibility,
        \\       content_hash, snapshot_key, support_files_json::text,
        \\       requirements_json::text, validation_status, created_at, updated_at
        \\FROM core.fleet_bundles
        \\WHERE workspace_id = $1::uuid AND id = $2::uuid
    , .{ workspace_id, bundle_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return .{
        .id = try dupeCol(row, alloc, 0),
        .name = try dupeCol(row, alloc, 1),
        .source_kind = try dupeCol(row, alloc, 2),
        .source_ref = try dupeCol(row, alloc, 3),
        .visibility = try dupeCol(row, alloc, 4),
        .content_hash = try dupeCol(row, alloc, 5),
        .snapshot_key = try dupeCol(row, alloc, 6),
        .support_files_json = try dupeCol(row, alloc, 7),
        .requirements_json = try dupeCol(row, alloc, 8),
        .validation_status = try dupeCol(row, alloc, 9),
        .created_at = try row.get(i64, 10),
        .updated_at = try row.get(i64, 11),
    };
}

pub fn fetchForInstall(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    bundle_id: []const u8,
) !?InstallBundle {
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, content_hash, snapshot_key, skill_markdown, trigger_markdown
        \\FROM core.fleet_bundles
        \\WHERE workspace_id = $1::uuid AND id = $2::uuid
    , .{ workspace_id, bundle_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const trigger_opt = try row.get(?[]const u8, 4);
    return .{
        .id = try dupeCol(row, alloc, 0),
        .content_hash = try dupeCol(row, alloc, 1),
        .snapshot_key = try dupeCol(row, alloc, 2),
        .skill_markdown = try dupeCol(row, alloc, 3),
        .trigger_markdown = if (trigger_opt) |tm| try alloc.dupe(u8, tm) else null,
    };
}

pub fn missingCredentialNames(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    names: []const []const u8,
) ![]const []const u8 {
    var missing: std.ArrayList([]const u8) = .empty;
    errdefer {
        for (missing.items) |name| alloc.free(name);
        missing.deinit(alloc);
    }
    for (names) |name| {
        if (!try credentialExists(conn, alloc, workspace_id, name)) {
            try missing.append(alloc, try alloc.dupe(u8, name));
        }
    }
    return missing.toOwnedSlice(alloc);
}

pub fn freeStringSlice(alloc: std.mem.Allocator, values: []const []const u8) void {
    for (values) |value| alloc.free(value);
    alloc.free(values);
}

fn credentialExists(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8, name: []const u8) !bool {
    const key_name = try credential_key.allocKeyName(alloc, name);
    defer alloc.free(key_name);
    var q = PgQuery.from(try conn.query(
        \\SELECT 1 FROM vault.secrets
        \\WHERE workspace_id = $1::uuid AND key_name = $2
        \\LIMIT 1
    , .{ workspace_id, key_name }));
    defer q.deinit();
    return (try q.next()) != null;
}

fn dupeCol(row: anytype, alloc: std.mem.Allocator, idx: usize) ![]const u8 {
    return alloc.dupe(u8, try row.get([]const u8, idx));
}
