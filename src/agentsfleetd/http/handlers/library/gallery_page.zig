//! Fleet gallery page construction — the merged two-library read, and how one
//! row becomes one card.
//!
//! Split from `gallery.zig` per RULE FLL when the stage instrumentation pushed
//! that file past its cap. The seam matches the one the tenant registry uses:
//! this module owns PRODUCING a page; `gallery.zig` owns everything about
//! ASKING for one — bounds, cursor decode, authorization, and the response.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const counters = @import("../../../observability/library_read_counters.zig");
const gallery_sql = @import("../../../fleet_library/gallery_sql.zig");
const keyset = @import("fleet_keyset.zig");
const pagination = @import("../../pagination.zig");
const entry_view = @import("entry_view.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const Hx = @import("../hx.zig").Hx;

/// One gallery card. Everything here is rendered; see the module note for the
/// one field deliberately absent.
pub const SummaryEntry = struct {
    id: []const u8,
    name: []const u8,
    description: []const u8,
    visibility: []const u8,
    source_ref: []const u8,
    created_at: i64,
    requirements: entry_view.Requirements,
    required_credentials_reasons: std.json.Value,
};

pub const Page = struct {
    items: []const SummaryEntry,
    /// Always null. Counting a keyset page costs the scan this pagination exists
    /// to avoid; §Interfaces requires the key present rather than omitted.
    total: ?u64 = null,
    next_cursor: ?[]const u8 = null,
};

pub fn buildPage(
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
