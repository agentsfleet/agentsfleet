//! Integration tier for §1 Dimension 1.1 — the tenant model registry page.
//!
//! Three claims, none of which a unit test over the cursor codec can reach,
//! because all three are about what the DATABASE returns and what the read path
//! touches on the way:
//!
//!   1. **Order is exact.** `created_at DESC, id DESC`. The fixture deliberately
//!      seeds a `created_at` TIE, because the tie is the only case where the
//!      second sort key is load-bearing — an implementation that forgot `id`
//!      entirely still passes a fixture with distinct timestamps, and then
//!      returns rows in whatever order the plan happened to produce.
//!   2. **The cursor walks the whole set exactly once.** Paged end to end, the
//!      concatenation of every page equals the unpaged order with no duplicate
//!      and no gap, and only the last page carries `next_cursor: null`. A
//!      boundary that is inclusive on one side rather than exclusive shows up
//!      here as a repeated row, which is precisely the bug keyset pagination is
//!      prone to.
//!   3. **The page decrypts nothing.** Invariant 5. `schema/036` promoted the
//!      four displayed fields to columns so the read never opens an envelope;
//!      this asserts the tally is still zero after a full walk, so a later edit
//!      that reaches for `vault.loadJson` on the read path fails here rather
//!      than quietly costing a Key Encryption Key unwrap per row.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const db_pool = @import("../db/pool.zig");

