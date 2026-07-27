//! GET /v1/workspaces/{workspace_id}/fleet-libraries — the workspace gallery:
//! the platform catalog unioned with this workspace's own tenant entries, and
//! nothing from another workspace (M103 Dimensions 5.1/5.2).
//!
//! ## One statement, one order, one page
//!
//! The predecessor issued TWO unbounded reads — every published platform row,
//! then every tenant row for the workspace — and concatenated them in Zig. A
//! workspace's page was therefore whatever the two tables happened to hold, and
//! its "order" was two orders stapled together.
//!
//! §3 replaces that with a single merged keyset read. The merge is not an
//! optimization: a keyset boundary has to be resolvable against the COMBINED
//! sequence, and neither half knows where the other's rows fall, so two
//! independently paged queries cannot produce a resumable total order at all.
//!
//! ## What the summary sheds, and what it keeps
//!
//! `support_files` is gone from the workspace plane entirely; `requirements`
//! and `required_credentials_reasons` stay. §3 asked for all three to move to
//! a per-entry detail route on a size argument. Measured, only one earned the
//! move — and the detail route itself was then removed (no product caller was
//! ever built), so the manifest now lives only on the admin plane
//! (`handlers/library/catalog.zig`).
//!
//! The manifest is capped at `MAX_SUPPORT_FILES` (32) x `MAX_SUPPORT_PATH_LEN`
//! (160), so it contributed up to ~6.3 KB per row and ~630 KB across a 100-row
//! page — past this body's 512 KiB ceiling by itself. It is also the only one
//! of the three with no reader: no component renders it, the install flow
//! ignores it, and the runner materializes real support-file BYTES out of
//! object storage from the lease's bundle hash, never from this path/size
//! manifest. Dropping it is what brings the page inside its ceiling.
//!
//! The other two are RENDERED. `requirements.credentials` becomes the chips on
//! the card, and `required_credentials_reasons` is the ConnectGate's per-
//! credential "why this fleet needs it" copy. Removing them would delete
//! information at the exact moment a user decides whether to install, to save
//! roughly a kilobyte. Spec amended in §Discovery rather than followed off a
//! cliff — the same call the owner made on the `visibility` rename.
//!
//! Still no field for `skill_markdown`, a support-file body, or an object-store
//! key — a read cannot leak bundle content because the struct it would leak
//! through does not exist (M128 Invariant 3).
//!
//! `visibility` keeps its name. Renaming a shipped v1 field to `tier` is what
//! `docs/REST_API_DESIGN_GUIDELINES.md` §9 forbids, and the owner declined it —
//! the tier rank is an internal sort key that never reaches a response body.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const gallery_sql = @import("../../../fleet_library/gallery_sql.zig");
const counters = @import("../../../observability/library_read_counters.zig");
const ReadScope = @import("../../../observability/library_read_scope.zig");
const pagination = @import("../../pagination.zig");
const query = @import("query.zig");
const keyset = @import("fleet_keyset.zig");
const response_size = @import("../../response_size.zig");
const entry_view = @import("entry_view.zig");

const Hx = hx_mod.Hx;

const Q_LIMIT = "limit";
const Q_STARTING_AFTER = "starting_after";
const Q_SEARCH = "q";

const S_QUERY_UNREADABLE = "Query string could not be parsed";
const S_LIMIT_RANGE = "limit must be an integer between 1 and 100";
const S_SEARCH_BOUNDS = "q must be at most 128 bytes once normalized, and valid UTF-8";
const S_CURSOR_MALFORMED = "starting_after is not a cursor this endpoint issued";
const S_CURSOR_MISMATCH = "starting_after was issued for a different workspace, filter or page size";
const S_PAGE_FAILED = "Failed to list this workspace's fleet libraries";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_BODY_CEILING = "This page of fleet libraries is too large to return";

/// MUST match the options the write below uses, or the ceiling compares one
/// body's size against a different body. Nulls are EMITTED: `total` and
/// `next_cursor` are required keys on the envelope, so omitting them when null
/// would make a client distinguish "no more pages" from "this server is old".
const GALLERY_JSON_OPTIONS: std.json.Stringify.Options = .{};

/// One gallery card. Everything here is rendered; see the module note for the
/// one field deliberately absent.
const SummaryEntry = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    visibility: []const u8,
    source_ref: []const u8,
    created_at: i64,
    requirements: entry_view.Requirements,
    required_credentials_reasons: std.json.Value,
};

