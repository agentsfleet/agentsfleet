//! PATCH /v1/admin/fleet-libraries/{id} — the operator's pencil.
//!
//! `catalog.zig` keeps the reads and the row-state that `onboard.zig` also
//! depends on; this file owns the write and its guards (RULE FLL keeps them in
//! separate files). The field-ownership line is the point of this surface:
//!
//!   the BUNDLE owns  — requirements, content_hash, and the markdown
//!   the OPERATOR owns — name, description, the per-credential copy, and the SOURCE
//!
//! Three guards: **a published row always has a bundle** (UZ-CATALOG-002),
//! **a published row is never deleted** (UZ-CATALOG-003, in catalog.zig), and
//! **a row never advertises a source it is not serving** — the tar in object
//! storage was built from the repository the row USED to name, so repointing the
//! source discards it: `content_hash` goes null and the row falls back to draft,
//! together, inside one statement (UPDATE_CATALOG_IDENTITY). A workspace already
//! running the fleet is untouched: its install pinned its own content hash and
//! downloads that, forever.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const etag = @import("../../etag.zig");
const ec = @import("../../../errors/error_registry.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const github_source = @import("../../../fleet_library/github_source.zig");
const sql = @import("../../../fleet_library/sql.zig");
const catalog = @import("catalog.zig");
const clock = @import("common").clock;
const log = @import("log").scoped(.library_catalog);

const Hx = hx_mod.Hx;
const RowState = catalog.RowState;

const ApplyOutcome = union(enum) {
    updated,
    stale_etag: []const u8,
    not_found,
    visibility_refused,
};

/// The `current_state` a publish-without-bundle 409 reports (docs/REST_API_DESIGN_GUIDELINES.md §4).
const STATE_NO_BUNDLE: []const u8 = "no_bundle";

const MSG_CATALOG_ID_REQUIRED = "A catalog id is required";
const MSG_BODY_REQUIRED = "A request body is required";
const MSG_MALFORMED_JSON = "The request body is not valid JSON";
const MSG_NOT_FOUND = "No fleet library entry has that catalog id";
const MSG_NAME_INVALID = "A name is required, and must be at most 200 characters";
const MSG_SOURCE_REPO_INVALID = "A repository must be owner/repo, using letters, digits, '.', '-' or '_'";
const MSG_SOURCE_REF_INVALID = "A ref must be a branch or tag name, using letters, digits, '.', '-' or '_'";
const MSG_REASONS_INVALID = "required_credentials_reasons must be an object mapping credential names to strings";
const MSG_PUBLISH_WITHOUT_BUNDLE = "This entry has no bundle. Fetch it from its repository first, then publish.";
const MSG_ROW_STALE = "This catalog entry changed since you loaded it. Refresh to see the latest, then re-apply your edit.";

const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";

/// Display-copy cap (the slug is the identifier); the number is spelled in MSG_NAME_INVALID.
const MAX_NAME_LEN: usize = 200;

/// A partial update: an absent field is untouched. `id` is never patchable — installs reference it as `platform_library_id`.
pub const PatchBody = struct {
    name: ?[]const u8 = null,
    description: ?[]const u8 = null,
    source_repo: ?[]const u8 = null,
    source_ref: ?[]const u8 = null,
    required_credentials_reasons: ?std.json.Value = null,
    published: ?bool = null,
};

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

    if (!validIdentity(hx, body)) return;

    var db = hx.db() orelse return;
    defer db.end();

    const outcome = applyPatch(hx.alloc, db.conn, id, body, etag.ifMatch(req)) catch |err| switch (err) {
        error.CatalogRaced => {
            hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
            return;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    switch (outcome) {
        .updated => {},
        .stale_etag => |current| {
            log.info("catalog_patch_stale_etag", .{ .error_code = ec.ERR_CATALOG_ROW_STALE, .req_id = hx.req_id });
            common.errorResponsePrecondition(hx.res, ec.ERR_CATALOG_ROW_STALE, MSG_ROW_STALE, hx.req_id, current);
            return;
        },
        .not_found => {
            hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
            return;
        },
        .visibility_refused => {
            conflictNoBundle(hx);
            return;
        },
    }
    catalog.respondEntry(hx, db.conn, id);
}

/// One spelling of the publish-without-bundle 409, so the two race arms can never answer differently.
fn conflictNoBundle(hx: Hx) void {
    common.errorResponseConflict(
        hx.res,
        ec.ERR_CATALOG_PUBLISH_WITHOUT_BUNDLE,
        MSG_PUBLISH_WITHOUT_BUNDLE,
        hx.req_id,
        STATE_NO_BUNDLE,
    );
}

/// Compared against the row as READ, not field presence — re-sending the stored value is a no-op, never a withdrawal.
pub fn changesSource(body: PatchBody, state: RowState) bool {
    if (body.source_repo) |repo| {
        if (!std.mem.eql(u8, repo, state.source_repo)) return true;
    }
    if (body.source_ref) |ref| {
        if (!std.mem.eql(u8, ref, state.source_ref)) return true;
    }
    return false;
}

/// Asks the import path's own validators, so the edit path and the add path cannot drift.
fn validIdentity(hx: Hx, body: PatchBody) bool {
    if (body.name) |name| {
        if (name.len == 0 or name.len > MAX_NAME_LEN) {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_NAME_INVALID);
            return false;
        }
    }
    if (body.source_repo) |repo| {
        if (github_source.parseOwnerRepo(repo) == null) {
            hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, MSG_SOURCE_REPO_INVALID);
            return false;
        }
    }
    if (body.source_ref) |ref| {
        if (!github_source.validSegment(ref)) {
            hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, MSG_SOURCE_REF_INVALID);
            return false;
        }
    }
    // The refetch prune walks this jsonb with jsonb_each_text — a non-object here
    // would kill every future refetch of the row.
    if (body.required_credentials_reasons) |reasons| {
        if (reasons != .object) {
            hx.fail(ec.ERR_INVALID_REQUEST, MSG_REASONS_INVALID);
            return false;
        }
        var it = reasons.object.iterator();
        while (it.next()) |kv| {
            if (kv.value_ptr.* != .string) {
                hx.fail(ec.ERR_INVALID_REQUEST, MSG_REASONS_INVALID);
                return false;
            }
        }
    }
    return true;
}

