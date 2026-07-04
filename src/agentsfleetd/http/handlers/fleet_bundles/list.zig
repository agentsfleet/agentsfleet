//! GET /v1/fleets/bundles — the first-party Fleet library catalog (the
//! dashboard gallery + `agentsfleet library` shop-window). Metadata only;
//! SKILL.md/TRIGGER.md content is fetched + snapshotted at import time.
//!
//! Source of truth is the curated `core.fleet_library` table (seeded
//! by migration, read-only at runtime). Adding an entry is a data write — no
//! code change. JSONB requirement arrays are decoded into string slices so the
//! response emits JSON arrays, not quoted JSONB text.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const sql = @import("../../../fleet_bundle/sql.zig");

const Hx = hx_mod.Hx;

// Visibility value set is enforced in app code (RULE STS) — the migration
// deliberately ships no CHECK constraint. Only `public` rows surface here.
const VISIBILITY_PUBLIC: []const u8 = "public";

// No LIMIT by design: unlike the user-data list endpoints (fleets/keys/events,
// which cap + cursor because they grow with usage), this is a curated catalog
// that only grows via migration — no runtime or attacker write path. The
// gallery must show every public template, so a cap would silently truncate it.
const SELECT_PUBLIC = sql.SELECT_BUNDLES_LIST;

const CatalogRow = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    required_credentials: []const []const u8,
    // Display-only {credential_name: reason} object, passed through as a JSON
    // value so the response emits a nested object the gallery + install gate read.
    required_credentials_reasons: std.json.Value,
    required_tools: []const []const u8,
    network_hosts: []const []const u8,
};

const ResponseBody = struct {
    items: []const CatalogRow,
};

pub fn innerList(hx: Hx, req: *httpz.Request) void {
    _ = req;
    var db = hx.db() orelse return;
    defer db.end();

    const items = buildCatalog(hx.alloc, db.conn) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.ok(.ok, ResponseBody{ .items = items });
}

fn buildCatalog(alloc: std.mem.Allocator, conn: *pg.Conn) ![]const CatalogRow {
    var rows: std.ArrayList(CatalogRow) = .empty;
    errdefer rows.deinit(alloc);

    var q = PgQuery.from(try conn.query(SELECT_PUBLIC, .{VISIBILITY_PUBLIC}));
    defer q.deinit();
    while (try q.next()) |row| {
        try appendRow(alloc, &rows, row);
    }
    return rows.toOwnedSlice(alloc);
}

fn appendRow(alloc: std.mem.Allocator, rows: *std.ArrayList(CatalogRow), row: anytype) !void {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    const name = try alloc.dupe(u8, try row.get([]const u8, 1));
    const description = try alloc.dupe(u8, try row.get([]const u8, 2));
    const required_credentials = try decodeStringArray(alloc, try row.get([]const u8, 3));
    const required_tools = try decodeStringArray(alloc, try row.get([]const u8, 4));
    const network_hosts = try decodeStringArray(alloc, try row.get([]const u8, 5));
    // Reasons is a {name: reason} object, not a string array — decode it as a
    // JSON value so it round-trips into the response as a nested object.
    const required_credentials_reasons = try std.json.parseFromSliceLeaky(std.json.Value, alloc, try row.get([]const u8, 6), .{});
    try rows.append(alloc, .{
        .id = id,
        .name = name,
        .description = description,
        .required_credentials = required_credentials,
        .required_credentials_reasons = required_credentials_reasons,
        .required_tools = required_tools,
        .network_hosts = network_hosts,
    });
}

// Decode a JSONB string-array column (e.g. `["github"]`) into owned slices.
// The leaky parse copies into `alloc` (the per-request arena), so the result
// outlives the row buffer that `row.get` borrows from.
fn decodeStringArray(alloc: std.mem.Allocator, json_text: []const u8) ![]const []const u8 {
    return std.json.parseFromSliceLeaky([]const []const u8, alloc, json_text, .{});
}
