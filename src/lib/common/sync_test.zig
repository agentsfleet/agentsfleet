// Tests for sync.zig — the blocking primitives every shared cache and sweeper
// leans on. The load-bearing claim is RwLock's: `lockShared` must not
// serialize readers (it is the whole reason `model_library_cache.fetch` can
// run concurrent catalogue reads), and a writer must be excluded while any
// reader holds the lock. Both are proved with a second thread and the
// deterministic `Event` handshake rather than sleeps.

const std = @import("std");
const sync = @import("sync.zig");

const testing = std.testing;

// Generous bound for "the other thread definitely ran": these waits resolve in
// microseconds when the property holds and only run out when it does not.
const HANDSHAKE_NS: u64 = 2 * std.time.ns_per_s;
// Short bound for asserting a thread is BLOCKED: long enough to schedule it,
// far too short to matter in the suite's runtime.
const BLOCKED_PROBE_NS: u64 = 50 * std.time.ns_per_ms;

test "Mutex: tryLock refuses while held and succeeds after unlock" {
    var m = sync.Mutex{};
    m.lock();
    try testing.expect(!m.tryLock());
    m.unlock();
    try testing.expect(m.tryLock());
    m.unlock();
}

const SharedProbe = struct {
    lock: *sync.RwLock,
    acquired: sync.Event = .{},

    fn run(self: *SharedProbe) void {
        self.lock.lockShared();
        self.acquired.set();
        self.lock.unlockShared();
    }
};

test "RwLock: shared holders do not serialize against each other" {
    var l = sync.RwLock{};
    l.lockShared();
    defer l.unlockShared();

    // A second reader acquires WHILE this thread still holds shared — if
    // lockShared were exclusive, the probe would block and the wait times out.
    var probe = SharedProbe{ .lock = &l };
    const t = try std.Thread.spawn(.{}, SharedProbe.run, .{&probe});
    defer t.join();
    try probe.acquired.timedWait(HANDSHAKE_NS);
}

const WriterProbe = struct {
    lock: *sync.RwLock,
    acquired: sync.Event = .{},

    fn run(self: *WriterProbe) void {
        self.lock.lock();
        self.acquired.set();
        self.lock.unlock();
    }
};

test "RwLock: a writer waits out a reader, then proceeds" {
    var l = sync.RwLock{};
    l.lockShared();

    var probe = WriterProbe{ .lock = &l };
    const t = try std.Thread.spawn(.{}, WriterProbe.run, .{&probe});
    defer t.join();

    // Excluded while the read lock is held. (A broken lock that admitted the
    // writer would set the event immediately and fail this expectation.)
    try testing.expectError(error.Timeout, probe.acquired.timedWait(BLOCKED_PROBE_NS));

    // Releasing the reader is what lets the writer through.
    l.unlockShared();
    try probe.acquired.timedWait(HANDSHAKE_NS);
}

test "WaitGroup: wait returns when every started unit finishes, and it is reusable" {
    var wg = sync.WaitGroup{};
    wg.start();
    wg.start();
    try testing.expectEqual(@as(usize, 2), wg.pending());
    wg.finish();
    wg.finish();
    wg.wait(); // count is zero — must not block
    try testing.expectEqual(@as(usize, 0), wg.pending());

    // Reuse across rounds is part of the surface.
    wg.start();
    wg.finish();
    wg.wait();
}

test "Event: timedWait times out unfired and returns once set" {
    var e = sync.Event{};
    try testing.expect(!e.isSet());
    try testing.expectError(error.Timeout, e.timedWait(BLOCKED_PROBE_NS));
    e.set();
    try e.timedWait(BLOCKED_PROBE_NS);
    try testing.expect(e.isSet());
}
