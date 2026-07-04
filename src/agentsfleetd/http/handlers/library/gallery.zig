//! GET /v1/workspaces/{workspace_id}/fleet-libraries — the workspace gallery
//! (M103 §5). Returns the union of the platform catalog and the requesting
//! workspace's own tenant entries, and nothing from another workspace
//! (Dimensions 5.1/5.2). Each entry carries identity, source, requirements, and
//! support-file summaries — never an object-store key (Dimension 5.3).

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const library_store = @import("../../../fleet_bundle/library_store.zig");
const sql = @import("../../../fleet_bundle/sql.zig");

const Hx = hx_mod.Hx;

// Only `public` platform rows are part of the gallery (matches list.zig).
const VISIBILITY_PUBLIC: []const u8 = "public";

const Requirements = struct {
    credentials: []const []const u8,
    tools: []const []const u8,
    network_hosts: []const []const u8,
    trigger_present: bool,
};

const SupportSummary = struct {
    path: []const u8,
    size_bytes: usize,
};

/// Manifest entry shape persisted in `support_files_json` — decoded to project
/// the public {path, size_bytes} summary (the per-file hash stays internal).
const ManifestEntry = struct {
    path: []const u8,
    size_bytes: usize,
    sha256: []const u8 = "",
};

const GalleryEntry = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    /// Catalog tier — "platform" or "tenant".
    visibility: []const u8,
    source_ref: []const u8,
    requirements: Requirements,
    /// Display-only {credential_name: reason} object driving the install gate's
    /// purpose copy. Platform rows surface their curated column; tenant rows
    /// surface `{}` (the importer derives no reasons) — M103 Dimension 5.4.
    required_credentials_reasons: std.json.Value,
    support_files: []const SupportSummary,
};

const ResponseBody = struct {
    items: []const GalleryEntry,
};

// Platform tier: requirements live in split JSONB columns; support manifest and
// trigger flag come from the onboarding snapshot (nullable for seed rows).
// Only ONBOARDED platform rows (a non-null content_hash / snapshot) are
// installable — `fetchPlatformInstall` enforces the same. A migration-seeded
// row with no snapshot yet would otherwise show in the gallery but dead-end at
// install with "not installable", so the gallery hides it until onboarded.
const SELECT_PLATFORM = sql.SELECT_GALLERY_PLATFORM;

const SELECT_TENANT = sql.SELECT_GALLERY_TENANT;

pub fn innerGallery(hx: Hx, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    var db = hx.db() orelse return;
    defer db.end();

    if (!common.authorizeWorkspace(db.conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const items = buildGallery(hx.alloc, db.conn, workspace_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.ok(.ok, ResponseBody{ .items = items });
}

fn buildGallery(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8) ![]const GalleryEntry {
    var rows: std.ArrayList(GalleryEntry) = .empty;
    errdefer rows.deinit(alloc);
    try appendPlatform(alloc, conn, &rows);
    try appendTenant(alloc, conn, workspace_id, &rows);
    return rows.toOwnedSlice(alloc);
}

fn appendPlatform(alloc: std.mem.Allocator, conn: *pg.Conn, rows: *std.ArrayList(GalleryEntry)) !void {
    var q = PgQuery.from(try conn.query(SELECT_PLATFORM, .{VISIBILITY_PUBLIC}));
    defer q.deinit();
    while (try q.next()) |row| {
        try rows.append(alloc, .{
            .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
            .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
            .visibility = library_store.TIER_PLATFORM,
            .source_ref = try alloc.dupe(u8, try row.get([]const u8, 3)),
            .requirements = .{
                .credentials = try decodeStrings(alloc, try row.get([]const u8, 4)),
                .tools = try decodeStrings(alloc, try row.get([]const u8, 5)),
                .network_hosts = try decodeStrings(alloc, try row.get([]const u8, 6)),
                .trigger_present = try row.get(bool, 9),
            },
            .required_credentials_reasons = try decodeReasons(alloc, try row.get([]const u8, 7)),
            .support_files = try decodeSummaries(alloc, try row.get([]const u8, 8)),
        });
    }
}

fn appendTenant(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8, rows: *std.ArrayList(GalleryEntry)) !void {
    var q = PgQuery.from(try conn.query(SELECT_TENANT, .{workspace_id}));
    defer q.deinit();
    while (try q.next()) |row| {
        const req = try std.json.parseFromSliceLeaky(Requirements, alloc, try row.get([]const u8, 4), .{ .ignore_unknown_fields = true });
        try rows.append(alloc, .{
            .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
            .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
            .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
            .visibility = library_store.TIER_TENANT,
            .source_ref = try alloc.dupe(u8, try row.get([]const u8, 3)),
            .requirements = req,
            .required_credentials_reasons = try decodeReasons(alloc, library_store.EMPTY_REASONS_JSON),
            .support_files = try decodeSummaries(alloc, try row.get([]const u8, 5)),
        });
    }
}

fn decodeStrings(alloc: std.mem.Allocator, json_text: []const u8) ![]const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, json_text, .{});
}

// Decode the `{credential_name: reason}` object as a JSON value so it round-trips
// into the response as a nested object (mirrors list.zig). Tenant rows pass the
// empty-object literal; platform rows pass their stored column.
fn decodeReasons(alloc: std.mem.Allocator, json_text: []const u8) !std.json.Value {
    return std.json.parseFromSliceLeaky(std.json.Value, alloc, json_text, .{});
}

// Project the stored manifest into {path, size_bytes} summaries; the per-file
// hash and any object-store key never reach the gallery (Dimension 5.3).
fn decodeSummaries(alloc: std.mem.Allocator, json_text: []const u8) ![]const SupportSummary {
    const manifest = try std.json.parseFromSliceLeaky([]const ManifestEntry, alloc, json_text, .{ .ignore_unknown_fields = true });
    const out = try alloc.alloc(SupportSummary, manifest.len);
    for (manifest, 0..) |entry, i| out[i] = .{ .path = entry.path, .size_bytes = entry.size_bytes };
    return out;
}
