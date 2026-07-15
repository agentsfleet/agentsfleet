//! The operator's platform catalog (M128) — /v1/admin/fleet-libraries: the reads,
//! the delete, and the row-state every guard on this surface is built from.
//!
//!   GET    /v1/admin/fleet-libraries        list every row, draft or published
//!   DELETE /v1/admin/fleet-libraries/{id}   remove an unpublished entry
//!
//! The PATCH lives in `catalog_patch.zig` — M130 widened it to own the row's
//! identity (name, repository, ref) and it outgrew this file's length cap. The
//! add / refetch write is `onboard.zig`. All of them sit behind
//! `platform-library:write` (route_scopes.zig), and together they are the whole
//! lifecycle — which is why the catalog needs no SQL seed: a fleet is born,
//! curated, published, withdrawn, and deleted from the dashboard.
//!
//! `RowState` + `fetchRowState` are public because three guards read them: this
//! file's delete, `catalog_patch.zig`'s publish check, and `onboard.zig`'s
//! id-collision check.
//!
//! **A published row is never deleted** — UZ-CATALOG-003; withdraw it first, so no
//! tenant loses a fleet mid-install. Its sibling guard, **a published row always
//! has a bundle** (UZ-CATALOG-002), is enforced in `catalog_patch.zig` and in the
//! SQL itself.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const etag = @import("../../etag.zig");
const ec = @import("../../../errors/error_registry.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const sql = @import("../../../fleet_library/sql.zig");
const entry_view = @import("entry_view.zig");

const Hx = hx_mod.Hx;

const MSG_CATALOG_ID_REQUIRED = "A catalog id is required";
const MSG_NOT_FOUND = "No fleet library entry has that catalog id";

/// One catalog row as the operator sees it. Deliberately carries no markdown and
/// no storage key — see `entry_view.zig`.
const CatalogEntry = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    source_repo: []const u8,
    source_ref: []const u8,
    /// The publish lifecycle: "draft" | "public".
    visibility: []const u8,
    /// null ⇒ no bundle has ever been fetched for this entry.
    content_hash: ?[]const u8,
    requirements: entry_view.Requirements,
    required_credentials_reasons: std.json.Value,
    support_files: []const entry_view.SupportSummary,
    updated_at: i64,
    /// Optimistic-concurrency tag over the operator-editable surface
    /// (`rowSurface`), so a row editor can send it as `If-Match` on PATCH.
    etag: []const u8,
};

const ListBody = struct { entries: []const CatalogEntry };

/// The lifecycle facts one row carries, read before any guarded write. Public
/// because the add path in onboard.zig reads it too (collision guard) and the
/// PATCH reads it for both the publish precheck and the `If-Match` verdict.
/// `name`/`description`/`reasons_raw` exist only to feed `rowSurface`.
pub const RowState = struct {
    source_repo: []const u8,
    source_ref: []const u8,
    visibility: []const u8,
    has_bundle: bool,
    name: []const u8,
    description: []const u8,
    reasons_raw: []const u8,
};

/// The catalog row's operator-editable surface, in a fixed order — the field
/// list its ETag hashes. `content_hash` is deliberately absent: a bundle
/// refetch changes the hash but not what the operator authored, so it must not
/// 412 an unrelated description edit. Read identically here (the wire tag) and
/// in `catalog_patch.zig` (the `If-Match` verdict), so the two tags a save
/// compares are computed the same way.
pub fn rowSurface(
    name: []const u8,
    description: []const u8,
    source_repo: []const u8,
    source_ref: []const u8,
    reasons_raw: []const u8,
    visibility: []const u8,
) [6]?[]const u8 {
    return .{ name, description, source_repo, source_ref, reasons_raw, visibility };
}

pub fn innerAdminCatalogList(hx: Hx) void {
    var db = hx.db() orelse return;
    defer db.end();

    const entries = buildCatalog(hx.alloc, db.conn) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.ok(.ok, ListBody{ .entries = entries });
}

/// One row of SELECT_ADMIN_CATALOG (or SELECT_ADMIN_CATALOG_ROW — the same
/// projection, so the column indices are shared by construction).
fn rowToEntry(alloc: std.mem.Allocator, row: anytype) !CatalogEntry {
    const hash_opt = try row.get(?[]const u8, 6);
    const name = try alloc.dupe(u8, try row.get([]const u8, 1));
    const description = try alloc.dupe(u8, try row.get([]const u8, 2));
    const source_repo = try alloc.dupe(u8, try row.get([]const u8, 3));
    const source_ref = try alloc.dupe(u8, try row.get([]const u8, 4));
    const visibility = try alloc.dupe(u8, try row.get([]const u8, 5));
    const reasons_raw = try row.get([]const u8, 10);
    return .{
        .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .name = name,
        .description = description,
        .source_repo = source_repo,
        .source_ref = source_ref,
        .visibility = visibility,
        .content_hash = if (hash_opt) |h| try alloc.dupe(u8, h) else null,
        .requirements = .{
            .credentials = try entry_view.decodeStrings(alloc, try row.get([]const u8, 7)),
            .tools = try entry_view.decodeStrings(alloc, try row.get([]const u8, 8)),
            .network_hosts = try entry_view.decodeStrings(alloc, try row.get([]const u8, 9)),
            .trigger_present = try row.get(bool, 12),
        },
        .required_credentials_reasons = try entry_view.decodeReasons(alloc, reasons_raw),
        .support_files = try entry_view.decodeSummaries(alloc, try row.get([]const u8, 11)),
        .updated_at = try row.get(i64, 13),
        .etag = try etag.compute(alloc, &rowSurface(name, description, source_repo, source_ref, reasons_raw, visibility)),
    };
}

