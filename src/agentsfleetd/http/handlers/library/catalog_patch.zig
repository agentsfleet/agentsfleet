//! PATCH /v1/admin/fleet-libraries/{id} — the operator's pencil (M128, widened M130).
//!
//! Split out of `catalog.zig` when M130 widened the write past the file's length
//! cap (RULE FLL). `catalog.zig` keeps the reads and the row-state that `onboard.zig`
//! also depends on; this file owns the write and its guards, which is the whole of
//! what M130 changed.
//!
//! Six fields, three of which M130 added. The field-ownership line is the point of
//! this surface:
//!
//!   the BUNDLE owns  — requirements, content_hash, and the markdown
//!   the OPERATOR owns — name, description, the per-credential copy, and the SOURCE
//!
//! Two guards survive from M128 and must keep surviving: **a published row always
//! has a bundle** (UZ-CATALOG-002) and **a published row is never deleted**
//! (UZ-CATALOG-003, in catalog.zig).
//!
//! M130 adds a third: **a row never advertises a source it is not serving.** The
//! tar in object storage was built from the repository the row USED to name, so
//! repointing the source discards it — `content_hash` goes null and the row falls
//! back to draft, together, inside one statement (UPDATE_CATALOG_IDENTITY). A
//! workspace already running the fleet is untouched: its install pinned its own
//! content hash and downloads that, forever.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const github_source = @import("../../../fleet_library/github_source.zig");
const sql = @import("../../../fleet_library/sql.zig");
const catalog = @import("catalog.zig");
const clock = @import("common").clock;
const log = @import("log").scoped(.library_catalog);

const Hx = hx_mod.Hx;
const RowState = catalog.RowState;

/// Reported when a conflict names the state that forbade the transition
/// (REST guide §4: every 409 carries `current_state`).
const STATE_NO_BUNDLE: []const u8 = "no_bundle";

const MSG_CATALOG_ID_REQUIRED = "A catalog id is required";
const MSG_BODY_REQUIRED = "A request body is required";
const MSG_MALFORMED_JSON = "The request body is not valid JSON";
const MSG_NOT_FOUND = "No fleet library entry has that catalog id";
const MSG_NAME_INVALID = "A name is required, and must be at most 200 characters";
const MSG_SOURCE_REPO_INVALID = "A repository must be owner/repo, using letters, digits, '.', '-' or '_'";
const MSG_SOURCE_REF_INVALID = "A ref must be a branch or tag name, using letters, digits, '.', '-' or '_'";
const MSG_PUBLISH_WITHOUT_BUNDLE = "This entry has no bundle. Fetch it from its repository first, then publish.";

const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";

/// The catalog name is display copy on an operator surface, not an identifier —
/// the slug is. Capped so a paste accident cannot fill the column; the number is
/// spelled in MSG_NAME_INVALID, so the two move together.
const MAX_NAME_LEN: usize = 200;

