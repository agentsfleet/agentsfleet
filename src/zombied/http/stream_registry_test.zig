//! StreamRegistry unit tests — pure (no sockets, no Redis): slot admission
//! against the cap, draining rejection, idempotent release, the gauge as a
//! pure function of registry size, and listing rows. The fd-shutdown drain
//! path over a real socket is covered by the SSE drain integration test.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const metrics = @import("../observability/metrics.zig");
const StreamRegistry = @import("stream_registry.zig");

const WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const ZID_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa01";
const ZID_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aaa02";
const CAP: u32 = 2;
/// Fixture start times — values are arbitrary, distinctness is what tests use.
const STARTED_A_MS: i64 = 1_000;
const STARTED_B_MS: i64 = 2_000;
const STARTED_C_MS: i64 = 3_000;
const STARTED_D_MS: i64 = 4_000;

test "registry: check-and-insert admission — at cap returns null, no over-claim" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    const a = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    const b = (try reg.tryRegister(WS, ZID_B, STARTED_B_MS, CAP)).?;
    try testing.expect(a != b);
    try testing.expectEqual(@as(usize, 2), reg.count());
    try testing.expectEqual(@as(u64, 2), metrics.snapshot().sse_in_flight_streams);

    // at cap: rejected without disturbing the live count
    try testing.expectEqual(@as(?u64, null), try reg.tryRegister(WS, ZID_A, STARTED_C_MS, CAP));
    try testing.expectEqual(@as(usize, 2), reg.count());

    reg.deregister(a);
    try testing.expectEqual(@as(usize, 1), reg.count());
    try testing.expectEqual(@as(u64, 1), metrics.snapshot().sse_in_flight_streams);

    // freed slot admits again
    const c = (try reg.tryRegister(WS, ZID_A, STARTED_D_MS, CAP)).?;
    reg.deregister(c);
    reg.deregister(b);
    try testing.expectEqual(@as(u64, 0), metrics.snapshot().sse_in_flight_streams);
}

test "registry: deregister is idempotent — a double release is a no-op" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    const id = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    reg.deregister(id);
    reg.deregister(id);
    try testing.expectEqual(@as(usize, 0), reg.count());
    try testing.expectEqual(@as(u64, 0), metrics.snapshot().sse_in_flight_streams);
}

test "registry: drain rejects new registrations; unattached entries are skipped" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    // entry with no attached fd (request-thread window) — drain must not
    // shutdown anything for it
    const id = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    reg.drain();
    reg.deregister(id);
    reg.awaitEmpty();
    try testing.expectEqual(@as(?u64, null), try reg.tryRegister(WS, ZID_B, STARTED_B_MS, CAP));
    try testing.expectEqual(@as(usize, 0), reg.count());
}

test "registry: listing rows carry workspace, zombie, and start time — never the fd" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    const a = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    defer reg.deregister(a);
    const b = (try reg.tryRegister(WS, ZID_B, STARTED_B_MS, CAP)).?;
    defer reg.deregister(b);

    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const rows = try reg.listAlloc(arena.allocator());
    try testing.expectEqual(@as(usize, 2), rows.len);
    for (rows) |row| {
        try testing.expectEqualStrings(WS, row.workspace_id);
        try testing.expect(row.started_ms == STARTED_A_MS or row.started_ms == STARTED_B_MS);
    }
    comptime {
        // the listing row type must never grow a socket field
        for (std.meta.fields(StreamRegistry.ListedStream)) |f| {
            std.debug.assert(!std.mem.eql(u8, f.name, "fd"));
        }
    }
}