const Page = struct {
    items: []const SummaryEntry,
    /// Always null. Counting a keyset page costs the scan this pagination exists
    /// to avoid; §Interfaces requires the key present rather than omitted.
    total: ?u64 = null,
    next_cursor: ?[]const u8 = null,
};

pub fn innerGallery(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    counters.beginRead();
    defer counters.endRead();

    var scope = ReadScope.begin(hx.ctx.io, .fleet_summary);
    defer scope.end();

    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        scope.classify(.invalid);
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    // Inputs are validated before a connection is acquired, so a bad limit or a
    // forged cursor costs no pool slot.
    const params = req.query() catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_QUERY_UNREADABLE);
        return;
    };
    const limit = pagination.parseLimit(params.get(Q_LIMIT)) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_LIMIT_RANGE);
        return;
    };
    const search = query.normalizeSearch(hx.alloc, params.get(Q_SEARCH)) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_SEARCH_BOUNDS);
        return;
    };
    const after = decodeStart(hx, &scope, workspace_id, search, limit, params.get(Q_STARTING_AFTER)) catch return;
    scope.endStage(.auth_verify);

    var db = hx.db() orelse {
        scope.classify(.dependency_error);
        scope.endStageWith(.pool_wait, .{ .pool_result = .@"error" });
        return;
    };
    defer db.end();
    counters.noteConnection();
    scope.endStageWith(.pool_wait, .{ .pool_result = .acquired });

    // Two of this read's three statements are here, not in the page query:
    // authorization is the handler's work because only it knows which workspace
    // the path names, and the measurement window opens ahead of it.
    if (!common.authorizeWorkspace(db.conn, hx.principal, workspace_id)) {
        scope.classify(.forbidden);
        scope.endStage(.authorize);
        hx.fail(ec.ERR_FORBIDDEN, S_WORKSPACE_ACCESS_DENIED);
        return;
    }
    scope.endStage(.authorize);

    const page = buildPage(hx, db.conn, workspace_id, search, after, limit) catch {
        scope.classify(.dependency_error);
        scope.endStage(.sql);
        common.internalOperationError(hx.res, S_PAGE_FAILED, hx.req_id);
        return;
    };
    scope.endStage(.sql);
    respond(hx, &scope, page);
}

/// Write the page, refusing one that would exceed §3's encoded-body ceiling.
///
/// The size is measured BEFORE the bytes exist (`Io.Writer.Discarding` counts
/// what the formatter would emit), so a rejected page is never built. It
/// refuses rather than truncates: a caller cannot tell a short page from a
/// complete one, so truncation converts a server fault into missing data the
/// client acts on.
///
/// Measuring here is also what makes the §3 bound checkable at all — the tally
/// has to be the bytes the client actually receives, and a handler that hands a
/// struct to a generic writer never learns that number.
fn respond(hx: Hx, scope: *ReadScope, page: Page) void {
    const encoded_bytes = response_size.encodedWithinCeiling(
        page,
        GALLERY_JSON_OPTIONS,
        counters.FLEET_SUMMARY_MAX_BODY_BYTES,
    ) catch |err| {
        scope.classify(.internal_error);
        scope.endStage(.serialize);
        if (err == response_size.CeilingError.BodyCeilingExceeded) {
            hx.fail(ec.ERR_LIBRARY_BODY_CEILING, S_BODY_CEILING);
        } else {
            common.internalOperationError(hx.res, S_PAGE_FAILED, hx.req_id);
        }
        return;
    };
    counters.noteEncodedBytes(encoded_bytes);
    common.writeJson(hx.res, .ok, page);
    scope.succeed();
    scope.endStageWith(.serialize, .{ .bytes = encoded_bytes, .count = page.items.len });
}

/// Decode and authorize `starting_after`. Null means the first page.
///
/// Two distinct rejections. A cursor that will not decode is `UZ-LIBRARY-001` —
/// not something this endpoint issued. A cursor that decodes but names a
/// different workspace, filter, or page size is `UZ-LIBRARY-002` — a real cursor
/// for a different query. The workspace arm is the one that matters most: it is
/// what stops a cursor minted in one workspace from seeking inside another.
///
/// Nothing is trusted from the cursor except the sort boundary; the workspace and
/// filter used for the read are always the request's.
fn decodeStart(
    hx: Hx,
    scope: *ReadScope,
    workspace_id: []const u8,
    search: ?[]const u8,
    limit: u32,
    raw: ?[]const u8,
) !?keyset.Position {
    const text = raw orelse return null;
    if (text.len == 0) return null;

    const cursor = pagination.decode(hx.alloc, keyset.Cursor, text) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MALFORMED, S_CURSOR_MALFORMED);
        return error.Rejected;
    };
    if (cursor.limit != limit or
        !std.mem.eql(u8, cursor.workspace_uuid, workspace_id) or
        !pagination.filterMatches(cursor.q, search))
    {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MISMATCH, S_CURSOR_MISMATCH);
        return error.Rejected;
    }
    return .{ .created_at = cursor.created_at, .tier_rank = cursor.tier_rank, .id = cursor.id };
}

