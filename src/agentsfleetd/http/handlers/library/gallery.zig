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

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const counters = @import("../../../observability/library_read_counters.zig");
const ReadScope = @import("../../../observability/library_read_scope.zig");

/// The one status that means the caller actually received the page.
const HTTP_STATUS_OK: u16 = @intFromEnum(std.http.Status.ok);
const pagination = @import("../../pagination.zig");
const keyset = @import("fleet_keyset.zig");
const response_size = @import("../../response_size.zig");

const Hx = hx_mod.Hx;

const Q_LIMIT = "limit";
const Q_STARTING_AFTER = "starting_after";

const S_QUERY_UNREADABLE = "Query string could not be parsed";
const S_LIMIT_RANGE = "limit must be an integer between 1 and 100";
const S_CURSOR_MALFORMED = "starting_after is not a cursor this endpoint issued";
const S_CURSOR_MISMATCH = "starting_after was issued for a different workspace or page size";
const S_PAGE_FAILED = "Failed to list this workspace's fleet libraries";
const S_WORKSPACE_ACCESS_DENIED = "Workspace access denied";
const S_BODY_CEILING = "This page of fleet libraries is too large to return";

/// MUST match the options the write below uses, or the ceiling compares one
/// body's size against a different body. Nulls are EMITTED: `total` and
/// `next_cursor` are required keys on the envelope, so omitting them when null
/// would make a client distinguish "no more pages" from "this server is old".
const GALLERY_JSON_OPTIONS: std.json.Stringify.Options = .{};

/// The page shape and its builder live next door; this file asks for a page,
/// that one produces it.
const page_mod = @import("gallery_page.zig");
const Page = page_mod.Page;
const buildPage = page_mod.buildPage;

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
    const after = decodeStart(hx, &scope, workspace_id, limit, params.get(Q_STARTING_AFTER)) catch return;
    scope.endStage(.auth_verify);

    var db = hx.db() catch |err| {
        scope.classify(if (err == error.PoolTimeout) .timeout else .dependency_error);
        scope.endStageWith(.pool_wait, .{
            .pool_result = if (err == error.PoolTimeout) .timeout else .@"error",
        });
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

    const page = buildPage(hx, db.conn, workspace_id, after, limit) catch {
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
    // Classified from the STATUS, not from the call having returned.
    // `common.writeJson` swallows a serialization failure and sets 500 rather
    // than returning an error, so `succeed()` here would report `ok` for a
    // request the client received as a 500 — the exact confusion this family
    // exists to remove.
    scope.classify(if (hx.res.status == HTTP_STATUS_OK) .ok else .internal_error);
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
/// Nothing is trusted from the cursor except the sort boundary; the workspace
/// used for the read is always the request's.
fn decodeStart(
    hx: Hx,
    scope: *ReadScope,
    workspace_id: []const u8,
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
    // The search-filter arm retired with the `q` parameter. Workspace and limit
    // remain: the first is the isolation boundary, the second changes the set a
    // cursor was minted against.
    if (cursor.limit != limit or
        !std.mem.eql(u8, cursor.workspace_uuid, workspace_id))
    {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MISMATCH, S_CURSOR_MISMATCH);
        return error.Rejected;
    }
    return .{ .created_at = cursor.created_at, .tier_rank = cursor.tier_rank, .id = cursor.id };
}

test {
    _ = @import("gallery_test.zig");
}
