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
/// Generous bound for the drained peer to observe EOF (instant in practice).
const DRAIN_OBSERVE_TIMEOUT_MS: i32 = 5_000;
const RACE_THREADS: usize = 100;
const RACE_CAP: u32 = 8;

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

test "registry: tryRegister unwinds cleanly under allocation failure" {
    // Fails each of tryRegister's allocation sites (both dupes + the map put)
    // in turn; std.testing.checkAllAllocationFailures asserts nothing leaks on
    // any of the error returns — the only real proof the errdefer chain holds.
    try std.testing.checkAllAllocationFailures(testing.allocator, registerRelease, .{});
    metrics.setSseInFlightStreams(0);
}

fn registerRelease(alloc: std.mem.Allocator) !void {
    var reg = StreamRegistry.init(alloc, common.globalIo());
    defer reg.deinit();
    const maybe = try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP);
    if (maybe) |id| reg.deregister(id);
}

test "registry: deinit frees entries that never deregistered" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    _ = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    // the leak detector is the assertion: deinit must free the live entry
    reg.deinit();
    // deinit is teardown, not release — it does not touch the gauge; reset
    // the global so sibling tests keep their absolute-value asserts
    metrics.setSseInFlightStreams(0);
}

test "registry: drain shuts down attached client sockets so peers see EOF" {
    // Pins drain()'s fd-shutdown loop directly: a real socketpair stands in
    // for the stream's client socket, and the peer end observing EOF is the
    // wake signal a write-blocked stream thread relies on at shutdown.
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    var pair: [2]std.c.fd_t = undefined;
    try testing.expectEqual(@as(c_int, 0), std.c.socketpair(std.posix.AF.LOCAL, std.posix.SOCK.STREAM, 0, &pair));
    defer osClose(pair[1]);
    defer osClose(pair[0]);

    const id = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    reg.attachFd(id, pair[0]);
    reg.drain();

    var fds = [_]std.posix.pollfd{.{ .fd = pair[1], .events = std.posix.POLL.IN, .revents = 0 }};
    const ready = try std.posix.poll(&fds, DRAIN_OBSERVE_TIMEOUT_MS);
    try testing.expect(ready > 0);
    var byte_buf: [1]u8 = undefined;
    // read of 0 = EOF: the RDWR shutdown reached the peer
    try testing.expectEqual(@as(usize, 0), try std.posix.read(pair[1], &byte_buf));

    reg.deregister(id);
}

test "registry: racing registrations admit exactly the cap, never more" {
    // The check-and-insert exists to fix a concurrent over-claim; this is the
    // race that proves it. A barrier releases all threads together so the
    // contention is real, not staggered by spawn latency.
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();

    var gate = std.atomic.Value(bool).init(false);
    var results: [RACE_THREADS]?u64 = @splat(null);
    var threads: [RACE_THREADS]std.Thread = undefined;
    for (&threads, &results) |*t, *slot| {
        t.* = try std.Thread.spawn(.{}, raceRegister, .{ &reg, &gate, slot });
    }
    // safe because: the release store pairs with raceRegister's acquire spin —
    // threads must not observe the open gate before they are all spawned
    gate.store(true, .release);
    for (&threads) |*t| t.join();

    var admitted: u32 = 0;
    for (results) |slot| {
        if (slot) |_| admitted += 1;
    }
    try testing.expectEqual(RACE_CAP, admitted);
    try testing.expectEqual(@as(usize, RACE_CAP), reg.count());
    try testing.expectEqual(@as(u64, RACE_CAP), metrics.snapshot().sse_in_flight_streams);

    for (results) |slot| {
        if (slot) |id| reg.deregister(id);
    }
    try testing.expectEqual(@as(usize, 0), reg.count());
    try testing.expectEqual(@as(u64, 0), metrics.snapshot().sse_in_flight_streams);
}

fn osClose(fd: std.posix.fd_t) void {
    // Zig 0.16 removed std.posix.close; raw-fd close routes through Io.File
    // on the process-global blocking io (the pipe_proto.osClose pattern).
    var file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = false } };
    file.close(common.globalIo());
}

fn raceRegister(reg: *StreamRegistry, gate: *std.atomic.Value(bool), slot: *?u64) void {
    // safe because: acquire pairs with the test body's release store on gate
    while (!gate.load(.acquire)) std.atomic.spinLoopHint();
    slot.* = reg.tryRegister(WS, ZID_A, STARTED_A_MS, RACE_CAP) catch null;
}

test "registry: listing rows carry workspace, agent, and start time — never the fd" {
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

test "registry: listAlloc unwinds partial rows under allocation failure" {
    var reg = StreamRegistry.init(testing.allocator, common.globalIo());
    defer reg.deinit();
    const a = (try reg.tryRegister(WS, ZID_A, STARTED_A_MS, CAP)).?;
    defer reg.deregister(a);
    const b = (try reg.tryRegister(WS, ZID_B, STARTED_B_MS, CAP)).?;
    defer reg.deregister(b);
    // Fails the rows slice + each per-row dupe in turn; nothing may leak on
    // any error return — the partial-rows errdefer chain is the surface
    // under test (a non-arena caller must inherit no leak).
    try std.testing.checkAllAllocationFailures(testing.allocator, listAndFree, .{&reg});
    metrics.setSseInFlightStreams(0);
}

fn listAndFree(alloc: std.mem.Allocator, reg: *StreamRegistry) !void {
    const rows = try reg.listAlloc(alloc);
    for (rows) |row| {
        alloc.free(row.workspace_id);
        alloc.free(row.agent_id);
    }
    alloc.free(rows);
}
