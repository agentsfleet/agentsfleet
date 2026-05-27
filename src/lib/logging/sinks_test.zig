//! Unit tests for the log sink registry. Split from sinks.zig to
//! keep the source file under the 350-line LENGTH GATE cap and to
//! match the codebase's *_test.zig discovery convention (see
//! src/queue/redis_pool_test.zig et al.).

const std = @import("std");
const sinks = @import("sinks.zig");

const BufferedSink = sinks.BufferedSink;
const registerSink = sinks.registerSink;
const clearSinks = sinks.clearSinks;
const sinksRegistered = sinks.sinksRegistered;
const emitToSinks = sinks.emitToSinks;
const emitTicketsForTest = sinks.emitTicketsForTest;

// MAX_SINKS is sinks.zig-private (intentional — production has 4 slots
// and the cap is an implementation detail). The capacity test below
// pins the externally observable consequence rather than the literal.
const MAX_SINKS: usize = 4;

test "registerSink + emitToSinks fans out to every registered sink" {
    var bs = BufferedSink.init(std.testing.allocator);
    defer bs.deinit();

    clearSinks();
    defer clearSinks();
    registerSink(bs.sink());

    emitToSinks(.warn, "test_scope", 1234, "event=hello x=1");
    emitToSinks(.err, "test_scope", 5678, "event=goodbye");

    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=hello") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=goodbye") != null);
}

test "clearSinks: subsequent emit fans out to nobody" {
    var bs = BufferedSink.init(std.testing.allocator);
    defer bs.deinit();

    clearSinks();
    registerSink(bs.sink());
    emitToSinks(.info, "s", 0, "event=first");
    clearSinks();
    emitToSinks(.info, "s", 0, "event=dropped");

    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=first") != null);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=dropped") == null);
}

test "registerSink: capacity capped at MAX_SINKS, extra registrations drop" {
    clearSinks();
    defer clearSinks();

    var bs = BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    // Fill up.
    var i: usize = 0;
    while (i < MAX_SINKS) : (i += 1) registerSink(bs.sink());
    try std.testing.expect(sinksRegistered());

    // Overflow drops silently — never realloc, never crash. The cap is
    // a static array; growth at runtime would require a thread-safe
    // realloc dance that's not worth the complexity for 4 sinks total.
    registerSink(bs.sink());

    // Emit once and confirm we still got exactly MAX_SINKS deliveries
    // (each appends one body+newline) — no overflow corruption.
    emitToSinks(.info, "s", 0, "x");
    const captured = try bs.snapshot();
    defer std.testing.allocator.free(captured);
    var newlines: usize = 0;
    for (captured) |c| {
        if (c == '\n') newlines += 1;
    }
    try std.testing.expectEqual(MAX_SINKS, newlines);
}

test "BufferedSink.deinit unregisters itself from the global registry" {
    // Safety invariant: bs.deinit() must remove its own entries from
    // the registry before freeing self.buf. Without that, defer
    // ordering (deinit declared after clearSinks → deinit runs first
    // per LIFO) leaves a stack-pointer ctx in sinks_buf that the next
    // emit dereferences. This test pins the property explicitly.
    clearSinks();
    defer clearSinks();

    var bs = BufferedSink.init(std.testing.allocator);
    registerSink(bs.sink());
    try std.testing.expect(sinksRegistered());

    bs.deinit();
    try std.testing.expect(!sinksRegistered());
}

test "unregisterByCtx leaves unrelated sinks intact" {
    // Two BufferedSinks registered side by side; deinit-ing one must
    // not pull the other out of the registry.
    clearSinks();
    defer clearSinks();

    var bs_a = BufferedSink.init(std.testing.allocator);
    var bs_b = BufferedSink.init(std.testing.allocator);
    defer bs_b.deinit();

    registerSink(bs_a.sink());
    registerSink(bs_b.sink());

    bs_a.deinit();

    // bs_b's emit still fires; bs_a's would crash if it were still in
    // the registry (stack-freed ctx).
    emitToSinks(.info, "s", 0, "event=after_a_deinit");
    const captured = try bs_b.snapshot();
    defer std.testing.allocator.free(captured);
    try std.testing.expect(std.mem.indexOf(u8, captured, "event=after_a_deinit") != null);
}

