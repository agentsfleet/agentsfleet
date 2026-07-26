//! Building and writing one catalogue page — the response half of `GET
//! /v1/models`.
//!
//! Split from `model_library.zig` per RULE FLL when paging pushed that file past
//! 350 lines. The division mirrors the one §1 already made between
//! `tenant_model_entries.zig` and `tenant_model_entries_list.zig`: the handler
//! owns everything about ASKING for a page — bounds, filters, cursor decode,
//! cache selection — while this file owns everything about PRODUCING one.
//!
//! The page is serialized ONCE. Those bytes are what gets cached, what the ETag
//! hashes, and what the response writes, so a cache hit and a cache miss cannot
//! disagree about a page's identity by formatting it differently.
//!
//! ## The body belongs to the RESPONSE arena, not the handler's
//!
//! `hx.alloc` is the per-dispatch arena `server.zig` builds and `deinit`s when
//! `dispatchMatchedRoute` returns — which is BEFORE httpz writes the response.
//! Anything still referenced by `res` at that moment is read after free: a
//! scrubbed body on a small allocation, an unmapped page and a `writev` EFAULT
//! on a large one. `etag.attach` already dupes the tag into `res.arena` for
//! exactly this reason; the body needs the same home, so `build` serializes
//! straight into `res.arena` and `respond` only ever receives memory that
//! outlives the handler.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const model_library_store = @import("../../state/model_library_store.zig");
const counters = @import("../../observability/library_read_counters.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const etag = @import("../etag.zig");
const pagination = @import("../pagination.zig");
const query = @import("library/query.zig");
const catalogue_key = @import("library/catalogue_key.zig");

const Hx = hx_mod.Hx;

const HEADER_CACHE_CONTROL = "Cache-Control";
const HEADER_VARY = "Vary";

/// `private` because the response is authorized per caller even though the
/// payload is identical for all of them — a shared proxy must not hand one
/// tenant's response to another. `no-cache` means "store it, but revalidate
/// before reuse", which is what makes the ETag load-bearing rather than
/// decorative.
const CACHE_CONTROL_VALUE = "private, no-cache";
const VARY_VALUE = "Authorization";

const S_PAGE_BUILD_FAILED = "Failed to build the catalogue page";

/// Serialization options, named because the body is serialized once and then
/// both hashed and cached — writing under different options than the ones the
/// tag was computed over would give one page two identities.
const PAGE_JSON_OPTIONS: std.json.Stringify.Options = .{};

/// The active filters, already normalized. Carried together so the cursor, the
/// cache key and the query cannot be built from three different readings of one
/// request.
pub const Filters = struct {
    q: ?[]const u8,
    provider: ?[]const u8,
};

/// Build and serialize one page.
///
/// Intermediates (the LIKE pattern, the rows, the cursor) come from `hx.alloc`
/// and die with the dispatch arena. Only the serialized body is allocated from
/// `res.arena`, because only it is still referenced once the handler returns.
pub fn build(
    hx: Hx,
    conn: *pg.Conn,
    filters: Filters,
    after: ?model_library_store.PageBoundary,
    limit: u32,
) ![]u8 {
    const like = if (filters.q) |term| try query.likeContains(hx.alloc, term) else null;
    const page = try model_library_store.listLibraryPage(
        hx.alloc,
        conn,
        .{ .like = like, .provider = filters.provider },
        after,
        limit,
    );
    counters.noteResults(page.models.len);

    const payload = .{
        .version = try formatVersion(hx.alloc, page.max_updated_ms),
        // `models`, not `items`: renaming a shipped v1 field is what
        // docs/REST_API_DESIGN_GUIDELINES.md §9 forbids. `total` and
        // `next_cursor` are ADDED beside it so the page is navigable without
        // breaking a client. `total` is always null — counting a keyset page
        // costs the scan this pagination exists to avoid, and §3 requires the
        // key present rather than omitted.
        .models = page.models,
        .total = std.json.Value{ .null = {} },
        .next_cursor = if (try nextCursor(hx.alloc, page, filters, limit)) |c|
            std.json.Value{ .string = c }
        else
            std.json.Value{ .null = {} },
    };
    return try std.json.Stringify.valueAlloc(hx.res.arena, payload, PAGE_JSON_OPTIONS);
}

/// The cursor resuming after this page, or null when it is the last.
fn nextCursor(
    alloc: std.mem.Allocator,
    page: model_library_store.LibraryPage,
    filters: Filters,
    limit: u32,
) !?[]u8 {
    if (!page.has_more) return null;
    const boundary = page.boundary orelse return null;
    return try pagination.encode(alloc, catalogue_key.Cursor, .{
        .display_key = boundary.display_key,
        .vendor_key = boundary.vendor_key,
        .id = boundary.uid,
        .q = filters.q,
        .provider = filters.provider,
        .limit = limit,
    });
}

/// Attach the validators §2 requires on BOTH answers.
///
/// A 304 that omitted them would tell a cache to stop revalidating the very
/// representation it just revalidated, so these are set before the conditional
/// branch rather than on the 200 path.
fn attachValidators(hx: Hx, tag: []const u8) !void {
    try etag.attach(hx.res, tag);
    try hx.res.headerOpts(HEADER_CACHE_CONTROL, CACHE_CONTROL_VALUE, .{});
    try hx.res.headerOpts(HEADER_VARY, VARY_VALUE, .{});
}

/// Write the page, as 200 or as a bodyless 304.
pub fn respond(hx: Hx, req: *httpz.Request, body: []const u8) void {
    counters.noteEncodedBytes(body.len);

    const tag = etag.compute(hx.alloc, &.{body}) catch {
        common.internalOperationError(hx.res, S_PAGE_BUILD_FAILED, hx.req_id);
        return;
    };
    attachValidators(hx, tag) catch {
        // A response missing its validators would be cached as unconditionally
        // fresh, so this fails the request rather than serving an untaggable
        // body that a proxy may then reuse without revalidating.
        common.internalOperationError(hx.res, S_PAGE_BUILD_FAILED, hx.req_id);
        return;
    };

    if (etag.ifNoneMatch(req)) |candidate| {
        if (etag.matchesIfNoneMatch(candidate, tag)) {
            hx.res.status = @intFromEnum(std.http.Status.not_modified);
            hx.res.body = "";
            return;
        }
    }

    // An empty catalogue is a valid state: the table ships unseeded and platform
    // admins populate it through /v1/admin/models. 200 with an empty `models`
    // array — the dashboard renders "no models yet" rather than treating
    // provisioning as broken.
    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.content_type = .JSON;
    hx.res.body = body;
}

/// Format the maximum updated_at_ms as YYYY-MM-DD (UTC). An empty catalogue
/// yields max_updated_ms = 0 → "1970-01-01", returned with a 200 and an empty
/// `models` array (a valid not-yet-provisioned state), never a 503.
fn formatVersion(alloc: std.mem.Allocator, max_updated_ms: i64) ![]const u8 {
    const seconds: i64 = @divTrunc(max_updated_ms, std.time.ms_per_s);
    const epoch_seconds = std.time.epoch.EpochSeconds{ .secs = @intCast(@max(seconds, 0)) };
    const epoch_day = epoch_seconds.getEpochDay();
    const year_day = epoch_day.calculateYearDay();
    const month_day = year_day.calculateMonthDay();
    return std.fmt.allocPrint(alloc, "{d:0>4}-{d:0>2}-{d:0>2}", .{
        year_day.year,
        @intFromEnum(month_day.month),
        month_day.day_index + 1,
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────

test "the conditional headers are the ones §2 names" {
    // pin test: literal is the contract — a shared cache keying on the wrong
    // header, or omitting Vary, would serve one tenant's page to another.
    try std.testing.expectEqualStrings("private, no-cache", CACHE_CONTROL_VALUE);
    try std.testing.expectEqualStrings("Authorization", VARY_VALUE);
}

test "formatVersion: epoch ms renders as YYYY-MM-DD UTC" {
    // 1745884800000 ms = 2025-04-29 00:00 UTC (the seed timestamp)
    const v = try formatVersion(std.testing.allocator, 1745884800000);
    defer std.testing.allocator.free(v);
    try std.testing.expectEqualStrings("2025-04-29", v);
}

test "formatVersion: zero / negative epoch clamps to 1970-01-01" {
    const v0 = try formatVersion(std.testing.allocator, 0);
    defer std.testing.allocator.free(v0);
    try std.testing.expectEqualStrings("1970-01-01", v0);

    const vn = try formatVersion(std.testing.allocator, -1);
    defer std.testing.allocator.free(vn);
    try std.testing.expectEqualStrings("1970-01-01", vn);
}
