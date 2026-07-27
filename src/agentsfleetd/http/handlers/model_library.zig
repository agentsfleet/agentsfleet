//! GET /v1/models — the model library catalogue (core.model_library), served to
//! any authenticated tenant as a bounded, cached, conditionally-revalidated page
//! (§2). The catalogue prices the platform's billing spine and has no anonymous
//! consumer — reads require an authenticated tenant.
//!
//! Provider hosting is encoded in the model_id itself
//! (`accounts/fireworks/...` is Fireworks; bare `kimi-k2.6` is Moonshot;
//! `claude-*` is Anthropic; etc.). Tenants pick provider via a user-named
//! credential body and `tenant provider set --credential <name>`.
//!
//! Per-token rates accompany each row. Rates are charged only under
//! platform-managed posture; self-managed pays a flat overhead and is billed by
//! the tenant's own provider account.
//!
//! This file owns everything about ASKING for a page — bounds, filters, cursor
//! decode, cache selection. `model_library_page.zig` owns producing one.
//!
//! ## The order of operations is the design
//!
//! Inputs are validated before a connection is acquired, so a bad `limit` or
//! cursor costs no pool slot. Then the revision is read — after authentication
//! and BEFORE cache selection — so the generation a response is stored under is
//! the one this request actually observed. That ordering is what makes a stale
//! candidate unreachable rather than dangerous: it lands under a key containing
//! its own generation, and the next request reads the next generation and looks
//! somewhere else.
//!
//! It also produces §3's statement budget exactly. A cache hit issues one
//! statement (the revision); a miss issues two (revision + page). Neither number
//! is arranged for — they fall out of reading the generation before deciding
//! whether the page has to be built at all.

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");
const pg = @import("pg");

const revision_state = @import("../../state/model_catalogue_revision.zig");
const id_format = @import("../../types/id_format.zig");
const model_library_store = @import("../../state/model_library_store.zig");
const counters = @import("../../observability/library_read_counters.zig");
const ReadScope = @import("../../observability/library_read_scope.zig");
const ec = @import("../../errors/error_registry.zig");
const hx_mod = @import("hx.zig");
const pagination = @import("../pagination.zig");
const query = @import("library/query.zig");
const catalogue_key = @import("library/catalogue_key.zig");
const page_mod = @import("model_library_page.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_model_library);

/// Route path — matched by the router and shared verbatim with the TypeScript
/// client (MODEL_LIBRARY_PATH in ui/packages/app/lib/api/model_library.ts).
pub const MODEL_LIBRARY_PATH = "/v1/models";

/// Query parameter names. `starting_after` is the request-side spelling even
/// though the response field is `next_cursor` — the Stripe convention
/// `docs/REST_API_DESIGN_GUIDELINES.md` §3 pins, not a slip.
const Q_LIMIT = "limit";
const Q_STARTING_AFTER = "starting_after";
const Q_SEARCH = "q";
const Q_PROVIDER = "provider";

const S_QUERY_UNREADABLE = "Query string could not be parsed";
const S_LIMIT_RANGE = "limit must be an integer between 1 and 100";
const S_SEARCH_BOUNDS = "q must be at most 128 bytes once normalized, and valid UTF-8";
const S_CURSOR_MALFORMED = "starting_after is not a cursor this endpoint issued";
const S_CURSOR_MISMATCH = "starting_after was issued for different filters or page size";
const S_REVISION_UNAVAILABLE = "The catalogue revision could not be read";
const S_PAGE_BUILD_FAILED = "Failed to build the catalogue page";

pub fn innerGetModelLibrary(hx: Hx, req: *httpz.Request) void {
    // Opens §3's measurement window at handler entry — after the middleware
    // chain, which is exactly the boundary §3 states its numeric table at.
    counters.beginRead();
    defer counters.endRead();

    var scope = ReadScope.begin(hx.ctx.io, .global_models);
    defer scope.end();

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
    const filters = normalizeFilters(hx, &scope, params) catch return;
    const raw_cursor = params.get(Q_STARTING_AFTER);
    const after = decodeStart(hx, &scope, filters, limit, raw_cursor) catch return;
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

    const revision = readRevisionOrFail(hx, db.conn) orelse {
        scope.classify(.dependency_error);
        scope.endStage(.cache_revision);
        return;
    };
    scope.endStage(.cache_revision);

    const key = catalogue_key.cacheKey(revision, filters.q, filters.provider, raw_cursor, limit);

    if (cachedBody(hx, key)) |cached| {
        scope.succeed();
        scope.endStageWith(.cache_lookup, .{ .cache = .hit, .bytes = cached.len });
        page_mod.respond(hx, req, cached);
        return;
    }
    scope.endStageWith(.cache_lookup, .{ .cache = .miss });

    const body = page_mod.build(hx, db.conn, filters, after, limit) catch |err| {
        scope.classify(.dependency_error);
        scope.endStage(.sql);
        log.err("page_build_failed", .{
            .error_code = ec.ERR_LIBRARY_DB_UNAVAILABLE,
            .err = @errorName(err),
        });
        hx.fail(ec.ERR_LIBRARY_DB_UNAVAILABLE, S_PAGE_BUILD_FAILED);
        return;
    };
    scope.endStage(.sql);

    storeBody(hx, key, body);
    scope.succeed();
    scope.endStageWith(.serialize, .{ .bytes = body.len });
    page_mod.respond(hx, req, body);
}

/// Normalize `q` and `provider`. Both out-of-bounds cases are `UZ-LIBRARY-003`.
fn normalizeFilters(hx: Hx, scope: *ReadScope, params: anytype) !page_mod.Filters {
    const q = query.normalizeSearch(hx.alloc, params.get(Q_SEARCH)) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_SEARCH_BOUNDS);
        return error.Rejected;
    };
    const provider = query.normalizeProvider(hx.alloc, params.get(Q_PROVIDER)) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, S_SEARCH_BOUNDS);
        return error.Rejected;
    };
    return .{ .q = q, .provider = provider };
}