test "snapshot returns owned copy — later emits do not mutate prior snapshot" {
    // This pins the core safety property of the snapshot owned-copy
    // fix. Before the fix, snapshot returned `self.buf.items` directly
    // — a slice aliasing the live ArrayList backing storage. An emit
    // that triggered realloc would free that backing buffer mid-read,
    // turning a caller's `indexOf` into a use-after-free. The owned
    // dupe pattern means snap1 captures bytes at the point of call;
    // subsequent emits grow self.buf without touching snap1's memory.
    clearSinks();
    defer clearSinks();

    var bs = BufferedSink.init(std.testing.allocator);
    defer bs.deinit();
    registerSink(bs.sink());

    emitToSinks(.info, "s", 0, "event=first");
    const snap1 = try bs.snapshot();
    defer std.testing.allocator.free(snap1);
    const snap1_len = snap1.len;

    // Drive enough emits to force ArrayList realloc (initial cap is
    // typically tiny — 100 emits of a ~40-byte body easily exceeds it).
    var i: usize = 0;
    while (i < 100) : (i += 1) {
        emitToSinks(.info, "s", 0, "event=growth_after_snapshot_xxxxxxxxx");
    }

    // snap1 must still be readable AND must NOT reflect any post-
    // snapshot growth. Length unchanged + no new event substring.
    try std.testing.expectEqual(snap1_len, snap1.len);
    try std.testing.expect(std.mem.indexOf(u8, snap1, "event=first") != null);
    try std.testing.expect(std.mem.indexOf(u8, snap1, "event=growth") == null);
}

test "emit after BufferedSink.deinit does not crash — sink is unregistered" {
    // Safety pin: even if callers forget `defer clearSinks()`, the
    // self-unregister in BufferedSink.deinit guarantees subsequent
    // emits don't reach the freed sink. Without unregisterByCtx in
    // deinit, this emit would dereference a stack-freed ctx and
    // either crash, corrupt nearby memory, or — worst case — appear
    // to "work" in debug mode while breaking under ReleaseSafe.
    clearSinks();

    {
        var bs = BufferedSink.init(std.testing.allocator);
        registerSink(bs.sink());
        try std.testing.expect(sinksRegistered());
        bs.deinit();
    }

    // bs is out of scope; its stack memory is now reclaimable. An emit
    // through the registry must NOT call into the freed sink. The
    // deinit-unregisters invariant from the previous test combined
    // with emitToSinks's snapshot-then-emit pattern means this emit
    // sees sinks_len == 0 and early-returns under lock.
    emitToSinks(.info, "s", 0, "event=should_be_dropped");

    // Registry confirms empty after the emit.
    try std.testing.expect(!sinksRegistered());
}

test "unregisterByCtx drain is bounded — no-sink emits don't grow drain_target" {
    // Pins the bounded-drain property that motivated the split into
    // started/completed ticket counters. With the old single
    // emit_in_flight counter, ANY in-flight emit forced
    // unregisterByCtx to wait — including emits whose registry
    // snapshot post-dated compaction and therefore didn't hold a
    // pointer to the removed sink. Two observable consequences must
    // hold after the fix:
    //   1. Emits taken while no sinks are registered short-circuit
    //      under sinks_mutex and never increment emit_started, so
    //      they can't extend the drain_target snapshot of a later
    //      deinit.
    //   2. After a deinit returns, started == completed (the
    //      pre-removal in-flight set has fully drained).
    clearSinks();
    defer clearSinks();

    const baseline = emitTicketsForTest();
    var i: usize = 0;
    while (i < 64) : (i += 1) emitToSinks(.info, "s", 0, "event=no_sinks");
    const after_empty_emits = emitTicketsForTest();
    try std.testing.expectEqual(baseline.started, after_empty_emits.started);

    var bs = BufferedSink.init(std.testing.allocator);
    registerSink(bs.sink());
    emitToSinks(.info, "s", 0, "event=real_emit");
    bs.deinit();
    const after_deinit = emitTicketsForTest();
    try std.testing.expectEqual(after_deinit.started, after_deinit.completed);
}