fn buildCatalog(alloc: std.mem.Allocator, conn: *pg.Conn) ![]const CatalogEntry {
    var rows: std.ArrayList(CatalogEntry) = .empty;
    errdefer rows.deinit(alloc);

    var q = PgQuery.from(try conn.query(sql.SELECT_ADMIN_CATALOG, .{}));
    defer q.deinit();
    while (try q.next()) |row| try rows.append(alloc, try rowToEntry(alloc, row));
    return rows.toOwnedSlice(alloc);
}

fn fetchEntry(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !?CatalogEntry {
    var q = PgQuery.from(try conn.query(sql.SELECT_ADMIN_CATALOG_ROW, .{id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    return try rowToEntry(alloc, row);
}

pub fn innerAdminCatalogDelete(hx: Hx, id: []const u8) void {
    if (id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_CATALOG_ID_REQUIRED);
        return;
    }
    var db = hx.db() orelse return;
    defer db.end();

    const state = fetchRowState(hx.alloc, db.conn, id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
        return;
    };

    // A live fleet is never deleted out from under the tenants who can install
    // it. Withdrawing first is one click, and it is reversible.
    if (std.mem.eql(u8, state.visibility, library_store.VISIBILITY_PUBLIC)) {
        common.errorResponseConflict(
            hx.res,
            ec.ERR_CATALOG_DELETE_PUBLISHED,
            "This fleet is published. Unpublish it first, then delete it.",
            hx.req_id,
            library_store.VISIBILITY_PUBLIC,
        );
        return;
    }

    // The DELETE is guarded on `visibility <> 'public'`, so a zero-row result
    // means the fleet was PUBLISHED between the check above and the write — the
    // race the guard exists for. Reporting 204 there would tell the operator a
    // live fleet was removed while every workspace could still install it.
    deleteDraft(db.conn, id) catch |err| switch (err) {
        error.CatalogRaced => {
            common.errorResponseConflict(
                hx.res,
                ec.ERR_CATALOG_DELETE_PUBLISHED,
                "This fleet was published while you were deleting it. Unpublish it first, then delete.",
                hx.req_id,
                library_store.VISIBILITY_PUBLIC,
            );
            return;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    hx.noContent();
}

fn deleteDraft(conn: *pg.Conn, id: []const u8) !void {
    var q = PgQuery.from(try conn.query(sql.DELETE_CATALOG_DRAFT, .{ id, library_store.VISIBILITY_PUBLIC }));
    defer q.deinit();
    _ = try q.next() orelse return error.CatalogRaced;
}

/// Read the lifecycle facts for one row. Returns null when the id names nothing.
/// Every guarded write reads this first, so "not found" and "forbidden by state"
/// stay distinguishable — a guarded UPDATE touching zero rows cannot tell them
/// apart, and the operator deserves to know which one happened.
pub fn fetchRowState(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !?RowState {
    return fetchRowStateWithSql(alloc, conn, id, sql.SELECT_CATALOG_ROW);
}

/// PATCH-only row snapshot. Caller owns the surrounding transaction, which
/// keeps this lock through the version check and all writes.
pub fn fetchRowStateForUpdate(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8) !?RowState {
    return fetchRowStateWithSql(alloc, conn, id, sql.SELECT_CATALOG_ROW_FOR_UPDATE);
}

fn fetchRowStateWithSql(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8, statement: []const u8) !?RowState {
    var q = PgQuery.from(try conn.query(statement, .{id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const repo = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(repo);
    const visibility = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(visibility);
    const hash = try row.get(?[]const u8, 2);
    const ref = try alloc.dupe(u8, try row.get([]const u8, 3));
    errdefer alloc.free(ref);
    const name = try alloc.dupe(u8, try row.get([]const u8, 4));
    errdefer alloc.free(name);
    const description = try alloc.dupe(u8, try row.get([]const u8, 5));
    errdefer alloc.free(description);
    const reasons_raw = try alloc.dupe(u8, try row.get([]const u8, 6));
    return .{
        .source_repo = repo,
        .source_ref = ref,
        .visibility = visibility,
        .has_bundle = hash != null,
        .name = name,
        .description = description,
        .reasons_raw = reasons_raw,
    };
}

/// Re-read the row and return it, so a PATCH answers with the entry as it now
/// stands rather than echoing the request back. Reads by id: rebuilding the whole
/// catalog to find one row is O(catalog) work for an O(1) answer, on the hot path
/// of this surface's most-used action.
pub fn respondEntry(hx: Hx, conn: *pg.Conn, id: []const u8) void {
    const entry = fetchEntry(hx.alloc, conn, id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
        return;
    };
    hx.ok(.ok, entry);
}