/// One row-locked transaction: version check, publish guard, then writes.
fn applyPatch(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    id: []const u8,
    body: PatchBody,
    if_match: ?[]const u8,
) !ApplyOutcome {
    const now_ms = clock.nowMillis();

    _ = try conn.exec(SQL_BEGIN, .{});
    var tx_open = true;
    // rollback() rather than exec("ROLLBACK") — exec short-circuits once the
    // connection is in FAIL state (mirrors state/tenant_provider.zig).
    defer if (tx_open) {
        conn.rollback() catch |err| log.warn("catalog_patch_rollback_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    };

    const state = try catalog.fetchRowStateForUpdate(alloc, conn, id) orelse return .not_found;
    if (try etag.staleTag(alloc, if_match, &catalog.rowSurface(
        state.name,
        state.description,
        state.source_repo,
        state.source_ref,
        state.reasons_raw,
        state.visibility,
    ))) |current| return .{ .stale_etag = current };
    if ((body.published orelse false) and (!state.has_bundle or changesSource(body, state))) {
        return .visibility_refused;
    }

    if (body.name != null or body.source_repo != null or body.source_ref != null) {
        try expectOneRow(conn, sql.UPDATE_CATALOG_IDENTITY, .{
            id,
            body.name,
            body.source_repo,
            body.source_ref,
            library_store.VISIBILITY_DRAFT,
            now_ms,
        });
    }

    if (body.description != null or body.required_credentials_reasons != null) {
        const reasons_json: ?[]const u8 = if (body.required_credentials_reasons) |v|
            try std.fmt.allocPrint(alloc, "{f}", .{std.json.fmt(v, .{})})
        else
            null;
        try expectOneRow(conn, sql.UPDATE_CATALOG_CURATE, .{ id, body.description, reasons_json, now_ms });
    }

    if (body.published) |publish| {
        const target = if (publish) library_store.VISIBILITY_PUBLIC else library_store.VISIBILITY_DRAFT;
        expectOneRow(conn, sql.UPDATE_CATALOG_VISIBILITY, .{
            id,
            target,
            now_ms,
            library_store.VISIBILITY_PUBLIC,
        }) catch |err| switch (err) {
            // Zero rows from THIS statement is not the generic race: its WHERE
            // carries the publish-needs-a-bundle guard, so the handler must
            // answer with the row's current truth, not a 404.
            error.CatalogRaced => return .visibility_refused,
            else => return err,
        };
    }

    _ = try conn.exec(SQL_COMMIT, .{});
    tx_open = false;
    return .updated;
}

/// Zero rows from a guarded write is the guard refusing (CatalogRaced), never a silent success.
fn expectOneRow(conn: *pg.Conn, statement: []const u8, args: anytype) !void {
    var q = PgQuery.from(try conn.query(statement, args));
    defer q.deinit();
    _ = try q.next() orelse return error.CatalogRaced;
}

test {
    _ = @import("catalog_patch_test.zig");
}