/// Decode and authorize `starting_after`. Null means the first page.
///
/// Two distinct rejections, and the difference is the point. A cursor that will
/// not decode is `UZ-LIBRARY-001` — not something this endpoint issued. A cursor
/// that decodes but names different filters or a different page size is
/// `UZ-LIBRARY-002` — a real cursor for a different query. Folding them together
/// would hide a filter change inside the same signal as a truncated URL.
///
/// Nothing is trusted from the cursor except the sort boundary: the filters used
/// for the read are always the request's, never the cursor's.
fn decodeStart(
    hx: Hx,
    scope: *ReadScope,
    filters: page_mod.Filters,
    limit: u32,
    raw: ?[]const u8,
) !?model_library_store.PageBoundary {
    const text = raw orelse return null;
    if (text.len == 0) return null;

    const cursor = pagination.decode(hx.alloc, catalogue_key.Cursor, text) catch {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MALFORMED, S_CURSOR_MALFORMED);
        return error.Rejected;
    };
    if (cursor.limit != limit or
        !pagination.filterMatches(cursor.q, filters.q) or
        !pagination.filterMatches(cursor.provider, filters.provider))
    {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MISMATCH, S_CURSOR_MISMATCH);
        return error.Rejected;
    }
    // The uid rides the page SQL as a `::uuid` cast, so a hand-minted cursor
    // whose id is not a UUID must fail here as the malformed input it is — not
    // downstream as a Postgres cast error dressed in a 503.
    if (!id_format.isUuidV7(cursor.id)) {
        scope.classify(.invalid);
        hx.fail(ec.ERR_LIBRARY_CURSOR_MALFORMED, S_CURSOR_MALFORMED);
        return error.Rejected;
    }
    return .{
        .display_key = cursor.display_key,
        .vendor_key = cursor.vendor_key,
        .uid = cursor.id,
    };
}

/// A cached page for this key, or null on a miss.
///
/// A cache fault reads as a miss: the page is rebuildable, so failing the
/// request over it would turn an optimization into a dependency.
///
/// The copy is taken from `res.arena`, not `hx.alloc`: a hit and a miss must
/// hand `respond` memory with the same lifetime, and only the response arena
/// survives the handler's return (see `model_library_page.zig`).
/// The catalogue generation, or null with the 503 already written. No cached
/// data may be served past a failure here: a page whose generation is unknown
/// is precisely the drift the revision exists to prevent.
fn readRevisionOrFail(hx: Hx, conn: *pg.Conn) ?i64 {
    return revision_state.read(conn) catch |err| {
        log.err("revision_read_failed", .{
            .error_code = ec.ERR_LIBRARY_REVISION_UNAVAILABLE,
            .err = @errorName(err),
        });
        hx.fail(ec.ERR_LIBRARY_REVISION_UNAVAILABLE, S_REVISION_UNAVAILABLE);
        return null;
    };
}

fn cachedBody(hx: Hx, key: anytype) ?[]u8 {
    const cache = hx.ctx.model_library_cache orelse return null;
    return cache.fetch(hx.res.arena, key) catch null;
}

/// Admit the page. A refusal (over budget) and an allocation fault are both
/// non-events — the response is already built and is served either way.
fn storeBody(hx: Hx, key: anytype, body: []const u8) void {
    const cache = hx.ctx.model_library_cache orelse return;
    cache.put(key, body) catch return;
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "MODEL_LIBRARY_PATH is the versioned models route" {
    // pin test: literal is the contract — the wire path the router and the
    // TypeScript client both key on.
    try std.testing.expectEqualStrings("/v1/models", MODEL_LIBRARY_PATH);
}
