//! Metadata-only writes for the two Fleet Library catalog tiers (M103):
//!   * platform — `core.fleet_library`, slug-keyed, UPSERT by id;
//!   * tenant   — `core.tenant_fleet_library`, workspace-scoped,
//!                INSERT deduped on `(workspace_id, content_hash)`.
//!
//! Both rows hold a support-file manifest (path/size/hash) and the content hash,
//! never support-file bytes — the canonical tar lives in Cloudflare R2 (R2). The
//! platform table carries requirements in split columns (`required_credentials`
//! etc.); rather than re-parse, this module extracts them from the importer's
//! `requirements_json` in SQL (`$req::jsonb -> 'credentials'`). The tenant table
//! stores `requirements_json` whole.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("sql.zig");

/// Tier literal reported in the onboarding response `visibility` field. The tier
/// is the table, not a stored column value; the platform table's own
/// `visibility` column stays the public/unlisted axis (`PLATFORM_CATALOG_VISIBILITY`).
pub const TIER_PLATFORM: []const u8 = "platform";
pub const TIER_TENANT: []const u8 = "tenant";

/// Onboarded platform rows are stored `public` so the existing gallery filter
/// (`list.zig`, visibility = 'public') surfaces them beside the seed rows.
const PLATFORM_CATALOG_VISIBILITY: []const u8 = "public";
/// The importer does not produce per-credential reason copy; onboarded rows
/// start with an empty reasons object (the seed rows carry curated copy). Shared
/// with the gallery handler, which emits the same empty object for tenant rows.
pub const EMPTY_REASONS_JSON: []const u8 = "{}";
/// One repo per entry, bundle at the repo root — no subpath filter.
const SOURCE_PATH_ROOT: []const u8 = "";
/// GitHub sources resolve at the default branch until per-source commit pinning.
const SOURCE_REF_DEFAULT: []const u8 = "main";

pub const PlatformInsertParams = struct {
    /// Slug id == the parsed SKILL name (seed convention).
    id: []const u8,
    name: []const u8,
    description: []const u8,
    /// The onboarding source reference (e.g. "owner/repo"), kept as provenance.
    source_repo: []const u8,
    content_hash: []const u8,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    /// The importer's requirements object; credentials/tools/network_hosts are
    /// extracted from it in SQL into the split columns.
    requirements_json: []const u8,
    support_files_json: []const u8,
    now_ms: i64,
};

pub const TenantInsertParams = struct {
    id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    description: []const u8,
    source_kind: []const u8,
    source_ref: []const u8,
    content_hash: []const u8,
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    support_files_json: []const u8,
    requirements_json: []const u8,
    now_ms: i64,
};

/// A library entry resolved for install — the SKILL/TRIGGER markdown the fleet row
/// stores plus the content hash the runner materializes support files from.
/// Caller passes the same allocator to `deinit`.
pub const InstallEntry = struct {
    skill_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    content_hash: []const u8,

    pub fn deinit(self: *const InstallEntry, alloc: std.mem.Allocator) void {
        alloc.free(self.skill_markdown);
        if (self.trigger_markdown) |tm| alloc.free(tm);
        alloc.free(self.content_hash);
    }
};

/// Resolve a platform entry for install by slug id. Only onboarded rows (a
/// non-null snapshot) are installable; a seed row not yet onboarded returns null.
pub fn fetchPlatformInstall(conn: *pg.Conn, alloc: std.mem.Allocator, id: []const u8) !?InstallEntry {
    var q = PgQuery.from(try conn.query(sql.SELECT_PLATFORM_INSTALL, .{id}));
    defer q.deinit();
    return rowToInstall(try q.next(), alloc);
}

/// Resolve a tenant entry for install, scoped to the caller's workspace — an
/// entry owned by another workspace is invisible (returns null), so install
/// enforces visibility (Dimension 4.2).
pub fn fetchTenantInstall(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8, id: []const u8) !?InstallEntry {
    var q = PgQuery.from(try conn.query(sql.SELECT_TENANT_INSTALL, .{ id, workspace_id }));
    defer q.deinit();
    return rowToInstall(try q.next(), alloc);
}

fn rowToInstall(row_opt: anytype, alloc: std.mem.Allocator) !?InstallEntry {
    const row = row_opt orelse return null;
    const skill = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(skill);
    const trigger_opt = try row.get(?[]const u8, 1);
    const trigger = if (trigger_opt) |tm| try alloc.dupe(u8, tm) else null;
    errdefer if (trigger) |tm| alloc.free(tm);
    const content_hash = try alloc.dupe(u8, try row.get([]const u8, 2));
    return .{ .skill_markdown = skill, .trigger_markdown = trigger, .content_hash = content_hash };
}

/// UPSERT a platform entry by slug id, refreshing the onboarding snapshot and
/// requirement columns on re-onboard. Returns the row id (caller owns it).
pub fn insertOrUpdatePlatform(conn: *pg.Conn, alloc: std.mem.Allocator, p: PlatformInsertParams) ![]const u8 {
    var q = PgQuery.from(try conn.query(sql.INSERT_PLATFORM, .{
        p.id,
        p.name,
        p.description,
        p.source_repo,
        SOURCE_PATH_ROOT,
        SOURCE_REF_DEFAULT,
        p.requirements_json,
        EMPTY_REASONS_JSON,
        PLATFORM_CATALOG_VISIBILITY,
        p.content_hash,
        p.skill_markdown,
        p.trigger_markdown,
        p.support_files_json,
        p.now_ms,
    }));
    defer q.deinit();
    const row = try q.next() orelse return error.TemplateInsertMissing;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}

/// INSERT a tenant entry, deduping on `(workspace_id, content_hash)`: a repeat
/// onboard of identical bytes returns the existing row id without a second row or
/// an R2 rewrite (Dimension 1.3). Returns the row id (caller owns it).
pub fn insertOrFetchTenant(conn: *pg.Conn, alloc: std.mem.Allocator, p: TenantInsertParams) ![]const u8 {
    var q = PgQuery.from(try conn.query(sql.INSERT_TENANT, .{
        p.id,
        p.workspace_id,
        p.name,
        p.description,
        p.source_kind,
        p.source_ref,
        TIER_TENANT,
        p.content_hash,
        p.skill_markdown,
        p.trigger_markdown,
        p.support_files_json,
        p.requirements_json,
        p.now_ms,
    }));
    defer q.deinit();
    const row = try q.next() orelse return error.TemplateInsertMissing;
    return try alloc.dupe(u8, try row.get([]const u8, 0));
}