const base = @import("../db/test_fixtures.zig");
const cp = @import("../secrets/crypto_primitives.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const entries_state = @import("tenant_model_entries.zig");
const pagination = @import("../http/pagination.zig");
const vault = @import("vault.zig");
const view = @import("../http/handlers/tenant_model_entries_view.zig");

const TENANT = base.TEST_TENANT_ID;
const WORKSPACE = "0195b4ba-8d3a-7f13-8abc-0000000e0001";

/// One credential backing every seeded entry. The registry page is a metadata
/// read, so which credential the rows name does not matter — that exactly one
/// exists is what keeps the projection on its normal path rather than the
/// degraded `custom_secret` branch, which would prove nothing about decrypts.
const SECRET_REF = "registry-page-key";

/// Fixed ids so the expected order is a constant in this file rather than
/// something the test recomputes with the same logic it is checking.
/// UUIDv7-shaped: lexicographic order equals the native UUID order the query
/// sorts by, so `...0007` sorts above `...0001` under `id DESC`.
fn entryId(n: u8) []const u8 {
    return switch (n) {
        1 => "0195b4ba-8d3a-7f13-8abc-0000000f0001",
        2 => "0195b4ba-8d3a-7f13-8abc-0000000f0002",
        3 => "0195b4ba-8d3a-7f13-8abc-0000000f0003",
        4 => "0195b4ba-8d3a-7f13-8abc-0000000f0004",
        5 => "0195b4ba-8d3a-7f13-8abc-0000000f0005",
        else => unreachable,
    };
}

/// `created_at` per entry. Entries 2, 3 and 4 SHARE a timestamp: that tie is
/// what forces `id DESC` to decide, and is the case a single-key sort passes by
/// accident on distinct timestamps.
fn createdAt(n: u8) i64 {
    return switch (n) {
        1 => 1_745_884_800_000,
        2, 3, 4 => 1_745_884_900_000,
        5 => 1_745_885_000_000,
        else => unreachable,
    };
}

const SEEDED: u8 = 5;

/// `created_at DESC, id DESC` over the fixture above: the lone newest row, then
/// the three tied rows highest-id first, then the oldest.
const EXPECTED_ORDER = [SEEDED]u8{ 5, 4, 3, 2, 1 };

const TestDb = struct {
    pool: *db_pool.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

/// Seed the registry. `create` stamps `created_at` from the clock, so every row
/// would land in the same millisecond or in an order the test does not control;
/// the UPDATE afterwards is what makes the tie deliberate rather than incidental.
fn seedRegistry(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspaceWithTenant(conn, WORKSPACE, TENANT);
    try vault.storeJsonPlaintext(alloc, conn, WORKSPACE, SECRET_REF,
        \\{"kind":"llm_provider","provider":"anthropic","api_key":"sk-test","base_url":"https://api.anthropic.com"}
    );

    var n: u8 = 1;
    while (n <= SEEDED) : (n += 1) {
        const model = try std.fmt.allocPrint(alloc, "claude-model-{d}", .{n});
        defer alloc.free(model);
        var entry = try entries_state.create(alloc, conn, .{
            .id = entryId(n),
            .tenant_id = TENANT,
            .model_id = model,
            .secret_ref = SECRET_REF,
        });
        entry.deinit(alloc);
        _ = try conn.exec(
            "UPDATE core.tenant_model_entries SET created_at = $1 WHERE id = $2::uuid",
            .{ createdAt(n), entryId(n) },
        );
    }
}

fn teardown(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.tenant_model_entries WHERE tenant_id = $1::uuid", .{TENANT}) catch |err|
        std.log.warn("entry wipe ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{WORKSPACE}) catch |err|
        std.log.warn("secret wipe ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, WORKSPACE);
}

/// Walk every page at `limit`, returning the ids in the order the pages produced
/// them. Drives `view.buildList`, not the raw state query, so the cursor is
/// encoded and decoded through exactly the codec the handler uses.
fn walkPages(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    limit: u32,
    out: *std.ArrayList([]u8),
) !void {
    // `pagination.decode` allocates the payload's slices and frees none of them
    // individually — it is written for a request-scoped allocator, which every
    // handler calling it supplies. This arena is that scope. It also outlives
    // each loop iteration, which matters: `after.id` points into it and is read
    // by the next page's query.
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();

    var after: ?entries_state.PageStart = null;
    // Bounded so a cursor that fails to advance ends the test with a clear
    // assertion instead of looping until the harness times out.
    var guard: usize = 0;
    while (guard <= SEEDED + 1) : (guard += 1) {
        var result = try view.buildList(alloc, conn, TENANT, limit, after, null);
        defer result.deinit(alloc);

        for (result.rows) |row| try out.append(alloc, try alloc.dupe(u8, row.id));

        const cursor = result.next_cursor orelse return;
        const decoded = try pagination.decode(arena.allocator(), view.Cursor, cursor);
        after = .{ .created_at = decoded.created_at, .id = decoded.id };
    }
    return error.CursorDidNotTerminate;
}

fn freeIds(alloc: std.mem.Allocator, ids: *std.ArrayList([]u8)) void {
    for (ids.items) |id| alloc.free(id);
    ids.deinit(alloc);
}

test "integration: test_tenant_registry_page_is_bounded" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.conn); // a prior run that died mid-test must not skew this one
    try seedRegistry(alloc, db.conn);
    defer teardown(db.conn);

    // ── 1. order is exact, including the id tiebreak on equal created_at ──
    crypto_store.resetDecryptCountForTest();
    var unpaged: std.ArrayList([]u8) = .empty;
    defer freeIds(alloc, &unpaged);
    try walkPages(alloc, db.conn, SEEDED, &unpaged);

    try std.testing.expectEqual(@as(usize, SEEDED), unpaged.items.len);
    for (EXPECTED_ORDER, 0..) |want, i| {
        try std.testing.expectEqualStrings(entryId(want), unpaged.items[i]);
    }

    // ── 3. the whole walk opened no envelope ──
    // Asserted here, before the paged walk, so a failure names the simplest
    // read that could have caused it.
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());

    // ── 2. paging reproduces that order exactly once ──
    // limit=2 over 5 rows gives 3 pages with a ragged last one, so it exercises
    // a full page, a boundary landing mid-tie (entries 4/3/2 share a timestamp),
    // and a short final page.
    crypto_store.resetDecryptCountForTest();
    var paged: std.ArrayList([]u8) = .empty;
    defer freeIds(alloc, &paged);
    try walkPages(alloc, db.conn, 2, &paged);

    try std.testing.expectEqual(unpaged.items.len, paged.items.len);
    for (unpaged.items, paged.items) |want, got| {
        try std.testing.expectEqualStrings(want, got);
    }

    // No duplicates: a boundary that is inclusive rather than exclusive repeats
    // the row it resumed from, and the length check above would still pass if a
    // different row had been dropped in exchange.
    for (paged.items, 0..) |a, i| {
        for (paged.items[i + 1 ..]) |b| try std.testing.expect(!std.mem.eql(u8, a, b));
    }

    // Paging did not reintroduce decryption — the per-page projection is the
    // place a per-row envelope open would most plausibly creep back in.
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());
}

test "integration: test_tenant_registry_page_is_bounded: the last page reports no successor" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    cp.setTestKek();
    teardown(db.conn);
    try seedRegistry(alloc, db.conn);
    defer teardown(db.conn);

    // A page exactly as large as the set must still say "no more". The
    // over-fetch probe is what decides this, and off-by-one there yields a
    // cursor pointing at an empty page — navigable, but it makes a client
    // request a page that can never contain anything.
    var full = try view.buildList(alloc, db.conn, TENANT, SEEDED, null, null);
    defer full.deinit(alloc);
    try std.testing.expectEqual(@as(usize, SEEDED), full.rows.len);
    try std.testing.expect(full.next_cursor == null);

    // One short of the set must say the opposite.
    var partial = try view.buildList(alloc, db.conn, TENANT, SEEDED - 1, null, null);
    defer partial.deinit(alloc);
    try std.testing.expectEqual(@as(usize, SEEDED - 1), partial.rows.len);
    try std.testing.expect(partial.next_cursor != null);
}
