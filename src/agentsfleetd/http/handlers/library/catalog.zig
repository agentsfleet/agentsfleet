//! The operator's platform catalog (M128) — /v1/admin/fleet-libraries.
//!
//!   GET    /v1/admin/fleet-libraries        list every row, draft or published
//!   PATCH  /v1/admin/fleet-libraries/{id}   curate the copy; publish / unpublish
//!   DELETE /v1/admin/fleet-libraries/{id}   remove an unpublished entry
//!
//! All three sit behind `platform-library:write` (route_scopes.zig); the add /
//! refetch write is `onboard.zig`. Together they are the whole lifecycle, which is
//! why the catalog needs no SQL seed: a fleet is born, curated, published,
//! withdrawn, and deleted from the dashboard.
//!
//! Two guards are the point of this file, not incidental to it. **A published row
//! always has a bundle** — publishing an entry whose bundle was never fetched is
//! UZ-CATALOG-002, because 'public' means "installable by every tenant" and an
//! empty row has nothing to install. **A published row is never deleted** —
//! UZ-CATALOG-003; withdraw it first, so no tenant loses a fleet mid-install.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const sql = @import("../../../fleet_library/sql.zig");
const entry_view = @import("entry_view.zig");
const clock = @import("common").clock;
const log = @import("log").scoped(.library_catalog);

const Hx = hx_mod.Hx;

/// Reported when a conflict names the state that forbade the transition
/// (REST guide §4: every 409 carries `current_state`).
const STATE_NO_BUNDLE: []const u8 = "no_bundle";

const MSG_CATALOG_ID_REQUIRED = "A catalog id is required";
const MSG_BODY_REQUIRED = "A request body is required";
const MSG_MALFORMED_JSON = "The request body is not valid JSON";
const MSG_NOT_FOUND = "No fleet library entry has that catalog id";

const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";

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
};

const ListBody = struct { entries: []const CatalogEntry };

/// A partial update. Every field is optional: an absent field is untouched, which
/// is what makes editing the description safe without resending the reasons.
const PatchBody = struct {
    description: ?[]const u8 = null,
    required_credentials_reasons: ?std.json.Value = null,
    published: ?bool = null,
};

/// The lifecycle facts one row carries, read before any guarded write. Public
/// because the add path in onboard.zig reads it too, for its collision guard.
pub const RowState = struct {
    source_repo: []const u8,
    visibility: []const u8,
    has_bundle: bool,
};

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
    return .{
        .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .source_repo = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .source_ref = try alloc.dupe(u8, try row.get([]const u8, 4)),
        .visibility = try alloc.dupe(u8, try row.get([]const u8, 5)),
        .content_hash = if (hash_opt) |h| try alloc.dupe(u8, h) else null,
        .requirements = .{
            .credentials = try entry_view.decodeStrings(alloc, try row.get([]const u8, 7)),
            .tools = try entry_view.decodeStrings(alloc, try row.get([]const u8, 8)),
            .network_hosts = try entry_view.decodeStrings(alloc, try row.get([]const u8, 9)),
            .trigger_present = try row.get(bool, 12),
        },
        .required_credentials_reasons = try entry_view.decodeReasons(alloc, try row.get([]const u8, 10)),
        .support_files = try entry_view.decodeSummaries(alloc, try row.get([]const u8, 11)),
        .updated_at = try row.get(i64, 13),
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

pub fn innerAdminCatalogPatch(hx: Hx, req: *httpz.Request, id: []const u8) void {
    if (id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_CATALOG_ID_REQUIRED);
        return;
    }
    const raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, raw, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, raw, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    var db = hx.db() orelse return;
    defer db.end();

    const state = fetchRowState(hx.alloc, db.conn, id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
        return;
    };

    // Publishing an entry with no bundle is the one transition the lifecycle
    // forbids outright: 'public' promises every tenant something installable.
    if (body.published) |publish| {
        if (publish and !state.has_bundle) {
            common.errorResponseConflict(
                hx.res,
                ec.ERR_CATALOG_PUBLISH_WITHOUT_BUNDLE,
                "This entry has no bundle. Fetch it from its repository first, then publish.",
                hx.req_id,
                STATE_NO_BUNDLE,
            );
            return;
        }
    }

    applyPatch(hx.alloc, db.conn, id, body) catch |err| switch (err) {
        error.CatalogRaced => {
            hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
            return;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    respondEntry(hx, db.conn, id);
}

/// A PATCH may curate AND publish in one call, so the two statements commit
/// together or not at all. Without the transaction a mid-flight failure would
/// leave the description persisted and the fleet still a draft — a half-applied
/// write, which is exactly what the publish guard exists to prevent.
///
/// Both statements are guarded in SQL, so a zero-row result is not "nothing to
/// do" — it means the row moved under us (deleted, or published by a second
/// operator) between the pre-check and the write. That is `error.CatalogRaced`,
/// never a silent success.
fn applyPatch(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8, body: PatchBody) !void {
    const now_ms = clock.nowMillis();

    _ = try conn.exec(SQL_BEGIN, .{});
    var tx_open = true;
    // rollback() rather than exec("ROLLBACK") — exec short-circuits once the
    // connection is in FAIL state (mirrors state/tenant_provider.zig).
    errdefer if (tx_open) {
        conn.rollback() catch |err| log.warn("catalog_patch_rollback_failed", .{ .err = @errorName(err) });
    };

    if (body.description != null or body.required_credentials_reasons != null) {
        const reasons_json: ?[]const u8 = if (body.required_credentials_reasons) |v|
            try std.fmt.allocPrint(alloc, "{f}", .{std.json.fmt(v, .{})})
        else
            null;
        try expectOneRow(conn, sql.UPDATE_CATALOG_CURATE, .{ id, body.description, reasons_json, now_ms });
    }

    if (body.published) |publish| {
        const target = if (publish) library_store.VISIBILITY_PUBLIC else library_store.VISIBILITY_DRAFT;
        try expectOneRow(conn, sql.UPDATE_CATALOG_VISIBILITY, .{
            id,
            target,
            now_ms,
            library_store.VISIBILITY_PUBLIC,
        });
    }

    _ = try conn.exec(SQL_COMMIT, .{});
    tx_open = false;
}

/// Run a guarded write and insist it touched its row. Every statement here
/// RETURNINGs its id precisely so "the guard refused" is distinguishable from
/// "it worked" — discarding that is how a refused write gets reported as a
/// success.
fn expectOneRow(conn: *pg.Conn, statement: []const u8, args: anytype) !void {
    var q = PgQuery.from(try conn.query(statement, args));
    defer q.deinit();
    _ = try q.next() orelse return error.CatalogRaced;
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
    var q = PgQuery.from(try conn.query(sql.SELECT_CATALOG_ROW, .{id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const repo = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(repo);
    const visibility = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(visibility);
    const hash = try row.get(?[]const u8, 2);
    return .{ .source_repo = repo, .visibility = visibility, .has_bundle = hash != null };
}

/// Re-read the row and return it, so a PATCH answers with the entry as it now
/// stands rather than echoing the request back. Reads by id: rebuilding the whole
/// catalog to find one row is O(catalog) work for an O(1) answer, on the hot path
/// of this surface's most-used action.
fn respondEntry(hx: Hx, conn: *pg.Conn, id: []const u8) void {
    const entry = fetchEntry(hx.alloc, conn, id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
        return;
    };
    hx.ok(.ok, entry);
}
