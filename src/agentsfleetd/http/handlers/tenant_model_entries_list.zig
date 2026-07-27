//! GET /v1/tenants/me/models — the paged list read.
//!
//! Split from tenant_model_entries.zig (the POST/PATCH/DELETE writers) per RULE
//! FLL when keyset pagination pushed that file past 350 lines. The division is
//! not arbitrary: this file owns everything about ASKING for entries — page
//! bounds, cursor decode, cursor authorization — while the writers own
//! everything about CHANGING them. The response projection lives one step
//! further out, in tenant_model_entries_view.zig.
//!
//! The read decrypts nothing and issues a fixed number of statements whatever
//! the page size; see tenant_model_entries_view.zig for how.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const counters = @import("../../observability/library_read_counters.zig");
const ReadScope = @import("../../observability/library_read_scope.zig");
const entries_state = @import("../../state/tenant_model_entries.zig");
const view = @import("tenant_model_entries_view.zig");
const pagination = @import("../pagination.zig");
const response_size = @import("../response_size.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_tenant_model_entries);

const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";
const S_QUERY_UNREADABLE = "Query string could not be parsed";
const S_LIMIT_RANGE = "limit must be an integer between 1 and 100";
const S_CURSOR_MALFORMED = "starting_after is not a cursor this endpoint issued";
const S_CURSOR_MISMATCH = "starting_after was issued for a different tenant or page size";
const S_BODY_CEILING = "The models page exceeded its encoded-body ceiling";
const S_LIST_BUILD_FAILED = "Failed to build the models list";

/// Serialization options for this page, named because they are used TWICE — to
/// measure the body against §3's ceiling and to write it. Measuring under one
/// set of options and writing under another compares one body's size to a
/// different body's ceiling; `emit_null_optional_fields` alone changes the count
/// of every row with an absent `provider` or `base_url`.
const LIST_JSON_OPTIONS: std.json.Stringify.Options = .{ .emit_null_optional_fields = false };

/// Query parameter names. `starting_after` is the request-side spelling even
/// though the response field is `next_cursor` — that asymmetry is the Stripe
/// convention `docs/REST_API_DESIGN_GUIDELINES.md` §3 pins, not a slip.
const Q_LIMIT = "limit";
const Q_STARTING_AFTER = "starting_after";

pub fn innerListModelEntries(hx: Hx, req: *httpz.Request) void {
    // Opens §3's measurement window. Handler entry is AFTER the middleware
    // chain, which is exactly the boundary §3 states its numeric table at, so
    // the token validation the bearer chain performs is outside this endpoint's
    // budget. The window compiles out of release builds.
    counters.beginRead();
    defer counters.endRead();

    // Opens the telemetry window over the same boundary. `defer scope.end()`
    // is what makes the per-request outcome counter total the requests served
    // rather than the exit paths someone remembered to instrument — this
    // handler has nine, and the tenth added later is covered by construction.
    var scope = ReadScope.begin(hx.ctx.io, .tenant_models);
    defer scope.end();

    const tenant_id = hx.principal.tenant_id orelse {
        scope.classify(.forbidden);
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
        return;
    };

    const query = req.query() catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_QUERY_UNREADABLE);
        return;
    };
    const limit = pagination.parseLimit(query.get(Q_LIMIT)) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_LIMIT_RANGE);
        return;
    };
    const after = decodeStart(hx, &scope, tenant_id, limit, query.get(Q_STARTING_AFTER)) catch return;
    scope.endStage(.auth_verify);

    // The two acquire failures are different operator problems: a timeout means
    // the pool is saturated and the fix is capacity, while anything else means
    // the datastore is unreachable. Folding them into one label would put both
    // behind the same alert. This handler used to acquire directly to see that
    // difference; `hx.db()` now names it, so the release discipline stays in the
    // one place that owns it.
    var db = hx.db() catch |err| {
        scope.classify(if (err == error.PoolTimeout) .timeout else .dependency_error);
        scope.endStageWith(.pool_wait, .{
            .pool_result = if (err == error.PoolTimeout) .timeout else .@"error",
        });
        return;
    };
    defer db.end();
    // §3's "1 connection" row is a claim about a read never holding two at once
    // — the shape that deadlocks a pool under load.
    counters.noteConnection();
    scope.endStageWith(.pool_wait, .{ .pool_result = .acquired });

    var result = view.buildList(hx.alloc, db.conn, tenant_id, limit, after) catch |err| {
        scope.classify(.dependency_error);
        scope.endStage(.sql);
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer result.deinit(hx.alloc);
    scope.endStage(.sql);

    respond(hx, &scope, result);
}