fn buildPage(
    hx: Hx,
    conn: *pg.Conn,
    workspace_id: []const u8,
    search: ?[]const u8,
    after: ?keyset.Position,
    limit: u32,
) !Page {
    // Over-fetch by one. The extra row never reaches the response; it only
    // answers "is there another page?" without a second COUNT.
    const fetch: i64 = @as(i64, limit) + 1;

    var rows: std.ArrayList(SummaryEntry) = .empty;
    errdefer rows.deinit(hx.alloc);

    var q = try openPage(conn, workspace_id, search, after, fetch);
    defer q.deinit();

    var seen: usize = 0;
    var has_more = false;
    while (try q.next()) |row| {
        seen += 1;
        // The over-fetched row is DRAINED with `continue`, never `break` — an
        // early break leaves the connection mid-result-set, which
        // `make check-pg-drain` exists to catch.
        if (seen > limit) {
            has_more = true;
            continue;
        }
        try rows.append(hx.alloc, try projectRow(hx.alloc, row));
    }

    const items = try rows.toOwnedSlice(hx.alloc);
    counters.noteResults(items.len);
    return .{
        .items = items,
        .next_cursor = if (has_more and items.len > 0)
            try encodeNext(hx.alloc, items[items.len - 1], workspace_id, search, limit)
        else
            null,
    };
}

fn openPage(
    conn: *pg.Conn,
    workspace_id: []const u8,
    like: ?[]const u8,
    after: ?keyset.Position,
    fetch: i64,
) !PgQuery {
    const pos = after orelse return PgQuery.from(try conn.query(
        gallery_sql.SELECT_GALLERY_PAGE_FIRST,
        .{ library_store.VISIBILITY_PUBLIC, workspace_id, like, fetch },
    ));
    return PgQuery.from(try conn.query(gallery_sql.SELECT_GALLERY_PAGE_AFTER, .{
        library_store.VISIBILITY_PUBLIC,
        workspace_id,
        like,
        pos.created_at,
        @as(i32, pos.tier_rank),
        pos.id,
        fetch,
    }));
}

fn projectRow(alloc: std.mem.Allocator, row: anytype) !SummaryEntry {
    const rank = try row.get(i32, 10);
    return .{
        .id = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .name = try alloc.dupe(u8, try row.get([]const u8, 1)),
        .description = try alloc.dupe(u8, try row.get([]const u8, 2)),
        .source_ref = try alloc.dupe(u8, try row.get([]const u8, 3)),
        .created_at = try row.get(i64, 4),
        .requirements = .{
            .credentials = try entry_view.decodeStrings(alloc, try row.get([]const u8, 5)),
            .tools = try entry_view.decodeStrings(alloc, try row.get([]const u8, 6)),
            .network_hosts = try entry_view.decodeStrings(alloc, try row.get([]const u8, 7)),
            .trigger_present = try row.get(bool, 9),
        },
        .required_credentials_reasons = try entry_view.decodeReasons(alloc, try row.get([]const u8, 8)),
        // The rank is a sort key, never a wire value: it is mapped back to its
        // label here, so an unrecognised rank is a loud failure rather than a
        // bare number leaking into a response body.
        .visibility = (keyset.Tier.fromRank(rank) orelse return error.UnknownTierRank).label(),
    };
}

fn encodeNext(
    alloc: std.mem.Allocator,
    last: SummaryEntry,
    workspace_id: []const u8,
    search: ?[]const u8,
    limit: u32,
) ![]u8 {
    const tier = keyset.Tier.fromLabel(last.visibility) orelse return error.UnknownTierRank;
    return pagination.encode(alloc, keyset.Cursor, .{
        .created_at = last.created_at,
        .tier_rank = tier.rank(),
        .id = last.id,
        .workspace_uuid = workspace_id,
        .q = search,
        .limit = limit,
    });
}
