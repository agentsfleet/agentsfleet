//! GET /v1/fleets/bundles — the first-party Fleet template catalog (the
//! dashboard gallery + `agentsfleet templates` shop-window). Metadata only;
//! SKILL.md/TRIGGER.md content is fetched + snapshotted at import time.
//!
//! Source of truth is the curated `core.fleet_bundle_templates` table (seeded
//! by migration, read-only at runtime). Adding a template is a data write — no
//! code change. JSONB requirement arrays are decoded into string slices so the
//! response emits JSON arrays, not quoted JSONB text.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

const Hx = hx_mod.Hx;

// Visibility value set is enforced in app code (RULE STS) — the migration
// deliberately ships no CHECK constraint. Only `public` rows surface here.
const VISIBILITY_PUBLIC: []const u8 = "public";

const SELECT_PUBLIC =
    \\SELECT id, name, description,
    \\       required_credentials::text, required_tools::text, network_hosts::text
    \\  FROM core.fleet_bundle_templates
    \\ WHERE visibility = $1
    \\ ORDER BY id
;

const CatalogRow = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    required_credentials: []const []const u8,
    required_tools: []const []const u8,
    network_hosts: []const []const u8,
};

const ResponseBody = struct {
    items: []const CatalogRow,
};

pub fn innerList(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const items = buildCatalog(hx.alloc, conn) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
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
    try rows.append(alloc, .{
        .id = id,
        .name = name,
        .description = description,
        .required_credentials = required_credentials,
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