/// A partial update. Every field is optional: an absent field is untouched, which
/// is what makes editing the description safe without resending the reasons.
///
/// `id` is deliberately not here and must never be added: it is the primary key,
/// and a workspace install references it as `platform_library_id`. Moving it would
/// orphan every install. `ignore_unknown_fields` discards one a caller sends.
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

    const state = catalog.fetchRowState(hx.alloc, db.conn, id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_CATALOG_NOT_FOUND, MSG_NOT_FOUND);
        return;
    };

    // Repointing the source discards the bundle, so a request that repoints AND
    // publishes is asking to publish a bundle it is throwing away in the same
    // breath. By the time the publish would apply there is nothing to serve —
    // which is exactly UZ-CATALOG-002. Report that, rather than let the SQL guard
    // refuse the visibility write and surface as a phantom race.
    if (body.published) |publish| {
        if (publish and (!state.has_bundle or changesSource(body, state))) {
            common.errorResponseConflict(
                hx.res,
                ec.ERR_CATALOG_PUBLISH_WITHOUT_BUNDLE,
                MSG_PUBLISH_WITHOUT_BUNDLE,
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
    catalog.respondEntry(hx, db.conn, id);
}

/// Does this request move the row off the source its stored bundle was built
/// from? Compared against the row as READ, not against field presence: re-sending
/// the repository a row already has is a no-op, and must not withdraw a live fleet
/// just because the dialog echoed the field back.
///
/// This is the handler's pre-check, used to report the publish conflict up front.
/// The write re-derives the same verdict inside UPDATE_CATALOG_IDENTITY's SET list
/// against the live row, so a race here cannot produce a row whose source and
/// content_hash disagree.
pub fn changesSource(body: PatchBody, state: RowState) bool {
    if (body.source_repo) |repo| {
        if (!std.mem.eql(u8, repo, state.source_repo)) return true;
    }
    if (body.source_ref) |ref| {
        if (!std.mem.eql(u8, ref, state.source_ref)) return true;
    }
    return false;
}

/// Reject a name or source the row must never hold. The source rules are the
/// import path's rules, asked of the same function (`github_source.parseOwnerRepo`),
/// so a repository this refuses is exactly a repository `Fetch bundle` would
/// refuse — the edit path and the add path cannot drift.
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
    return true;
}

/// A PATCH may rename, repoint, curate AND publish in one call, so the statements
/// commit together or not at all. Without the transaction a mid-flight failure
/// would leave the description persisted and the fleet still a draft — a
/// half-applied write, which is exactly what the publish guard exists to prevent.
///
/// Every statement is guarded in SQL, so a zero-row result is not "nothing to
/// do" — it means the row moved under us (deleted, or published by a second
/// operator) between the pre-check and the write. That is `error.CatalogRaced`,
/// never a silent success.
///
/// The identity write runs FIRST. It is the one that can null `content_hash`, and
/// the visibility write is guarded on that column — so ordering it first means a
/// request that repoints and publishes is refused by the database too, not only by
/// the handler's pre-check above.
fn applyPatch(alloc: std.mem.Allocator, conn: *pg.Conn, id: []const u8, body: PatchBody) !void {
    const now_ms = clock.nowMillis();

    _ = try conn.exec(SQL_BEGIN, .{});
    var tx_open = true;
    // rollback() rather than exec("ROLLBACK") — exec short-circuits once the
    // connection is in FAIL state (mirrors state/tenant_provider.zig).
    errdefer if (tx_open) {
        conn.rollback() catch |err| log.warn("catalog_patch_rollback_failed", .{ .err = @errorName(err) });
    };

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

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;

// ── Tests ────────────────────────────────────────────────────────────────────

const testing = std.testing;

fn stateOf(repo: []const u8, ref: []const u8, has_bundle: bool) RowState {
    return .{
        .source_repo = repo,
        .source_ref = ref,
        .visibility = library_store.VISIBILITY_PUBLIC,
        .has_bundle = has_bundle,
    };
}

const REPO = "agentsfleet/github-pr-reviewer";
const REF = "main";

test "changesSource: an absent source field changes nothing" {
    const body: PatchBody = .{ .description = "curated" };
    try testing.expect(!changesSource(body, stateOf(REPO, REF, true)));
}

// Dimension 2.4. The dialog echoes every field back, so an operator saving a
// description alone re-sends the repository it already had. Treating "present" as
// "changed" would withdraw a live fleet on a copy edit.
test "changesSource: re-sending the SAME source is a no-op" {
    const body: PatchBody = .{ .source_repo = REPO, .source_ref = REF };
    try testing.expect(!changesSource(body, stateOf(REPO, REF, true)));
}

test "changesSource: a different repository changes the source" {
    const body: PatchBody = .{ .source_repo = "agentsfleet/other" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}

test "changesSource: a different ref changes the source" {
    const body: PatchBody = .{ .source_ref = "v2" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}

// The bundle is keyed to BOTH halves of the source, so pinning a tag on the same
// repository invalidates it just as repointing the repository does.
test "changesSource: same repo, different ref still changes the source" {
    const body: PatchBody = .{ .source_repo = REPO, .source_ref = "release" };
    try testing.expect(changesSource(body, stateOf(REPO, REF, true)));
}
