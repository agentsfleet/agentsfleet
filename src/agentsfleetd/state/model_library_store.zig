//! Single owner of core.model_library row access — the priced model catalogue
//! (the billing spine). The catalogue read handler, the admin CRUD handler, and
//! the platform-default cap snapshot call these helpers instead of embedding
//! row mapping themselves; the statements and table name live in
//! model_library/sql.zig (SQL Statement Modules rule). Consolidated (M100) so a
//! catalogue schema change touches one domain.
//!
//! Memory: read helpers dupe row strings from the caller-supplied allocator and
//! return owned slices — pass a request-scoped allocator (hx.alloc); the
//! response lifetime frees them. Write helpers return the affected-row count so
//! the caller maps 0 → 404/409 without inspecting the driver's error.
//!
//! The rate-cache populator (model_rate_cache.zig) keeps its own load path but
//! reads its statement from the same model_library/sql.zig.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("model_library/sql.zig");

/// Mutable caps + rates of one row (no identity columns). Shared by create and
/// update inputs and by the rate projection.
pub const Rates = struct {
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

/// Admin-facing row: identity by uid (slash-free URL key) + provider/model_id +
/// rates. Strings owned by the allocator passed to listForAdmin.
pub const AdminRow = struct {
    uid: []const u8,
    provider: []const u8,
    model_id: []const u8,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

/// Library row: model_id (as `id`) + provider + rates. No uid (never exposed
/// outside the admin plane), no updated_at_ms (returned separately as the
/// catalogue version).
pub const PublicRow = struct {
    id: []const u8,
    provider: []const u8,
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
};

/// The catalogue plus the max updated_at_ms across the returned rows
/// (drives the version stamp; 0 for an empty catalogue).
pub const PublicList = struct {
    models: []PublicRow,
    max_updated_ms: i64,
};

/// Fields for a new catalogue row. provider+model_id are the immutable identity;
/// rates carry the caps/prices.
pub const NewRow = struct {
    uid: []const u8,
    provider: []const u8,
    model_id: []const u8,
    rates: Rates,
};

/// Every catalogue row, ordered by (provider, model_id), for the admin list.
pub fn listForAdmin(alloc: std.mem.Allocator, conn: *pg.Conn) ![]AdminRow {
    var q = PgQuery.from(try conn.query(sql.LIST_ADMIN, .{}));
    defer q.deinit();

    var rows: std.ArrayList(AdminRow) = .empty;
    errdefer rows.deinit(alloc);
    while (try q.next()) |row| {
        const uid = try alloc.dupe(u8, try row.get([]const u8, 0));
        const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
        const model_id = try alloc.dupe(u8, try row.get([]const u8, 2));
        try rows.append(alloc, .{
            .uid = uid,
            .provider = provider,
            .model_id = model_id,
            .context_cap_tokens = try row.get(i32, 3),
            .input_nanos_per_mtok = try row.get(i64, 4),
            .cached_input_nanos_per_mtok = try row.get(i64, 5),
            .output_nanos_per_mtok = try row.get(i64, 6),
        });
    }
    return rows.toOwnedSlice(alloc);
}

/// The catalogue as the authenticated library read serves it: all rows ordered
/// by model_id, plus the max updated_at_ms seen (the version stamp). The former
/// `?model=` filter retired with the public endpoint — no remaining consumer.
pub fn listForPublic(alloc: std.mem.Allocator, conn: *pg.Conn) !PublicList {
    var rows: std.ArrayList(PublicRow) = .empty;
    errdefer rows.deinit(alloc);
    var max_updated_ms: i64 = 0;

    var q = PgQuery.from(try conn.query(sql.LIST_LIBRARY, .{}));
    defer q.deinit();
    while (try q.next()) |row| try appendPublic(alloc, &rows, &max_updated_ms, row);

    return .{ .models = try rows.toOwnedSlice(alloc), .max_updated_ms = max_updated_ms };
}

fn appendPublic(
    alloc: std.mem.Allocator,
    rows: *std.ArrayList(PublicRow),
    max_updated_ms: *i64,
    row: anytype,
) !void {
    const id = try alloc.dupe(u8, try row.get([]const u8, 0));
    const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
    try rows.append(alloc, .{
        .id = id,
        .provider = provider,
        .context_cap_tokens = try row.get(i32, 2),
        .input_nanos_per_mtok = try row.get(i64, 3),
        .cached_input_nanos_per_mtok = try row.get(i64, 4),
        .output_nanos_per_mtok = try row.get(i64, 5),
    });
    const updated = try row.get(i64, 6);
    if (updated > max_updated_ms.*) max_updated_ms.* = updated;
}

/// context_cap_tokens of the priced (provider, model_id) row, or null when the
/// pair is uncatalogued. Used by the platform-default PUT to snapshot the cap.
pub fn capFor(conn: anytype, provider: []const u8, model: []const u8) ?i32 {
    var q = PgQuery.from(conn.query(sql.CAP_FOR, .{ provider, model }) catch return null);
    defer q.deinit();
    const row = (q.next() catch return null) orelse return null;
    return row.get(i32, 0) catch null;
}

/// True iff the row `uid` is the (provider, model) the active platform_provider_defaults
/// row resolves to — the delete-guard that blocks removing the live default.
/// Propagates the query error (matching this file's create/updateRates/remove
/// siblings) so the caller fails CLOSED on a DB fault instead of collapsing a
/// blip to `false` and letting the live default's model be deleted.
pub fn isReferencedByActiveDefault(conn: anytype, uid: []const u8) !bool {
    var q = PgQuery.from(try conn.query(sql.IS_REFERENCED_BY_ACTIVE_DEFAULT, .{uid}));
    defer q.deinit();
    return (try q.next()) != null;
}

/// Insert a new priced row. ON CONFLICT (provider, model_id) DO NOTHING, so the
/// affected count is 1 on create and 0 on a duplicate (caller → 409).
pub fn create(conn: anytype, row: NewRow, now_ms: i64) !?i64 {
    return conn.exec(sql.INSERT_ROW, .{
        row.uid,                        row.model_id,                          row.provider,                    row.rates.context_cap_tokens,
        row.rates.input_nanos_per_mtok, row.rates.cached_input_nanos_per_mtok, row.rates.output_nanos_per_mtok, now_ms,
    });
}

/// Update caps/rates of the row identified by uid. Affected 0 → no such uid
/// (caller → 404).
pub fn updateRates(conn: anytype, uid: []const u8, rates: Rates, now_ms: i64) !?i64 {
    return conn.exec(sql.UPDATE_RATES, .{
        uid,                               rates.context_cap_tokens,    rates.input_nanos_per_mtok,
        rates.cached_input_nanos_per_mtok, rates.output_nanos_per_mtok, now_ms,
    });
}

/// Delete the row identified by uid. Affected 0 → no such uid (caller → 404).
pub fn remove(conn: anytype, uid: []const u8) !?i64 {
    return conn.exec(sql.DELETE_BY_UID, .{uid});
}