/// Write the page, once it is proven to fit §3's encoded-body ceiling.
///
/// Split from the handler so the request-shaping half (bounds, cursor, auth)
/// and the response-shaping half stay separately readable, and so the ceiling
/// check sits next to the serialization whose size it governs.
fn respond(hx: Hx, scope: *ReadScope, result: view.ListResult) void {
    counters.noteResults(result.rows.len);

    const payload = .{
        // `models`, not `items`: renaming a shipped v1 field is what
        // docs/REST_API_DESIGN_GUIDELINES.md §9 forbids, and the owner
        // declined the equivalent rename on the Fleet gallery. `total` and
        // `next_cursor` are ADDED beside it, so the page is navigable
        // without breaking a client. `total` is always null — counting a
        // keyset page costs a scan the pagination exists to avoid, and §3
        // requires the key to be present rather than omitted.
        .models = result.rows,
        // `std.json.Value`, not `?T`. §3 requires `total` and `next_cursor`
        // to be PRESENT on every page, including the last, but the row
        // projection needs `emit_null_optional_fields = false` so an absent
        // `provider` or `base_url` is omitted rather than serialized as
        // null (§3 again, and the shape every client already parses). The
        // flag is global, so these two carry an explicit JSON null instead
        // of being Zig optionals, and both rules hold at once.
        //
        // `total` is always null: counting a keyset page costs the scan
        // this pagination exists to avoid, and §3 declares null to mean
        // "not computed" rather than allowing the key to vanish.
        .total = std.json.Value{ .null = {} },
        .next_cursor = if (result.next_cursor) |c|
            std.json.Value{ .string = c }
        else
            std.json.Value{ .null = {} },
        .platform_default_available = result.platform_default_available,
        .platform_default = result.platform_default,
    };

    // Measured, not estimated — and measured before the body exists, so a page
    // over the ceiling is never partially written. §3 requires a typed refusal
    // rather than a truncated page: a client cannot distinguish a short page
    // from a complete one, so truncating turns a server-side fault into
    // silently missing data the caller will act on.
    const ceiling = counters.tenantRegistryBodyCeiling();
    const encoded_bytes = response_size.encodedWithinCeiling(
        payload,
        LIST_JSON_OPTIONS,
        ceiling,
    ) catch |err| switch (err) {
        response_size.CeilingError.BodyCeilingExceeded => {
            scope.classify(.internal_error);
            scope.endStage(.serialize);
            log.err("body_ceiling_exceeded", .{
                .error_code = ec.ERR_LIBRARY_BODY_CEILING,
                .ceiling_bytes = ceiling,
                .rows = result.rows.len,
            });
            hx.fail(ec.ERR_LIBRARY_BODY_CEILING, S_BODY_CEILING);
            return;
        },
        else => {
            scope.classify(.internal_error);
            scope.endStage(.serialize);
            common.internalOperationError(hx.res, S_LIST_BUILD_FAILED, hx.req_id);
            return;
        },
    };
    counters.noteEncodedBytes(encoded_bytes);

    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.json(payload, LIST_JSON_OPTIONS) catch {
        // Reclassified AFTER the optimistic success below would have run: the
        // write is the last thing that can fail, and a page that failed to
        // serialize is not a page the caller received.
        scope.classify(.internal_error);
        scope.endStageWith(.serialize, .{ .count = result.rows.len });
        common.internalOperationError(hx.res, S_LIST_BUILD_FAILED, hx.req_id);
        return;
    };
    scope.succeed();
    scope.endStageWith(.serialize, .{ .bytes = encoded_bytes, .count = result.rows.len });
}

/// Decode and authorize `starting_after`. Returns null for the first page.
///
/// Two distinct rejections, and the difference is the point. A cursor that will
/// not decode is `UZ-LIBRARY-001` — the client did not send something this
/// endpoint issued. A cursor that decodes but names another tenant or another
/// page size is `UZ-LIBRARY-002` — it is a real cursor for a different query.
/// Folding them into one code would hide a cross-tenant replay attempt inside
/// the same signal as a truncated URL.
///
/// Nothing is trusted from the cursor except the sort boundary: the tenant used
/// for the read is always the authenticated one, never the cursor's.
fn decodeStart(hx: Hx, scope: *ReadScope, tenant_id: []const u8, limit: u32, raw: ?[]const u8) !?entries_state.PageStart {
    const text = raw orelse return null;
    if (text.len == 0) return null;

    const cursor = pagination.decode(hx.alloc, view.Cursor, text) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MALFORMED, S_CURSOR_MALFORMED);
        return error.Rejected;
    };
    if (!pagination.identityMatches(cursor.tenant_uuid, tenant_id, cursor.limit, limit)) {
        // `invalid`, not `unauthorized`: a cursor naming another tenant is a
        // malformed REQUEST for this endpoint, and the two error codes already
        // keep the replay signal distinguishable in the log.
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MISMATCH, S_CURSOR_MISMATCH);
        return error.Rejected;
    }
    return .{ .created_at = cursor.created_at, .id = cursor.id };
}
