//! Input-refusal proofs for the workspace fleet-library gallery.
//!
//! Every assertion here lands before a pool connection is acquired, which is
//! the property being pinned as much as the responses themselves: a forged
//! cursor or an out-of-range limit must cost no pool slot, so a caller cannot
//! exhaust the pool with requests that were never going to run a query. A
//! context with no pool is exactly the right fixture for that — any arm that
//! reached `hx.db()` would crash here rather than quietly pass.
//!
//! The two cursor rejections are deliberately distinct codes, and the reason is
//! a tenant boundary: a cursor that decodes but names another workspace is a
//! real cursor being replayed across an isolation edge, not a corrupt string.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;
const common_mod = @import("common");

const gallery = @import("gallery.zig");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-gallery-1";
const WORKSPACE_ID = "01932b7c-0000-7000-8000-0000000000c1";
const Q_LIMIT = "limit";
const Q_STARTING_AFTER = "starting_after";
const K_ERROR_CODE = "error_code";

/// `io` is read by the read-scope timer on entry; `pool` is deliberately left
/// undefined so a regression that acquires a connection before validating input
/// fails loudly instead of silently costing a slot.
fn buildCtx() common.Context {
    // SAFETY: see above — every path under test returns before `hx.db()`.
    var ctx: common.Context = undefined;
    ctx.io = common_mod.globalIo();
    return ctx;
}

/// An arena, because production hands one per request (`server.zig`) and the
/// cursor decoder allocates before it can know the cursor is unusable — the
/// scratch is reclaimed with the request, not freed on the rejection path.
fn buildHx(res: *httpz.Response, ctx: *common.Context, alloc: std.mem.Allocator) Hx {
    return Hx{
        .alloc = alloc,
        // SAFETY: authorization runs after the input gates, past every return here.
        .principal = undefined,
        .req_id = REQ_ID,
        .ctx = ctx,
        .res = res,
    };
}

test "should refuse a workspace id that is not a supported identifier" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtx();

    gallery.innerGallery(buildHx(ht.res, &ctx, arena.allocator()), ht.req, "not-a-uuid");

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_INVALID_REQUEST, json.object.get(K_ERROR_CODE).?.string);
}

test "should refuse every page size outside the documented range" {
    const limits = [_][]const u8{ "0", "101", "-1", "abc", "1e2", " " };
    for (limits) |limit| {
        var arena = std.heap.ArenaAllocator.init(testing.allocator);
        defer arena.deinit();
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        ht.query(Q_LIMIT, limit);
        var ctx = buildCtx();

        gallery.innerGallery(buildHx(ht.res, &ctx, arena.allocator()), ht.req, WORKSPACE_ID);

        const json = try ht.getJson();
        testing.expectEqualStrings(ec.ERR_LIBRARY_INPUT_OUT_OF_BOUNDS, json.object.get(K_ERROR_CODE).?.string) catch |e| {
            std.debug.print("limit accepted or misrouted: \"{s}\"\n", .{limit});
            return e;
        };
    }
}

test "should refuse a cursor this endpoint could not have issued" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.query(Q_STARTING_AFTER, "not-base64-and-not-a-cursor");
    var ctx = buildCtx();

    gallery.innerGallery(buildHx(ht.res, &ctx, arena.allocator()), ht.req, WORKSPACE_ID);

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_LIBRARY_CURSOR_MALFORMED, json.object.get(K_ERROR_CODE).?.string);
}

// NOT TESTED HERE — that an empty `starting_after` means "first page" rather
// than a corrupt cursor. The arm exists (`decodeStart` returns null on a
// zero-length value) but proving it requires letting the handler run past the
// input gates into `hx.db()`, and a fixture with no pool cannot survive that.
// It belongs in the integration suite, where a live pool makes the whole
// request observable.
